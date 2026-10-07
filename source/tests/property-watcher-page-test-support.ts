import { type Context, createContext, runInContext } from "node:vm"
import type { PropertyBuildingTerminalStatus } from "../scripts/property-watcher-payload.ts"
import { FakeDocument, FakeElement, FakeTableCellElement } from "./property-watcher-fake-dom.ts"

export { FakeDocument, FakeElement, FakeTableCellElement } from "./property-watcher-fake-dom.ts"

export type IsolatedWorldFixture = {
  readonly context: Context
  readonly document: FakeDocument
  readonly emitted: string[]
  readonly mainWorld: Record<string, unknown>
  readonly selectedElement: () => FakeElement | null
  readonly setUrl: (value: string) => void
}

export type IsolatedWorldOptions = {
  readonly clipboardWrite?: (text: string) => Promise<void>
  readonly executeCopy?: (document: FakeDocument) => boolean
}

export function createIsolatedWorldFixture(
  bindingName: string,
  onBinding?: (payload: string, document: FakeDocument) => void,
  options: IsolatedWorldOptions = {},
): IsolatedWorldFixture {
  const location = { href: "http://scpweb.softgraphy.biz/board" }
  const document = new FakeDocument(options.executeCopy)
  const emitted: string[] = []
  const navigator =
    options.clipboardWrite === undefined ? {} : { clipboard: { writeText: options.clipboardWrite } }
  const sandbox: Record<string, unknown> & {
    getSelection?: () => ReturnType<FakeDocument["getSelection"]>
    top?: unknown
    window?: unknown
  } = {
    document,
    Element: FakeElement,
    Event,
    HTMLElement: FakeElement,
    HTMLTableCellElement: FakeTableCellElement,
    location,
    navigator,
    TextEncoder,
    URL,
  }
  sandbox.window = sandbox
  sandbox.top = sandbox
  sandbox.getSelection = () => document.getSelection()
  sandbox[bindingName] = (payload: string) => {
    emitted.push(payload)
    onBinding?.(payload, document)
  }
  return {
    context: createContext(sandbox),
    document,
    emitted,
    mainWorld: {},
    selectedElement: () => document.getSelectedElement(),
    setUrl: (value) => {
      location.href = value
    },
  }
}

export function runIsolatedSource(source: string, context: Context): void {
  runInContext(source, context)
}

export function updateIsolatedBuildingStatus(
  context: Context,
  selectionId: number,
  status: PropertyBuildingTerminalStatus,
): unknown {
  const invocation = `(() => {
    const state = globalThis["__opencodePropertyWatcherListenerV3"];
    if (state === undefined || typeof state.updateBuildingStatus !== "function") return "missing";
    return state.updateBuildingStatus(${JSON.stringify(selectionId)}, ${JSON.stringify(status)});
  })()`
  return runInContext(invocation, context)
}
