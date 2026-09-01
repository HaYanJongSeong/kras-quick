import assert from "node:assert/strict"
import { describe, it } from "node:test"
import type { KrasAutoStageResult } from "../scripts/kras-auto-stages.ts"
import {
  createAddressReturningTerminal,
  finishQuickOzWorkflow,
  handleQuickCaptcha,
  nextPreviousAddress,
  type PropertyAutoDependencies,
  type PropertyAutoStage1Dependencies,
  type PropertyAutoStage2Dependencies,
  parseQuickAddressInput,
  ReturnToAddressError,
  runPropertyAutoPipeline,
  runPropertyAutoStage1,
  runPropertyAutoStage2,
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
    const answers = [" /주소 ", "101호"]
    const terminal = createAddressReturningTerminal({
      question: async () => answers.shift() ?? "",
      close: () => undefined,
    })

    await assert.rejects(() => terminal.question("stage"), ReturnToAddressError)
    assert.equal(await terminal.question("stage"), "101호")
  })

  it("returns from a filename conflict without closing the OZ viewer", async () => {
    // Given
    const answers = ["1", "/주소", "1"]
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

  it("submits a QA OCR candidate only after explicit 1 confirmation", async () => {
    const calls: string[] = []
    const answers = ["1"]
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

  it("falls back to existing manual input when QA candidate is declined with 2", async () => {
    const calls: string[] = []
    const answers = ["2", "54321"]
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

  it("requires an explicit Stage 2 building index and persists the selected option", async () => {
    const writes: Array<{ path: string; result: KrasAutoStageResult }> = []
    const stage1: KrasAutoStageResult = {
      version: 1,
      stage: 1,
      status: "completed",
      address: "서울특별시 용산구 용산동2가 5-227",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "주건축물" }],
    }
    const dependencies: PropertyAutoStage2Dependencies = {
      readResult: async () => JSON.stringify(stage1),
      writeResult: async (path, result) => {
        writes.push({ path, result })
      },
    }

    await runPropertyAutoStage2(stage1.address, 1, dependencies)

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
