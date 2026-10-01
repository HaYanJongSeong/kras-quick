import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import { describe, it } from "node:test"

import { KrasLookupError } from "../scripts/kras-page.ts"
import type { ParcelAddress } from "../scripts/property-address.ts"
import { ProgressFileError } from "../scripts/property-progress.ts"
import {
  PROPERTY_WATCHER_EVENTS,
  type PropertyWatcherDependencies,
  type PropertyWatcherOperation,
  startPropertyWatcher,
} from "../scripts/property-watcher.ts"
import type {
  PropertyBuildingTerminalStatus,
  PropertyPager,
  PropertyWatcherSelectionHandle,
} from "../scripts/property-watcher-payload.ts"
import { parsePropertyPager } from "../scripts/property-watcher-payload.ts"

const ADDRESS_A = "서울특별시 강서구 개화동 255-1"
const ADDRESS_B = "서울특별시 종로구 청운동 1"
const TARGET = Symbol("selected-kras-page")
const NO_FAILURE = new Promise<never>(() => undefined)

type PreparedUpdate = {
  readonly address: ParcelAddress
}

class SyntheticDependencyError extends Error {
  override readonly name = "SyntheticDependencyError"
}

function requirePager(value: string): PropertyPager {
  const pager = parsePropertyPager(value)
  if (pager === undefined) throw new SyntheticDependencyError(`invalid test pager: ${value}`)
  return pager
}

const PAGER = requirePager("8/16")

function assertNeverOperation(operation: never): never {
  throw new SyntheticDependencyError(`unexpected operation: ${String(operation)}`)
}

function isRecoverableError(operation: PropertyWatcherOperation, error: unknown): boolean {
  switch (operation) {
    case "prepare":
    case "commit":
      return error instanceof ProgressFileError
    case "lookup":
      return error instanceof KrasLookupError
    default:
      return assertNeverOperation(operation)
  }
}

function rowWithAddress(address: string): readonly string[] {
  return ["1", "공유재산", "토지", "구", "동", "일반", address, "미처리"]
}

type SelectionEmitter = (
  payload: unknown,
  selection: PropertyWatcherSelectionHandle,
  pager: PropertyPager,
) => void

function requireEmitter(emitter: SelectionEmitter | undefined): (payload: unknown) => void {
  if (emitter === undefined) throw new SyntheticDependencyError("listener was not installed")
  let selectionId = 0
  return (payload) => {
    selectionId += 1
    emitter(payload, selectionHandle(selectionId), PAGER)
  }
}

function requireSelectionEmitter(emitter: SelectionEmitter | undefined): SelectionEmitter {
  if (emitter === undefined)
    throw new SyntheticDependencyError("selection listener was not installed")
  return emitter
}

function selectionHandle(selectionId: number): PropertyWatcherSelectionHandle {
  return {
    contextId: 7,
    contextUniqueId: "context-7",
    generation: 0,
    selectionId,
  }
}

const APPLY_STATUS = async (): Promise<"applied"> => "applied"

describe("property row watcher orchestration", () => {
  it("accepts synchronously and runs prepare, lookup, status, and commit FIFO at concurrency one", async () => {
    // Given
    const actions: string[] = []
    let emitter: SelectionEmitter | undefined
    let active = 0
    let maximumActive = 0
    const runOperation = async <Result>(name: string, result: Result): Promise<Result> => {
      active += 1
      maximumActive = Math.max(maximumActive, active)
      actions.push(name)
      await Promise.resolve()
      active -= 1
      return result
    }
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => {
            actions.push("dispose")
          },
        }
      },
      prepareProgress: (address, pager) =>
        runOperation(`prepare:${pager}:${address}`, { address } satisfies PreparedUpdate),
      lookupKras: (target, address) => {
        assert.equal(target, TARGET)
        return runOperation(`lookup:${address}`, "present")
      },
      commitProgress: (update) => runOperation(`commit:${update.address}`, undefined),
      isRecoverableError,
      logger: { log: () => undefined },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)

    // When
    const firstResult = emit(rowWithAddress(ADDRESS_A))
    const secondResult = emit(rowWithAddress(ADDRESS_B))
    await watcher.stop()

    // Then
    assert.equal(firstResult, undefined)
    assert.equal(secondResult, undefined)
    assert.equal(maximumActive, 1)
    assert.deepEqual(actions, [
      `prepare:8/16:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `commit:${ADDRESS_A}`,
      `prepare:8/16:${ADDRESS_B}`,
      `lookup:${ADDRESS_B}`,
      `commit:${ADDRESS_B}`,
      "dispose",
    ])
  })

  it("logs invalid payloads without row values and performs no property work", async () => {
    // Given
    const calls: unknown[][] = []
    const operations: string[] = []
    let emitter: SelectionEmitter | undefined
    const secret = "private-row-value-9182"
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => undefined,
        }
      },
      prepareProgress: async (address) => {
        operations.push("prepare")
        return { address }
      },
      lookupKras: async () => {
        operations.push("lookup")
        return "present"
      },
      commitProgress: async () => {
        operations.push("commit")
      },
      isRecoverableError,
      logger: {
        log: (...arguments_) => {
          calls.push(arguments_)
        },
      },
    }
    const watcher = await startPropertyWatcher(dependencies)

    // When
    requireEmitter(emitter)(["1", "2", "3", "4", "5", "6", secret])
    await watcher.stop()

    // Then
    assert.deepEqual(operations, [])
    assert.deepEqual(calls, [[PROPERTY_WATCHER_EVENTS.invalidRow, 1]])
    assert.equal(JSON.stringify(calls).includes(secret), false)
  })

  it("reports failed and continues without lookup when preparation fails recoverably", async () => {
    // Given
    const actions: string[] = []
    const logs: Array<readonly unknown[]> = []
    const statuses: Array<readonly [number, PropertyBuildingTerminalStatus]> = []
    let emitter: SelectionEmitter | undefined
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: async (selection, status) => {
            statuses.push([selection.selectionId, status])
            return "applied"
          },
          dispose: async () => undefined,
        }
      },
      prepareProgress: async (address) => {
        actions.push(`prepare:${address}`)
        if (address === ADDRESS_A) throw new ProgressFileError("IO_FAILURE")
        return { address }
      },
      lookupKras: async (_target, address) => {
        actions.push(`lookup:${address}`)
        return "present"
      },
      commitProgress: async (update) => {
        actions.push(`commit:${update.address}`)
      },
      isRecoverableError,
      logger: { log: (...arguments_) => logs.push(arguments_) },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A))
    emit(rowWithAddress(ADDRESS_B))
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `prepare:${ADDRESS_B}`,
      `lookup:${ADDRESS_B}`,
      `commit:${ADDRESS_B}`,
    ])
    assert.deepEqual(statuses, [
      [1, "failed"],
      [2, "present"],
    ])
    assert.deepEqual(logs, [[PROPERTY_WATCHER_EVENTS.prepareFailed, 1]])
  })

  it("does not commit after lookup failure and allows the same address to retry", async () => {
    // Given
    const actions: string[] = []
    const logs: Array<readonly unknown[]> = []
    let emitter: SelectionEmitter | undefined
    let lookupAttempt = 0
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => undefined,
        }
      },
      prepareProgress: async (address) => {
        actions.push(`prepare:${address}`)
        return { address }
      },
      lookupKras: async (_target, address) => {
        lookupAttempt += 1
        actions.push(`lookup:${address}`)
        if (lookupAttempt === 1) {
          throw new KrasLookupError({ cause: new SyntheticDependencyError("lookup failed") })
        }
        return "present"
      },
      commitProgress: async (update) => {
        actions.push(`commit:${update.address}`)
      },
      isRecoverableError,
      logger: { log: (...arguments_) => logs.push(arguments_) },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A))
    emit(rowWithAddress(ADDRESS_A))
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `commit:${ADDRESS_A}`,
    ])
    assert.deepEqual(logs, [[PROPERTY_WATCHER_EVENTS.lookupFailed, 1]])
  })

  it("keeps an address retryable after a progress snapshot commit failure", async () => {
    // Given
    const actions: string[] = []
    const logs: Array<readonly unknown[]> = []
    const statuses: Array<readonly [number, PropertyBuildingTerminalStatus]> = []
    let emitter: SelectionEmitter | undefined
    let commitAttempt = 0
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: async (selection, status) => {
            statuses.push([selection.selectionId, status])
            return "applied"
          },
          dispose: async () => undefined,
        }
      },
      prepareProgress: async (address) => {
        actions.push(`prepare:${address}`)
        return { address }
      },
      lookupKras: async (_target, address) => {
        actions.push(`lookup:${address}`)
        return "present"
      },
      commitProgress: async (update) => {
        commitAttempt += 1
        actions.push(`commit:${update.address}`)
        if (commitAttempt === 1) throw new ProgressFileError("SOURCE_CHANGED")
      },
      isRecoverableError,
      logger: { log: (...arguments_) => logs.push(arguments_) },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A))
    emit(rowWithAddress(ADDRESS_A))
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `commit:${ADDRESS_A}`,
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `commit:${ADDRESS_A}`,
    ])
    assert.deepEqual(statuses, [
      [1, "present"],
      [2, "present"],
    ])
    assert.deepEqual(logs, [[PROPERTY_WATCHER_EVENTS.commitFailed, 1]])
  })

  it("skips only consecutive successful duplicates while processing A-B-A", async () => {
    // Given
    const prepared: string[] = []
    const logs: Array<readonly unknown[]> = []
    let emitter: SelectionEmitter | undefined
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => undefined,
        }
      },
      prepareProgress: async (address) => {
        prepared.push(address)
        return { address }
      },
      lookupKras: async () => "present",
      commitProgress: async () => undefined,
      isRecoverableError,
      logger: { log: (...arguments_) => logs.push(arguments_) },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A))
    emit(rowWithAddress(ADDRESS_A))
    emit(rowWithAddress(ADDRESS_B))
    emit(rowWithAddress(ADDRESS_A))
    await watcher.stop()

    // Then
    assert.deepEqual(prepared, [ADDRESS_A, ADDRESS_B, ADDRESS_A])
    assert.deepEqual(logs, [[PROPERTY_WATCHER_EVENTS.duplicateSkipped, 1]])
  })

  for (const terminalStatus of ["present", "absent"] as const) {
    it(`orders prepare, lookup, ${terminalStatus} status, and commit`, async () => {
      // Given
      const actions: string[] = []
      let emitter: SelectionEmitter | undefined
      const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
        installTargetListener: async (onRow: SelectionEmitter) => {
          emitter = onRow
          return {
            failed: NO_FAILURE,
            target: TARGET,
            dispose: async () => undefined,
            updateBuildingStatus: async (
              selection: PropertyWatcherSelectionHandle,
              status: PropertyBuildingTerminalStatus,
            ) => {
              actions.push(`status:${selection.selectionId}:${status}`)
              return "applied" as const
            },
          }
        },
        prepareProgress: async (address: ParcelAddress) => {
          actions.push(`prepare:${address}`)
          return { address } satisfies PreparedUpdate
        },
        lookupKras: async (_target: typeof TARGET, address: ParcelAddress) => {
          actions.push(`lookup:${address}`)
          return terminalStatus
        },
        commitProgress: async (update: PreparedUpdate) => {
          actions.push(`commit:${update.address}`)
        },
        isRecoverableError,
        logger: { log: () => undefined },
      }
      const watcher = await startPropertyWatcher(dependencies)

      // When
      requireSelectionEmitter(emitter)(rowWithAddress(ADDRESS_A), selectionHandle(1), PAGER)
      await watcher.stop()

      // Then
      assert.deepEqual(actions, [
        `prepare:${ADDRESS_A}`,
        `lookup:${ADDRESS_A}`,
        `status:1:${terminalStatus}`,
        `commit:${ADDRESS_A}`,
      ])
    })
  }

  it("reports a recoverable lookup failure and leaves the same address retryable", async () => {
    // Given
    const actions: string[] = []
    let emitter: SelectionEmitter | undefined
    let lookupAttempt = 0
    const lookupKras = async (
      _target: typeof TARGET,
      address: ParcelAddress,
    ): Promise<PropertyBuildingTerminalStatus> => {
      lookupAttempt += 1
      actions.push(`lookup:${address}:${lookupAttempt}`)
      if (lookupAttempt === 1) {
        throw new KrasLookupError({ cause: new SyntheticDependencyError("lookup failed") })
      }
      return "present"
    }
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow: SelectionEmitter) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          dispose: async () => undefined,
          updateBuildingStatus: async (
            selection: PropertyWatcherSelectionHandle,
            status: PropertyBuildingTerminalStatus,
          ) => {
            actions.push(`status:${selection.selectionId}:${status}`)
            return "applied" as const
          },
        }
      },
      prepareProgress: async (address: ParcelAddress) => {
        actions.push(`prepare:${address}`)
        return { address } satisfies PreparedUpdate
      },
      lookupKras,
      commitProgress: async (update: PreparedUpdate) => {
        actions.push(`commit:${update.address}`)
      },
      isRecoverableError,
      logger: { log: () => undefined },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireSelectionEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A), selectionHandle(1), PAGER)
    emit(rowWithAddress(ADDRESS_A), selectionHandle(2), PAGER)
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}:1`,
      "status:1:failed",
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}:2`,
      "status:2:present",
      `commit:${ADDRESS_A}`,
    ])
  })

  it("reapplies the cached terminal status for a consecutive successful duplicate", async () => {
    // Given
    const actions: string[] = []
    let emitter: SelectionEmitter | undefined
    const lookupKras = async (
      _target: typeof TARGET,
      address: ParcelAddress,
    ): Promise<PropertyBuildingTerminalStatus> => {
      actions.push(`lookup:${address}`)
      return "absent"
    }
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow: SelectionEmitter) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          dispose: async () => undefined,
          updateBuildingStatus: async (
            selection: PropertyWatcherSelectionHandle,
            status: PropertyBuildingTerminalStatus,
          ) => {
            actions.push(`status:${selection.selectionId}:${status}`)
            return "applied" as const
          },
        }
      },
      prepareProgress: async (address: ParcelAddress) => {
        actions.push(`prepare:${address}`)
        return { address } satisfies PreparedUpdate
      },
      lookupKras,
      commitProgress: async (update: PreparedUpdate) => {
        actions.push(`commit:${update.address}`)
      },
      isRecoverableError,
      logger: { log: () => undefined },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireSelectionEmitter(emitter)

    // When
    emit(rowWithAddress(ADDRESS_A), selectionHandle(1), PAGER)
    emit(rowWithAddress(ADDRESS_A), selectionHandle(2), PAGER)
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      "status:1:absent",
      `commit:${ADDRESS_A}`,
      "status:2:absent",
    ])
  })

  it("stops intake, drains queued work, and disposes only after the drain", async () => {
    // Given
    const started = Promise.withResolvers<void>()
    const release = Promise.withResolvers<void>()
    const actions: string[] = []
    let emitter: SelectionEmitter | undefined
    let disposeCount = 0
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => {
            disposeCount += 1
            actions.push("dispose")
          },
        }
      },
      prepareProgress: async (address) => {
        actions.push(`prepare:${address}`)
        started.resolve()
        await release.promise
        return { address }
      },
      lookupKras: async (_target, address) => {
        actions.push(`lookup:${address}`)
        return "present"
      },
      commitProgress: async (update) => {
        actions.push(`commit:${update.address}`)
      },
      isRecoverableError,
      logger: { log: () => undefined },
    }
    const watcher = await startPropertyWatcher(dependencies)
    const emit = requireEmitter(emitter)
    emit(rowWithAddress(ADDRESS_A))
    await started.promise

    // When
    const stopping = watcher.stop()
    emit(rowWithAddress(ADDRESS_B))
    assert.equal(disposeCount, 0)
    release.resolve()
    await stopping
    await watcher.stop()

    // Then
    assert.deepEqual(actions, [
      `prepare:${ADDRESS_A}`,
      `lookup:${ADDRESS_A}`,
      `commit:${ADDRESS_A}`,
      "dispose",
    ])
    assert.equal(disposeCount, 1)
  })

  for (const fatalOperation of ["prepare", "lookup", "commit"] as const) {
    it(`stops and preserves an unexpected ${fatalOperation} failure`, async () => {
      // Given
      const fatal = new TypeError(`${fatalOperation} defect`)
      const actions: string[] = []
      const logs: Array<readonly unknown[]> = []
      let emitter: SelectionEmitter | undefined
      let disposeCount = 0
      const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
        installTargetListener: async (onRow) => {
          emitter = onRow
          return {
            failed: NO_FAILURE,
            target: TARGET,
            updateBuildingStatus: APPLY_STATUS,
            dispose: async () => {
              disposeCount += 1
              actions.push("dispose")
            },
          }
        },
        prepareProgress: async (address) => {
          actions.push(`prepare:${address}`)
          if (fatalOperation === "prepare") throw fatal
          return { address }
        },
        lookupKras: async (_target, address) => {
          actions.push(`lookup:${address}`)
          if (fatalOperation === "lookup") throw fatal
          return "present"
        },
        commitProgress: async (update) => {
          actions.push(`commit:${update.address}`)
          if (fatalOperation === "commit") throw fatal
        },
        isRecoverableError,
        logger: { log: (...arguments_) => logs.push(arguments_) },
      }
      const watcher = await startPropertyWatcher(dependencies)
      const emit = requireEmitter(emitter)

      // When
      emit(rowWithAddress(ADDRESS_A))
      emit(rowWithAddress(ADDRESS_B))

      // Then
      await assert.rejects(watcher.stopped, (error: unknown) => error === fatal)
      emit(rowWithAddress(ADDRESS_B))
      await assert.rejects(watcher.stop(), (error: unknown) => error === fatal)
      await assert.rejects(watcher.stop(), (error: unknown) => error === fatal)
      assert.equal(actions.includes(`prepare:${ADDRESS_B}`), false)
      assert.equal(disposeCount, 1)
      assert.deepEqual(logs, [])
    })
  }

  it("preserves a fatal cause when listener disposal also fails", async () => {
    // Given
    const fatal = new TypeError("prepare defect")
    const disposalFailure = new SyntheticDependencyError("dispose failed")
    let emitter: SelectionEmitter | undefined
    let disposeCount = 0
    const dependencies: PropertyWatcherDependencies<typeof TARGET, PreparedUpdate> = {
      installTargetListener: async (onRow) => {
        emitter = onRow
        return {
          failed: NO_FAILURE,
          target: TARGET,
          updateBuildingStatus: APPLY_STATUS,
          dispose: async () => {
            disposeCount += 1
            throw disposalFailure
          },
        }
      },
      prepareProgress: async () => {
        throw fatal
      },
      lookupKras: async () => "present",
      commitProgress: async () => undefined,
      isRecoverableError,
      logger: { log: () => undefined },
    }
    const watcher = await startPropertyWatcher(dependencies)

    // When
    requireEmitter(emitter)(rowWithAddress(ADDRESS_A))

    // Then
    await assert.rejects(
      watcher.stop(),
      (error: unknown) =>
        error instanceof AggregateError &&
        error.cause === fatal &&
        error.errors[0] === fatal &&
        error.errors[1] === disposalFailure,
    )
    assert.equal(disposeCount, 1)
  })

  it("has no browser lifecycle, navigation, storage, or tab capabilities", async () => {
    // Given
    const source = await readFile(
      new URL("../scripts/property-watcher.ts", import.meta.url),
      "utf8",
    )
    const forbiddenPatterns = [
      /\bplaywright\b/u,
      /\bconnectOverCDP\b/u,
      /\.(?:bringToFront|click|close|disconnect|goto|newContext|newPage|reload|storageState)\s*\(/u,
    ]

    // When / Then
    for (const pattern of forbiddenPatterns) assert.doesNotMatch(source, pattern)
  })
})
