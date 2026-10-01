import { type SpawnOptions, spawn } from "node:child_process"
import { randomUUID } from "node:crypto"
import { open, unlink } from "node:fs/promises"
import { isAbsolute } from "node:path"

import { z } from "zod"

import {
  ensureWatcherArtifactDirectory,
  inspectWatcherProcess,
  matchesWatcherProcessIdentity,
  PROPERTY_WATCHER_LOG_FILE,
  PROPERTY_WATCHER_PID_FILE,
  PROPERTY_WATCHER_TOKEN_ENV,
  PropertyWatcherProcessError,
  reserveWatcherPidFile,
  sameWatcherPidRecord,
  type WatcherPidRecord,
  type WatcherPidReservation,
  type WatcherProcessSnapshot,
  watcherPipeName,
} from "./property-watcher-process.ts"
import { readWatcherPidRecord } from "./property-watcher-state.ts"

export type DetachedSpawnRequest = {
  readonly executable: string
  readonly arguments: readonly string[]
  readonly options: {
    readonly detached: true
    readonly env: NodeJS.ProcessEnv
    readonly shell: false
    readonly stdio: readonly ["ignore", number, number]
    readonly windowsHide: true
  }
}

export type DetachedStartDependencies = {
  readonly ensureArtifactDirectory: () => Promise<void>
  readonly inspectProcess: (pid: number) => Promise<WatcherProcessSnapshot | null>
  readonly openAppendLog: () => Promise<{
    readonly fd: number
    readonly close: () => Promise<void>
  }>
  readonly randomUuid: () => string
  readonly readRecord: () => Promise<WatcherPidRecord>
  readonly removeReservation: () => Promise<void>
  readonly reservePid: () => Promise<WatcherPidReservation>
  readonly spawn: (request: DetachedSpawnRequest) => {
    readonly pid: number
    readonly unref: () => void
  }
}

const nodeStartDependencies: DetachedStartDependencies = {
  ensureArtifactDirectory: () => ensureWatcherArtifactDirectory(),
  inspectProcess: inspectWatcherProcess,
  openAppendLog: async () => {
    const handle = await open(PROPERTY_WATCHER_LOG_FILE, "a")
    return { fd: handle.fd, close: () => handle.close() }
  },
  randomUuid: randomUUID,
  readRecord: () => readWatcherPidRecord(),
  removeReservation: () => unlink(PROPERTY_WATCHER_PID_FILE),
  reservePid: () => reserveWatcherPidFile(PROPERTY_WATCHER_PID_FILE),
  spawn: (request) => {
    const options: SpawnOptions = {
      ...request.options,
      stdio: ["ignore", request.options.stdio[1], request.options.stdio[2]],
    }
    const child = spawn(request.executable, request.arguments, options)
    if (child.pid === undefined) throw new PropertyWatcherProcessError("SPAWN_FAILED")
    return { pid: child.pid, unref: () => child.unref() }
  },
}

async function reserveForStart(
  dependencies: DetachedStartDependencies,
): Promise<WatcherPidReservation> {
  try {
    return await dependencies.reservePid()
  } catch (error) {
    if (!(error instanceof PropertyWatcherProcessError) || error.code !== "PID_ALREADY_RESERVED") {
      throw error
    }
    const stale = await dependencies.readRecord()
    const snapshot = await dependencies.inspectProcess(stale.pid)
    if (snapshot !== null && matchesWatcherProcessIdentity(stale, snapshot)) throw error
    const rechecked = await dependencies.readRecord()
    if (!sameWatcherPidRecord(stale, rechecked)) {
      throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
    }
    await dependencies.removeReservation()
    return dependencies.reservePid()
  }
}

export async function startDetachedPropertyWatcher(
  runnerPath: string,
  dependencies: DetachedStartDependencies = nodeStartDependencies,
): Promise<WatcherPidRecord> {
  if (!isAbsolute(runnerPath)) throw new PropertyWatcherProcessError("SPAWN_FAILED")
  try {
    await dependencies.ensureArtifactDirectory()
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT") {
      throw new PropertyWatcherProcessError("ARTIFACT_DIRECTORY_MISSING", { cause: error })
    }
    throw error
  }
  const reservation = await reserveForStart(dependencies)
  try {
    const tokenResult = z.uuid().safeParse(dependencies.randomUuid())
    if (!tokenResult.success) throw new PropertyWatcherProcessError("SPAWN_FAILED")
    const instanceToken = tokenResult.data
    const log = await dependencies.openAppendLog()
    const child = await (async () => {
      try {
        const spawned = dependencies.spawn({
          executable: process.execPath,
          arguments: [`--title=opencode-property-watcher:${instanceToken}`, runnerPath, "watch"],
          options: {
            detached: true,
            env: { ...process.env, [PROPERTY_WATCHER_TOKEN_ENV]: instanceToken },
            shell: false,
            stdio: ["ignore", log.fd, log.fd],
            windowsHide: true,
          },
        })
        spawned.unref()
        return spawned
      } finally {
        await log.close()
      }
    })()
    const snapshot = await dependencies.inspectProcess(child.pid)
    if (snapshot === null) throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
    const record: WatcherPidRecord = {
      version: 1,
      state: "starting",
      pid: child.pid,
      executablePath: snapshot.executablePath,
      runnerPath,
      instanceToken,
      creationIdentity: snapshot.creationIdentity,
      pipeName: watcherPipeName(instanceToken),
    }
    if (!matchesWatcherProcessIdentity(record, snapshot)) {
      throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
    }
    await reservation.write(JSON.stringify(record))
    await reservation.sync()
    await reservation.close()
    return record
  } catch (error) {
    await Promise.allSettled([reservation.close(), dependencies.removeReservation()])
    throw error
  }
}
