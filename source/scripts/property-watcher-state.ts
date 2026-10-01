import { randomUUID } from "node:crypto"
import { open, readFile, rename, unlink, watch } from "node:fs/promises"
import { basename, dirname, join } from "node:path"

import { sendWatcherStop } from "./property-watcher-control.ts"
import {
  inspectWatcherProcess,
  matchesWatcherProcessIdentity,
  PROPERTY_WATCHER_PID_FILE,
  PropertyWatcherProcessError,
  parseWatcherPidRecord,
  sameWatcherInstance,
  sameWatcherPidRecord,
  type WatcherPidRecord,
  type WatcherProcessSnapshot,
} from "./property-watcher-process.ts"

function hasErrorCode(error: unknown, code: string): boolean {
  return error instanceof Error && "code" in error && error.code === code
}

export async function readWatcherPidRecord(
  path = PROPERTY_WATCHER_PID_FILE,
): Promise<WatcherPidRecord> {
  let text: string
  try {
    text = await readFile(path, "utf8")
  } catch (error) {
    if (hasErrorCode(error, "ENOENT")) throw new PropertyWatcherProcessError("PID_RECORD_MISSING")
    throw error
  }
  try {
    return parseWatcherPidRecord(JSON.parse(text))
  } catch (error) {
    if (error instanceof SyntaxError) throw new PropertyWatcherProcessError("PID_RECORD_INVALID")
    throw error
  }
}

export type WatcherReadyStateHandle = {
  readonly write: (value: string) => Promise<void>
  readonly sync: () => Promise<void>
  readonly close: () => Promise<void>
}

export type WatcherReadyStateOperations = {
  readonly randomUuid: () => string
  readonly readRecord: (path: string) => Promise<WatcherPidRecord>
  readonly openExclusive: (path: string) => Promise<WatcherReadyStateHandle>
  readonly rename: (from: string, to: string) => Promise<void>
  readonly unlink: (path: string) => Promise<void>
}

export const watcherReadyStateOperations: WatcherReadyStateOperations = {
  randomUuid: randomUUID,
  readRecord: readWatcherPidRecord,
  openExclusive: async (path) => {
    const handle = await open(path, "wx")
    return {
      write: async (value) => {
        await handle.writeFile(value)
      },
      sync: () => handle.sync(),
      close: () => handle.close(),
    }
  },
  rename,
  unlink,
}

export async function markWatcherPidReady(
  expected: WatcherPidRecord,
  path: string,
  operations: WatcherReadyStateOperations,
): Promise<WatcherPidRecord> {
  const current = await operations.readRecord(path)
  if (!sameWatcherPidRecord(current, expected)) {
    throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
  }
  const ready = { ...current, state: "ready" } as const
  const temporaryPath = join(dirname(path), `.${basename(path)}.${operations.randomUuid()}.tmp`)
  let handle: WatcherReadyStateHandle | undefined
  let closed = false
  try {
    handle = await operations.openExclusive(temporaryPath)
    await handle.write(JSON.stringify(ready))
    await handle.sync()
    await handle.close()
    closed = true
    const rechecked = await operations.readRecord(path)
    if (!sameWatcherPidRecord(current, rechecked)) {
      throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
    }
    await operations.rename(temporaryPath, path)
    return ready
  } catch (error) {
    if (handle !== undefined && !closed) await Promise.allSettled([handle.close()])
    await Promise.allSettled([operations.unlink(temporaryPath)])
    throw error
  }
}

export async function removeMatchingWatcherPidRecord(
  path: string,
  expected: WatcherPidRecord,
): Promise<void> {
  let current: WatcherPidRecord
  try {
    current = await readWatcherPidRecord(path)
  } catch (error) {
    if (error instanceof PropertyWatcherProcessError && error.code === "PID_RECORD_MISSING") return
    throw error
  }
  if (!sameWatcherInstance(current, expected)) return
  const rechecked = await readWatcherPidRecord(path)
  if (sameWatcherPidRecord(current, rechecked)) await unlink(path)
}

async function matchingRecord(
  instanceToken: string,
  pid: number,
): Promise<WatcherPidRecord | undefined> {
  try {
    const record = await readWatcherPidRecord()
    return record.pid === pid && record.instanceToken === instanceToken ? record : undefined
  } catch (error) {
    if (error instanceof PropertyWatcherProcessError) return undefined
    throw error
  }
}

export async function waitForStartingWatcherRecord(
  instanceToken: string,
  pid = process.pid,
): Promise<WatcherPidRecord> {
  const changes = watch(dirname(PROPERTY_WATCHER_PID_FILE), {
    signal: AbortSignal.timeout(5000),
  })
  try {
    const current = await matchingRecord(instanceToken, pid)
    if (current !== undefined) return current
    for await (const _change of changes) {
      const record = await matchingRecord(instanceToken, pid)
      if (record !== undefined) return record
    }
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError") {
      throw new PropertyWatcherProcessError("PID_RECORD_MISSING")
    }
    throw error
  }
  throw new PropertyWatcherProcessError("PID_RECORD_MISSING")
}

export type WatcherStatus =
  | { readonly status: "stopped" }
  | { readonly status: "stale"; readonly pid: number }
  | { readonly status: "starting" | "ready"; readonly pid: number }

export type WatcherLifecycleDependencies = {
  readonly inspectProcess: (pid: number) => Promise<WatcherProcessSnapshot | null>
  readonly readRecord: () => Promise<WatcherPidRecord>
  readonly sendStop: (pipeName: string) => Promise<void>
}

const lifecycleDependencies: WatcherLifecycleDependencies = {
  inspectProcess: inspectWatcherProcess,
  readRecord: () => readWatcherPidRecord(),
  sendStop: sendWatcherStop,
}

export async function getWatcherStatus(
  dependencies: WatcherLifecycleDependencies = lifecycleDependencies,
): Promise<WatcherStatus> {
  let record: WatcherPidRecord
  try {
    record = await dependencies.readRecord()
  } catch (error) {
    if (error instanceof PropertyWatcherProcessError && error.code === "PID_RECORD_MISSING") {
      return { status: "stopped" }
    }
    throw error
  }
  const snapshot = await dependencies.inspectProcess(record.pid)
  if (snapshot === null || !matchesWatcherProcessIdentity(record, snapshot)) {
    return { status: "stale", pid: record.pid }
  }
  return { status: record.state, pid: record.pid }
}

export async function requestWatcherStop(
  dependencies: WatcherLifecycleDependencies = lifecycleDependencies,
): Promise<void> {
  const record = await dependencies.readRecord()
  const snapshot = await dependencies.inspectProcess(record.pid)
  if (snapshot === null || !matchesWatcherProcessIdentity(record, snapshot)) {
    throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
  }
  await dependencies.sendStop(record.pipeName)
}
