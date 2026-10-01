import { randomUUID } from "node:crypto"
import type { PropertyWatcher, PropertyWatcherLogger } from "./property-watcher.ts"
import { openWatcherStopServer, type WatcherStopServer } from "./property-watcher-control.ts"
import { PROPERTY_WATCHER_PID_FILE, type WatcherPidRecord } from "./property-watcher-process.ts"
import { startLivePropertyWatcher } from "./property-watcher-runtime.ts"
import {
  markWatcherPidReady,
  removeMatchingWatcherPidRecord,
  waitForStartingWatcherRecord,
  watcherReadyStateOperations,
} from "./property-watcher-state.ts"

type LifecycleFailure =
  | { readonly kind: "none" }
  | { readonly kind: "failed"; readonly error: unknown }

type ObservedOutcome =
  | { readonly kind: "succeeded" }
  | { readonly kind: "failed"; readonly error: unknown }

const NO_OUTCOME = new Promise<never>(() => undefined)

function retainFirstFailure(current: LifecycleFailure, error: unknown): LifecycleFailure {
  return current.kind === "none" ? { kind: "failed", error } : current
}

function observeOutcome(settled: Promise<void>): Promise<ObservedOutcome> {
  return settled.then<ObservedOutcome, ObservedOutcome>(
    () => ({ kind: "succeeded" }),
    (error: unknown) => ({ kind: "failed", error }),
  )
}

function observeFailure(stopped: Promise<void>): Promise<ObservedOutcome> {
  return stopped.then<ObservedOutcome, ObservedOutcome>(
    () => NO_OUTCOME,
    (error: unknown) => ({ kind: "failed", error }),
  )
}

export type WatcherLifecycleRuntimeDependencies = {
  readonly waitForRecord: (instanceToken: string) => Promise<WatcherPidRecord>
  readonly startWatcher: (instanceToken: string) => Promise<PropertyWatcher>
  readonly openControl: (
    pipeName: string,
    onStop: () => Promise<void>,
  ) => Promise<WatcherStopServer>
  readonly markReady: (record: WatcherPidRecord) => Promise<WatcherPidRecord>
  readonly removePid: (path: string, record: WatcherPidRecord) => Promise<void>
}

export type ForegroundWatcherDependencies = {
  readonly startWatcher: (instanceToken: string) => Promise<PropertyWatcher>
  readonly waitForStop: (signal: AbortSignal) => Promise<void>
}

function waitForForegroundStop(signal: AbortSignal): Promise<void> {
  const stopped = Promise.withResolvers<void>()
  const stop = (): void => stopped.resolve()
  process.once("SIGINT", stop)
  process.once("SIGTERM", stop)
  signal.addEventListener("abort", stop, { once: true })
  return stopped.promise.finally(() => {
    process.off("SIGINT", stop)
    process.off("SIGTERM", stop)
    signal.removeEventListener("abort", stop)
  })
}

export async function runForegroundPropertyWatcherLifecycle(
  logger: PropertyWatcherLogger,
  dependencies: ForegroundWatcherDependencies = {
    startWatcher: (instanceToken) => startLivePropertyWatcher(instanceToken, logger),
    waitForStop: waitForForegroundStop,
  },
): Promise<void> {
  const stopWaiting = new AbortController()
  let watcher: PropertyWatcher | undefined
  let failure: LifecycleFailure = { kind: "none" }
  try {
    watcher = await dependencies.startWatcher(randomUUID())
    await Promise.race([dependencies.waitForStop(stopWaiting.signal), watcher.stopped])
  } catch (error) {
    failure = retainFirstFailure(failure, error)
  } finally {
    stopWaiting.abort()
    if (watcher !== undefined) {
      try {
        await watcher.stop()
      } catch (error) {
        failure = retainFirstFailure(failure, error)
      }
    }
  }
  if (failure.kind === "failed") throw failure.error
}

export async function runDetachedPropertyWatcherLifecycle(
  instanceToken: string,
  dependencies: WatcherLifecycleRuntimeDependencies,
): Promise<void> {
  let starting: WatcherPidRecord | undefined
  let ready: WatcherPidRecord | undefined
  let watcher: PropertyWatcher | undefined
  let control: WatcherStopServer | undefined
  let watcherState: "absent" | "active" | "stopped" = "absent"
  let failure: LifecycleFailure = { kind: "none" }
  try {
    starting = await dependencies.waitForRecord(instanceToken)
    watcher = await dependencies.startWatcher(instanceToken)
    const watcherFailure = observeFailure(watcher.stopped)
    watcherState = "active"
    const installedWatcher = watcher
    control = await dependencies.openControl(starting.pipeName, async () => {
      await installedWatcher.stop()
      watcherState = "stopped"
    })
    const controlOutcome = observeOutcome(control.stopped)
    ready = await dependencies.markReady(starting)
    const outcome = await Promise.race([controlOutcome, watcherFailure])
    switch (outcome.kind) {
      case "succeeded":
        break
      case "failed":
        throw outcome.error
      default: {
        const exhaustive: never = outcome
        throw exhaustive
      }
    }
  } catch (error) {
    failure = retainFirstFailure(failure, error)
  } finally {
    if (control !== undefined) {
      try {
        await control.close()
      } catch (error) {
        failure = retainFirstFailure(failure, error)
      }
    }
    if (watcher !== undefined && watcherState === "active") {
      try {
        await watcher.stop()
        watcherState = "stopped"
      } catch (error) {
        failure = retainFirstFailure(failure, error)
      }
    }
    const record = ready ?? starting
    if (record !== undefined) {
      try {
        await dependencies.removePid(PROPERTY_WATCHER_PID_FILE, record)
      } catch (error) {
        failure = retainFirstFailure(failure, error)
      }
    }
  }
  if (failure.kind === "failed") throw failure.error
}

export function runLiveDetachedPropertyWatcher(
  instanceToken: string,
  logger: PropertyWatcherLogger,
): Promise<void> {
  return runDetachedPropertyWatcherLifecycle(instanceToken, {
    waitForRecord: waitForStartingWatcherRecord,
    startWatcher: (token) => startLivePropertyWatcher(token, logger),
    openControl: openWatcherStopServer,
    markReady: (record) =>
      markWatcherPidReady(record, PROPERTY_WATCHER_PID_FILE, watcherReadyStateOperations),
    removePid: removeMatchingWatcherPidRecord,
  })
}
