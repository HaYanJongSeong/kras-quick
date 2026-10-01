import assert from "node:assert/strict"
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises"
import { describe, it } from "node:test"
import { join } from "node:path"
import type { KrasAutoStageResult } from "../scripts/kras-auto-stages.ts"
import { propertyAutoStagePaths } from "../scripts/kras-auto-stages.ts"
import { withOzTimeout } from "../scripts/kras-oz.ts"
import {
  createAddressReturningTerminal,
  finishQuickOzWorkflow,
  handleQuickCaptcha,
  askAutoBatchScope,
  formatAutoBatchProgress,
  parseAutoBatchScopeAnswer,
  pickNextAutoTarget,
  KrasAutoLoginExpiredError,
  KrasAutoLookupNeededError,
  nextPreviousAddress,
  type PropertyAutoDependencies,
  type PropertyAutoStage1Dependencies,
  type PropertyAutoStage2Dependencies,
  parseQuickAddressInput,
  QUICK_ADDRESS_PROMPT,
  ReturnToAddressError,
  runPropertyAutoPipeline,
  runPropertyAutoStage1,
  runPropertyAutoStage2,
  selectBuildingIndex,
  splitQuickAddressBatch,
  withKrasWorkflowPage,
  writeManualStage2ForRoot,
} from "../scripts/property-auto-runner.ts"
import type { PropertyWatcherRuntimeBrowser } from "../scripts/property-watcher-runtime.ts"

function dependencies(calls: string[], cliExitCode = 0): PropertyAutoDependencies {
  return {
    runCli: async (args) => {
      calls.push(`cli:${args.join(" ")}`)
      return cliExitCode
    },
    runWatcher: async (command) => {
      calls.push(`watcher:${command}`)
    },
  }
}

describe("property auto CLI pipeline", () => {
  it("routes exact slash-address input while preserving ordinary answers", async () => {
    const answers = [" * ", "101호"]
    const terminal = createAddressReturningTerminal({
      question: async () => answers.shift() ?? "",
      close: () => undefined,
    })

    await assert.rejects(() => terminal.question("stage"), ReturnToAddressError)
    assert.equal(await terminal.question("stage"), "101호")
  })

  it("returns from a filename conflict without closing the OZ viewer", async () => {
    // Given
    const answers = ["", "*", "unused"]
    let closeCalls = 0
    const terminal = createAddressReturningTerminal({
      question: async () => answers.shift() ?? "",
      close: () => undefined,
    })

    // When
    const execution = finishQuickOzWorkflow(
      {
        closeOzViewer: async () => {
          closeCalls += 1
          return true
        },
      },
      "서울특별시 노원구 월계동 392-19",
      terminal,
      "C:\\Users\\admin\\Downloads\\KRAS",
      false,
      async (_address, options) => {
        const resolveNameConflict = options.resolveNameConflict
        assert.notEqual(resolveNameConflict, undefined)
        if (resolveNameConflict !== undefined) await resolveNameConflict("existing.pdf")
        return "unused"
      },
    )

    // Then
    await assert.rejects(execution, ReturnToAddressError)
    assert.equal(closeCalls, 0)
  })

  it("activates QA mode only for exact top-level QA input", () => {
    assert.deepEqual(parseQuickAddressInput("QA"), { kind: "qa" })
    assert.deepEqual(parseQuickAddressInput("qa"), { kind: "address", address: "qa" })
    assert.deepEqual(parseQuickAddressInput(" QA "), { kind: "address", address: " QA " })
  })

  it("detects a multiline address batch without changing a single address", () => {
    assert.deepEqual(splitQuickAddressBatch("서울특별시 노원구 월계동 392-19"), [
      "서울특별시 노원구 월계동 392-19",
    ])
    assert.deepEqual(
      splitQuickAddressBatch("서울특별시 노원구 월계동 392-19\n서울특별시 중구 태평로1가 31"),
      ["서울특별시 노원구 월계동 392-19", "서울특별시 중구 태평로1가 31"],
    )
    assert.deepEqual(splitQuickAddressBatch("\n\n"), [])
  })

  it("recognizes exact 시작 as batch execution command", () => {
    assert.deepEqual(parseQuickAddressInput("시작"), { kind: "start" })
    assert.deepEqual(parseQuickAddressInput("일시정지"), { kind: "pause" })
    assert.deepEqual(parseQuickAddressInput("재개"), { kind: "resume" })
    assert.deepEqual(parseQuickAddressInput(" 시작 "), { kind: "address", address: " 시작 " })
    assert.deepEqual(parseQuickAddressInput("..."), { kind: "batch" })
    assert.deepEqual(parseQuickAddressInput("+"), { kind: "address", address: "+" })
    assert.equal(QUICK_ADDRESS_PROMPT.includes("대규모 처리"), false)
  })

  it("parses and prompts for batch processing scope", async () => {
    assert.equal(parseAutoBatchScopeAnswer("1"), "land-only")
    assert.equal(parseAutoBatchScopeAnswer("2"), "ordinary-only")
    assert.equal(parseAutoBatchScopeAnswer("3"), "full")
    assert.equal(parseAutoBatchScopeAnswer(""), "full")
    assert.equal(parseAutoBatchScopeAnswer("x"), undefined)

    const answers = ["x", "1"]
    assert.equal(
      await askAutoBatchScope({
        question: async () => answers.shift() ?? "",
        close: () => undefined,
      }),
      "land-only",
    )
  })

  it("stops target selection after land output in land-only batch scope", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-batch-scope-"))
    const address = "서울특별시 용산구 용산동2가 5-227"
    const paths = propertyAutoStagePaths(address, root)
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
    }
    try {
      await mkdir(paths.directory, { recursive: true })
      await mkdir(paths.otherDirectory, { recursive: true })
      await writeFile(paths.pdf, "pdf")
      await writeFile(paths.pagePng(1), "png")
      await writeFile(paths.pageXml, "xml")
      await writeFile(paths.structureJson, "json")
      assert.equal(pickNextAutoTarget(paths, stage1, new Map(), "land-only"), undefined)
      assert.deepEqual(pickNextAutoTarget(paths, stage1, new Map(), "full"), {
        kind: "building",
        building: stage1.buildingOptions[0],
      })
      assert.deepEqual(pickNextAutoTarget(paths, stage1, new Map(), "ordinary-only"), {
        kind: "building",
        building: stage1.buildingOptions[0],
      })
      const aggregateBuilding = stage1.buildingOptions[1]
      assert.ok(aggregateBuilding)
      const aggregateOnly = { ...stage1, buildingOptions: [aggregateBuilding] }
      assert.equal(pickNextAutoTarget(paths, aggregateOnly, new Map(), "ordinary-only"), undefined)
      assert.deepEqual(pickNextAutoTarget(paths, aggregateOnly, new Map(), "full"), {
        kind: "aggregate-total",
        building: aggregateOnly.buildingOptions[0],
      })
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("formats address and collective-building batch progress", () => {
    assert.equal(
      formatAutoBatchProgress({
        totalAddresses: 24,
        processedAddresses: 7,
        totalAggregateBuildings: 58,
        processedAggregateBuildings: 12,
      }),
      "현재 전체 24개 주소 중 7개 처리 | 집합건물 전체 58개 중 12개 처리\n",
    )
  })

  it("switches AUTO CAPTCHA to manual input after three OCR failures", async () => {
    const calls: string[] = []
    const answers = ["54321"]
    await handleQuickCaptcha({
      page: {
        captureCaptchaImage: async () => {
          calls.push("capture")
          return "cG5n"
        },
        renderCaptchaTerminal: async () => {
          calls.push("ansi")
          return "ANSI"
        },
        fillCaptchaAndSubmit: async (candidate) => {
          calls.push(`submit:${candidate}`)
          return "success"
        },
      },
      terminal: { question: async () => answers.shift() ?? "", close: () => undefined },
      mode: { kind: "auto" },
      recognize: async () => {
        calls.push("ocr")
        return null
      },
    })
    assert.deepEqual(calls, [
      "ansi",
      "capture",
      "ocr",
      "capture",
      "ocr",
      "capture",
      "ocr",
      "submit:54321",
    ])
  })

  it("submits a different OCR candidate after each AUTO CAPTCHA failure", async () => {
    const calls: string[] = []
    const candidates = ["11111", "22222"]
    let submissions = 0
    await handleQuickCaptcha({
      page: {
        captureCaptchaImage: async () => {
          calls.push("capture")
          return "cG5n"
        },
        renderCaptchaTerminal: async () => "ANSI",
        fillCaptchaAndSubmit: async (candidate) => {
          calls.push(`submit:${candidate}`)
          submissions += 1
          return submissions === 2 ? "success" : "error"
        },
      },
      terminal: { question: async () => "", close: () => undefined },
      mode: { kind: "auto" },
      recognize: async (_png, attempt) => {
        calls.push(`ocr:${attempt}`)
        return candidates[(attempt ?? 1) - 1] ?? null
      },
    })
    assert.deepEqual(calls, [
      "capture",
      "ocr:1",
      "submit:11111",
      "capture",
      "ocr:2",
      "submit:22222",
    ])
  })

  it("keeps AUTO lookup failures retryable after manual CAPTCHA entry", async () => {
    const answers = ["54321"]
    await assert.rejects(
      handleQuickCaptcha({
        page: {
          captureCaptchaImage: async () => "cG5n",
          renderCaptchaTerminal: async () => "ANSI",
          fillCaptchaAndSubmit: async () => "need_lookup",
        },
        terminal: { question: async () => answers.shift() ?? "", close: () => undefined },
        mode: { kind: "auto" },
        recognize: async () => null,
      }),
      KrasAutoLookupNeededError,
    )
  })

  it("pauses AUTO for re-login when CAPTCHA submission reports an expired session", async () => {
    const prompts: string[] = []
    await assert.rejects(
      handleQuickCaptcha({
        page: {
          captureCaptchaImage: async () => "cG5n",
          renderCaptchaTerminal: async () => "ANSI",
          fillCaptchaAndSubmit: async () => "login_expired",
        },
        terminal: {
          question: async (prompt) => {
            prompts.push(prompt)
            return ""
          },
          close: () => undefined,
        },
        mode: { kind: "auto" },
        recognize: async () => "12345",
      }),
      KrasAutoLoginExpiredError,
    )
    assert.deepEqual(prompts, ["로그인을 다시 해주시고 엔터버튼을 눌러주세요: "])
  })

  it("fails a stalled OZ operation instead of waiting forever", async () => {
    await assert.rejects(
      withOzTimeout("테스트 캡처", new Promise<never>(() => undefined), 5),
      /테스트 캡처 시간 초과 \(0\.005초\)/,
    )
  })

  it("submits a QA OCR candidate after an explicit Enter confirmation", async () => {
    const calls: string[] = []
    const answers = [""]
    await handleQuickCaptcha({
      page: {
        captureCaptchaImage: async () => {
          calls.push("capture")
          return "cG5n"
        },
        renderCaptchaTerminal: async () => {
          calls.push("ansi")
          return "ANSI"
        },
        fillCaptchaAndSubmit: async (candidate) => {
          calls.push(`submit:${candidate}`)
          return "success"
        },
      },
      terminal: { question: async () => answers.shift() ?? "", close: () => undefined },
      mode: { kind: "qa" },
      recognize: async () => {
        calls.push("ocr")
        return "12345"
      },
    })
    assert.deepEqual(calls, ["ansi", "capture", "ocr", "submit:12345"])
  })

  it("falls back to existing manual input when QA candidate is declined with 0", async () => {
    const calls: string[] = []
    const answers = ["0", "54321"]
    await handleQuickCaptcha({
      page: {
        captureCaptchaImage: async () => "cG5n",
        renderCaptchaTerminal: async () => {
          calls.push("ansi")
          return "ANSI"
        },
        fillCaptchaAndSubmit: async (candidate) => {
          calls.push(`submit:${candidate}`)
          return "success"
        },
      },
      terminal: { question: async () => answers.shift() ?? "", close: () => undefined },
      mode: { kind: "qa" },
      recognize: async () => "12345",
    })
    assert.deepEqual(calls, ["ansi", "submit:54321"])
  })

  it("never captures or invokes OCR for an ordinary address", async () => {
    const calls: string[] = []
    const answers = ["54321"]
    await handleQuickCaptcha({
      page: {
        captureCaptchaImage: async () => {
          calls.push("capture")
          return "cG5n"
        },
        renderCaptchaTerminal: async () => {
          calls.push("ansi")
          return "ANSI"
        },
        fillCaptchaAndSubmit: async (candidate) => {
          calls.push(`submit:${candidate}`)
          return "success"
        },
      },
      terminal: { question: async () => answers.shift() ?? "", close: () => undefined },
      mode: { kind: "address", address: "서울특별시 노원구 월계동 392-19" },
      recognize: async () => {
        calls.push("ocr")
        return "12345"
      },
    })
    assert.deepEqual(calls, ["ansi", "submit:54321"])
  })

  it("keeps the previous address when the current workflow is not reusable", () => {
    // Given
    const previousAddress = "서울특별시 용산구 용산동2가 5-227"
    const currentAddress = "서울특별시 노원구 월계동 392-19"

    // When
    const retained = nextPreviousAddress(previousAddress, currentAddress, false)
    const replaced = nextPreviousAddress(previousAddress, currentAddress, true)

    // Then
    assert.equal(retained, previousAddress)
    assert.equal(replaced, currentAddress)
  })

  it("disconnects an owned workflow page when its action fails", async () => {
    // Given
    const failure = new Error("action failed")
    const calls: string[] = []
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
        calls.push("disconnect")
      },
    }

    // When
    const execution = withKrasWorkflowPage(
      async () => workflowPage,
      async () => {
        throw failure
      },
    )

    // Then
    await assert.rejects(execution, failure)
    assert.deepEqual(calls, ["disconnect"])
  })

  it("runs Stage 1 against the discovered KRAS page and persists its result", async () => {
    const saved: { path?: string; result?: KrasAutoStageResult } = {}
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
    }
    const browser: PropertyWatcherRuntimeBrowser = {
      contexts: () => [{ pages: () => [krasPage] }],
    }
    const dependencies: PropertyAutoStage1Dependencies = {
      connect: async () => browser,
      writeResult: async (path, result) => {
        saved.path = path
        saved.result = result
      },
    }

    const path = await runPropertyAutoStage1("서울특별시 용산구 용산동2가 5-227", dependencies)

    assert.equal(path, saved.path)
    assert.equal(saved.result?.status, "completed")
    assert.equal(saved.result?.address, "서울특별시 용산구 용산동2가 5-227")
  })

  it("auto-selects the first Stage 2 building while preserving explicit selection", async () => {
    const stage1 = {
      buildingOptions: [
        { index: 1, value: "A", label: "주건축물" },
        { index: 2, value: "B", label: "별관" },
      ],
    }
    assert.equal(selectBuildingIndex(stage1), 1)
    assert.equal(selectBuildingIndex(stage1, 2), 2)
  })

  it("persists an automatically selected Stage 2 building", async () => {
    const writes: Array<{ path: string; result: KrasAutoStageResult }> = []
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
    }
    const dependencies: PropertyAutoStage2Dependencies = {
      readResult: async () => JSON.stringify(stage1),
      writeResult: async (path, result) => {
        writes.push({ path, result })
      },
    }

    await runPropertyAutoStage2(stage1.address, undefined, dependencies)

    assert.equal(writes.length, 1)
    assert.equal(writes[0]?.result.stage, 2)
    assert.equal(writes[0]?.result.status, "completed")
  })

  it("writes no-building manual Stage 2 under the quick output root", async () => {
    const root = "C:\\Users\\admin\\Downloads\\KRAS"
    const blockedStage1: KrasAutoStageResult = {
      version: 1,
      stage: 1,
      status: "blocked",
      address: "서울특별시 노원구 월계동 392-19",
      reason: "no_building",
      hasBuilding: false,
      buildingOptions: [],
    }
    const writes: Array<{ path: string; result: KrasAutoStageResult }> = []

    await writeManualStage2ForRoot(blockedStage1, root, async (path, result) => {
      writes.push({ path, result })
    })

    assert.equal(writes.length, 1)
    assert.match(
      writes[0]?.path ?? "",
      /Downloads\\KRAS\\서울특별시 노원구 월계동 392-19\\기타\\stage2\.json$/u,
    )
    assert.equal(writes[0]?.result.stage, 2)
    assert.equal(writes[0]?.result.status, "completed")
  })

  it("starts watcher, forwards CLI arguments, and leaves watcher running", async () => {
    // Given
    const calls: string[] = []

    // When
    const exitCode = await runPropertyAutoPipeline(["--continue"], dependencies(calls))

    // Then
    assert.equal(exitCode, 0)
    assert.deepEqual(calls, ["watcher:start", "cli:--continue"])
  })

  it("preserves CLI exit code without stopping watcher", async () => {
    // Given
    const calls: string[] = []

    // When
    const exitCode = await runPropertyAutoPipeline([], dependencies(calls, 7))

    // Then
    assert.equal(exitCode, 7)
    assert.deepEqual(calls, ["watcher:start", "cli:"])
  })

  it("leaves watcher running when CLI launch fails", async () => {
    // Given
    const calls: string[] = []
    const failure = new Error("CLI failed")
    const failingDependencies: PropertyAutoDependencies = {
      runCli: async () => {
        calls.push("cli")
        throw failure
      },
      runWatcher: async (command) => {
        calls.push(`watcher:${command}`)
      },
    }

    // When
    const execution = runPropertyAutoPipeline([], failingDependencies)

    // Then
    await assert.rejects(execution, failure)
    assert.deepEqual(calls, ["watcher:start", "cli"])
  })
})
