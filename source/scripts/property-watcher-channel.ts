import type {
  PropertyWatcherBindingEvent,
  PropertyWatcherContextEvent,
  PropertyWatcherFrameEvent,
  PropertyWatcherProtocol,
} from "./property-watcher-cdp.ts"
import { createPropertyWatcherChannelState } from "./property-watcher-channel-state.ts"
import { PROPERTY_WATCHER_BINDING_NAME } from "./property-watcher-listener.ts"
import type {
  PropertyBuildingStatusUpdateResult,
  PropertyBuildingTerminalStatus,
  PropertyPager,
  PropertyRow,
  PropertyWatcherSelectionHandle,
} from "./property-watcher-payload.ts"

export type PropertyWatcherChannel = {
  readonly dispose: () => Promise<void>
  readonly failed: Promise<never>
  readonly updateBuildingStatus: (
    selection: PropertyWatcherSelectionHandle,
    status: PropertyBuildingTerminalStatus,
  ) => Promise<PropertyBuildingStatusUpdateResult>
}

export type PropertyWatcherChannelErrorCode = "SESSION_CLOSED"

const ERROR_MESSAGES = {
  SESSION_CLOSED: "SCP click channel session closed.",
} as const satisfies Record<PropertyWatcherChannelErrorCode, string>

export class PropertyWatcherChannelError extends Error {
  override readonly name = "PropertyWatcherChannelError"
  readonly code: PropertyWatcherChannelErrorCode

  constructor(code: PropertyWatcherChannelErrorCode) {
    super(ERROR_MESSAGES[code])
    this.code = code
  }
}

type ChannelHandlers = {
  readonly binding: (event: PropertyWatcherBindingEvent) => void
  readonly close: () => void
  readonly contextsCleared: () => void
  readonly contextCreated: (event: PropertyWatcherContextEvent) => void
  readonly contextDestroyed: (contextId: number) => void
  readonly frameNavigated: (event: PropertyWatcherFrameEvent) => void
  readonly navigatedWithinDocument: (event: PropertyWatcherFrameEvent) => void
}

function registerHandlers(protocol: PropertyWatcherProtocol, handlers: ChannelHandlers): void {
  protocol.onBindingCalled(handlers.binding)
  protocol.onClose(handlers.close)
  protocol.onContextsCleared(handlers.contextsCleared)
  protocol.onContextCreated(handlers.contextCreated)
  protocol.onContextDestroyed(handlers.contextDestroyed)
  protocol.onFrameNavigated(handlers.frameNavigated)
  protocol.onNavigatedWithinDocument(handlers.navigatedWithinDocument)
}

function removeHandlers(protocol: PropertyWatcherProtocol, handlers: ChannelHandlers): void {
  protocol.offBindingCalled(handlers.binding)
  protocol.offClose(handlers.close)
  protocol.offContextsCleared(handlers.contextsCleared)
  protocol.offContextCreated(handlers.contextCreated)
  protocol.offContextDestroyed(handlers.contextDestroyed)
  protocol.offFrameNavigated(handlers.frameNavigated)
  protocol.offNavigatedWithinDocument(handlers.navigatedWithinDocument)
}

async function throwCleanupFailures(failures: readonly unknown[]): Promise<void> {
  if (failures.length === 0) return
  const first = failures[0]
  if (failures.length === 1) throw first
  throw new AggregateError(failures, "Property watcher channel cleanup failed.", { cause: first })
}

export async function installPropertyWatcherChannel(
  protocol: PropertyWatcherProtocol,
  onRow: (
    row: PropertyRow,
    selection: PropertyWatcherSelectionHandle,
    pager: PropertyPager,
  ) => void | Promise<void>,
): Promise<PropertyWatcherChannel> {
  const failed = Promise.withResolvers<never>()
  void failed.promise.catch(() => undefined)
  let fatalRaised = false
  const raiseFatal = (error: unknown): void => {
    if (fatalRaised) return
    fatalRaised = true
    state.deactivate()
    failed.reject(error)
  }
  const state = createPropertyWatcherChannelState({ onFatal: raiseFatal, onRow })
  const handlers: ChannelHandlers = {
    binding: state.acceptBinding,
    close: () => raiseFatal(new PropertyWatcherChannelError("SESSION_CLOSED")),
    contextsCleared: state.clearContexts,
    contextCreated: state.createContext,
    contextDestroyed: state.destroyContext,
    frameNavigated: state.navigateDocument,
    navigatedWithinDocument: state.navigateHistory,
  }
  registerHandlers(protocol, handlers)

  let bindingInstalled = false
  let scriptIdentifier: string | undefined
  try {
    await protocol.pageEnable()
    await protocol.runtimeEnable()
    state.initializeRoot(await protocol.getRootFrame())
    await protocol.addBinding()
    bindingInstalled = true
    scriptIdentifier = await protocol.addListenerScript()
  } catch (error) {
    state.deactivate()
    removeHandlers(protocol, handlers)
    const failures: unknown[] = [error]
    if (scriptIdentifier !== undefined) {
      try {
        await protocol.removeListenerScript(scriptIdentifier)
      } catch (cleanupError) {
        failures.push(cleanupError)
      }
    }
    if (bindingInstalled) {
      try {
        await protocol.removeBinding()
      } catch (cleanupError) {
        failures.push(cleanupError)
      }
    }
    if (failures.length === 1) throw error
    throw new AggregateError(failures, "Property watcher channel installation failed.", {
      cause: error,
    })
  }

  let disposing: Promise<void> | undefined
  return {
    failed: failed.promise,
    updateBuildingStatus: async (selection, status) => {
      if (!state.isCurrentSelection(selection)) return "stale"
      return protocol.evaluateBuildingStatus(selection.contextId, selection.selectionId, status)
    },
    dispose: () => {
      if (disposing !== undefined) return disposing
      const contextIds = state.deactivate()
      removeHandlers(protocol, handlers)
      disposing = (async () => {
        const failures: unknown[] = []
        for (const contextId of contextIds) {
          try {
            await protocol.evaluateCleanup(contextId)
          } catch (error) {
            failures.push(error)
          }
        }
        try {
          await protocol.removeListenerScript(scriptIdentifier)
        } catch (error) {
          failures.push(error)
        }
        try {
          await protocol.removeBinding()
        } catch (error) {
          failures.push(error)
        }
        await throwCleanupFailures(failures)
      })()
      return disposing
    },
  }
}

export { PROPERTY_WATCHER_BINDING_NAME }
