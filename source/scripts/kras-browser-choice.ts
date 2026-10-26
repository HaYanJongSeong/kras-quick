import { createHash } from "node:crypto";
import { existsSync, statSync } from "node:fs";
import { basename, extname, isAbsolute, join, resolve } from "node:path";

import type { TerminalReader } from "./terminal-reader.ts";

export type KrasBrowserChoice = {
	readonly engine: "chromium" | "firefox";
	readonly label: string;
	readonly executablePath: string;
	readonly profileDirectory: string;
};

const knownBrowsers: readonly {
	readonly engine: KrasBrowserChoice["engine"];
	readonly label: string;
	readonly displayName: string;
	readonly profile: string;
	readonly programPath: readonly string[];
	readonly localPath: readonly string[];
	readonly roamingPath?: readonly string[];
}[] = [
	{
		engine: "chromium",
		label: "Chrome",
		displayName: "크롬",
		profile: ".kras-chrome-profile",
		programPath: ["Google", "Chrome", "Application", "chrome.exe"],
		localPath: ["Google", "Chrome", "Application", "chrome.exe"],
	},
	{
		engine: "chromium",
		label: "Edge",
		displayName: "엣지",
		profile: ".kras-edge-profile",
		programPath: ["Microsoft", "Edge", "Application", "msedge.exe"],
		localPath: ["Microsoft", "Edge", "Application", "msedge.exe"],
	},
	{
		engine: "chromium",
		label: "Brave",
		displayName: "브레이브 브라우저",
		profile: ".kras-brave-profile",
		programPath: ["BraveSoftware", "Brave-Browser", "Application", "brave.exe"],
		localPath: ["BraveSoftware", "Brave-Browser", "Application", "brave.exe"],
	},
	{
		engine: "firefox",
		label: "Firefox (시험 지원)",
		displayName: "파이어폭스 (시험 지원)",
		profile: ".kras-firefox-profile",
		programPath: ["Mozilla Firefox", "firefox.exe"],
		localPath: ["Programs", "Mozilla Firefox", "firefox.exe"],
	},
	{
		engine: "chromium",
		label: "Vivaldi",
		displayName: "비발디",
		profile: ".kras-vivaldi-profile",
		programPath: ["Vivaldi", "Application", "vivaldi.exe"],
		localPath: ["Vivaldi", "Application", "vivaldi.exe"],
	},
	{
		engine: "chromium",
		label: "Opera",
		displayName: "오페라",
		profile: ".kras-opera-profile",
		programPath: ["Opera", "opera.exe"],
		localPath: ["Programs", "Opera", "opera.exe"],
		roamingPath: ["Opera Software", "Opera Stable", "opera.exe"],
	},
	{
		engine: "chromium",
		label: "Whale",
		displayName: "웨일",
		profile: ".kras-whale-profile",
		programPath: ["Naver", "Naver Whale", "Application", "whale.exe"],
		localPath: ["Naver", "Naver Whale", "Application", "whale.exe"],
	},
];

export function discoverKrasBrowsers(
	rootDirectory: string,
	environment: NodeJS.ProcessEnv = process.env,
): readonly KrasBrowserChoice[] {
	const candidates: KrasBrowserChoice[] = [];
	const seen = new Set<string>();
	for (const browser of knownBrowsers) {
		const locations: readonly (readonly [
			string | undefined,
			readonly string[] | undefined,
		])[] = [
			[environment["ProgramW6432"], browser.programPath],
			[environment["ProgramFiles"], browser.programPath],
			[environment["ProgramFiles(x86)"], browser.programPath],
			[environment["LOCALAPPDATA"], browser.localPath],
			[environment["APPDATA"], browser.roamingPath],
		];
		for (const [directory, parts] of locations) {
			if (!directory || !isAbsolute(directory) || parts === undefined) continue;
			const executablePath = join(directory, ...parts);
			const key = executablePath.toLowerCase();
			if (seen.has(key) || !existsSync(executablePath)) continue;
			seen.add(key);
			candidates.push({
				engine: browser.engine,
				label: browser.label,
				executablePath,
				profileDirectory: join(rootDirectory, browser.profile),
			});
		}
	}
	return candidates;
}

export async function chooseKrasBrowser(
	terminal: Pick<TerminalReader, "question">,
	candidates: readonly KrasBrowserChoice[],
	rootDirectory: string,
): Promise<KrasBrowserChoice | undefined> {
	const matches = (
		known: (typeof knownBrowsers)[number],
		choice: KrasBrowserChoice,
	): boolean =>
		known.label === choice.label ||
		(known.engine === "firefox" && choice.engine === "firefox");
	const options = [
		...candidates.map((choice) => ({
			label:
				knownBrowsers.find((known) => matches(known, choice))?.displayName ??
				choice.label,
			choice,
		})),
		...knownBrowsers
			.filter((known) => !candidates.some((choice) => matches(known, choice)))
			.map((known) => ({ label: known.displayName, choice: undefined })),
	];
	process.stdout.write("\n사용하실 브라우저가 무엇인가요?\n");
	options.forEach((option, index) => {
		process.stdout.write(
			`${index + 1}. ${option.label} (${option.choice === undefined ? "설치 안 됨" : "설치됨"})\n`,
		);
	});
	const manualNumber = options.length + 1;
	process.stdout.write(
		"기본 설치 경로 기준입니다. 다른 위치는 실행 파일을 직접 지정하세요.\n",
	);
	process.stdout.write(
		`${manualNumber}. Chromium 실행 파일 직접 지정\n0. 취소 (Enter도 취소)\n`,
	);
	while (true) {
		const answer = (await terminal.question("브라우저 번호 > ")).trim();
		if (answer === "" || answer === "0") return undefined;
		const number = /^\d+$/.test(answer) ? Number(answer) : -1;
		const option = options[number - 1];
		if (option !== undefined) {
			if (option.choice !== undefined) return option.choice;
			process.stdout.write(
				"설치되어 있지 않습니다. 설치된 브라우저나 실행 파일 직접 지정을 선택하세요.\n",
			);
			continue;
		}
		if (number !== manualNumber) {
			process.stdout.write(`0부터 ${manualNumber}까지의 번호를 입력하세요.\n`);
			continue;
		}
		const rawPath = (
			await terminal.question(
				"Chromium 실행 파일 절대 경로 (.exe, Enter: 취소) > ",
			)
		).trim();
		const inputPath =
			(rawPath.startsWith('"') && rawPath.endsWith('"')) ||
			(rawPath.startsWith("'") && rawPath.endsWith("'"))
				? rawPath.slice(1, -1).trim()
				: rawPath;
		if (inputPath === "") return undefined;
		if (!isAbsolute(inputPath) || extname(inputPath).toLowerCase() !== ".exe") {
			process.stdout.write(
				`기존 .exe 파일의 절대 경로가 필요합니다: ${inputPath}\n`,
			);
			continue;
		}
		const executablePath = resolve(inputPath);
		if (basename(executablePath).toLowerCase() === "firefox.exe") {
			process.stdout.write(
				`Firefox는 기본 설치 경로에 설치하고 메뉴의 Firefox 시험 지원 항목을 선택하세요: ${executablePath}\n`,
			);
			continue;
		}
		try {
			if (!statSync(executablePath).isFile()) {
				process.stdout.write(`실행 파일이 아닙니다: ${executablePath}\n`);
				continue;
			}
		} catch (error: unknown) {
			if (!(error instanceof Error)) throw error;
			process.stdout.write(
				`실행 파일 확인 실패: ${executablePath}\n${error.message}\n`,
			);
			continue;
		}
		const hash = createHash("sha256")
			.update(executablePath.toLowerCase())
			.digest("hex")
			.slice(0, 12);
		return {
			engine: "chromium",
			label: "Chromium (직접 지정)",
			executablePath,
			profileDirectory: join(rootDirectory, `.kras-chromium-${hash}-profile`),
		};
	}
}
