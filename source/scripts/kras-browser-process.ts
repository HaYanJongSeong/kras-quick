import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { win32 } from "node:path";
import { setTimeout } from "node:timers/promises";
import { promisify } from "node:util";
import { z } from "zod";

import type { KrasBrowserChoice } from "./kras-browser-choice.ts";

const execute = promisify(execFile);
const ownerSchema = z
	.object({
		executablePath: z.string().min(1),
		commandLine: z.string().min(1),
		addresses: z.array(z.string()).min(1),
	})
	.nullable();

export class KrasBrowserLaunchError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "KrasBrowserLaunchError";
	}
}

export type KrasCdpOwner = z.infer<typeof ownerSchema>;

export function matchesKrasBrowserOwner(
	choice: KrasBrowserChoice,
	owner: KrasCdpOwner,
): boolean {
	if (owner === null) return false;
	const normalize = (path: string): string => win32.resolve(path).toLowerCase();
	const args = (owner.commandLine.match(/(?:[^\s"]+|"[^"]*")+/g) ?? []).map(
		(argument) => argument.replaceAll('"', ""),
	);
	const profileFlag = args.find((argument) =>
		argument.startsWith("--user-data-dir="),
	);
	const profileIndex = args.indexOf("--user-data-dir");
	const profile =
		profileFlag?.slice("--user-data-dir=".length) ??
		(profileIndex < 0 ? undefined : args[profileIndex + 1]);
	return (
		normalize(owner.executablePath) === normalize(choice.executablePath) &&
		profile !== undefined &&
		normalize(profile) === normalize(choice.profileDirectory) &&
		owner.addresses.every(
			(address) => address === "127.0.0.1" || address === "::1",
		)
	);
}

export async function readKrasCdpOwner(port: number): Promise<KrasCdpOwner> {
	if (!Number.isSafeInteger(port) || port < 1 || port > 65535) {
		throw new KrasBrowserLaunchError("브라우저 연결 포트가 유효하지 않습니다.");
	}
	const script = `
    $ErrorActionPreference='Stop'
    [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new()
    $listeners=@(Get-NetTCPConnection -State Listen | Where-Object { $_.LocalPort -eq ${port} })
    if($listeners.Count -eq 0) { Write-Output 'null'; exit 0 }
    $owners=@($listeners | Select-Object -ExpandProperty OwningProcess -Unique)
    if($owners.Count -ne 1) { throw 'Ambiguous debugging port ownership' }
    $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$owners[0])
    @{executablePath=$process.ExecutablePath;commandLine=$process.CommandLine;addresses=@($listeners.LocalAddress)} | ConvertTo-Json -Compress
  `;
	const { stdout } = await execute(
		"powershell.exe",
		[
			"-NoLogo",
			"-NoProfile",
			"-NonInteractive",
			"-EncodedCommand",
			Buffer.from(script, "utf16le").toString("base64"),
		],
		{ timeout: 8000, windowsHide: true, encoding: "utf8" },
	);
	return ownerSchema.parse(JSON.parse(stdout.trim()));
}

export async function verifyKrasBrowserOwner(
	choice: KrasBrowserChoice,
	endpoint: string,
): Promise<void> {
	const url = new URL(endpoint);
	if (
		url.protocol !== "http:" ||
		!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)
	) {
		throw new KrasBrowserLaunchError(
			"업무용 브라우저는 로컬 디버깅 주소만 사용할 수 있습니다.",
		);
	}
	const owner = await readKrasCdpOwner(Number(url.port || "80"));
	if (!matchesKrasBrowserOwner(choice, owner)) {
		throw new KrasBrowserLaunchError(
			"연결 포트의 실행파일·업무용 프로필이 선택한 브라우저와 다릅니다. 다른 창을 자동 연결하거나 종료하지 않습니다.",
		);
	}
}

export async function ensureKrasChromium(
	choice: KrasBrowserChoice,
	endpoint: string,
): Promise<void> {
	const url = new URL(endpoint);
	const port = Number(url.port || "80");
	if (
		url.protocol !== "http:" ||
		!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)
	) {
		throw new KrasBrowserLaunchError(
			"업무용 브라우저는 로컬 디버깅 주소만 사용할 수 있습니다.",
		);
	}
	const owner = await readKrasCdpOwner(port);
	if (owner !== null) {
		await verifyKrasBrowserOwner(choice, endpoint);
		return;
	}
	const child = spawn(
		choice.executablePath,
		[
			"--remote-debugging-address=127.0.0.1",
			`--remote-debugging-port=${port}`,
			`--user-data-dir=${choice.profileDirectory}`,
			"--disable-popup-blocking",
			"--no-first-run",
			"https://www.kras.go.kr/kras/cert/certView.do",
		],
		{ detached: true, stdio: "ignore" },
	);
	await once(child, "spawn");
	child.unref();
	for (let attempt = 0; attempt < 30; attempt += 1) {
		await setTimeout(500);
		const current = await readKrasCdpOwner(port);
		if (current !== null) {
			await verifyKrasBrowserOwner(choice, endpoint);
			return;
		}
	}
	throw new KrasBrowserLaunchError(
		`${choice.label} 연결을 시작하지 못했습니다. 브라우저 창과 디버깅 포트 상태를 확인하세요.`,
	);
}
