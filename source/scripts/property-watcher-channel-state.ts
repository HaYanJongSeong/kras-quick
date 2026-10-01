import type {
  PropertyWatcherBindingEvent,
  PropertyWatcherContextEvent,
  PropertyWatcherFrameEvent,
} from "./property-watcher-cdp.ts"
import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_WORLD_NAME,
} from "./property-watcher-listener.ts"
import {
  type PropertyPager,
  type PropertyRow,
  type PropertyWatcherSelectionHandle,
  parsePropertyWatcherPayload,
} from "./property-watcher-payload.ts"

const SCP_ORIGIN = "http://scpweb.softgraphy.biz"
const SCP_PATHNAME = "/board"

type LiveContext = {
  readonly context: PropertyWatcherContextEvent
  generation: number
  latestSelectionId: number | undefined
}

export type PropertyWatcherChannelState = {
  readonly acceptBinding: (event: PropertyWatcherBindingEvent) => void
  readonly clearContexts: () => void
  readonly createContext: (event: PropertyWatcherContextEvent) => void
  readonly deactivate: () => readonly number[]
  readonly destroyContext: (contextId: number) => void
  readonly initializeRoot: (event: PropertyWatcherFrameEvent) => void
  readonly isCurrentSelection: (selection: PropertyWatcherSelectionHandle) => boolean
  readonly navigateDocument: (event: PropertyWatcherFrameEvent) => void
  readonly navigateHistory: (event: PropertyWatcherFrameEvent) => void
}

type StateDependencies = {
  readonly onFatal: (error: unknown) => void
  readonly onRow: (
    row: PropertyRow,
    selection: PropertyWatcherSelectionHandle,
    pager: PropertyPager,
  ) => void | Promise<void>
}

function matchesScpUrl(value: string): boolean {
  if (!URL.canParse(value)) return false
  const url = new URL(value)
  return url.origin === SCP_ORIGIN && url.pathname === SCP_PATHNAME
}

export function createPropertyWatcherChannelState(
  dependencies: StateDependencies,
): PropertyWatcherChannelState {
  let active = true
  let cleanupContextIds: readonly number[] | undefined
  let generation = 0
  let rootFrameId: string | undefined
  let rootUrl: string | undefined
  const contexts = new Map<number, LiveContext>()

  const retagContexts = (): void => {
    for (const live of contexts.values()) live.generation = generation
  }

  const isWatcherRootContext = (context: PropertyWatcherContextEvent): boolean =>
    context.name === PROPERTY_WATCHER_WORLD_NAME &&
    context.type === "isolated" &&
    context.frameId === rootFrameId &&
    context.origin === SCP_ORIGIN

  const isLiveSource = (event: PropertyWatcherBindingEvent): boolean => {
    if (!active || event.name !== PROPERTY_WATCHER_BINDING_NAME) return false
    const live = contexts.get(event.contextId)
    if (live === undefined || live.generation !== generation || rootUrl === undefined) return false
    const context = live.context
    return (
      isWatcherRootContext(context) &&
      matchesScpUrl(rootUrl) &&
      new URL(rootUrl).origin === context.origin
    )
  }

  const isCurrentSelection = (selection: PropertyWatcherSelectionHandle): boolean => {
    if (!active) return false
    const live = contexts.get(selection.contextId)
    return (
      live !== undefined &&
      live.generation === generation &&
      live.generation === selection.generation &&
      live.context.uniqueId === selection.contextUniqueId &&
      live.latestSelectionId === selection.selectionId &&
      isWatcherRootContext(live.context) &&
      rootUrl !== undefined &&
      matchesScpUrl(rootUrl)
    )
  }

  return {
    acceptBinding: (event) => {
      if (!isLiveSource(event)) return
      const payload = parsePropertyWatcherPayload(event.payload)
      if (payload === undefined) return
      const live = contexts.get(event.contextId)
      if (live === undefined) return
      live.latestSelectionId = payload.selectionId
      const selection: PropertyWatcherSelectionHandle = {
        contextId: live.context.contextId,
        contextUniqueId: live.context.uniqueId,
        generation: live.generation,
        selectionId: payload.selectionId,
      }
      try {
        void Promise.resolve(dependencies.onRow(payload.cells, selection, payload.pager)).catch(
          dependencies.onFatal,
        )
      } catch (error) {
        dependencies.onFatal(error)
      }
    },
    clearContexts: () => {
      if (!active) return
      generation += 1
      contexts.clear()
    },
    createContext: (event) => {
      if (!active) return
      contexts.set(event.contextId, { context: event, generation, latestSelectionId: undefined })
    },
    deactivate: () => {
      if (!active) return cleanupContextIds ?? []
      const contextIds = [...contexts.values()]
        .filter((live) => isWatcherRootContext(live.context))
        .map((live) => live.context.contextId)
      cleanupContextIds = contextIds
      active = false
      generation += 1
      return contextIds
    },
    destroyContext: (contextId) => {
      if (!active) return
      contexts.delete(contextId)
    },
    initializeRoot: (event) => {
      if (!active) return
      rootFrameId = event.frameId
      rootUrl = event.url
    },
    isCurrentSelection,
    navigateDocument: (event) => {
      if (!active || event.frameId !== rootFrameId) return
      generation += 1
      rootUrl = event.url
      contexts.clear()
    },
    navigateHistory: (event) => {
      if (!active || event.frameId !== rootFrameId) return
      generation += 1
      rootUrl = event.url
      retagContexts()
    },
  }
}
