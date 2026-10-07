import type {
  PropertyWatcherBindingEvent,
  PropertyWatcherContextEvent,
  PropertyWatcherFrameEvent,
  PropertyWatcherProtocol,
} from "../scripts/property-watcher-cdp.ts"
import type {
  PropertyBuildingStatus,
  PropertyBuildingStatusUpdateResult,
  PropertyBuildingTerminalStatus,
  PropertyWatcherSelection,
  PropertyWatcherSelectionHandle,
} from "../scripts/property-watcher-payload.ts"

export type {
  PropertyBuildingStatus,
  PropertyBuildingTerminalStatus,
  PropertyWatcherSelection,
  PropertyWatcherSelectionHandle,
}

export const SCP_URL = "http://scpweb.softgraphy.biz/board"
export const SCP_ORIGIN = "http://scpweb.softgraphy.biz"
export const ROOT_FRAME_ID = "root-frame"

export type FakeProtocolState = {
  readonly calls: string[]
  readonly evaluations: number[]
  readonly statusEvaluations: Array<{
    readonly contextId: number
    readonly selectionId: number
    readonly status: PropertyBuildingTerminalStatus
  }>
  bindingName: string | undefined
  scriptIdentifier: string | undefined
  worldName: string | undefined
}

export class FakePropertyWatcherProtocol implements PropertyWatcherProtocol {
  readonly state: FakeProtocolState = {
    calls: [],
    evaluations: [],
    statusEvaluations: [],
    bindingName: undefined,
    scriptIdentifier: undefined,
    worldName: undefined,
  }
  private bindingListener: ((event: PropertyWatcherBindingEvent) => void) | undefined
  private closeListener: (() => void) | undefined
  private contextClearedListener: (() => void) | undefined
  private contextCreatedListener: ((event: PropertyWatcherContextEvent) => void) | undefined
  private contextDestroyedListener: ((contextId: number) => void) | undefined
  private frameListener: ((event: PropertyWatcherFrameEvent) => void) | undefined
  private historyListener: ((event: PropertyWatcherFrameEvent) => void) | undefined
  private statusResult: PropertyBuildingStatusUpdateResult = "applied"

  pageEnable(): Promise<void> {
    this.state.calls.push("Page.enable")
    return Promise.resolve()
  }

  runtimeEnable(): Promise<void> {
    this.state.calls.push("Runtime.enable")
    return Promise.resolve()
  }

  getRootFrame(): Promise<PropertyWatcherFrameEvent> {
    this.state.calls.push("Page.getFrameTree")
    return Promise.resolve({ frameId: ROOT_FRAME_ID, url: SCP_URL })
  }

  addBinding(): Promise<void> {
    this.state.calls.push("Runtime.addBinding")
    this.state.bindingName = "__opencodePropertyWatcherRowV3"
    this.state.worldName = "opencode-property-watcher-v1"
    return Promise.resolve()
  }

  async addListenerScript(): Promise<string> {
    this.state.calls.push("Page.addScriptToEvaluateOnNewDocument")
    this.state.scriptIdentifier = "watcher-script"
    return "watcher-script"
  }

  evaluateCleanup(contextId: number): Promise<void> {
    this.state.calls.push("Runtime.evaluate")
    this.state.evaluations.push(contextId)
    return Promise.resolve()
  }

  evaluateBuildingStatus(
    contextId: number,
    selectionId: number,
    status: PropertyBuildingTerminalStatus,
  ): Promise<PropertyBuildingStatusUpdateResult> {
    this.state.calls.push("Runtime.evaluate:building-status")
    this.state.statusEvaluations.push({ contextId, selectionId, status })
    return Promise.resolve(this.statusResult)
  }

  removeListenerScript(identifier: string): Promise<void> {
    this.state.calls.push(`Page.removeScriptToEvaluateOnNewDocument:${identifier}`)
    return Promise.resolve()
  }

  removeBinding(): Promise<void> {
    this.state.calls.push("Runtime.removeBinding:__opencodePropertyWatcherRowV3")
    return Promise.resolve()
  }

  onBindingCalled(listener: (event: PropertyWatcherBindingEvent) => void): void {
    this.state.calls.push("on:Runtime.bindingCalled")
    this.bindingListener = listener
  }

  offBindingCalled(listener: (event: PropertyWatcherBindingEvent) => void): void {
    this.state.calls.push("off:Runtime.bindingCalled")
    if (this.bindingListener === listener) this.bindingListener = undefined
  }

  onClose(listener: () => void): void {
    this.state.calls.push("on:close")
    this.closeListener = listener
  }

  offClose(listener: () => void): void {
    this.state.calls.push("off:close")
    if (this.closeListener === listener) this.closeListener = undefined
  }

  onContextsCleared(listener: () => void): void {
    this.state.calls.push("on:Runtime.executionContextsCleared")
    this.contextClearedListener = listener
  }

  offContextsCleared(listener: () => void): void {
    this.state.calls.push("off:Runtime.executionContextsCleared")
    if (this.contextClearedListener === listener) this.contextClearedListener = undefined
  }

  onContextCreated(listener: (event: PropertyWatcherContextEvent) => void): void {
    this.state.calls.push("on:Runtime.executionContextCreated")
    this.contextCreatedListener = listener
  }

  offContextCreated(listener: (event: PropertyWatcherContextEvent) => void): void {
    this.state.calls.push("off:Runtime.executionContextCreated")
    if (this.contextCreatedListener === listener) this.contextCreatedListener = undefined
  }

  onContextDestroyed(listener: (contextId: number) => void): void {
    this.state.calls.push("on:Runtime.executionContextDestroyed")
    this.contextDestroyedListener = listener
  }

  offContextDestroyed(listener: (contextId: number) => void): void {
    this.state.calls.push("off:Runtime.executionContextDestroyed")
    if (this.contextDestroyedListener === listener) this.contextDestroyedListener = undefined
  }

  onFrameNavigated(listener: (event: PropertyWatcherFrameEvent) => void): void {
    this.state.calls.push("on:Page.frameNavigated")
    this.frameListener = listener
  }

  offFrameNavigated(listener: (event: PropertyWatcherFrameEvent) => void): void {
    this.state.calls.push("off:Page.frameNavigated")
    if (this.frameListener === listener) this.frameListener = undefined
  }

  onNavigatedWithinDocument(listener: (event: PropertyWatcherFrameEvent) => void): void {
    this.state.calls.push("on:Page.navigatedWithinDocument")
    this.historyListener = listener
  }

  offNavigatedWithinDocument(listener: (event: PropertyWatcherFrameEvent) => void): void {
    this.state.calls.push("off:Page.navigatedWithinDocument")
    if (this.historyListener === listener) this.historyListener = undefined
  }

  emitBinding(event: PropertyWatcherBindingEvent): void {
    this.bindingListener?.(event)
  }

  emitClose(): void {
    this.closeListener?.()
  }

  emitContextsCleared(): void {
    this.contextClearedListener?.()
  }

  emitContextCreated(event: PropertyWatcherContextEvent): void {
    this.contextCreatedListener?.(event)
  }

  emitContextDestroyed(contextId: number): void {
    this.contextDestroyedListener?.(contextId)
  }

  emitFrameNavigated(url: string): void {
    this.frameListener?.({ frameId: ROOT_FRAME_ID, url })
  }

  emitHistory(url: string, frameId = ROOT_FRAME_ID): void {
    this.historyListener?.({ frameId, url })
  }

  setStatusResult(result: PropertyBuildingStatusUpdateResult): void {
    this.statusResult = result
  }
}

export function isolatedContext(
  contextId: number,
  worldName: string,
  frameId = ROOT_FRAME_ID,
): PropertyWatcherContextEvent {
  return {
    contextId,
    frameId,
    name: worldName,
    origin: SCP_ORIGIN,
    type: "isolated",
    uniqueId: `context-${contextId}`,
  }
}

export function validPayload(cells: readonly string[], selectionId = 1, pager = "8/16"): string {
  return JSON.stringify({ version: 3, selectionId, cells, pager })
}
