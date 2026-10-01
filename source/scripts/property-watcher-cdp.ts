import type { CDPSession } from "playwright-core"

import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_CLEANUP_SOURCE,
  PROPERTY_WATCHER_LISTENER_SOURCE,
  PROPERTY_WATCHER_WORLD_NAME,
} from "./property-watcher-listener.ts"
import type {
  PropertyBuildingStatusUpdateResult,
  PropertyBuildingTerminalStatus,
} from "./property-watcher-payload.ts"

export type PropertyWatcherProtocolErrorCode =
  | "STATUS_EVALUATION_EXCEPTION"
  | "STATUS_EVALUATION_RESULT_INVALID"

const PROTOCOL_ERROR_MESSAGES = {
  STATUS_EVALUATION_EXCEPTION: "SCP building status evaluation failed.",
  STATUS_EVALUATION_RESULT_INVALID: "SCP building status evaluation returned an invalid result.",
} as const satisfies Record<PropertyWatcherProtocolErrorCode, string>

export class PropertyWatcherProtocolError extends Error {
  override readonly name = "PropertyWatcherProtocolError"
  readonly code: PropertyWatcherProtocolErrorCode

  constructor(code: PropertyWatcherProtocolErrorCode) {
    super(PROTOCOL_ERROR_MESSAGES[code])
    this.code = code
  }
}

export function parsePropertyBuildingStatusEvaluation(
  exceptionDetails: unknown,
  value: unknown,
): PropertyBuildingStatusUpdateResult {
  if (exceptionDetails !== undefined) {
    throw new PropertyWatcherProtocolError("STATUS_EVALUATION_EXCEPTION")
  }
  if (value === "applied" || value === "stale") return value
  throw new PropertyWatcherProtocolError("STATUS_EVALUATION_RESULT_INVALID")
}

export type PropertyWatcherBindingEvent = {
  readonly contextId: number
  readonly name: string
  readonly payload: string
}

export type PropertyWatcherContextEvent = {
  readonly contextId: number
  readonly frameId: string | undefined
  readonly name: string
  readonly origin: string
  readonly type: string | undefined
  readonly uniqueId: string
}

export type PropertyWatcherFrameEvent = {
  readonly frameId: string
  readonly url: string
}

export type PropertyWatcherProtocol = {
  readonly pageEnable: () => Promise<void>
  readonly runtimeEnable: () => Promise<void>
  readonly getRootFrame: () => Promise<PropertyWatcherFrameEvent>
  readonly addBinding: () => Promise<void>
  readonly addListenerScript: () => Promise<string>
  readonly evaluateCleanup: (contextId: number) => Promise<void>
  readonly evaluateBuildingStatus: (
    contextId: number,
    selectionId: number,
    status: PropertyBuildingTerminalStatus,
  ) => Promise<PropertyBuildingStatusUpdateResult>
  readonly removeListenerScript: (identifier: string) => Promise<void>
  readonly removeBinding: () => Promise<void>
  readonly onBindingCalled: (listener: (event: PropertyWatcherBindingEvent) => void) => void
  readonly offBindingCalled: (listener: (event: PropertyWatcherBindingEvent) => void) => void
  readonly onClose: (listener: () => void) => void
  readonly offClose: (listener: () => void) => void
  readonly onContextsCleared: (listener: () => void) => void
  readonly offContextsCleared: (listener: () => void) => void
  readonly onContextCreated: (listener: (event: PropertyWatcherContextEvent) => void) => void
  readonly offContextCreated: (listener: (event: PropertyWatcherContextEvent) => void) => void
  readonly onContextDestroyed: (listener: (contextId: number) => void) => void
  readonly offContextDestroyed: (listener: (contextId: number) => void) => void
  readonly onFrameNavigated: (listener: (event: PropertyWatcherFrameEvent) => void) => void
  readonly offFrameNavigated: (listener: (event: PropertyWatcherFrameEvent) => void) => void
  readonly onNavigatedWithinDocument: (listener: (event: PropertyWatcherFrameEvent) => void) => void
  readonly offNavigatedWithinDocument: (
    listener: (event: PropertyWatcherFrameEvent) => void,
  ) => void
}

export function createPropertyWatcherProtocol(session: CDPSession): PropertyWatcherProtocol {
  const bindingHandlers = new Map<
    (event: PropertyWatcherBindingEvent) => void,
    (event: {
      readonly executionContextId: number
      readonly name: string
      readonly payload: string
    }) => void
  >()
  const closeHandlers = new Map<() => void, (session: CDPSession) => void>()
  const contextHandlers = new Map<
    (event: PropertyWatcherContextEvent) => void,
    Parameters<typeof session.on<"Runtime.executionContextCreated">>[1]
  >()
  const destroyedHandlers = new Map<
    (contextId: number) => void,
    Parameters<typeof session.on<"Runtime.executionContextDestroyed">>[1]
  >()
  const frameHandlers = new Map<
    (event: PropertyWatcherFrameEvent) => void,
    Parameters<typeof session.on<"Page.frameNavigated">>[1]
  >()
  const historyHandlers = new Map<
    (event: PropertyWatcherFrameEvent) => void,
    Parameters<typeof session.on<"Page.navigatedWithinDocument">>[1]
  >()

  return {
    pageEnable: async () => {
      await session.send("Page.enable")
    },
    runtimeEnable: async () => {
      await session.send("Runtime.enable")
    },
    getRootFrame: async () => {
      const result = await session.send("Page.getFrameTree")
      return { frameId: result.frameTree.frame.id, url: result.frameTree.frame.url }
    },
    addBinding: async () => {
      await session.send("Runtime.addBinding", {
        name: PROPERTY_WATCHER_BINDING_NAME,
        executionContextName: PROPERTY_WATCHER_WORLD_NAME,
      })
    },
    addListenerScript: async () => {
      const result = await session.send("Page.addScriptToEvaluateOnNewDocument", {
        source: PROPERTY_WATCHER_LISTENER_SOURCE,
        worldName: PROPERTY_WATCHER_WORLD_NAME,
        runImmediately: true,
      })
      return result.identifier
    },
    evaluateCleanup: async (contextId) => {
      await session.send("Runtime.evaluate", {
        expression: PROPERTY_WATCHER_CLEANUP_SOURCE,
        contextId,
        returnByValue: true,
      })
    },
    evaluateBuildingStatus: async (contextId, selectionId, status) => {
      const result = await session.send("Runtime.evaluate", {
        expression: `globalThis.__opencodePropertyWatcherListenerV3.updateBuildingStatus(${JSON.stringify(selectionId)}, ${JSON.stringify(status)})`,
        contextId,
        returnByValue: true,
      })
      const value: unknown = result.result.value
      return parsePropertyBuildingStatusEvaluation(result.exceptionDetails, value)
    },
    removeListenerScript: async (identifier) => {
      await session.send("Page.removeScriptToEvaluateOnNewDocument", { identifier })
    },
    removeBinding: async () => {
      await session.send("Runtime.removeBinding", { name: PROPERTY_WATCHER_BINDING_NAME })
    },
    onBindingCalled: (listener) => {
      const handler = (event: {
        readonly executionContextId: number
        readonly name: string
        readonly payload: string
      }): void =>
        listener({
          contextId: event.executionContextId,
          name: event.name,
          payload: event.payload,
        })
      bindingHandlers.set(listener, handler)
      session.on("Runtime.bindingCalled", handler)
    },
    offBindingCalled: (listener) => {
      const handler = bindingHandlers.get(listener)
      if (handler === undefined) return
      session.off("Runtime.bindingCalled", handler)
      bindingHandlers.delete(listener)
    },
    onClose: (listener) => {
      const handler = (): void => listener()
      closeHandlers.set(listener, handler)
      session.on("close", handler)
    },
    offClose: (listener) => {
      const handler = closeHandlers.get(listener)
      if (handler === undefined) return
      session.off("close", handler)
      closeHandlers.delete(listener)
    },
    onContextsCleared: (listener) => session.on("Runtime.executionContextsCleared", listener),
    offContextsCleared: (listener) => session.off("Runtime.executionContextsCleared", listener),
    onContextCreated: (listener) => {
      const handler: Parameters<typeof session.on<"Runtime.executionContextCreated">>[1] = ({
        context,
      }) => {
        const { frameId, type } = context.auxData ?? {}
        listener({
          contextId: context.id,
          frameId,
          name: context.name,
          origin: context.origin,
          type,
          uniqueId: context.uniqueId,
        })
      }
      contextHandlers.set(listener, handler)
      session.on("Runtime.executionContextCreated", handler)
    },
    offContextCreated: (listener) => {
      const handler = contextHandlers.get(listener)
      if (handler === undefined) return
      session.off("Runtime.executionContextCreated", handler)
      contextHandlers.delete(listener)
    },
    onContextDestroyed: (listener) => {
      const handler: Parameters<typeof session.on<"Runtime.executionContextDestroyed">>[1] = (
        event,
      ) => listener(event.executionContextId)
      destroyedHandlers.set(listener, handler)
      session.on("Runtime.executionContextDestroyed", handler)
    },
    offContextDestroyed: (listener) => {
      const handler = destroyedHandlers.get(listener)
      if (handler === undefined) return
      session.off("Runtime.executionContextDestroyed", handler)
      destroyedHandlers.delete(listener)
    },
    onFrameNavigated: (listener) => {
      const handler: Parameters<typeof session.on<"Page.frameNavigated">>[1] = ({ frame }) =>
        listener({ frameId: frame.id, url: frame.url })
      frameHandlers.set(listener, handler)
      session.on("Page.frameNavigated", handler)
    },
    offFrameNavigated: (listener) => {
      const handler = frameHandlers.get(listener)
      if (handler === undefined) return
      session.off("Page.frameNavigated", handler)
      frameHandlers.delete(listener)
    },
    onNavigatedWithinDocument: (listener) => {
      const handler: Parameters<typeof session.on<"Page.navigatedWithinDocument">>[1] = (event) =>
        listener({ frameId: event.frameId, url: event.url })
      historyHandlers.set(listener, handler)
      session.on("Page.navigatedWithinDocument", handler)
    },
    offNavigatedWithinDocument: (listener) => {
      const handler = historyHandlers.get(listener)
      if (handler === undefined) return
      session.off("Page.navigatedWithinDocument", handler)
      historyHandlers.delete(listener)
    },
  }
}
