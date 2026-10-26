import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
	mkdir,
	mkdtemp,
	readdir,
	readFile,
	rm,
	writeFile,
} from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it, type TestContext } from "node:test";

import {
	ensureRuntime,
	launchQuickTerminal,
	runtimeZipName,
	TERMINAL_REENTRY_FLAG,
	terminalBootstrap,
	terminalEntry,
	terminalHandoffEligible,
} from "../scripts/kras-quick-runtime.ts";

const VERSION = "1.0.3";
const ZIP_NAME = runtimeZipName(VERSION);
const EXTRACT_DIR = `kras-quick-runtime-v${VERSION}`;
const NODE_EXE = "fake node binary\n";
const TERMINAL_ARGS: readonly string[] = [
	"한글",
	"",
	"a b",
	'"quoted"',
	"C:\\끝\\",
	'a\\"b',
	";%PATH%&|<>^`$()'",
	TERMINAL_REENTRY_FLAG,
];
type TerminalProbeCase = readonly [
	"path" | "standard" | "inbox",
	number | string,
	readonly boolean[],
	number,
	number,
	boolean,
];

function sha256Of(data: Buffer): string {
	return createHash("sha256").update(data).digest("hex");
}

function makeEnv(
	cacheDir: string,
	overrides?: Record<string, string>,
): NodeJS.ProcessEnv {
	return { ...overrides, KRAS_QUICK_CACHE_DIR: cacheDir };
}

async function makeRuntimeZip(dir: string): Promise<Buffer> {
	const staging = join(dir, "staging");
	await mkdir(join(staging, "node"), { recursive: true });
	await writeFile(join(staging, "node", "node.exe"), NODE_EXE);
	await mkdir(join(staging, "scripts"), { recursive: true });
	await writeFile(
		join(staging, "scripts", "property-auto-runner.ts"),
		'console.log("quick")\n',
	);
	await mkdir(join(staging, "tesseract"), { recursive: true });
	await writeFile(
		join(staging, "tesseract", "tesseract.exe"),
		"fake tesseract binary\n",
	);
	const zipPath = join(dir, "fixture.zip");
	const escaped = staging.replaceAll("'", "''");
	const result = spawnSync(
		"powershell.exe",
		[
			"-NoProfile",
			"-NonInteractive",
			"-Command",
			`Compress-Archive -Path '${escaped}\\*' -DestinationPath '${zipPath}'`,
		],
		{ encoding: "utf8" },
	);
	assert.equal(result.status, 0, result.stderr);
	return await readFile(zipPath);
}

async function serveZip(
	bytes: Buffer,
): Promise<{ url: string; hits: () => number; close: () => Promise<void> }> {
	let hitCount = 0;
	const server = createServer((_request, response) => {
		hitCount += 1;
		response.writeHead(200, { "content-type": "application/zip" });
		response.end(bytes);
	});
	await new Promise<void>((resolve, reject) => {
		server.once("error", reject);
		server.listen(0, "127.0.0.1", resolve);
	});
	const address = server.address();
	if (address === null || typeof address === "string")
		throw new Error("test server has no TCP port");
	return {
		url: `http://127.0.0.1:${address.port}/kras-quick-runtime.zip`,
		hits: () => hitCount,
		close: () =>
			new Promise<void>((resolve, reject) =>
				server.close((error) => (error ? reject(error) : resolve())),
			),
	};
}

describe("ensureRuntime", () => {
	it("reuses verified cached zip and extracted dir without network", async (t: TestContext) => {
		const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"));
		t.after(() => rm(dir, { recursive: true, force: true }));
		const zip = await makeRuntimeZip(dir);
		const cacheDir = join(dir, "cache");
		await mkdir(join(cacheDir, EXTRACT_DIR, "node"), { recursive: true });
		await writeFile(join(cacheDir, EXTRACT_DIR, "node", "node.exe"), NODE_EXE);
		await mkdir(join(cacheDir, EXTRACT_DIR, "scripts"), { recursive: true });
		await writeFile(
			join(cacheDir, EXTRACT_DIR, "scripts", "property-auto-runner.ts"),
			'console.log("quick")\n',
		);
		await mkdir(join(cacheDir, EXTRACT_DIR, "tesseract"), { recursive: true });
		await writeFile(
			join(cacheDir, EXTRACT_DIR, "tesseract", "tesseract.exe"),
			"fake tesseract binary\n",
		);
		await writeFile(
			join(cacheDir, EXTRACT_DIR, ".runtime-sha256"),
			sha256Of(zip),
		);
		await writeFile(join(cacheDir, ZIP_NAME), zip);
		const meta = {
			version: VERSION,
			url: "http://127.0.0.1:1/unreachable",
			sha256: sha256Of(zip).toUpperCase(),
		};
		const result = await ensureRuntime(meta, makeEnv(cacheDir));
		assert.equal(result, join(cacheDir, EXTRACT_DIR));
		assert.equal(
			await readFile(join(result, "node", "node.exe"), "utf8"),
			NODE_EXE,
		);
	});

	it("downloads via env URL seam, verifies, and extracts into cache dir", async (t: TestContext) => {
		const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"));
		t.after(() => rm(dir, { recursive: true, force: true }));
		const zip = await makeRuntimeZip(dir);
		const server = await serveZip(zip);
		t.after(() => server.close());
		const cacheDir = join(dir, "cache");
		const meta = {
			version: VERSION,
			url: "http://127.0.0.1:1/wrong-url",
			sha256: sha256Of(zip).toUpperCase(),
		};
		const result = await ensureRuntime(
			meta,
			makeEnv(cacheDir, { KRAS_QUICK_RUNTIME_URL: server.url }),
		);
		assert.equal(server.hits(), 1);
		assert.equal(result, join(cacheDir, EXTRACT_DIR));
		assert.equal(
			await readFile(join(result, "node", "node.exe"), "utf8"),
			NODE_EXE,
		);
		assert.equal(
			sha256Of(await readFile(join(cacheDir, ZIP_NAME))),
			sha256Of(zip),
		);
		assert.deepEqual(
			(await readdir(cacheDir)).filter((name) => name.endsWith(".tmp")),
			[],
		);
	});

	it("uses a verified local runtime ZIP before attempting network download", async (t: TestContext) => {
		const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"));
		t.after(() => rm(dir, { recursive: true, force: true }));
		const zip = await makeRuntimeZip(dir);
		const localZipPath = join(dir, "kras-quick-runtime.zip");
		await writeFile(localZipPath, zip);
		const cacheDir = join(dir, "cache");
		const meta = {
			version: VERSION,
			url: "http://127.0.0.1:1/unreachable",
			sha256: sha256Of(zip),
		};
		const result = await ensureRuntime(
			meta,
			makeEnv(cacheDir, { KRAS_QUICK_RUNTIME_PATH: localZipPath }),
		);
		assert.equal(result, join(cacheDir, EXTRACT_DIR));
		assert.equal(
			await readFile(join(result, "node", "node.exe"), "utf8"),
			NODE_EXE,
		);
	});

	it("rejects on SHA-256 mismatch via env hash seam and leaves nothing cached", async (t: TestContext) => {
		const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"));
		t.after(() => rm(dir, { recursive: true, force: true }));
		const zip = await makeRuntimeZip(dir);
		const server = await serveZip(zip);
		t.after(() => server.close());
		const cacheDir = join(dir, "cache");
		const meta = { version: VERSION, url: server.url, sha256: sha256Of(zip) };
		await assert.rejects(
			ensureRuntime(
				meta,
				makeEnv(cacheDir, { KRAS_QUICK_RUNTIME_SHA256: "0".repeat(64) }),
			),
			/SHA-256/,
		);
		assert.equal(server.hits(), 1);
		assert.deepEqual(await readdir(cacheDir), []);
	});

	it("re-downloads when the cached zip hash does not match", async (t: TestContext) => {
		const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"));
		t.after(() => rm(dir, { recursive: true, force: true }));
		const zip = await makeRuntimeZip(dir);
		const server = await serveZip(zip);
		t.after(() => server.close());
		const cacheDir = join(dir, "cache");
		await mkdir(cacheDir, { recursive: true });
		await writeFile(join(cacheDir, ZIP_NAME), Buffer.from("stale bytes"));
		const meta = { version: VERSION, url: server.url, sha256: sha256Of(zip) };
		const result = await ensureRuntime(meta, makeEnv(cacheDir));
		assert.equal(server.hits(), 1);
		assert.equal(
			sha256Of(await readFile(join(cacheDir, ZIP_NAME))),
			sha256Of(zip),
		);
		assert.equal(
			await readFile(join(result, "node", "node.exe"), "utf8"),
			NODE_EXE,
		);
	});
});

function probeTerminal(
	shell: string,
	wt: number | string,
	cwd: string,
): unknown {
	const bootstrap = Buffer.from(
		terminalBootstrap("Capture-'Reentry; %", TERMINAL_ARGS, cwd),
		"base64",
	).toString("utf16le");
	const code = `[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$script:calls=@(); $script:waits=0; $script:app=0; $script:entry=$null; $script:wt=${JSON.stringify(String(wt))}
$script:inbox=Join-Path $env:SystemRoot 'System32\\WindowsPowerShell\\v1.0\\powershell.exe'; $env:ProgramFiles=$env:SystemRoot
function Get-Command { param($Name,[ValidateSet('Application')]$CommandType,$ErrorAction)
  if ($Name -eq 'pwsh.exe' -and '${shell}' -eq 'path') { [pscustomobject]@{Path=(Join-Path $env:SystemRoot 'on-path\\pwsh.exe')} }
  if ($Name -eq 'wt.exe' -and $script:wt -ne 'missing') { [pscustomobject]@{Path='wt.exe'} }
}
function Test-Path { param($LiteralPath,[ValidateSet('Leaf')]$PathType); return '${shell}' -ne 'inbox' }
function Start-Process { param($FilePath,$ArgumentList,[switch]$PassThru,[switch]$Wait,[switch]$NoNewWindow,$ErrorAction)
  $target=if ($PassThru) { ($ArgumentList -split '"')[1] } else { $FilePath }
  $script:calls += [pscustomobject]@{shell=$target;wt=$PassThru.IsPresent;wait=$Wait.IsPresent;same=$NoNewWindow.IsPresent;interactive=($ArgumentList -notmatch '-NonInteractive');prefix=$(if ($PassThru) { ($ArgumentList -split '"')[0] } else { '' })}
  if ($script:wt -eq 'both' -or ($PassThru -and $script:wt -eq 'throw')) { throw 'creation failed' }
  if (!$PassThru) { return [pscustomobject]@{ExitCode=17} }
  if ($script:wt -eq '0') {
    $childCode=[Text.Encoding]::Unicode.GetString([Convert]::FromBase64String(($ArgumentList -split ' ')[-1]))
    $capture=@'
Set-Item -LiteralPath 'Function:\\Capture-''Reentry; %' -Value { [Console]::Out.WriteLine((@{args=@($args);cwd=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes((Get-Location).Path))} | ConvertTo-Json -Compress)); $global:LASTEXITCODE=17 };
'@
    $script:entry= & $script:inbox -NoLogo -NoProfile -NonInteractive -EncodedCommand ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($capture+$childCode))) | ConvertFrom-Json
    $script:app=$LASTEXITCODE
  }
  $client=[pscustomobject]@{ExitCode=$script:wt;Handle=0}
  $client | Add-Member ScriptMethod WaitForExit { $script:waits++ }
  $client | Add-Member ScriptMethod Dispose { }
  return $client
}
$failed=$false; try { & { ${bootstrap} } } catch { $failed=$true }
[pscustomobject]@{calls=@($script:calls);waits=$script:waits;app=$script:app;entry=$script:entry;failed=$failed} | ConvertTo-Json -Depth 5 -Compress`;
	const result = spawnSync(
		join(
			process.env["SystemRoot"] ?? "",
			"System32",
			"WindowsPowerShell",
			"v1.0",
			"powershell.exe",
		),
		[
			"-NoLogo",
			"-NoProfile",
			"-NonInteractive",
			"-EncodedCommand",
			Buffer.from(code, "utf16le").toString("base64"),
		],
		{ encoding: "utf8", windowsHide: true },
	);
	assert.equal(result.status, 0, result.stderr);
	const parsed: unknown = JSON.parse(result.stdout);
	return parsed;
}

describe("quick terminal", () => {
	it("restores exact arguments only at the internal marker boundary", () => {
		const entry = terminalEntry([
			TERMINAL_REENTRY_FLAG,
			Buffer.from(JSON.stringify(TERMINAL_ARGS), "utf8").toString("base64"),
		]);
		assert.deepEqual(entry, { args: TERMINAL_ARGS, reentered: true });
		assert.deepEqual(terminalEntry([TERMINAL_REENTRY_FLAG, "W10="]), {
			args: [],
			reentered: true,
		});
		assert.throws(
			() => terminalEntry([TERMINAL_REENTRY_FLAG, "eA=="]),
			SyntaxError,
		);
		assert.deepEqual(terminalEntry(["normal", TERMINAL_REENTRY_FLAG]), {
			args: ["normal", TERMINAL_REENTRY_FLAG],
			reentered: false,
		});
		for (const invalid of [[], ["%%%"], ["e30="], ["WzFd"], ["W10=", "extra"]])
			assert.throws(
				() => terminalEntry([TERMINAL_REENTRY_FLAG, ...invalid]),
				TypeError,
			);
	});
	it("hands off interactive Windows once even with an inherited WT_SESSION", (t) => {
		const original = process.env["WT_SESSION"];
		process.env["WT_SESSION"] = "inherited-session";
		t.after(() => {
			if (original === undefined) delete process.env["WT_SESSION"];
			else process.env["WT_SESSION"] = original;
		});
		for (const [platform, interactive, reentered, expected] of [
			["win32", true, false, true],
			["win32", false, false, false],
			["linux", true, false, false],
			["win32", true, true, false],
			["win32", false, true, false],
		] satisfies readonly (readonly [
			NodeJS.Platform,
			boolean,
			boolean,
			boolean,
		])[])
			assert.equal(
				terminalHandoffEligible(platform, interactive, reentered),
				expected,
			);
	});
	for (const [shell, wt, routes, waits, app, failed] of [
		["path", 0, [true], 1, 17, false],
		["standard", 0, [true], 1, 17, false],
		["inbox", 0, [true], 1, 17, false],
		["path", "missing", [false], 0, 0, false],
		["inbox", "missing", [false], 0, 0, false],
		["path", "throw", [true, false], 0, 0, false],
		["path", 3, [true, false], 1, 0, false],
		["path", "both", [true, false], 0, 0, true],
	] satisfies readonly TerminalProbeCase[])
		it(`routes ${shell} PowerShell when WT is ${wt}`, {
			skip: process.platform !== "win32",
		}, async (t) => {
			const cwd = await mkdtemp(join(tmpdir(), "한글 ;%' quick-"));
			t.after(() => rm(cwd, { recursive: true, force: true }));
			const root = process.env["SystemRoot"] ?? "";
			const shells = {
				path: join(root, "on-path", "pwsh.exe"),
				standard: join(root, "PowerShell", "7", "pwsh.exe"),
				inbox: join(
					root,
					"System32",
					"WindowsPowerShell",
					"v1.0",
					"powershell.exe",
				),
			};
			const calls = routes.map((wt) => ({
				shell: shells[shell],
				wt,
				wait: false,
				same: false,
				interactive: true,
				prefix: wt ? "-w 0 new-tab " : "",
			}));
			const entry =
				app === 17
					? {
							args: [
								TERMINAL_REENTRY_FLAG,
								Buffer.from(JSON.stringify(TERMINAL_ARGS), "utf8").toString(
									"base64",
								),
							],
							cwd: Buffer.from(cwd, "utf8").toString("base64"),
						}
					: null;
			assert.deepEqual(probeTerminal(shell, wt, cwd), {
				calls,
				waits,
				app,
				entry,
				failed,
			});
		});
	it("rejects excessive payloads before creating any host process", {
		skip: process.platform !== "win32",
	}, async () => {
		await assert.rejects(
			launchQuickTerminal("portable.exe", ["가".repeat(32_767)]),
			RangeError,
		);
	});
});
