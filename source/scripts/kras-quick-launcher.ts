import { dirname, join } from "node:path";
import runtimeMeta from "../kras-quick-runtime.meta.json" with { type: "json" };
import {
	ensureRuntime,
	launchQuickTerminal,
	runProcess,
	terminalEntry,
	terminalHandoffEligible,
} from "./kras-quick-runtime.ts";

const QUICK_CDP_PORT = 9333;
const QUICK_CDP_ENDPOINT = `http://127.0.0.1:${QUICK_CDP_PORT}`;
const interactive = Boolean(process.stdin.isTTY && process.stdout.isTTY);

try {
	const entry = terminalEntry(process.argv.slice(2));
	if (terminalHandoffEligible(process.platform, interactive, entry.reentered)) {
		await launchQuickTerminal(process.execPath, entry.args);
	} else {
		const exeDirectory = dirname(process.execPath);
		process.stdout.write(`kras-quick v${runtimeMeta.version}\n`);
		process.stdout.write(`PDF 저장 경로: ${join(exeDirectory, "KRAS")}\n`);
		const runtimeZipPath = join(
			exeDirectory,
			`kras-quick-runtime-v${runtimeMeta.version}.zip`,
		);
		const runtimeEnv = {
			...process.env,
			KRAS_CDP_ENDPOINT: QUICK_CDP_ENDPOINT,
			KRAS_QUICK_ROOT: exeDirectory,
			KRAS_QUICK_RUNTIME_PATH: runtimeZipPath,
		};
		const root = await ensureRuntime(runtimeMeta, runtimeEnv);
		const nodePath = join(root, "node", "node.exe");
		const scriptPath = join(root, "scripts", "property-auto-runner.ts");
		const exitCode = await runProcess(
			nodePath,
			[scriptPath, "quick", ...entry.args],
			root,
			runtimeEnv,
		);
		process.exitCode = exitCode;
		if (exitCode !== 0 && interactive && process.platform === "win32")
			await runProcess("cmd.exe", ["/c", "pause"]);
	}
} catch (error: unknown) {
	process.stderr.write(
		`kras_quick 실행 실패: ${error instanceof Error ? error.message : String(error)}\n`,
	);
	if (interactive && process.platform === "win32")
		await runProcess("cmd.exe", ["/c", "pause"]);
	process.exitCode = 1;
}
