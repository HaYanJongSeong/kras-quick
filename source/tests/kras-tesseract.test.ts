import assert from "node:assert/strict";
import { access, mkdtemp, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it, type TestContext } from "node:test";

import {
	bundledTesseractPath,
	recognizeCaptchaCandidate,
	TesseractBinaryMissingError,
	type TesseractRunner,
	TesseractTimeoutError,
} from "../scripts/kras-tesseract.ts";
import { handleQuickCaptcha } from "../scripts/property-auto-runner.ts";

const PNG_BASE64 = Buffer.from("png fixture").toString("base64");

async function withTempRoot(t: TestContext): Promise<string> {
	const root = await mkdtemp(join(tmpdir(), "kras-tesseract-test-"));
	t.after(() => rm(root, { recursive: true, force: true }));
	return root;
}

describe("recognizeCaptchaCandidate", () => {
	it("uses distinct OCR modes through the AUTO retry path before the third submission succeeds", async (t) => {
		// Given
		const root = await withTempRoot(t);
		const modes: string[] = [];
		const submitted: string[] = [];
		const runner: TesseractRunner = async (_executable, args) => {
			const mode = args[args.indexOf("--psm") + 1] ?? "";
			modes.push(mode);
			return {
				stdout: mode === "7" ? "11111" : mode === "8" ? "22222" : "33333",
				stderr: "",
			};
		};

		// When
		await handleQuickCaptcha({
			mode: { kind: "auto" },
			page: {
				renderCaptchaTerminal: async () => "fixture",
				captureCaptchaImage: async () => PNG_BASE64,
				fillCaptchaAndSubmit: async (candidate) => {
					submitted.push(candidate);
					return candidate === "33333" ? "success" : "error";
				},
			},
			terminal: {
				question: async () => {
					assert.fail(
						"three distinct OCR modes must run before manual fallback",
					);
				},
				close: () => undefined,
			},
			recognize: (image, options) =>
				recognizeCaptchaCandidate(image, {
					...options,
					binaryPath: join(root, "tesseract.exe"),
					tempRoot: root,
					runner,
				}),
		});

		// Then
		assert.deepEqual(modes, ["7", "8", "13"]);
		assert.deepEqual(submitted, ["11111", "22222", "33333"]);
	});

	it("skips repeated AUTO candidates and falls back to manual input after all OCR modes", async (t) => {
		// Given
		const root = await withTempRoot(t);
		const modes: string[] = [];
		const submitted: string[] = [];
		let manualInputs = 0;
		const runner: TesseractRunner = async (_executable, args) => {
			modes.push(args[args.indexOf("--psm") + 1] ?? "");
			return { stdout: "11111", stderr: "" };
		};

		// When
		await handleQuickCaptcha({
			mode: { kind: "auto" },
			page: {
				renderCaptchaTerminal: async () => "fixture",
				captureCaptchaImage: async () => PNG_BASE64,
				fillCaptchaAndSubmit: async (candidate) => {
					submitted.push(candidate);
					return candidate === "54321" ? "success" : "error";
				},
			},
			terminal: {
				question: async () => {
					manualInputs += 1;
					return "54321";
				},
				close: () => undefined,
			},
			recognize: (image, options) =>
				recognizeCaptchaCandidate(image, {
					...options,
					binaryPath: join(root, "tesseract.exe"),
					tempRoot: root,
					runner,
				}),
		});

		// Then
		assert.deepEqual(modes, ["7", "8", "13"]);
		assert.deepEqual(submitted, ["11111", "54321"]);
		assert.equal(manualInputs, 1);
	});

	it("resolves bundled Tesseract beside the portable runtime, not beside node", () => {
		const nodePath = "C:\\runtime\\node\\node.exe";
		assert.equal(
			bundledTesseractPath(nodePath),
			"C:\\runtime\\tesseract\\tesseract.exe",
		);
	});

	it("returns only an exact five-ASCII-digit candidate and invokes process without shell", async (t) => {
		const root = await withTempRoot(t);
		const binaryPath = join(root, "tesseract.exe");
		const runner: TesseractRunner = async (executable, args, options) => {
			assert.equal(executable, binaryPath);
			assert.equal(options.shell, false);
			assert.equal(args[1], "stdout");
			assert.deepEqual(args.slice(2), [
				"-l",
				"eng",
				"--psm",
				"7",
				"--oem",
				"1",
				"-c",
				"tessedit_char_whitelist=0123456789",
				"-c",
				"classify_bln_numeric_mode=1",
			]);
			await access(args[0] ?? "");
			return { stdout: "12345\n", stderr: "" };
		};

		assert.equal(
			await recognizeCaptchaCandidate(PNG_BASE64, { binaryPath, runner }),
			"12345",
		);
	});

	it("rejects candidates that are not exactly five ASCII digits", async (t) => {
		const root = await withTempRoot(t);
		const outputs = ["1234", "123456", "12 345", "１２３４５", "abcde"];
		for (const stdout of outputs) {
			const runner: TesseractRunner = async (_executable, _args) => {
				return { stdout: `${stdout}\n`, stderr: "" };
			};
			assert.equal(
				await recognizeCaptchaCandidate(PNG_BASE64, {
					binaryPath: join(root, "tesseract.exe"),
					runner,
				}),
				null,
			);
		}
	});

	it("accepts an exact five-digit candidate without confidence metadata", async (t) => {
		const root = await withTempRoot(t);
		const runner: TesseractRunner = async (_executable, _args) => {
			return { stdout: "59464\n", stderr: "" };
		};

		assert.equal(
			await recognizeCaptchaCandidate(PNG_BASE64, {
				binaryPath: join(root, "tesseract.exe"),
				runner,
			}),
			"59464",
		);
	});

	it("reports a missing bundled binary before invoking OCR", async (t) => {
		const root = await withTempRoot(t);
		await assert.rejects(
			recognizeCaptchaCandidate(PNG_BASE64, {
				binaryPath: join(root, "missing.exe"),
			}),
			TesseractBinaryMissingError,
		);
	});

	it("times out through the bounded runner contract", async (t) => {
		const root = await withTempRoot(t);
		const runner: TesseractRunner = async (_executable, _args, options) => {
			assert.equal(options.timeoutMs, 25);
			throw new TesseractTimeoutError(options.timeoutMs);
		};
		await assert.rejects(
			recognizeCaptchaCandidate(PNG_BASE64, {
				binaryPath: join(root, "tesseract.exe"),
				timeoutMs: 25,
				runner,
			}),
			TesseractTimeoutError,
		);
	});

	it("removes the temporary PNG after OCR completes", async (t) => {
		const root = await withTempRoot(t);
		let inputPath = "";
		const runner: TesseractRunner = async (_executable, args) => {
			inputPath = args[0] ?? "";
			await access(inputPath);
			return { stdout: "54321", stderr: "" };
		};

		await recognizeCaptchaCandidate(PNG_BASE64, {
			binaryPath: join(root, "tesseract.exe"),
			tempRoot: root,
			runner,
		});

		await assert.rejects(access(inputPath));
		assert.deepEqual(await readdir(root), []);
	});
});
