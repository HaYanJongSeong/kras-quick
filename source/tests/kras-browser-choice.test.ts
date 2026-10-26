import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { describe, it, type TestContext } from "node:test";

import {
	chooseKrasBrowser,
	discoverKrasBrowsers,
	type KrasBrowserChoice,
} from "../scripts/kras-browser-choice.ts";
import type { TerminalReader } from "../scripts/terminal-reader.ts";

async function fixture(t: TestContext): Promise<string> {
	const directory = await mkdtemp(join(tmpdir(), "kras-browser-choice-test-"));
	t.after(() => rm(directory, { recursive: true, force: true }));
	return directory;
}

async function executable(
	root: string,
	parts: readonly string[],
): Promise<string> {
	const path = join(root, ...parts);
	await mkdir(dirname(path), { recursive: true });
	await writeFile(path, "fake browser binary\n");
	return path;
}

function reader(
	t: TestContext,
	answers: readonly string[],
): {
	readonly question: TerminalReader["question"];
	readonly prompts: readonly string[];
	readonly output: readonly string[];
} {
	const prompts: string[] = [];
	const output: string[] = [];
	t.mock.method(process.stdout, "write", (chunk: string | Uint8Array) => {
		output.push(chunk.toString());
		return true;
	});
	return {
		prompts,
		output,
		question: async (prompt) => {
			const answer = answers[prompts.length];
			prompts.push(prompt);
			assert.ok(answer !== undefined, "unexpected additional question");
			return answer;
		},
	};
}

const chrome: KrasBrowserChoice = {
	engine: "chromium",
	label: "Chrome",
	executablePath: "unused-chrome.exe",
	profileDirectory: ".kras-chrome-profile",
};
const edge: KrasBrowserChoice = {
	engine: "chromium",
	label: "Edge",
	executablePath: "unused-edge.exe",
	profileDirectory: ".kras-edge-profile",
};

describe("discoverKrasBrowsers", () => {
	it("orders installed variants and deduplicates paths case-insensitively", async (t) => {
		const directory = await fixture(t);
		const environment = {
			ProgramW6432: join(directory, "Program Files"),
			ProgramFiles: join(directory, "PROGRAM FILES"),
			"ProgramFiles(x86)": join(directory, "Program Files (x86)"),
			LOCALAPPDATA: join(directory, "local"),
			APPDATA: join(directory, "roaming"),
		};
		const chromeParts = ["Google", "Chrome", "Application", "chrome.exe"];
		const first = await executable(environment.ProgramW6432, chromeParts);
		await executable(environment.ProgramFiles, chromeParts);
		const second = await executable(
			environment["ProgramFiles(x86)"],
			chromeParts,
		);
		const edgePath = await executable(environment.ProgramW6432, [
			"Microsoft",
			"Edge",
			"Application",
			"msedge.exe",
		]);
		const bravePath = await executable(environment.LOCALAPPDATA, [
			"BraveSoftware",
			"Brave-Browser",
			"Application",
			"brave.exe",
		]);
		const firefoxPath = await executable(environment.ProgramW6432, [
			"Mozilla Firefox",
			"firefox.exe",
		]);
		const vivaldiPath = await executable(environment.LOCALAPPDATA, [
			"Vivaldi",
			"Application",
			"vivaldi.exe",
		]);
		const operaPath = await executable(environment.LOCALAPPDATA, [
			"Programs",
			"Opera",
			"opera.exe",
		]);
		const whalePath = await executable(environment.ProgramW6432, [
			"Naver",
			"Naver Whale",
			"Application",
			"whale.exe",
		]);
		const candidates = discoverKrasBrowsers(directory, environment);

		assert.deepEqual(
			candidates.map((candidate) => candidate.executablePath),
			[
				first,
				second,
				edgePath,
				bravePath,
				firefoxPath,
				vivaldiPath,
				operaPath,
				whalePath,
			],
		);
		assert.deepEqual(
			candidates.map((candidate) => candidate.engine),
			[
				"chromium",
				"chromium",
				"chromium",
				"chromium",
				"firefox",
				"chromium",
				"chromium",
				"chromium",
			],
		);
		assert.deepEqual(
			candidates.map((candidate) => candidate.profileDirectory),
			[
				"chrome",
				"chrome",
				"edge",
				"brave",
				"firefox",
				"vivaldi",
				"opera",
				"whale",
			].map((name) => join(directory, `.kras-${name}-profile`)),
		);
	});

	it("finds Firefox per-user and Opera roaming installations", async (t) => {
		const directory = await fixture(t);
		const environment = {
			LOCALAPPDATA: join(directory, "local"),
			APPDATA: join(directory, "roaming"),
		};
		const firefoxPath = await executable(environment.LOCALAPPDATA, [
			"Programs",
			"Mozilla Firefox",
			"firefox.exe",
		]);
		const operaPath = await executable(environment.APPDATA, [
			"Opera Software",
			"Opera Stable",
			"opera.exe",
		]);

		assert.deepEqual(
			discoverKrasBrowsers(directory, environment).map(
				(candidate) => candidate.executablePath,
			),
			[firefoxPath, operaPath],
		);
	});

	it("ignores absent or empty environment roots instead of looking in the working directory", async (t) => {
		const directory = await fixture(t);
		await executable(directory, [
			"Google",
			"Chrome",
			"Application",
			"chrome.exe",
		]);
		const previousDirectory = process.cwd();
		process.chdir(directory);
		try {
			assert.deepEqual(discoverKrasBrowsers(directory, {}), []);
			assert.deepEqual(
				discoverKrasBrowsers(directory, {
					ProgramW6432: "",
					ProgramFiles: "",
					"ProgramFiles(x86)": "",
					LOCALAPPDATA: "",
					APPDATA: "",
				}),
				[],
			);
		} finally {
			process.chdir(previousDirectory);
		}
	});
});

describe("chooseKrasBrowser", () => {
	it("asks even when exactly one browser is installed", async (t) => {
		const terminal = reader(t, ["1"]);
		assert.equal(await chooseKrasBrowser(terminal, [chrome], "."), chrome);
		assert.equal(terminal.prompts.length, 1);
	});

	it("retries invalid numbers without implicitly choosing Chrome", async (t) => {
		const terminal = reader(t, ["bad", "9", "1.5", "1x", "-1", "2"]);
		assert.equal(await chooseKrasBrowser(terminal, [chrome, edge], "."), edge);
		assert.equal(terminal.prompts.length, 6);
	});

	it("preserves an explicitly selected Firefox engine when Chrome is absent", async (t) => {
		const firefox: KrasBrowserChoice = {
			engine: "firefox",
			label: "Firefox",
			executablePath: "unused-firefox.exe",
			profileDirectory: ".kras-firefox-profile",
		};
		const terminal = reader(t, ["2"]);
		assert.equal(
			await chooseKrasBrowser(terminal, [edge, firefox], "."),
			firefox,
		);
		assert.equal(terminal.prompts.length, 1);
	});

	it("rejects an uninstalled entry and keeps the installed choice available", async (t) => {
		const terminal = reader(t, ["2", "1"]);
		assert.equal(await chooseKrasBrowser(terminal, [edge], "."), edge);
		assert.equal(terminal.prompts.length, 2);
	});

	for (const answer of ["", " \t ", "0"]) {
		it(`cancels on ${JSON.stringify(answer)} rather than selecting the only browser`, async (t) => {
			const terminal = reader(t, [answer]);
			assert.equal(await chooseKrasBrowser(terminal, [chrome], "."), undefined);
			assert.equal(terminal.prompts.length, 1);
		});
	}

	it("offers manual Chromium without inventing Chrome when discovery is empty", async (t) => {
		const terminal = reader(t, ["0"]);
		assert.equal(await chooseKrasBrowser(terminal, [], "."), undefined);
		assert.equal(terminal.prompts.length, 1);
	});

	it("cancels when the manual path is empty, including the terminal EOF representation", async (t) => {
		const terminal = reader(t, ["8", ""]);
		assert.equal(await chooseKrasBrowser(terminal, [], "."), undefined);
		assert.equal(terminal.prompts.length, 2);
	});

	for (const quote of ['"', "'"]) {
		it(`accepts an absolute Chromium executable enclosed in ${quote}`, async (t) => {
			const directory = await fixture(t);
			const path = await executable(directory, [
				"Portable Chromium",
				"browser.EXE",
			]);
			const terminal = reader(t, ["8", `  ${quote}${path}${quote}  `]);
			const selected = await chooseKrasBrowser(terminal, [], directory);
			const hash = createHash("sha256")
				.update(path.toLowerCase())
				.digest("hex")
				.slice(0, 12);

			assert.equal(selected?.engine, "chromium");
			assert.equal(selected?.executablePath, path);
			assert.equal(
				selected?.profileDirectory,
				join(directory, `.kras-chromium-${hash}-profile`),
			);
		});
	}

	it("reports invalid custom attempts and accepts a subsequent valid executable", async (t) => {
		const directory = await fixture(t);
		const text = await executable(directory, ["browser.txt"]);
		const firefox = await executable(directory, ["FIREFOX.EXE"]);
		const valid = await executable(directory, ["browser.exe"]);
		const missing = join(directory, "missing.exe");
		const folder = join(directory, "folder.exe");
		await mkdir(folder);
		const terminal = reader(t, [
			"8",
			"relative.exe",
			"8",
			text,
			"8",
			folder,
			"8",
			missing,
			"8",
			firefox,
			"8",
			valid,
		]);
		const selected = await chooseKrasBrowser(terminal, [], directory);

		assert.equal(selected?.executablePath, valid);
		assert.equal(terminal.prompts.length, 12);
		for (const rejected of ["relative.exe", text, folder, missing, firefox]) {
			assert.ok(terminal.output.some((line) => line.includes(rejected)));
		}
	});

	it("isolates custom executable profiles from each other and the installed Chrome profile", async (t) => {
		const directory = await fixture(t);
		const firstPath = await executable(directory, [
			"Google",
			"Chrome",
			"Application",
			"chrome.exe",
		]);
		const secondPath = await executable(directory, ["portable", "chrome.exe"]);
		const installed = discoverKrasBrowsers(directory, {
			ProgramFiles: directory,
		});
		const terminal = reader(t, ["8", firstPath, "8", secondPath]);
		const first = await chooseKrasBrowser(terminal, installed, directory);
		const second = await chooseKrasBrowser(terminal, installed, directory);

		assert.ok(first !== undefined && second !== undefined);
		assert.notEqual(first.profileDirectory, second.profileDirectory);
		assert.notEqual(
			first.profileDirectory,
			join(directory, ".kras-chrome-profile"),
		);
		assert.notEqual(
			second.profileDirectory,
			join(directory, ".kras-chrome-profile"),
		);
	});
});
