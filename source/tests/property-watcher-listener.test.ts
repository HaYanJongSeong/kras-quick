import assert from "node:assert/strict"
import { describe, it } from "node:test"

import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_CLEANUP_SOURCE,
  PROPERTY_WATCHER_LISTENER_SOURCE,
} from "../scripts/property-watcher-listener.ts"
import type { PropertyBuildingTerminalStatus } from "../scripts/property-watcher-payload.ts"
import {
  createIsolatedWorldFixture,
  FakeElement,
  FakeTableCellElement,
  runIsolatedSource,
  updateIsolatedBuildingStatus,
} from "./property-watcher-page-test-support.ts"

const ROW = ["1", "공유재산", "토지", "강서구", "개화동", "일반", "주소", "미처리"] as const
const PANEL_ID = "opencode-property-guidance-panel"
const STYLE_ID = "opencode-property-guidance-style"
const LABEL_ID = "opencode-property-guidance-label"
const BUILDING_STATUS_ID = "opencode-property-guidance-building-status"
const FEEDBACK_ID = "opencode-property-guidance-feedback"
const ABSENT_PROMPT =
  "아래 첫 번째 안내를 눌러 ‘부동산 종합증명서 조회결과 건물 조회 불가’를 복사하세요."
const HEADING = "건물이 있는곳이면 일사편리로 부동산 종합증명서 조회를 해보고,"
const GUIDANCE_ITEMS = [
  "건물구분에 건물이 안뜨면 - 부동산 종합증명서 조회결과 건물 조회 불가",
  "건물이 있는데 서울시 소유가 아니면 - 부동산 종합증명서 조회결과 건물 시유재산 아님",
  "소유자가 아예 안 보이면 - 부동산 종합증명서 조회결과 건물 소유 확인 불명",
  "서울시 소유면 - 부동산 종합증명서 조회결과 건물 시유재산 확인",
] as const

function nestedRow(values: readonly string[], tagName = "TD"): FakeElement {
  const cells = values.map((value) => new FakeTableCellElement(` ${value} `, tagName))
  const row = new FakeElement(cells, "TR")
  return new FakeElement([], "SPAN", null, row)
}

function pager(position: string): FakeElement {
  return new FakeElement([
    new FakeElement([], "BUTTON", "이전"),
    new FakeElement([], "SPAN", position),
    new FakeElement([], "BUTTON", "다음"),
    new FakeElement([], "SELECT"),
  ])
}

function createListenerFixture(
  onBinding?: Parameters<typeof createIsolatedWorldFixture>[1],
  pagerPosition = "7 / 16",
): ReturnType<typeof createIsolatedWorldFixture> {
  const fixture = createIsolatedWorldFixture(PROPERTY_WATCHER_BINDING_NAME, onBinding)
  fixture.document.body.append(pager(pagerPosition))
  return fixture
}

function requireElement(element: FakeElement | null, name: string): FakeElement {
  assert.ok(element, `${name} was not created`)
  return element
}

describe("isolated SCP click listener source", () => {
  it("keeps the binding and sentinel absent from the main world", () => {
    // Given
    const fixture = createListenerFixture()

    // When
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // Then
    assert.equal(Reflect.has(fixture.mainWorld, PROPERTY_WATCHER_BINDING_NAME), false)
    assert.deepEqual(Reflect.ownKeys(fixture.mainWorld), [])
    assert.equal(fixture.document.getElementById(PANEL_ID), null)
  })

  it("ignores synthetic clicks without mutating event behavior", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    const event = fixture.document.dispatchClick(nestedRow(ROW), false)

    // Then
    assert.deepEqual(fixture.emitted, [])
    assert.equal(event.defaultPrevented, false)
    assert.equal(event.cancelBubble, false)
    assert.equal(fixture.document.getElementById(PANEL_ID), null)
  })

  it("emits one bounded versioned JSON payload for a trusted nested row click", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // Then
    assert.equal(fixture.emitted.length, 1)
    assert.deepEqual(JSON.parse(fixture.emitted[0] ?? ""), {
      version: 3,
      selectionId: 1,
      cells: ROW,
      pager: "7/16",
    })
    assert.equal(Buffer.byteLength(fixture.emitted[0] ?? "", "utf8") <= 65_536, true)
  })

  it("increments a positive safe selection id on every valid click", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 6), "다음 주소", ROW[7]]), true)

    // Then
    const payloads: unknown[] = fixture.emitted.map((payload) => JSON.parse(payload))
    assert.deepEqual(payloads, [
      { version: 3, selectionId: 1, cells: ROW, pager: "7/16" },
      {
        version: 3,
        selectionId: 2,
        cells: [...ROW.slice(0, 6), "다음 주소", ROW[7]],
        pager: "7/16",
      },
    ])
    for (const payload of payloads) {
      assert.equal(typeof payload === "object" && payload !== null, true)
      if (typeof payload !== "object" || payload === null) continue
      assert.deepEqual(Reflect.ownKeys(payload), ["version", "selectionId", "cells", "pager"])
      const selectionId = Reflect.get(payload, "selectionId")
      assert.equal(
        typeof selectionId === "number" && Number.isSafeInteger(selectionId) && selectionId > 0,
        true,
      )
    }
  })

  it("renders pending synchronously for the latest selection before calling the binding", () => {
    // Given
    const statusesAtBinding: Array<string | null> = []
    const fixture = createListenerFixture((_payload, document) => {
      statusesAtBinding.push(document.getElementById(BUILDING_STATUS_ID)?.textContent ?? null)
    })
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 6), "다음 주소", ROW[7]]), true)

    // Then
    assert.deepEqual(statusesAtBinding, ["건물 확인 대기중", "건물 확인 대기중"])
  })

  it("renders the present state with its emphasized semantic treatment", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // When
    const result = updateIsolatedBuildingStatus(fixture.context, 1, "present")

    // Then
    const buildingStatus = requireElement(
      fixture.document.getElementById(BUILDING_STATUS_ID),
      "building status",
    )
    assert.equal(result, "applied")
    assert.equal(buildingStatus.textContent, "건물 있음")
    assert.equal(
      buildingStatus.getAttribute("class"),
      "opencode-property-guidance-building-status--present",
    )
    assert.equal(fixture.document.getElementById(FEEDBACK_ID)?.textContent, "")
  })

  it("shows the first-control copy prompt only for the absent state", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // When
    const result = updateIsolatedBuildingStatus(fixture.context, 1, "absent")

    // Then
    const buildingStatus = requireElement(
      fixture.document.getElementById(BUILDING_STATUS_ID),
      "building status",
    )
    assert.equal(result, "applied")
    assert.equal(buildingStatus.textContent, "건물 없음")
    assert.equal(
      buildingStatus.getAttribute("class"),
      "opencode-property-guidance-building-status--absent",
    )
    assert.equal(fixture.document.getElementById(FEEDBACK_ID)?.textContent, ABSENT_PROMPT)
    assert.deepEqual(fixture.document.execCommands, [])
  })

  for (const status of ["present", "failed"] as const) {
    it(`does not show the absent copy prompt for the ${status} state`, () => {
      // Given
      const fixture = createListenerFixture()
      runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
      fixture.document.dispatchClick(nestedRow(ROW), true)
      updateIsolatedBuildingStatus(fixture.context, 1, "absent")

      // When
      updateIsolatedBuildingStatus(fixture.context, 1, status)

      // Then
      assert.equal(fixture.document.getElementById(FEEDBACK_ID)?.textContent, "")
    })
  }

  it("resets absent guidance to pending on the next valid selection", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)
    updateIsolatedBuildingStatus(fixture.context, 1, "absent")

    // When
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 6), "다음 주소", ROW[7]]), true)

    // Then
    const buildingStatus = requireElement(
      fixture.document.getElementById(BUILDING_STATUS_ID),
      "building status",
    )
    assert.equal(buildingStatus.textContent, "건물 확인 대기중")
    assert.equal(
      buildingStatus.getAttribute("class"),
      "opencode-property-guidance-building-status--pending",
    )
    assert.equal(fixture.document.getElementById(FEEDBACK_ID)?.textContent, "")
  })

  for (const [status, label] of [
    ["present", "건물 있음"],
    ["absent", "건물 없음"],
    ["failed", "건물 확인 실패"],
  ] as const satisfies ReadonlyArray<readonly [PropertyBuildingTerminalStatus, string]>) {
    it(`renders the ${status} terminal building status for the current selection`, () => {
      // Given
      const fixture = createListenerFixture()
      runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
      fixture.document.dispatchClick(nestedRow(ROW), true)

      // When
      const result = updateIsolatedBuildingStatus(fixture.context, 1, status)

      // Then
      assert.equal(result, "applied")
      assert.equal(fixture.document.getElementById(BUILDING_STATUS_ID)?.textContent, label)
    })
  }

  it("rejects a stale absent update without overwriting the latest status or prompt", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 6), "다음 주소", ROW[7]]), true)
    updateIsolatedBuildingStatus(fixture.context, 2, "present")

    // When
    const staleResult = updateIsolatedBuildingStatus(fixture.context, 1, "absent")

    // Then
    assert.equal(staleResult, "stale")
    assert.equal(fixture.document.getElementById(BUILDING_STATUS_ID)?.textContent, "건물 있음")
    assert.equal(
      fixture.document.getElementById(BUILDING_STATUS_ID)?.getAttribute("class"),
      "opencode-property-guidance-building-status--present",
    )
    assert.equal(fixture.document.getElementById(FEEDBACK_ID)?.textContent, "")
  })

  it("creates one accessible guidance panel and updates its address before binding", () => {
    // Given
    const bindingAddresses: Array<string | null> = []
    const fixture = createListenerFixture((_payload, document) => {
      bindingAddresses.push(
        document.getElementById("opencode-property-guidance-address")?.textContent ?? null,
      )
    })
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // Then
    const panel = requireElement(fixture.document.getElementById(PANEL_ID), "guidance panel")
    const [label, address, , heading, list, feedback] = panel.children
    assert.equal(panel.tagName, "ASIDE")
    assert.equal(panel.getAttribute("aria-labelledby"), "opencode-property-guidance-heading")
    assert.equal(label?.textContent, "현재 지번 주소 (7/16)")
    assert.deepEqual(bindingAddresses, ["주소"])
    assert.equal(address?.getAttribute("role"), "status")
    assert.equal(address?.getAttribute("aria-live"), "polite")
    assert.equal(address?.getAttribute("aria-atomic"), "true")
    assert.equal(heading?.tagName, "H2")
    assert.equal(heading?.getAttribute("id"), "opencode-property-guidance-heading")
    assert.equal(heading?.textContent, HEADING)
    assert.equal(list?.tagName, "UL")
    assert.deepEqual(
      list?.children.map((item) => item.children[0]?.textContent),
      GUIDANCE_ITEMS,
    )
    assert.equal(feedback?.getAttribute("role"), "status")
    assert.equal(feedback?.getAttribute("aria-live"), "polite")
    assert.equal(feedback?.getAttribute("aria-atomic"), "true")
    assert.equal(fixture.document.elementsById(PANEL_ID).length, 1)
    assert.equal(fixture.document.elementsById(STYLE_ID).length, 1)
  })

  it("reads and normalizes the unique SCP pager at click time without retaining stale position", () => {
    // Given
    const labelsAtBinding: Array<string | null> = []
    const fixture = createListenerFixture((_payload, document) => {
      labelsAtBinding.push(document.getElementById(LABEL_ID)?.textContent ?? null)
    }, " 6 / 16 ")
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)
    fixture.document.body.append(pager("7 / 16"))
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 6), "다음 주소", ROW[7]]), true)

    // Then
    assert.deepEqual(labelsAtBinding, ["현재 지번 주소 (6/16)"])
    assert.equal(fixture.emitted.length, 1)
    assert.equal(
      fixture.document.getElementById("opencode-property-guidance-address")?.textContent,
      "주소",
    )
  })

  it("does not emit or mutate panel state when pager resolution is missing or invalid", () => {
    // Given
    const missing = createIsolatedWorldFixture(PROPERTY_WATCHER_BINDING_NAME)
    const invalid = createIsolatedWorldFixture(PROPERTY_WATCHER_BINDING_NAME)
    invalid.document.body.append(pager("17 / 16"))
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, missing.context)
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, invalid.context)

    // When
    missing.document.dispatchClick(nestedRow(ROW), true)
    invalid.document.dispatchClick(nestedRow(ROW), true)

    // Then
    assert.deepEqual(missing.emitted, [])
    assert.deepEqual(invalid.emitted, [])
    assert.equal(missing.document.getElementById(PANEL_ID), null)
    assert.equal(invalid.document.getElementById(PANEL_ID), null)
  })

  it("keeps panel styling responsive, scoped, static, and dark-mode aware", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // Then
    const css = requireElement(fixture.document.getElementById(STYLE_ID), "panel style").textContent
    assert.match(css ?? "", /#opencode-property-guidance-panel\s*\{/)
    assert.match(css ?? "", /position:\s*fixed/)
    assert.match(css ?? "", /pointer-events:\s*auto/)
    assert.match(css ?? "", /user-select:\s*text/)
    assert.match(css ?? "", /@media\s*\(max-width:\s*767px\)/)
    assert.match(css ?? "", /@media\s*\(prefers-color-scheme:\s*dark\)/)
    assert.match(css ?? "", /:has\(#opencode-dark-mode\).*#opencode-property-guidance-panel/)
    assert.match(css ?? "", /word-break:\s*keep-all/)
    assert.match(css ?? "", /overflow-wrap:\s*break-word/)
    assert.doesNotMatch(css ?? "", /overflow-wrap:\s*anywhere/)
    assert.match(css ?? "", /#opencode-property-guidance-heading\s*\{[^}]*text-wrap:\s*balance/s)
    for (const color of [
      "#fff",
      "#f6f8fa",
      "#1f2328",
      "#57606a",
      "#d0d7de",
      "#292b2f",
      "#33363b",
      "#e8eaed",
      "#b6bbc2",
      "#555b63",
      "#087f5b",
      "#7ee2c0",
    ]) {
      assert.match(css ?? "", new RegExp(color))
    }
    assert.doesNotMatch(css ?? "", /animation|gradient|backdrop|filter\s*:|url\(/i)
    assert.doesNotMatch(css ?? "", /z-index:\s*2147483647/)
    assert.match(
      css ?? "",
      /#opencode-property-guidance-building-status\.opencode-property-guidance-building-status--present\s*\{[^}]*color:\s*var\(--opencode-guidance-present\)[^}]*font-size:\s*18px[^}]*font-weight:\s*700/s,
    )
  })

  it("updates only the address on later valid clicks and treats markup as text", () => {
    // Given
    const bindingAddresses: Array<string | null> = []
    const fixture = createListenerFixture((_payload, document) => {
      bindingAddresses.push(
        document.getElementById("opencode-property-guidance-address")?.textContent ?? null,
      )
    })
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)
    const panel = requireElement(fixture.document.getElementById(PANEL_ID), "guidance panel")
    const style = requireElement(fixture.document.getElementById(STYLE_ID), "panel style")
    const address = requireElement(
      fixture.document.getElementById("opencode-property-guidance-address"),
      "address status",
    )

    // When
    fixture.document.dispatchClick(
      nestedRow([...ROW.slice(0, 6), " <img src=x onerror=alert(1)> ", ROW[7]]),
      true,
    )

    // Then
    assert.equal(address.textContent, "<img src=x onerror=alert(1)>")
    assert.deepEqual(bindingAddresses, ["주소", "<img src=x onerror=alert(1)>"])
    assert.equal(address.children.length, 0)
    assert.equal(fixture.document.getElementById(PANEL_ID), panel)
    assert.equal(fixture.document.getElementById(STYLE_ID), style)
    assert.equal(fixture.document.elementsById(PANEL_ID).length, 1)
    assert.equal(fixture.document.elementsById(STYLE_ID).length, 1)
  })

  it("rejects wrong URL, wrong direct-cell type, wrong tuple size, and oversized cells", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.setUrl("http://scpweb.softgraphy.biz/board/detail/1")
    fixture.document.dispatchClick(nestedRow(ROW), true)
    fixture.setUrl("http://scpweb.softgraphy.biz/board")
    fixture.document.dispatchClick(nestedRow(ROW, "TH"), true)
    fixture.document.dispatchClick(nestedRow(ROW.slice(0, 7)), true)
    fixture.document.dispatchClick(nestedRow([...ROW.slice(0, 7), "x".repeat(4097)]), true)

    // Then
    assert.deepEqual(fixture.emitted, [])
    assert.equal(fixture.document.getElementById(PANEL_ID), null)
  })

  it("rejects payloads over the byte limit without creating the panel", () => {
    // Given
    const fixture = createListenerFixture()
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)

    // When
    fixture.document.dispatchClick(
      nestedRow(Array.from({ length: 8 }, () => "가".repeat(4096))),
      true,
    )

    // Then
    assert.deepEqual(fixture.emitted, [])
    assert.equal(fixture.document.getElementById(PANEL_ID), null)
  })

  it("installs idempotently and fixed cleanup removes only the watcher listener", () => {
    // Given
    const fixture = createListenerFixture()
    const unrelated: EventListener = () => undefined
    const unrelatedStyle = fixture.document.createElement("style")
    unrelatedStyle.setAttribute("id", "unrelated-style")
    fixture.document.head.append(unrelatedStyle)
    fixture.document.addEventListener("click", unrelated)
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    fixture.document.dispatchClick(nestedRow(ROW), true)

    // When
    runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
    runIsolatedSource(PROPERTY_WATCHER_CLEANUP_SOURCE, fixture.context)
    runIsolatedSource(PROPERTY_WATCHER_CLEANUP_SOURCE, fixture.context)

    // Then
    assert.equal(fixture.document.listeners.size, 1)
    assert.equal(fixture.document.listeners.has(unrelated), true)
    assert.equal(fixture.document.addCalls, 2)
    assert.equal(fixture.document.removeCalls, 1)
    assert.deepEqual(fixture.document.addCaptures, [false, true])
    assert.deepEqual(fixture.document.removeCaptures, [true])
    assert.equal(fixture.document.getElementById(PANEL_ID), null)
    assert.equal(fixture.document.getElementById(STYLE_ID), null)
    assert.equal(fixture.document.getElementById("unrelated-style"), unrelatedStyle)
  })
})
