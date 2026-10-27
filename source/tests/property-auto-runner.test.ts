import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { describe, it } from "node:test";
import type { KrasAutoStageResult } from "../scripts/kras-auto-stages.ts";
import {
	createKrasManualStage2Result,
	propertyAutoStagePaths,
	runKrasAutoStage3,
} from "../scripts/kras-auto-stages.ts";
import { withOzTimeout } from "../scripts/kras-oz.ts";
import {
	askAutoBatchScope,
	createAddressReturningTerminal,
	finishQuickOzWorkflow,
	formatAutoBatchProgress,
	handleQuickCaptcha,
	KrasAutoLoginExpiredError,
	KrasAutoLookupNeededError,
	nextPreviousAddress,
	type PropertyAutoDependencies,
	type PropertyAutoStage1Dependencies,
	type PropertyAutoStage2Dependencies,
	parseAutoBatchScopeAnswer,
	parseQuickAddressInput,
	pickNextAutoTarget,
	QUICK_ADDRESS_PROMPT,
	ReturnToAddressError,
	runPropertyAutoPipeline,
	runPropertyAutoStage1,
	runPropertyAutoStage2,
	runPropertyAutoStage3,
	selectBuildingIndex,
	splitQuickAddressBatch,
	withKrasWorkflowPage,
	writeManualStage2ForRoot,
} from "../scripts/property-auto-runner.ts";
import type { PropertyWatcherRuntimeBrowser } from "../scripts/property-watcher-runtime.ts";
import { TEST_PDF, TEST_PNG, TEST_XML } from "./kras-capture-fixture.ts";

function dependencies(
	calls: string[],
	cliExitCode = 0,
): PropertyAutoDependencies {
	return {
		runCli: async (args) => {
			calls.push(`cli:${args.join(" ")}`);
			return cliExitCode;
		},
		runWatcher: async (command) => {
			calls.push(`watcher:${command}`);
		},
	};
}

function driveQuick(inputs: readonly string[], pauseAfter = 0): unknown {
	const runnerUrl = new URL(
		"../scripts/property-auto-runner.ts",
		import.meta.url,
	).href;
	const child = spawnSync(
		process.execPath,
		[
			"--experimental-test-module-mocks",
			"--input-type=module",
			"-e",
			`
    import assert from "node:assert/strict"
    import { mkdtempSync, rmSync, writeSync } from "node:fs"
    import { registerHooks } from "node:module"
    import { tmpdir } from "node:os"
    import { join } from "node:path"
    import { mock } from "node:test"
    import { fileURLToPath } from "node:url"
    const runnerUrl = ${JSON.stringify(runnerUrl)}
    const root = mkdtempSync(join(tmpdir(), "kras-quick-loop-"))
    process.env.KRAS_QUICK_ROOT = root
    const answers = ${JSON.stringify(["", ...inputs, "", ""])}
    const events = []
    const pending = []
    let autoCalls = 0
    globalThis.__quickAuto = async (address, _terminal, _root, scope) => {
      events.push(["auto", address, scope])
      if (++autoCalls === ${pauseAfter}) pending.push("일시정지")
      return { totalAggregateBuildings: 0, processedAggregateBuildings: 0 }
    }
    globalThis.__quickManual = async (address, _terminal, options) => {
      events.push(["manual", address, options.inputMode])
      return true
    }
    const terminal = {
      question: async () => {
        const answer = answers.shift()
        assert.notEqual(answer, undefined)
        if (answer !== "") events.push(["input", answer])
        return answer
      },
      drainPending: () => pending.splice(0),
      close: () => undefined,
    }
    for (const [name, overrides] of [
      ["kras-browser.ts", {
        prepareKrasQuickBrowser: async () => true,
        shutdownKrasBrowser: async () => {
          try {
            assert.equal(answers.length, 0)
            writeSync(1, JSON.stringify(events))
          } finally {
            rmSync(root, { recursive: true, force: true })
          }
        },
      }],
      ["terminal-reader.ts", { createTerminal: () => terminal }],
      ["kras-workflow-watcher.ts", {
        connectKrasWorkflowWatcher: async () => ({
          awaitOzViewer: async (options) => {
            events.push(["viewer", options?.allowExisting ?? false])
            return false
          },
        }),
        connectOrOpenKrasWorkflowWatcher: async () => ({}),
      }],
    ]) {
      const url = new URL(name, runnerUrl).href
      mock.module(url, { namedExports: { ...await import(url), ...overrides } })
    }
    registerHooks({
      load(url, context, nextLoad) {
        const loaded = nextLoad(url, context)
        if (url !== runnerUrl) return loaded
        const source = typeof loaded.source === "string"
          ? loaded.source : Buffer.from(loaded.source).toString("utf8")
        return { ...loaded, source:
          "const runAutoParcelWorkflow = globalThis.__quickAuto;\\n" +
          "const runPropertyKrasWorkflow = globalThis.__quickManual;\\n" + source
            .replace("export async function runAutoParcelWorkflow(", "async function originalRunAutoParcelWorkflow(")
            .replace("export async function runPropertyKrasWorkflow(", "async function originalRunPropertyKrasWorkflow(")
        }
      },
    })
    process.stdout.write = () => true
    process.argv = [process.execPath, fileURLToPath(runnerUrl), "quick"]
    await import(runnerUrl)
  `,
		],
		{ encoding: "utf8", timeout: 10_000 },
	);
	assert.equal(child.status, 0, child.error?.message ?? child.stderr);
	return JSON.parse(child.stdout);
}

describe("quick address loop", () => {
	const address = "서울특별시 노원구 월계동 392-19";
	const otherAddress = "서울특별시 중구 태평로1가 31";
	const addedAddress = "서울특별시 용산구 용산동2가 5-227";

	for (const { name, inputs, expected } of [
		{
			name: "runs the default address when normal input is a dash",
			inputs: ["-"],
			expected: [
				["input", "-"],
				["manual", address, { kind: "address", address }],
			],
		},
		{
			name: "runs the default AUTO address with the selected land-only scope",
			inputs: ["AUTO", "-", "1"],
			expected: [
				["input", "AUTO"],
				["input", "-"],
				["input", "1"],
				["auto", address, "land-only"],
			],
		},
		{
			name: "runs a single AUTO address without waiting for 시작",
			inputs: ["AUTO", otherAddress, "2"],
			expected: [
				["input", "AUTO"],
				["input", otherAddress],
				["input", "2"],
				["auto", otherAddress, "ordinary-only"],
			],
		},
		{
			name: "dispatches AUTO normally when it follows a QA toggle",
			inputs: ["QA", "AUTO", otherAddress, "3"],
			expected: [
				["input", "QA"],
				["input", "AUTO"],
				["input", otherAddress],
				["input", "3"],
				["auto", otherAddress, "full"],
			],
		},
		{
			name: "uses full scope when a single AUTO scope answer is Enter",
			inputs: ["AUTO", otherAddress, ""],
			expected: [
				["input", "AUTO"],
				["input", otherAddress],
				["auto", otherAddress, "full"],
			],
		},
		{
			name: "allows an existing report when manual input is 0",
			inputs: ["-", "0"],
			expected: [
				["input", "-"],
				["manual", address, { kind: "address", address }],
				["input", "0"],
				["viewer", true],
			],
		},
	] as const) {
		it(name, () => {
			// Given
			const answers = inputs;
			// When
			const events = driveQuick(answers);
			// Then
			assert.deepEqual(events, expected);
		});
	}

	for (const { name, prefix } of [
		{ name: "explicit batch", prefix: ["AUTO", "...", address, otherAddress] },
		{
			name: "initial multiline batch",
			prefix: [address + "\n" + otherAddress],
		},
		{
			name: "AUTO multiline batch",
			prefix: ["AUTO", address + "\n" + otherAddress],
		},
	] as const) {
		it(`keeps collecting addresses until 시작 when input is an ${name}`, () => {
			// Given
			const inputs = [...prefix, addedAddress, "시작", "2"];
			// When
			const events = driveQuick(inputs);
			// Then
			assert.deepEqual(events, [
				...inputs.map((input) => ["input", input]),
				["auto", address, "ordinary-only"],
				["auto", otherAddress, "ordinary-only"],
				["auto", addedAddress, "ordinary-only"],
			]);
		});
	}

	it("selects a fresh scope when a single AUTO address follows a land-only batch", () => {
		// Given
		const batch = ["AUTO", "...", address, "시작", "1"];
		// When
		const events = driveQuick([...batch, "AUTO", otherAddress, "3"]);
		// Then
		assert.deepEqual(events, [
			...batch.map((input) => ["input", input]),
			["auto", address, "land-only"],
			["input", "AUTO"],
			["input", otherAddress],
			["input", "3"],
			["auto", otherAddress, "full"],
		]);
	});

	it("selects a separate scope for each single AUTO address", () => {
		// Given
		const inputs = ["AUTO", address, "2", otherAddress, "1"];
		// When
		const events = driveQuick(inputs);
		// Then
		assert.deepEqual(events, [
			["input", "AUTO"],
			["input", address],
			["input", "2"],
			["auto", address, "ordinary-only"],
			["input", otherAddress],
			["input", "1"],
			["auto", otherAddress, "land-only"],
		]);
	});

	it("returns to AUTO address input when scope selection is cancelled", () => {
		// Given
		const inputs = ["AUTO", address, "*", otherAddress, "1"];
		// When
		const events = driveQuick(inputs);
		// Then
		assert.deepEqual(events, [
			["input", "AUTO"],
			["input", address],
			["input", "*"],
			["input", otherAddress],
			["input", "1"],
			["auto", otherAddress, "land-only"],
		]);
	});

	for (const queued of [[], [otherAddress]] as const) {
		it(`preserves scope and collects paused input when ${queued.length} addresses remain`, () => {
			// Given
			const batch = ["AUTO", "...", address, ...queued, "시작", "2"];
			// When
			const events = driveQuick([...batch, addedAddress, "재개"], 1);
			// Then
			assert.deepEqual(events, [
				...batch.map((input) => ["input", input]),
				["auto", address, "ordinary-only"],
				["input", addedAddress],
				["input", "재개"],
				...[...queued, addedAddress].map((item) => [
					"auto",
					item,
					"ordinary-only",
				]),
			]);
		});
	}
});

describe("property auto CLI pipeline", () => {
	it("keeps a missing scoped viewer blocked instead of falling back to another report", async () => {
		// Given
		const address = "서울특별시 노원구 월계동 392-19";
		const root = await mkdtemp(
			join(process.env["TEMP"] ?? ".", "kras-scoped-capture-"),
		);
		let scopedCalls = 0;
		let globalCalls = 0;
		const stage2 = createKrasManualStage2Result({
			version: 1,
			stage: 1,
			status: "blocked",
			address,
			reason: "no_building",
			hasBuilding: false,
			buildingOptions: [],
		});
		try {
			// When
			const execution = finishQuickOzWorkflow(
				{
					connectOzViewer: async () => {
						scopedCalls += 1;
						return undefined;
					},
					closeOzViewer: async () => true,
				},
				address,
				undefined,
				root,
				false,
				(stage3Address, options) =>
					runPropertyAutoStage3(
						stage3Address,
						{
							readResult: async () => JSON.stringify(stage2),
							connect: async () => {
								globalCalls += 1;
								throw new Error("Unrelated viewer must not be acquired");
							},
						},
						options,
					),
			);
			// Then
			await assert.rejects(execution, { name: "KrasOzCaptureError" });
			assert.equal(scopedCalls, 1);
			assert.equal(globalCalls, 0);
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	});

	it("propagates AUTO capture errors before the caller can mark the target complete", async () => {
		// Given
		let closeCalls = 0;
		const error = new Error("MOCK capture failure");
		// When
		const execution = finishQuickOzWorkflow(
			{
				closeOzViewer: async () => {
					closeCalls += 1;
					return true;
				},
			},
			"서울특별시 노원구 월계동 392-19",
			undefined,
			"unused",
			false,
			async () => {
				throw error;
			},
		);
		// Then
		await assert.rejects(execution, (caught: unknown) => caught === error);
		assert.equal(closeCalls, 0);
	});

	for (const { name, content, errorName } of [
		{
			name: "blocked",
			content: JSON.stringify({
				version: 1,
				stage: 3,
				status: "blocked",
				address: "서울특별시 노원구 월계동 392-19",
				reason: "viewer_not_open",
			}),
			errorName: "KrasOzCaptureError",
		},
		{ name: "malformed", content: "{", errorName: "SyntaxError" },
	])
		it(`rejects AUTO completion when Stage 3 metadata is ${name}`, async () => {
			// Given
			const root = await mkdtemp(
				join(process.env["TEMP"] ?? ".", "kras-capture-result-"),
			);
			const path = join(root, "stage3.json");
			await writeFile(path, content);
			try {
				// When
				const execution = finishQuickOzWorkflow(
					{ closeOzViewer: async () => true },
					"서울특별시 노원구 월계동 392-19",
					undefined,
					root,
					false,
					async () => path,
				);
				// Then
				await assert.rejects(execution, { name: errorName });
			} finally {
				await rm(root, { recursive: true, force: true });
			}
		});

	it("preserves the report without asking to close it when manual capture fails", async () => {
		// Given
		let questions = 0;
		let closeCalls = 0;
		// When
		await finishQuickOzWorkflow(
			{
				closeOzViewer: async () => {
					closeCalls += 1;
					return true;
				},
			},
			"서울특별시 노원구 월계동 392-19",
			{
				question: async () => {
					questions += 1;
					return "";
				},
				close: () => undefined,
			},
			"unused",
			false,
			async () => {
				throw new Error("MOCK capture failure");
			},
		);
		// Then
		assert.equal(questions, 1);
		assert.equal(closeCalls, 0);
	});

	it("routes exact slash-address input while preserving ordinary answers", async () => {
		const answers = [" * ", "101호"];
		const terminal = createAddressReturningTerminal({
			question: async () => answers.shift() ?? "",
			close: () => undefined,
		});

		await assert.rejects(
			() => terminal.question("stage"),
			ReturnToAddressError,
		);
		assert.equal(await terminal.question("stage"), "101호");
	});

	it("returns from a filename conflict without closing the OZ viewer", async () => {
		// Given
		const answers = ["", "*", "unused"];
		let closeCalls = 0;
		const terminal = createAddressReturningTerminal({
			question: async () => answers.shift() ?? "",
			close: () => undefined,
		});

		// When
		const execution = finishQuickOzWorkflow(
			{
				closeOzViewer: async () => {
					closeCalls += 1;
					return true;
				},
			},
			"서울특별시 노원구 월계동 392-19",
			terminal,
			"C:\\Users\\admin\\Downloads\\KRAS",
			false,
			async (_address, options) => {
				const resolveNameConflict = options.resolveNameConflict;
				assert.notEqual(resolveNameConflict, undefined);
				if (resolveNameConflict !== undefined)
					await resolveNameConflict("existing.pdf");
				return "unused";
			},
		);

		// Then
		await assert.rejects(execution, ReturnToAddressError);
		assert.equal(closeCalls, 0);
	});

	it("activates QA mode only for exact top-level QA input", () => {
		assert.deepEqual(parseQuickAddressInput("QA"), { kind: "qa" });
		assert.deepEqual(parseQuickAddressInput("qa"), {
			kind: "address",
			address: "qa",
		});
		assert.deepEqual(parseQuickAddressInput(" QA "), {
			kind: "address",
			address: " QA ",
		});
	});

	it("detects a multiline address batch without changing a single address", () => {
		assert.deepEqual(
			splitQuickAddressBatch("서울특별시 노원구 월계동 392-19"),
			["서울특별시 노원구 월계동 392-19"],
		);
		assert.deepEqual(
			splitQuickAddressBatch(
				"서울특별시 노원구 월계동 392-19\n서울특별시 중구 태평로1가 31",
			),
			["서울특별시 노원구 월계동 392-19", "서울특별시 중구 태평로1가 31"],
		);
		assert.deepEqual(splitQuickAddressBatch("\n\n"), []);
	});

	it("recognizes exact 시작 as batch execution command", () => {
		assert.deepEqual(parseQuickAddressInput("시작"), { kind: "start" });
		assert.deepEqual(parseQuickAddressInput("일시정지"), { kind: "pause" });
		assert.deepEqual(parseQuickAddressInput("재개"), { kind: "resume" });
		assert.deepEqual(parseQuickAddressInput(" 시작 "), {
			kind: "address",
			address: " 시작 ",
		});
		assert.deepEqual(parseQuickAddressInput("..."), { kind: "batch" });
		assert.deepEqual(parseQuickAddressInput("+"), {
			kind: "address",
			address: "+",
		});
		assert.equal(QUICK_ADDRESS_PROMPT.includes("대규모 처리"), false);
	});

	it("parses and prompts for batch processing scope", async () => {
		assert.equal(parseAutoBatchScopeAnswer("1"), "land-only");
		assert.equal(parseAutoBatchScopeAnswer("2"), "ordinary-only");
		assert.equal(parseAutoBatchScopeAnswer("3"), "full");
		assert.equal(parseAutoBatchScopeAnswer(""), "full");
		assert.equal(parseAutoBatchScopeAnswer("x"), undefined);

		const answers = ["x", "1"];
		assert.equal(
			await askAutoBatchScope({
				question: async () => answers.shift() ?? "",
				close: () => undefined,
			}),
			"land-only",
		);
	});

	it("stops target selection after land output in land-only batch scope", async () => {
		const root = await mkdtemp(
			join(process.env["TEMP"] ?? ".", "kras-batch-scope-"),
		);
		const address = "서울특별시 용산구 용산동2가 5-227";
		const paths = propertyAutoStagePaths(address, root);
		const stage1 = {
			version: 1 as const,
			stage: 1 as const,
			status: "completed" as const,
			address,
			hasBuilding: true as const,
			buildingOptions: [
				{ index: 1, value: "A", label: "주건축물" },
				{ index: 2, value: "B", label: "집합건물 (집합)" },
			],
		};
		try {
			await mkdir(paths.directory, { recursive: true });
			await mkdir(paths.otherDirectory, { recursive: true });
			await runKrasAutoStage3(
				{
					version: 1,
					stage: 2,
					status: "completed",
					address,
					building: { index: 0, value: "", label: "" },
				},
				{
					url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
					hasVisibleCaptcha: async () => false,
					capturePdf: async () => TEST_PDF,
					capturePagePngs: async () => [TEST_PNG],
					readReportXml: async () => TEST_XML,
				},
				paths,
			);
			assert.equal(
				pickNextAutoTarget(paths, stage1, new Map(), "land-only"),
				undefined,
			);
			assert.deepEqual(pickNextAutoTarget(paths, stage1, new Map(), "full"), {
				kind: "building",
				building: stage1.buildingOptions[0],
			});
			assert.deepEqual(
				pickNextAutoTarget(paths, stage1, new Map(), "ordinary-only"),
				{
					kind: "building",
					building: stage1.buildingOptions[0],
				},
			);
			const aggregateBuilding = stage1.buildingOptions[1];
			assert.ok(aggregateBuilding);
			const aggregateOnly = { ...stage1, buildingOptions: [aggregateBuilding] };
			assert.equal(
				pickNextAutoTarget(paths, aggregateOnly, new Map(), "ordinary-only"),
				undefined,
			);
			assert.deepEqual(
				pickNextAutoTarget(paths, aggregateOnly, new Map(), "full"),
				{
					kind: "aggregate-total",
					building: aggregateOnly.buildingOptions[0],
				},
			);
		} finally {
			await rm(root, { recursive: true, force: true });
		}
	});

	it("formats address and collective-building batch progress", () => {
		assert.equal(
			formatAutoBatchProgress({
				totalAddresses: 24,
				processedAddresses: 7,
				totalAggregateBuildings: 58,
				processedAggregateBuildings: 12,
			}),
			"현재 전체 24개 주소 중 7개 처리 | 집합건물 전체 58개 중 12개 처리\n",
		);
	});

	it("switches AUTO CAPTCHA to manual input after three OCR failures", async () => {
		const calls: string[] = [];
		const answers = ["54321"];
		await handleQuickCaptcha({
			page: {
				captureCaptchaImage: async () => {
					calls.push("capture");
					return "cG5n";
				},
				renderCaptchaTerminal: async () => {
					calls.push("ansi");
					return "ANSI";
				},
				fillCaptchaAndSubmit: async (candidate) => {
					calls.push(`submit:${candidate}`);
					return "success";
				},
			},
			terminal: {
				question: async () => answers.shift() ?? "",
				close: () => undefined,
			},
			mode: { kind: "auto" },
			recognize: async () => {
				calls.push("ocr");
				return null;
			},
		});
		assert.deepEqual(calls, [
			"ansi",
			"capture",
			"ocr",
			"capture",
			"ocr",
			"capture",
			"ocr",
			"submit:54321",
		]);
	});

	it("submits a different OCR candidate after each AUTO CAPTCHA failure", async () => {
		const calls: string[] = [];
		const candidates = ["11111", "22222"];
		let submissions = 0;
		await handleQuickCaptcha({
			page: {
				captureCaptchaImage: async () => {
					calls.push("capture");
					return "cG5n";
				},
				renderCaptchaTerminal: async () => "ANSI",
				fillCaptchaAndSubmit: async (candidate) => {
					calls.push(`submit:${candidate}`);
					submissions += 1;
					return submissions === 2 ? "success" : "error";
				},
			},
			terminal: { question: async () => "", close: () => undefined },
			mode: { kind: "auto" },
			recognize: async (_png, attempt) => {
				calls.push(`ocr:${attempt}`);
				return candidates[(attempt ?? 1) - 1] ?? null;
			},
		});
		assert.deepEqual(calls, [
			"capture",
			"ocr:1",
			"submit:11111",
			"capture",
			"ocr:2",
			"submit:22222",
		]);
	});

	it("keeps AUTO lookup failures retryable after manual CAPTCHA entry", async () => {
		const answers = ["54321"];
		await assert.rejects(
			handleQuickCaptcha({
				page: {
					captureCaptchaImage: async () => "cG5n",
					renderCaptchaTerminal: async () => "ANSI",
					fillCaptchaAndSubmit: async () => "need_lookup",
				},
				terminal: {
					question: async () => answers.shift() ?? "",
					close: () => undefined,
				},
				mode: { kind: "auto" },
				recognize: async () => null,
			}),
			KrasAutoLookupNeededError,
		);
	});

	it("pauses AUTO for re-login when CAPTCHA submission reports an expired session", async () => {
		const prompts: string[] = [];
		await assert.rejects(
			handleQuickCaptcha({
				page: {
					captureCaptchaImage: async () => "cG5n",
					renderCaptchaTerminal: async () => "ANSI",
					fillCaptchaAndSubmit: async () => "login_expired",
				},
				terminal: {
					question: async (prompt) => {
						prompts.push(prompt);
						return "";
					},
					close: () => undefined,
				},
				mode: { kind: "auto" },
				recognize: async () => "12345",
			}),
			KrasAutoLoginExpiredError,
		);
		assert.deepEqual(prompts, [
			"로그인을 다시 해주시고 엔터버튼을 눌러주세요: ",
		]);
	});

	it("fails a stalled OZ operation instead of waiting forever", async () => {
		await assert.rejects(
			withOzTimeout("테스트 캡처", new Promise<never>(() => undefined), 5),
			/테스트 캡처 시간 초과 \(0\.005초\)/,
		);
	});

	it("submits a QA OCR candidate after an explicit Enter confirmation", async () => {
		const calls: string[] = [];
		const answers = [""];
		await handleQuickCaptcha({
			page: {
				captureCaptchaImage: async () => {
					calls.push("capture");
					return "cG5n";
				},
				renderCaptchaTerminal: async () => {
					calls.push("ansi");
					return "ANSI";
				},
				fillCaptchaAndSubmit: async (candidate) => {
					calls.push(`submit:${candidate}`);
					return "success";
				},
			},
			terminal: {
				question: async () => answers.shift() ?? "",
				close: () => undefined,
			},
			mode: { kind: "qa" },
			recognize: async () => {
				calls.push("ocr");
				return "12345";
			},
		});
		assert.deepEqual(calls, ["ansi", "capture", "ocr", "submit:12345"]);
	});

	it("falls back to existing manual input when QA candidate is declined with 0", async () => {
		const calls: string[] = [];
		const answers = ["0", "54321"];
		await handleQuickCaptcha({
			page: {
				captureCaptchaImage: async () => "cG5n",
				renderCaptchaTerminal: async () => {
					calls.push("ansi");
					return "ANSI";
				},
				fillCaptchaAndSubmit: async (candidate) => {
					calls.push(`submit:${candidate}`);
					return "success";
				},
			},
			terminal: {
				question: async () => answers.shift() ?? "",
				close: () => undefined,
			},
			mode: { kind: "qa" },
			recognize: async () => "12345",
		});
		assert.deepEqual(calls, ["ansi", "submit:54321"]);
	});

	it("never captures or invokes OCR for an ordinary address", async () => {
		const calls: string[] = [];
		const answers = ["54321"];
		await handleQuickCaptcha({
			page: {
				captureCaptchaImage: async () => {
					calls.push("capture");
					return "cG5n";
				},
				renderCaptchaTerminal: async () => {
					calls.push("ansi");
					return "ANSI";
				},
				fillCaptchaAndSubmit: async (candidate) => {
					calls.push(`submit:${candidate}`);
					return "success";
				},
			},
			terminal: {
				question: async () => answers.shift() ?? "",
				close: () => undefined,
			},
			mode: { kind: "address", address: "서울특별시 노원구 월계동 392-19" },
			recognize: async () => {
				calls.push("ocr");
				return "12345";
			},
		});
		assert.deepEqual(calls, ["ansi", "submit:54321"]);
	});

	it("keeps the previous address when the current workflow is not reusable", () => {
		// Given
		const previousAddress = "서울특별시 용산구 용산동2가 5-227";
		const currentAddress = "서울특별시 노원구 월계동 392-19";

		// When
		const retained = nextPreviousAddress(
			previousAddress,
			currentAddress,
			false,
		);
		const replaced = nextPreviousAddress(previousAddress, currentAddress, true);

		// Then
		assert.equal(retained, previousAddress);
		assert.equal(replaced, currentAddress);
	});

	it("disconnects an owned workflow page when its action fails", async () => {
		// Given
		const failure = new Error("action failed");
		const calls: string[] = [];
		const workflowPage = {
			url: () => "https://www.kras.go.kr/kras/cert/certView.do",
			lookup: async () => ({ kind: "lookup" as const, hasBuilding: false }),
			readBuildingOptions: async () => [],
			selectBuilding: async () => [],
			selectFloorRoom: async () => undefined,
			showCaptchaPrompt: async () => undefined,
			awaitOzViewer: async () => false,
			captureCaptchaImage: async () => null,
			renderCaptchaTerminal: async () => null,
			fillCaptchaAndSubmit: async () => "error" as const,
			closeOzViewer: async () => false,
			disconnect: async () => {
				calls.push("disconnect");
			},
		};

		// When
		const execution = withKrasWorkflowPage(
			async () => workflowPage,
			async () => {
				throw failure;
			},
		);

		// Then
		await assert.rejects(execution, failure);
		assert.deepEqual(calls, ["disconnect"]);
	});

	it("runs Stage 1 against the discovered KRAS page and persists its result", async () => {
		const saved: { path?: string; result?: KrasAutoStageResult } = {};
		const krasPage = {
			url: () => "https://www.kras.go.kr/kras/cert/certView.do",
			installPropertyWatcherChannel: async () => ({
				dispose: async () => undefined,
				failed: new Promise<never>(() => undefined),
				updateBuildingStatus: async () => "applied" as const,
			}),
			evaluate: async () => ({
				kind: "lookup" as const,
				hasBuilding: true,
				buildingOptions: [{ index: 1, value: "A", label: "주건축물" }],
			}),
		};
		const browser: PropertyWatcherRuntimeBrowser = {
			contexts: () => [{ pages: () => [krasPage] }],
		};
		const dependencies: PropertyAutoStage1Dependencies = {
			connect: async () => browser,
			writeResult: async (path, result) => {
				saved.path = path;
				saved.result = result;
			},
		};

		const path = await runPropertyAutoStage1(
			"서울특별시 용산구 용산동2가 5-227",
			dependencies,
		);

		assert.equal(path, saved.path);
		assert.equal(saved.result?.status, "completed");
		assert.equal(saved.result?.address, "서울특별시 용산구 용산동2가 5-227");
	});

	it("auto-selects the first Stage 2 building while preserving explicit selection", async () => {
		const stage1 = {
			buildingOptions: [
				{ index: 1, value: "A", label: "주건축물" },
				{ index: 2, value: "B", label: "별관" },
			],
		};
		assert.equal(selectBuildingIndex(stage1), 1);
		assert.equal(selectBuildingIndex(stage1, 2), 2);
	});

	it("persists an automatically selected Stage 2 building", async () => {
		const writes: Array<{ path: string; result: KrasAutoStageResult }> = [];
		const stage1: KrasAutoStageResult = {
			version: 1,
			stage: 1,
			status: "completed",
			address: "서울특별시 용산구 용산동2가 5-227",
			hasBuilding: true,
			buildingOptions: [
				{ index: 1, value: "A", label: "주건축물" },
				{ index: 2, value: "B", label: "별관" },
			],
		};
		const dependencies: PropertyAutoStage2Dependencies = {
			readResult: async () => JSON.stringify(stage1),
			writeResult: async (path, result) => {
				writes.push({ path, result });
			},
		};

		await runPropertyAutoStage2(stage1.address, undefined, dependencies);

		assert.equal(writes.length, 1);
		assert.equal(writes[0]?.result.stage, 2);
		assert.equal(writes[0]?.result.status, "completed");
	});

	it("writes no-building manual Stage 2 under the quick output root", async () => {
		const root = "C:\\Users\\admin\\Downloads\\KRAS";
		const blockedStage1: KrasAutoStageResult = {
			version: 1,
			stage: 1,
			status: "blocked",
			address: "서울특별시 노원구 월계동 392-19",
			reason: "no_building",
			hasBuilding: false,
			buildingOptions: [],
		};
		const writes: Array<{ path: string; result: KrasAutoStageResult }> = [];

		await writeManualStage2ForRoot(
			blockedStage1,
			root,
			async (path, result) => {
				writes.push({ path, result });
			},
		);

		assert.equal(writes.length, 1);
		assert.match(
			writes[0]?.path ?? "",
			/Downloads\\KRAS\\서울특별시 노원구 월계동 392-19\\기타\\stage2\.json$/u,
		);
		assert.equal(writes[0]?.result.stage, 2);
		assert.equal(writes[0]?.result.status, "completed");
	});

	it("starts watcher, forwards CLI arguments, and leaves watcher running", async () => {
		// Given
		const calls: string[] = [];

		// When
		const exitCode = await runPropertyAutoPipeline(
			["--continue"],
			dependencies(calls),
		);

		// Then
		assert.equal(exitCode, 0);
		assert.deepEqual(calls, ["watcher:start", "cli:--continue"]);
	});

	it("preserves CLI exit code without stopping watcher", async () => {
		// Given
		const calls: string[] = [];

		// When
		const exitCode = await runPropertyAutoPipeline([], dependencies(calls, 7));

		// Then
		assert.equal(exitCode, 7);
		assert.deepEqual(calls, ["watcher:start", "cli:"]);
	});

	it("leaves watcher running when CLI launch fails", async () => {
		// Given
		const calls: string[] = [];
		const failure = new Error("CLI failed");
		const failingDependencies: PropertyAutoDependencies = {
			runCli: async () => {
				calls.push("cli");
				throw failure;
			},
			runWatcher: async (command) => {
				calls.push(`watcher:${command}`);
			},
		};

		// When
		const execution = runPropertyAutoPipeline([], failingDependencies);

		// Then
		await assert.rejects(execution, failure);
		assert.deepEqual(calls, ["watcher:start", "cli"]);
	});
});
