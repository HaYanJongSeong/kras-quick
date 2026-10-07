import assert from "node:assert/strict"

import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_LISTENER_SOURCE,
} from "../scripts/property-watcher-listener.ts"
import {
  createIsolatedWorldFixture,
  FakeElement,
  FakeTableCellElement,
  type IsolatedWorldFixture,
  type IsolatedWorldOptions,
  runIsolatedSource,
} from "./property-watcher-page-test-support.ts"

export const GUIDANCE_ROW = [
  "1",
  "공유재산",
  "토지",
  "강서구",
  "개화동",
  "일반",
  "주소",
  "미처리",
] as const

export type GuidanceSurface = {
  readonly buttons: readonly FakeElement[]
  readonly feedback: FakeElement
  readonly fixture: IsolatedWorldFixture
  readonly list: FakeElement
  readonly panel: FakeElement
}

export function nestedGuidanceRow(values: readonly string[] = GUIDANCE_ROW): FakeElement {
  const cells = values.map((value) => new FakeTableCellElement(` ${value} `))
  const row = new FakeElement(cells, "TR")
  return new FakeElement([], "SPAN", null, row)
}

export function requireGuidanceElement(
  element: FakeElement | null | undefined,
  name: string,
): FakeElement {
  assert.ok(element, `${name} was not created`)
  return element
}

export function createGuidanceSurface(options: IsolatedWorldOptions = {}): GuidanceSurface {
  const fixture = createIsolatedWorldFixture(PROPERTY_WATCHER_BINDING_NAME, undefined, options)
  fixture.document.body.append(
    new FakeElement([
      new FakeElement([], "BUTTON", "이전"),
      new FakeElement([], "SPAN", "8 / 16"),
      new FakeElement([], "BUTTON", "다음"),
      new FakeElement([], "SELECT"),
    ]),
  )
  runIsolatedSource(PROPERTY_WATCHER_LISTENER_SOURCE, fixture.context)
  fixture.document.dispatchClick(nestedGuidanceRow(), true)
  const panel = requireGuidanceElement(
    fixture.document.getElementById("opencode-property-guidance-panel"),
    "guidance panel",
  )
  const list = requireGuidanceElement(panel.children[4], "guidance list")
  const feedback = requireGuidanceElement(panel.children[5], "copy feedback")
  const buttons = list.children.map((item, index) =>
    requireGuidanceElement(item.children[0], `guidance button ${index}`),
  )
  return { buttons, feedback, fixture, list, panel }
}

export function activateGuidanceButton(
  surface: GuidanceSurface,
  button: FakeElement,
  activation: "pointer" | "Enter" | "Space",
  trusted = true,
): Event {
  void activation
  return surface.fixture.document.dispatchClick(button, trusted)
}

export async function settleGuidanceCopy(): Promise<void> {
  await new Promise<void>((resolve) => setImmediate(resolve))
}
