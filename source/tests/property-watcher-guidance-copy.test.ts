import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { PROPERTY_WATCHER_LISTENER_SOURCE } from "../scripts/property-watcher-listener.ts"
import {
  activateGuidanceButton,
  createGuidanceSurface,
  GUIDANCE_ROW,
  nestedGuidanceRow,
  requireGuidanceElement,
  settleGuidanceCopy,
} from "./property-watcher-guidance-test-support.ts"

const HEADING = "건물이 있는곳이면 일사편리로 부동산 종합증명서 조회를 해보고,"
const GUIDANCE = [
  [
    "건물구분에 건물이 안뜨면 - 부동산 종합증명서 조회결과 건물 조회 불가",
    "부동산 종합증명서 조회결과 건물 조회 불가",
  ],
  [
    "건물이 있는데 서울시 소유가 아니면 - 부동산 종합증명서 조회결과 건물 시유재산 아님",
    "부동산 종합증명서 조회결과 건물 시유재산 아님",
  ],
  [
    "소유자가 아예 안 보이면 - 부동산 종합증명서 조회결과 건물 소유 확인 불명",
    "부동산 종합증명서 조회결과 건물 소유 확인 불명",
  ],
  [
    "서울시 소유면 - 부동산 종합증명서 조회결과 건물 시유재산 확인",
    "부동산 종합증명서 조회결과 건물 시유재산 확인",
  ],
] as const

function guidanceCss(): string {
  const surface = createGuidanceSurface()
  const style = requireGuidanceElement(
    surface.fixture.document.getElementById("opencode-property-guidance-style"),
    "guidance style",
  )
  return style.textContent ?? ""
}

describe("SCP guidance result copy", () => {
  it("renders the exact guidance as four semantic controls with an accessible feedback region", () => {
    // Given / When
    const surface = createGuidanceSurface()
    const [, address, buildingStatus, heading] = surface.panel.children

    // Then
    assert.equal(address?.textContent, "주소")
    assert.equal(buildingStatus?.textContent, "건물 확인 대기중")
    assert.equal(heading?.textContent, HEADING)
    assert.equal(surface.list.children.length, 4)
    assert.deepEqual(
      surface.buttons.map((button) => button.textContent),
      GUIDANCE.map(([sentence]) => sentence),
    )
    for (const [index, item] of surface.list.children.entries()) {
      assert.equal(item.tagName, "LI")
      assert.equal(item.children.length, 1)
      assert.equal(surface.buttons[index]?.tagName, "BUTTON")
      assert.equal(surface.buttons[index]?.getAttribute("type"), "button")
    }
    assert.equal(surface.feedback.getAttribute("role"), "status")
    assert.equal(surface.feedback.getAttribute("aria-live"), "polite")
    assert.equal(surface.feedback.getAttribute("aria-atomic"), "true")
  })

  it("copies each fixed result through the Clipboard API and reports exact success", async () => {
    // Given
    const copied: string[] = []
    const surface = createGuidanceSurface({
      clipboardWrite: async (text) => {
        copied.push(text)
      },
      executeCopy: () => {
        throw new TypeError("fallback must not run")
      },
    })

    // When
    for (const button of surface.buttons) {
      activateGuidanceButton(surface, button, "pointer")
      await settleGuidanceCopy()
    }

    // Then
    assert.deepEqual(
      copied,
      GUIDANCE.map(([, result]) => result),
    )
    assert.equal(surface.feedback.textContent, `복사됨: ${GUIDANCE[3][1]}`)
    assert.deepEqual(surface.fixture.document.execCommands, [])
  })

  it("ignores untrusted activation and a trusted activation overlapping drag selection", async () => {
    // Given
    const copied: string[] = []
    const surface = createGuidanceSurface({
      clipboardWrite: async (text) => {
        copied.push(text)
      },
    })
    const button = surface.buttons[2]
    assert.ok(button)

    // When
    activateGuidanceButton(surface, button, "pointer", false)
    surface.fixture.document.setSelection(button, false)
    activateGuidanceButton(surface, button, "pointer")
    await settleGuidanceCopy()

    // Then
    assert.deepEqual(copied, [])
    assert.equal(surface.feedback.textContent, "")
  })

  for (const activation of ["Enter", "Space"] as const) {
    it(`uses native ${activation} button activation without a keydown listener`, async () => {
      // Given
      const copied: string[] = []
      const surface = createGuidanceSurface({
        clipboardWrite: async (text) => {
          copied.push(text)
        },
      })
      const button = surface.buttons[3]
      assert.ok(button)

      // When
      activateGuidanceButton(surface, button, activation)
      await settleGuidanceCopy()

      // Then
      assert.deepEqual(copied, [GUIDANCE[3][1]])
      assert.equal(surface.fixture.document.addCalls, 1)
    })
  }

  it("does not emit a watcher payload for copy and preserves later row payload and lifecycle", async () => {
    // Given
    const surface = createGuidanceSurface({ clipboardWrite: async () => undefined })
    const panel = surface.panel
    const style = requireGuidanceElement(
      surface.fixture.document.getElementById("opencode-property-guidance-style"),
      "guidance style",
    )
    surface.fixture.emitted.length = 0
    const button = surface.buttons[0]
    assert.ok(button)

    // When
    activateGuidanceButton(surface, button, "pointer")
    await settleGuidanceCopy()
    surface.fixture.document.dispatchClick(nestedGuidanceRow(), true)

    // Then
    assert.equal(surface.fixture.emitted.length, 1)
    assert.deepEqual(JSON.parse(surface.fixture.emitted[0] ?? ""), {
      version: 3,
      selectionId: 2,
      cells: GUIDANCE_ROW,
      pager: "8/16",
    })
    assert.equal(surface.fixture.document.getElementById("opencode-property-guidance-panel"), panel)
    assert.equal(surface.fixture.document.getElementById("opencode-property-guidance-style"), style)
    assert.equal(
      surface.fixture.document.elementsById("opencode-property-guidance-panel").length,
      1,
    )
    assert.equal(
      surface.fixture.document.elementsById("opencode-property-guidance-style").length,
      1,
    )
  })

  it("aligns selectable copy controls and shows a persistent token-based affordance", () => {
    // Given / When
    const css = guidanceCss()

    // Then
    assert.match(css ?? "", /pointer-events:\s*auto/)
    assert.match(css ?? "", /user-select:\s*text/)
    assert.match(css ?? "", /-webkit-user-select:\s*text/)
    assert.match(css ?? "", /button\s*\{[^}]*background:\s*transparent/s)
    assert.match(css ?? "", /button\s*\{[^}]*display:\s*block/s)
    assert.match(css ?? "", /button\s*\{[^}]*inline-size:\s*100%/s)
    assert.match(css ?? "", /button\s*\{[^}]*cursor:\s*copy/s)
    assert.match(css ?? "", /button\s*\{[^}]*text-decoration-line:\s*underline/s)
    assert.match(css ?? "", /button\s*\{[^}]*text-decoration-style:\s*dotted/s)
    assert.match(
      css ?? "",
      /button\s*\{[^}]*text-decoration-color:\s*var\(--opencode-guidance-muted\)/s,
    )
    assert.match(css ?? "", /button\s*\{[^}]*text-underline-offset:\s*4px/s)
    assert.doesNotMatch(css ?? "", /animation|gradient|backdrop|filter\s*:|url\(/i)
  })

  it("uses high-contrast focus and a hidden-until-needed raised feedback state", () => {
    // Given / When
    const css = guidanceCss()

    // Then
    assert.match(
      css,
      /button:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--opencode-guidance-text\)/s,
    )
    assert.match(css, /button:focus-visible\s*\{[^}]*outline-offset:\s*2px/s)
    assert.match(css, /button:focus-visible\s*\{[^}]*border-radius:\s*4px/s)
    assert.match(css, /#opencode-property-guidance-feedback:empty\s*\{[^}]*display:\s*none/s)
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*background-color:\s*var\(--opencode-guidance-raised\)/s,
    )
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*color:\s*var\(--opencode-guidance-text\)/s,
    )
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*padding:\s*8px/s,
    )
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*border-radius:\s*4px/s,
    )
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*font-weight:\s*600/s,
    )
    assert.match(
      css ?? "",
      /#opencode-property-guidance-feedback:not\(:empty\)\s*\{[^}]*text-wrap:\s*balance/s,
    )
  })

  it("contains no popup, dialog, window, or tab-opening path", () => {
    assert.doesNotMatch(
      PROPERTY_WATCHER_LISTENER_SOURCE,
      /window\.open|open\s*\(|showModal|<dialog|target\s*=\s*["']_blank["']/i,
    )
  })
})
