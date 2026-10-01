import { type ParcelAddress, PropertyAddressError, parsePropertyRow } from "./property-address.ts"
import type {
  PropertyBuildingStatusUpdateResult,
  PropertyBuildingTerminalStatus,
  PropertyPager,
  PropertyWatcherSelectionHandle,
} from "./property-watcher-payload.ts"

export const PROPERTY_WATCHER_EVENTS = {
  commitFailed: "property_watcher.commit_failed",
  duplicateSkipped: "property_watcher.duplicate_skipped",
  invalidRow: "property_watcher.invalid_row",
  lookupFailed: "property_watcher.lookup_failed",
  prepareFailed: "property_watcher.prepare_failed",
} as const

export type PropertyWatcherEvent =
  (typeof PROPERTY_WATCHER_EVENTS)[keyof typeof PROPERTY_WATCHER_EVENTS]

export type PropertyWatcherLogger = {
  readonly log: (event: PropertyWatcherEvent, count: number) => void
}

export type InstalledPropertyWatcherTarget<Target> = {
  readonly target: Target
  readonly dispose: () => Promise<void>
  readonly failed: Promise<never>
  readonly updateBuildingStatus: (
    selection: PropertyWatcherSelectionHandle,
    status: PropertyBuildingTerminalStatus,
  ) => Promise<PropertyBuildingStatusUpdateResult>
}

export type PropertyWatcherDependencies<Target, PreparedUpdate> = {
  readonly installTargetListener: (
    onRow: (
      payload: unknown,
      selection: PropertyWatcherSelectionHandle,
      pager: PropertyPager,
    ) => void,
  ) => Promise<InstalledPropertyWatcherTarget<Target>>
  readonly prepareProgress: (
    address: ParcelAddress,
    pager: PropertyPager,
  ) => Promise<PreparedUpdate>
  readonly lookupKras: (
    target: Target,
    address: ParcelAddress,
  ) => Promise<PropertyBuildingTerminalStatus>
  readonly commitProgress: (update: PreparedUpdate) => Promise<void>
  readonly isRecoverableError: (operation: PropertyWatcherOperation, error: unknown) => boolean
  readonly logger: PropertyWatcherLogger
}

export type PropertyWatcherOperation = "prepare" | "lookup" | "commit"

export type PropertyWatcher = {
  readonly stopped: Promise<void>
  readonly stop: () => Promise<void>
}

type FailureState = { readonly kind: "none" } | { readonly kind: "failed"; readonly error: unknown }
type PropertyBuildingSuccessfulStatus = Exclude<PropertyBuildingTerminalStatus, "failed">
type SuccessfulSelection = {
  readonly address: ParcelAddress
  readonly status: PropertyBuildingSuccessfulStatus
}

class PropertyWatcherInvariantError extends Error {
  override readonly name = "PropertyWatcherInvariantError"
}

function assertNever(value: never): never {
  throw new PropertyWatcherInvariantError(`Unexpected property watcher state: ${String(value)}`)
}

export async function startPropertyWatcher<Target, PreparedUpdate>(
  dependencies: PropertyWatcherDependencies<Target, PreparedUpdate>,
): Promise<PropertyWatcher> {
  const targetReady = Promise.withResolvers<Target>()
  const resourcesReady = Promise.withResolvers<InstalledPropertyWatcherTarget<Target>>()
  const stopped = Promise.withResolvers<void>()
  let accepting = true
  let fatalFailure: FailureState = { kind: "none" }
  let lastSuccessfulSelection: SuccessfulSelection | undefined
  let queue = Promise.resolve()
  let stopping: Promise<void> | undefined

  void stopped.promise.then(
    () => undefined,
    () => undefined,
  )

  const finalizeStop = async (): Promise<void> => {
    const resources = await resourcesReady.promise
    let cleanupFailure: FailureState = { kind: "none" }
    try {
      await resources.dispose()
    } catch (error) {
      cleanupFailure = { kind: "failed", error }
    }

    switch (fatalFailure.kind) {
      case "failed":
        switch (cleanupFailure.kind) {
          case "failed":
            throw new AggregateError(
              [fatalFailure.error, cleanupFailure.error],
              "Property watcher failed and disposal also failed.",
              { cause: fatalFailure.error },
            )
          case "none":
            throw fatalFailure.error
          default:
            return assertNever(cleanupFailure)
        }
      case "none":
        switch (cleanupFailure.kind) {
          case "failed":
            throw cleanupFailure.error
          case "none":
            return
          default:
            return assertNever(cleanupFailure)
        }
      default:
        return assertNever(fatalFailure)
    }
  }

  const beginStop = (): Promise<void> => {
    accepting = false
    if (stopping !== undefined) return stopping
    stopping = queue.then(finalizeStop)
    void stopping.then(stopped.resolve, stopped.reject)
    return stopping
  }

  const markFatal = (error: unknown): void => {
    switch (fatalFailure.kind) {
      case "failed":
        return
      case "none":
        fatalFailure = { kind: "failed", error }
        void beginStop()
        return
      default:
        assertNever(fatalFailure)
    }
  }

  const processAddress = async (
    address: ParcelAddress,
    selection: PropertyWatcherSelectionHandle,
    pager: PropertyPager,
  ): Promise<void> => {
    const resources = await resourcesReady.promise
    if (address === lastSuccessfulSelection?.address) {
      await resources.updateBuildingStatus(selection, lastSuccessfulSelection.status)
      dependencies.logger.log(PROPERTY_WATCHER_EVENTS.duplicateSkipped, 1)
      return
    }
    lastSuccessfulSelection = undefined

    let update: PreparedUpdate
    try {
      update = await dependencies.prepareProgress(address, pager)
    } catch (error) {
      if (dependencies.isRecoverableError("prepare", error)) {
        await resources.updateBuildingStatus(selection, "failed")
        dependencies.logger.log(PROPERTY_WATCHER_EVENTS.prepareFailed, 1)
        return
      }
      markFatal(error)
      return
    }

    const target = await targetReady.promise
    let status: PropertyBuildingTerminalStatus
    try {
      status = await dependencies.lookupKras(target, address)
    } catch (error) {
      if (dependencies.isRecoverableError("lookup", error)) {
        await resources.updateBuildingStatus(selection, "failed")
        dependencies.logger.log(PROPERTY_WATCHER_EVENTS.lookupFailed, 1)
        return
      }
      markFatal(error)
      return
    }

    await resources.updateBuildingStatus(selection, status)
    switch (status) {
      case "failed":
        dependencies.logger.log(PROPERTY_WATCHER_EVENTS.lookupFailed, 1)
        return
      case "absent":
      case "present":
        break
      default:
        return assertNever(status)
    }

    try {
      await dependencies.commitProgress(update)
    } catch (error) {
      if (dependencies.isRecoverableError("commit", error)) {
        dependencies.logger.log(PROPERTY_WATCHER_EVENTS.commitFailed, 1)
        return
      }
      markFatal(error)
      return
    }
    lastSuccessfulSelection = { address, status }
  }

  const enqueue = (
    payload: unknown,
    selection: PropertyWatcherSelectionHandle,
    pager: PropertyPager,
  ): void => {
    if (!accepting) return

    let address: ParcelAddress
    try {
      address = parsePropertyRow(payload)
    } catch (error) {
      if (error instanceof PropertyAddressError) {
        try {
          dependencies.logger.log(PROPERTY_WATCHER_EVENTS.invalidRow, 1)
        } catch (logError) {
          markFatal(logError)
        }
        return
      }
      markFatal(error)
      return
    }

    const run = async (): Promise<void> => {
      try {
        switch (fatalFailure.kind) {
          case "failed":
            return
          case "none":
            await processAddress(address, selection, pager)
            return
          default:
            return assertNever(fatalFailure)
        }
      } catch (error) {
        markFatal(error)
      }
    }
    queue = queue.then(run)
  }

  const resources = await dependencies.installTargetListener(enqueue)
  resourcesReady.resolve(resources)
  targetReady.resolve(resources.target)
  void resources.failed.catch(markFatal)

  return {
    stopped: stopped.promise,
    stop: beginStop,
  }
}
