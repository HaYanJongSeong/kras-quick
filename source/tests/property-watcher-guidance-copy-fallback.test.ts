import assert from "node:assert/strict"
import { describe, it } from "node:test"

import {
  activateGuidanceButton,
  createGuidanceSurface,
  settleGuidanceCopy,
} from "./property-watcher-guidance-test-support.ts"

const RESULT = "부동산 종합증명서 조회결과 건물 조회 불가"
const FAILURE = "복사하지 못했습니다. 다시 시도하거나 문장을 직접 선택해 주세요."

describe("SCP guidance copy fallback", () => {
  for (const testCase of [
    { name: "an absent Clipboard API", clipboardWrite: undefined },
    {
      name: "a rejected Clipboard API",
      clipboardWrite: async () => Promise.reject(new TypeError("clipboard denied")),
    },
  ] as const) {
    it(`uses and removes a safe temporary textarea for ${testCase.name}`, async () => {
      // Given
      let temporary:
        | {
            readonly active: boolean
            readonly ariaHidden: string | null
            readonly readOnly: string | null
            readonly selected: boolean
            readonly tabIndex: string | null
            readonly value: string
          }
        | undefined
      const surface = createGuidanceSurface({
        ...(testCase.clipboardWrite === undefined
          ? {}
          : { clipboardWrite: testCase.clipboardWrite }),
        executeCopy: (document) => {
          const textarea = document.body.children.find((element) => element.tagName === "TEXTAREA")
          temporary = textarea
            ? {
                active: document.activeElement === textarea,
                ariaHidden: textarea.getAttribute("aria-hidden"),
                readOnly: textarea.getAttribute("readonly"),
                selected: surface.fixture.selectedElement() === textarea,
                tabIndex: textarea.getAttribute("tabindex"),
                value: textarea.value,
              }
            : undefined
          return true
        },
      })
      const button = surface.buttons[0]
      assert.ok(button)
      button.focus()

      // When
      activateGuidanceButton(surface, button, "pointer")
      await settleGuidanceCopy()

      // Then
      assert.deepEqual(temporary, {
        active: true,
        ariaHidden: "true",
        readOnly: "",
        selected: true,
        tabIndex: "-1",
        value: RESULT,
      })
      assert.deepEqual(surface.fixture.document.execCommands, ["copy"])
      assert.equal(surface.fixture.document.activeElement, button)
      assert.equal(surface.fixture.selectedElement(), null)
      assert.equal(
        surface.fixture.document.body.children.some((element) => element.tagName === "TEXTAREA"),
        false,
      )
      assert.deepEqual(surface.fixture.document.focusCalls.at(-1), {
        element: button,
        preventScroll: true,
      })
      assert.equal(surface.feedback.textContent, `복사됨: ${RESULT}`)
    })
  }

  for (const testCase of [
    { name: "false", executeCopy: () => false },
    {
      name: "throw",
      executeCopy: () => {
        throw new TypeError("copy blocked")
      },
    },
  ] as const) {
    it(`reports failure and cleans fallback state when execCommand returns ${testCase.name}`, async () => {
      // Given
      const surface = createGuidanceSurface({ executeCopy: testCase.executeCopy })
      const button = surface.buttons[1]
      assert.ok(button)
      button.focus()

      // When
      activateGuidanceButton(surface, button, "pointer")
      await settleGuidanceCopy()

      // Then
      assert.equal(surface.feedback.textContent, FAILURE)
      assert.equal(surface.fixture.document.activeElement, button)
      assert.equal(surface.fixture.selectedElement(), null)
      assert.equal(
        surface.fixture.document.body.children.some((element) => element.tagName === "TEXTAREA"),
        false,
      )
    })
  }
})
