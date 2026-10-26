import { type SpawnOptions, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import {
	copyFile,
	mkdir,
	open,
	readFile,
	rename,
	rm,
	writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, win32 } from "node:path";

export type RuntimeMeta = {
	readonly version: string;
	readonly url: string;
	readonly sha256: string;
};

export const RUNTIME_DOWNLOAD_TIMEOUT_MS = 300_000;

export function runtimeZipName(version: string): string {
	return `kras-quick-runtime-v${version}.zip`;
}

export function runtimeCacheDir(env: NodeJS.ProcessEnv): string {
	return (
		env["KRAS_QUICK_CACHE_DIR"] ??
		join(env["LOCALAPPDATA"] ?? tmpdir(), "kras-quick")
	);
}

export async function runProcess(
	command: string,
	args: readonly string[],
	cwd?: string,
	env?: NodeJS.ProcessEnv,
): Promise<number> {
	return await new Promise((resolve, reject) => {
		const options: SpawnOptions = { stdio: "inherit", windowsHide: false };
		if (cwd !== undefined) options.cwd = cwd;
		if (env !== undefined) options.env = env;
		const child = spawn(command, args, options);
		child.once("error", reject);
		child.once("close", (code) => resolve(code ?? 1));
	});
}

export const TERMINAL_REENTRY_FLAG = "--kras-quick-terminal";

export function terminalEntry(args: readonly string[]): {
	readonly args: readonly string[];
	readonly reentered: boolean;
} {
	if (args[0] !== TERMINAL_REENTRY_FLAG) return { args, reentered: false };
	const payload = args[1];
	if (
		args.length !== 2 ||
		payload === undefined ||
		Buffer.from(payload, "base64").toString("base64") !== payload
	)
		throw new TypeError("잘못된 터미널 재진입 인수입니다.");
	const restored: unknown = JSON.parse(
		Buffer.from(payload, "base64").toString("utf8"),
	);
	if (
		!Array.isArray(restored) ||
		!restored.every((arg: unknown): arg is string => typeof arg === "string")
	)
		throw new TypeError("터미널 재진입 인수는 문자열 배열이어야 합니다.");
	return { args: restored, reentered: true };
}

export function terminalHandoffEligible(
	platform: NodeJS.Platform,
	interactive: boolean,
	reentered: boolean,
): boolean {
	return platform === "win32" && interactive && !reentered;
}

export function terminalBootstrap(
	exePath: string,
	args: readonly string[],
	cwd: string,
): string {
	const payload = Buffer.from(JSON.stringify(args), "utf8").toString("base64");
	const childCode = `$ErrorActionPreference='Stop'
Set-Location -LiteralPath '${cwd.replaceAll("'", "''")}'
& '${exePath.replaceAll("'", "''")}' '${TERMINAL_REENTRY_FLAG}' '${payload}'
exit $LASTEXITCODE`;
	const encoded = Buffer.from(childCode, "utf16le").toString("base64");
	const code = `$ErrorActionPreference='Stop'
$pwsh = Get-Command pwsh.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$standard = if ($env:ProgramFiles) { Join-Path $env:ProgramFiles 'PowerShell\\7\\pwsh.exe' } else { '' }
$shell = if ($pwsh) { $pwsh.Path } elseif ($standard -and (Test-Path -LiteralPath $standard -PathType Leaf)) { $standard } else { Join-Path $env:SystemRoot 'System32\\WindowsPowerShell\\v1.0\\powershell.exe' }
$arguments = '-NoLogo -NoProfile -EncodedCommand ${encoded}'
$wt = Get-Command wt.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
$wtFailure = 'wt.exe unavailable'
if ($wt) {
  try {
    $client = Start-Process -FilePath $wt.Path -ArgumentList ('-w 0 new-tab "' + $shell + '" ' + $arguments) -PassThru -ErrorAction Stop
    try {
      $null = $client.Handle
      # Wait only for the WT client, never Start-Process -Wait's process tree.
      $client.WaitForExit()
      if ($client.ExitCode -eq 0) { return }
      $wtFailure = "wt.exe exit $($client.ExitCode)"
    } finally { $client.Dispose() }
  } catch { $wtFailure = $_.Exception.Message }
}
try { $null = Start-Process -FilePath $shell -ArgumentList $arguments -ErrorAction Stop }
catch { throw "$wtFailure; PowerShell: $($_.Exception.Message)" }`;
	return Buffer.from(code, "utf16le").toString("base64");
}

export class TerminalLaunchError extends Error {
	override readonly name = "TerminalLaunchError";
	readonly exitCode: number;
	constructor(exitCode: number, detail: string) {
		super(`터미널 시작 실패 (exit ${exitCode}): ${detail}`);
		this.exitCode = exitCode;
	}
}

export async function launchQuickTerminal(
	exePath: string,
	args: readonly string[],
): Promise<void> {
	const systemRoot = process.env["SystemRoot"];
	if (!systemRoot)
		throw new TypeError(
			"SystemRoot가 없어 Windows PowerShell을 찾을 수 없습니다.",
		);
	const command = win32.join(
		systemRoot,
		"System32",
		"WindowsPowerShell",
		"v1.0",
		"powershell.exe",
	);
	const bootstrapArgs = [
		"-NoLogo",
		"-NoProfile",
		"-NonInteractive",
		"-EncodedCommand",
		terminalBootstrap(exePath, args, process.cwd()),
	];
	if (command.length + bootstrapArgs.join(" ").length + 4 >= 32_767)
		throw new RangeError(
			"터미널 시작 인수가 Windows 명령줄 길이 한도를 초과합니다.",
		);
	// Success confirms the host start request, not a visible window or the quick app's exit.
	await new Promise<void>((resolve, reject) => {
		const child = spawn(command, bootstrapArgs, {
			windowsHide: true,
			stdio: ["ignore", "ignore", "pipe"],
		});
		let stderr = "";
		child.stderr.setEncoding("utf8");
		child.stderr.on("data", (data: string) => {
			stderr += data;
		});
		child.once("error", reject);
		child.once("close", (code) =>
			code === 0
				? resolve()
				: reject(new TerminalLaunchError(code ?? 1, stderr.trim())),
		);
	});
}

export async function sha256File(path: string): Promise<string> {
	const hash = createHash("sha256");
	const file = await open(path, "r");
	try {
		const buffer = Buffer.alloc(1 << 20);
		while (true) {
			const { bytesRead } = await file.read(buffer, 0, buffer.length, null);
			if (bytesRead === 0) break;
			hash.update(buffer.subarray(0, bytesRead));
		}
	} finally {
		await file.close();
	}
	return hash.digest("hex");
}

async function downloadZip(
	url: string,
	tmpPath: string,
	expectedSha: string,
): Promise<void> {
	const response = await fetch(url, {
		signal: AbortSignal.timeout(RUNTIME_DOWNLOAD_TIMEOUT_MS),
	});
	if (!response.ok)
		throw new Error(
			`런타임 ZIP 다운로드 실패 (HTTP ${response.status}): ${url}`,
		);
	if (response.body === null)
		throw new Error(`런타임 ZIP 다운로드 실패: 응답 본문이 없습니다: ${url}`);
	const hash = createHash("sha256");
	const file = await open(tmpPath, "w");
	const reader = response.body.getReader();
	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			hash.update(value);
			await file.write(value);
		}
	} finally {
		reader.releaseLock();
		await file.close();
	}
	const digest = hash.digest("hex");
	if (digest.toLowerCase() !== expectedSha.toLowerCase()) {
		throw new Error(
			`런타임 ZIP SHA-256 불일치 (기대: ${expectedSha}, 실제: ${digest})`,
		);
	}
}

async function extractZip(zipPath: string, extractDir: string): Promise<void> {
	const escapedZip = zipPath.replaceAll("'", "''");
	const escapedDir = extractDir.replaceAll("'", "''");
	const code = `$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath '${escapedZip}' -DestinationPath '${escapedDir}' -Force`;
	const exitCode = await runProcess("powershell.exe", [
		"-NoProfile",
		"-NonInteractive",
		"-Command",
		code,
	]);
	if (exitCode !== 0)
		throw new Error(`런타임 ZIP 압축 해제 실패 (exit ${exitCode})`);
}

export async function ensureRuntime(
	meta: RuntimeMeta,
	env: NodeJS.ProcessEnv,
): Promise<string> {
	const cacheDir = runtimeCacheDir(env);
	const url = env["KRAS_QUICK_RUNTIME_URL"] ?? meta.url;
	const expectedSha = env["KRAS_QUICK_RUNTIME_SHA256"] ?? meta.sha256;
	const zipPath = join(cacheDir, runtimeZipName(meta.version));
	const extractDir = join(cacheDir, `kras-quick-runtime-v${meta.version}`);
	const nodeMarker = join(extractDir, "node", "node.exe");
	const scriptMarker = join(extractDir, "scripts", "property-auto-runner.ts");
	const tesseractMarker = join(extractDir, "tesseract", "tesseract.exe");
	const extractHashMarker = join(extractDir, ".runtime-sha256");

	await mkdir(cacheDir, { recursive: true });

	// 재사용 검증: 캐시된 ZIP의 해시가 기대값과 일치하면 재다운로드 없이 사용.
	let zipReady = false;
	if (existsSync(zipPath)) {
		if (
			(await sha256File(zipPath)).toLowerCase() === expectedSha.toLowerCase()
		) {
			zipReady = true;
		} else {
			await rm(zipPath, { force: true });
		}
	}

	if (!zipReady) {
		const localZipPath = env["KRAS_QUICK_RUNTIME_PATH"];
		if (localZipPath !== undefined && existsSync(localZipPath)) {
			if (
				(await sha256File(localZipPath)).toLowerCase() !==
				expectedSha.toLowerCase()
			) {
				throw new Error(`로컬 런타임 ZIP SHA-256 불일치: ${localZipPath}`);
			}
			await copyFile(localZipPath, zipPath);
			zipReady = true;
		}
	}

	if (!zipReady) {
		const tmpPath = `${zipPath}.tmp`;
		await rm(tmpPath, { force: true });
		try {
			await downloadZip(url, tmpPath, expectedSha);
			// 검증 완료된 바이트만 최종 이름으로 원자적 이동.
			await rename(tmpPath, zipPath);
		} finally {
			await rm(tmpPath, { force: true });
		}
	}

	let extractedHash = "";
	try {
		extractedHash = (await readFile(extractHashMarker, "utf8"))
			.trim()
			.toLowerCase();
	} catch {
		extractedHash = "";
	}
	const extractedReady =
		existsSync(nodeMarker) &&
		existsSync(scriptMarker) &&
		existsSync(tesseractMarker) &&
		extractedHash === expectedSha.toLowerCase();
	if (!extractedReady) {
		await rm(extractDir, { recursive: true, force: true });
		await extractZip(zipPath, extractDir);
		await writeFile(extractHashMarker, expectedSha.toLowerCase(), "utf8");
	}
	return extractDir;
}
