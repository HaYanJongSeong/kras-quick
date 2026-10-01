import assert from "node:assert/strict"
import { describe, it } from "node:test"

import type { KrasBuildingOption, KrasEvaluationResult } from "../scripts/kras-domain.ts"
import {
  KrasFloorRoomLoadTimeoutError,
  classifyKrasDialogText,
  isKrasLoginUrl,
  renderCaptchaTerminalPixels,
  runKrasWorkflowWatcher,
  shouldWaitForFloorRoomOptions,
  type KrasWorkflowPage,
} from "../scripts/kras-workflow-watcher.ts"
import type { KrasAutoStageResult } from "../scripts/kras-auto-stages.ts"

const address = "서울특별시 용산구 한남동 726-78"

function page(result: KrasEvaluationResult, calls: string[]): KrasWorkflowPage {
  return {
    url: () => "https://www.kras.go.kr/kras/cert/certView.do",
    lookup: async () => result,
    readBuildingOptions: async () =>
      result.kind === "lookup" ? [...(result.buildingOptions ?? [])] : [],
    selectBuilding: async (value) => {
      calls.push(`select:${value}`)
      return []
    },
    selectFloorRoom: async (option) => {
      calls.push(`floor:${option.value}`)
      return option
    },
    showCaptchaPrompt: async () => {
      calls.push("prompt")
    },
    awaitOzViewer: async () => {
      calls.push("oz_detected")
      return true
    },
    captureCaptchaImage: async () => null,
    renderCaptchaTerminal: async () => null,
    fillCaptchaAndSubmit: async () => "success",
    closeOzViewer: async () => {
      calls.push("close_oz")
      return true
    },
  }
}

describe("KRAS staged workflow watcher", () => {
  it("classifies the login-expired SweetAlert", () => {
    assert.equal(
      classifyKrasDialogText("로그인이 만료되었습니다. 로그인 후 다시 이용해주세요. OK"),
      "login_expired",
    )
  })

  it("confirms the output-scale warning dialog", () => {
    assert.equal(
      classifyKrasDialogText("열람하고자 하는 지번은 도호가 불일치하여 출력축척에 맞추어 도면을 재설정합니다."),
      "confirm",
    )
  })

  it("detects login redirect URLs", () => {
    assert.equal(isKrasLoginUrl("https://www.kras.go.kr/login/loginView.do?uri=/kras/cert/certView.do"), true)
    assert.equal(isKrasLoginUrl("https://www.kras.go.kr/kras/cert/certView.do"), false)
  })

  it("does not wait for floor/unit options on a general building", () => {
    assert.equal(shouldWaitForFloorRoomOptions("건물", []), false)
    assert.equal(shouldWaitForFloorRoomOptions("건물(집합)", []), true)
    assert.equal(shouldWaitForFloorRoomOptions("건물", [{ index: 1, value: "101", label: "101호" }]), true)
  })

  it("renders CAPTCHA with colored ANSI half-blocks", () => {
    const white = [255, 255, 255, 255]
    const black = [0, 0, 0, 255]
    const rendered = renderCaptchaTerminalPixels([...white, ...black], 1, 2)
    assert.match(rendered, /38;2;255;255;255m\x1b\[48;2;0;0;0m▀/)
    assert.match(rendered, /\x1b\[0m/)
  })

  it("retries building floor/unit loading after a timeout", async () => {
    const calls: string[] = []
    let attempts = 0
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async (value) => {
        calls.push(`select:${value}`)
        attempts += 1
        if (attempts === 1) throw new KrasFloorRoomLoadTimeoutError()
        return []
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      handleFloorRoomOptions: async (_building, status) => {
        if (attempts === 1) {
          assert.equal(status, "timeout")
          return "retry"
        }
        assert.equal(status, "empty")
        return "continue"
      },
    })

    assert.equal(result.stage2?.status, "completed")
    assert.equal(attempts, 2)
    assert.deepEqual(calls, ["select:A", "select:A", "prompt"])
  })

  it("continues without floor/unit after a declined reload", async () => {
    const calls: string[] = []
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async () => {
        throw new KrasFloorRoomLoadTimeoutError()
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      handleFloorRoomOptions: async (_building, status) => {
        assert.equal(status, "timeout")
        return "continue"
      },
    })

    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status === "completed") assert.equal(result.stage2.floorRoom, undefined)
  })

  it("recognizes a Playwright-wrapped browser timeout", async () => {
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A(집합)" }],
    }, [])
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async () => {
        throw new Error("page.evaluate: Error: 층-호명칭 목록 로딩 시간이 초과되었습니다.")
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      handleFloorRoomOptions: async (_building, status) => {
        assert.equal(status, "timeout")
        return "continue"
      },
    })

    assert.equal(result.stage2?.status, "completed")
  })

  it("offers reload when floor/unit information is confirmed absent", async () => {
    const calls: string[] = []
    let attempts = 0
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async (value) => {
        calls.push(`select:${value}`)
        attempts += 1
        return attempts === 1 ? [] : [{ index: 1, value: "101", label: "1층-101호" }]
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      handleFloorRoomOptions: async (_building, status) => {
        assert.equal(status, "empty")
        return "retry"
      },
    })

    assert.equal(result.stage2?.status, "completed")
    assert.equal(attempts, 2)
    assert.deepEqual(calls, ["select:A", "select:A", "floor:101", "prompt"])
  })

  it("allows aggregate building to continue without floor/unit after timeout", async () => {
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A(집합)" }],
    }, [])
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async () => {
        throw new KrasFloorRoomLoadTimeoutError()
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      handleFloorRoomOptions: async (_building, status) => {
        assert.equal(status, "timeout")
        return "continue"
      },
    })
    assert.equal(result.stage2?.status, "completed")
  })

  it("auto-advances one real building and waits for user 열람", async () => {
    const calls: string[] = []
    const saved: KrasAutoStageResult[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page(
        {
          kind: "lookup",
          hasBuilding: true,
          buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
        },
        calls,
      ),
      {
        writeResult: async (_path, stage) => {
          saved.push(stage)
        },
        chooseBuilding: async () => 1,
      },
    )

    assert.equal(result.awaitingUser, false)
    assert.ok(result.stage2 !== undefined)
    assert.equal(result.stage2.status, "completed")
    if (result.stage2.status === "completed") assert.equal(result.stage2.building.label, "건물 A")
    assert.deepEqual(calls, ["select:A", "prompt"])
    assert.deepEqual(saved.map(({ stage }) => stage), [1, 2])
  })

  it("stops before Stage 2 when multiple real buildings exist", async () => {
    const calls: string[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page(
        {
          kind: "lookup",
          hasBuilding: true,
          buildingOptions: [
            { index: 1, value: "A", label: "건물 A" },
            { index: 2, value: "B", label: "건물 B" },
          ],
        },
        calls,
      ),
      { writeResult: async () => undefined },
    )

    assert.equal(result.awaitingUser, true)
    assert.equal(result.stage2, undefined)
    assert.deepEqual(calls, [])
  })

  it("persists no floorRoom when a building offers no floor/unit", async () => {
    const calls: string[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page({
        kind: "lookup",
        hasBuilding: true,
        buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
      }, calls),
      { writeResult: async () => undefined, chooseBuilding: async () => 1 },
    )

    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status !== "completed") throw new Error("Stage 2 should complete")
    assert.equal(result.stage2.floorRoom, undefined)
    assert.deepEqual(calls, ["select:A", "prompt"])
  })

  it("auto-selects and persists one offered floor/unit", async () => {
    const calls: string[] = []
    const floorRoom = { index: 1, value: "101", label: "1층-101호" }
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async (value) => {
        calls.push(`select:${value}`)
        return [floorRoom]
      },
    }, { writeResult: async () => undefined, chooseBuilding: async () => 1 })

    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status !== "completed") throw new Error("Stage 2 should complete")
    assert.deepEqual(result.stage2.floorRoom, floorRoom)
    assert.deepEqual(calls, ["select:A", "floor:101", "prompt"])
  })

  it("blocks with building and floor/unit options when noninteractive selection is ambiguous", async () => {
    const calls: string[] = []
    const floorRoomOptions = [
      { index: 1, value: "101", label: "1층-101호" },
      { index: 2, value: "102", label: "1층-102호" },
    ]
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async (value) => {
        calls.push(`select:${value}`)
        return floorRoomOptions
      },
    }, { writeResult: async () => undefined, chooseBuilding: async () => 1 })

    assert.equal(result.awaitingUser, true)
    assert.deepEqual(result.stage2, {
      version: 1,
      stage: 2,
      status: "blocked",
      address,
      reason: "floor_room_selection_required",
      building: { index: 1, value: "A", label: "건물 A" },
      floorRoomOptions,
    })
    assert.deepEqual(calls, ["select:A"])
  })

  it("selects exact offered floor/unit and rejects stale choice", async () => {
    const calls: string[] = []
    const floorRoomOptions = [
      { index: 1, value: "101", label: "1층-101호" },
      { index: 2, value: "102", label: "1층-102호" },
    ]
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
    }, calls)
    const selectingPage = {
      ...workflowPage,
      selectBuilding: async () => floorRoomOptions,
    }
    let selectedBuilding: KrasBuildingOption | undefined
    const result = await runKrasWorkflowWatcher(address, selectingPage, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      chooseFloorRoom: async (_options, building) => {
        selectedBuilding = building
        return 2
      },
    })
    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status !== "completed") throw new Error("Stage 2 should complete")
    assert.deepEqual(result.stage2.floorRoom, floorRoomOptions[1])
    assert.deepEqual(selectedBuilding, { index: 1, value: "A", label: "건물 A" })

    await assert.rejects(() => runKrasWorkflowWatcher(address, selectingPage, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      chooseFloorRoom: async () => 9,
    }), /유효하지/u)
  })

  it("accepts \"none\" from chooseFloorRoom as 집합 총괄 (no floor selection)", async () => {
    const calls: string[] = []
    const floorRoomOptions = [
      { index: 1, value: "101", label: "1층-101호" },
      { index: 2, value: "102", label: "1층-102호" },
    ]
    const workflowPage = page({
      kind: "lookup",
      hasBuilding: true,
      buildingOptions: [{ index: 1, value: "A", label: "건물 A(집합)" }],
    }, calls)
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      selectBuilding: async (value) => {
        calls.push(`select:${value}`)
        return floorRoomOptions
      },
    }, {
      writeResult: async () => undefined,
      chooseBuilding: async () => 1,
      chooseFloorRoom: async () => "none",
    })
    assert.equal(result.awaitingUser, false)
    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status !== "completed") throw new Error("Stage 2 should complete")
    assert.equal(result.stage2.floorRoom, undefined)
    assert.deepEqual(calls, ["select:A", "prompt"])
  })

  it("re-reads loaded building options after an evaluation error instead of re-submitting 조회", async () => {
    const calls: string[] = []
    let lookupCount = 0
    const workflowPage = page(
      {
        kind: "lookup",
        hasBuilding: true,
        buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
      },
      calls,
    )
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      lookup: async () => {
        lookupCount += 1
        return { kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" }
      },
      readBuildingOptions: async () => [{ index: 1, value: "A", label: "건물 A" }],
    }, {
      writeResult: async () => undefined,
      handleLookupError: async () => "retry",
      chooseBuilding: async () => 1,
    })

    assert.equal(lookupCount, 1)
    assert.equal(result.stage2?.status, "completed")
    assert.deepEqual(calls, ["select:A", "prompt"])
  })

  it("blocks Stage 1 when re-read finds no building options after an error", async () => {
    const calls: string[] = []
    const saved: KrasAutoStageResult[] = []
    const workflowPage = page(
      {
        kind: "lookup",
        hasBuilding: true,
        buildingOptions: [{ index: 1, value: "A", label: "건물 A" }],
      },
      calls,
    )
    const result = await runKrasWorkflowWatcher(address, {
      ...workflowPage,
      lookup: async () => ({ kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" }),
      readBuildingOptions: async () => [],
    }, {
      writeResult: async (_path, stage) => {
        saved.push(stage)
      },
      handleLookupError: async () => "retry",
    })

    assert.equal(result.stage1.status, "blocked")
    assert.equal(result.stage1.reason, "no_building")
    assert.equal(result.stage2, undefined)
    assert.deepEqual(saved.map(({ stage }) => stage), [1])
    assert.deepEqual(calls, [])
  })

  it("continues to manual 열람 after an ignored lookup error", async () => {
    const calls: string[] = []
    const saved: KrasAutoStageResult[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page({ kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" }, calls),
      {
        writeResult: async (_path, stage) => {
          saved.push(stage)
        },
        handleLookupError: async () => "manual",
      },
    )

    assert.equal(result.stage1.status, "blocked")
    assert.equal(result.stage2?.status, "completed")
    if (result.stage2?.status === "completed") {
      assert.deepEqual(result.stage2.building, { index: 0, value: "", label: "" })
    }
    assert.deepEqual(saved.map(({ stage }) => stage), [1, 2])
    assert.deepEqual(calls, ["prompt"])
  })

  it("marks an explicit empty KRAS request as no parcel", async () => {
    const calls: string[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page(
        { kind: "lookup", hasBuilding: false, buildingOptions: [], noParcel: true },
        calls,
      ),
    )

    assert.equal(result.stage1.status, "blocked")
    assert.equal(result.stage1.reason, "no_parcel")
    assert.equal(result.stage2, undefined)
    assert.deepEqual(calls, [])
  })

  it("proceeds to manual 열람 when the user declines building selection", async () => {
    const calls: string[] = []
    const saved: KrasAutoStageResult[] = []
    const result = await runKrasWorkflowWatcher(
      address,
      page(
        {
          kind: "lookup",
          hasBuilding: true,
          buildingOptions: [
            { index: 1, value: "A", label: "건물 A" },
            { index: 2, value: "B", label: "건물 B" },
          ],
        },
        calls,
      ),
      {
        writeResult: async (_path, stage) => {
          saved.push(stage)
        },
        chooseBuilding: async () => undefined,
      },
    )

    assert.equal(result.awaitingUser, false)
    assert.ok(result.stage2 !== undefined)
    assert.equal(result.stage2.status, "completed")
    if (result.stage2.status === "completed") {
      assert.deepEqual(result.stage2.building, { index: 0, value: "", label: "" })
    }
    assert.deepEqual(calls, ["prompt"])
    assert.deepEqual(saved.map(({ stage }) => stage), [1, 2])
  })
})
