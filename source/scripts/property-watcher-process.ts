import { execFile } from "node:child_process"
import { open, stat } from "node:fs/promises"

import { z } from "zod"

export const PROPERTY_WATCHER_ARTIFACT_DIRECTORY =
  "C:\\Users\\admin\\AppData\\Local\\Temp\\opencode"
export const PROPERTY_WATCHER_PID_FILE = `${PROPERTY_WATCHER_ARTIFACT_DIRECTORY}\\property-watcher.pid.json`
export const PROPERTY_WATCHER_LOG_FILE = `${PROPERTY_WATCHER_ARTIFACT_DIRECTORY}\\property-watcher.log`
export const PROPERTY_WATCHER_TOKEN_ENV = "PROPERTY_WATCHER_INSTANCE_TOKEN"

export type PropertyWatcherProcessErrorCode =
  | "ARTIFACT_DIRECTORY_MISSING"
  | "IDENTITY_MISMATCH"
  | "PID_ALREADY_RESERVED"
  | "PID_RECORD_INVALID"
  | "PID_RECORD_MISSING"
  | "SPAWN_FAILED"

export class PropertyWatcherProcessError extends Error {
  override readonly name = "PropertyWatcherProcessError"
  readonly code: PropertyWatcherProcessErrorCode

  constructor(code: PropertyWatcherProcessErrorCode, options?: ErrorOptions) {
    super(code, options)
    this.code = code
  }
}

const pidRecordSchema = z
  .object({
    version: z.literal(1),
    state: z.union([z.literal("starting"), z.literal("ready")]),
    pid: z.number().int().positive(),
    executablePath: z.string().min(1),
    runnerPath: z.string().min(1),
    instanceToken: z.uuid(),
    creationIdentity: z.string().min(1),
    pipeName: z.string().min(1),
  })
  .strict()

const processSnapshotSchema = z
  .object({
    pid: z.number().int().positive(),
    executablePath: z.string().min(1),
    commandLine: z.string().min(1),
    creationIdentity: z.string().min(1),
  })
  .strict()
  .nullable()

export type WatcherPidRecord = z.infer<typeof pidRecordSchema>
export type WatcherProcessSnapshot = Exclude<z.infer<typeof processSnapshotSchema>, null>

function hasErrorCode(error: unknown, code: string): boolean {
  return error instanceof Error && "code" in error && error.code === code
}

export function watcherPipeName(instanceToken: string): string {
  return `\\\\.\\pipe\\opencode-property-watcher-${instanceToken}`
}

export function parseWatcherPidRecord(value: unknown): WatcherPidRecord {
  const result = pidRecordSchema.safeParse(value)
  if (!result.success || result.data.pipeName !== watcherPipeName(result.data.instanceToken)) {
    throw new PropertyWatcherProcessError("PID_RECORD_INVALID")
  }
  return result.data
}

function normalizedPath(value: string): string {
  return value.replaceAll("/", "\\").toLocaleLowerCase("en-US")
}

export function matchesWatcherProcessIdentity(
  record: WatcherPidRecord,
  snapshot: WatcherProcessSnapshot,
): boolean {
  const command = normalizedPath(snapshot.commandLine)
  return (
    record.pid === snapshot.pid &&
    normalizedPath(record.executablePath) === normalizedPath(snapshot.executablePath) &&
    record.creationIdentity === snapshot.creationIdentity &&
    command.includes(normalizedPath(record.runnerPath)) &&
    command.includes(" watch") &&
    command.includes(`--title=opencode-property-watcher:${record.instanceToken}`)
  )
}

export function sameWatcherInstance(left: WatcherPidRecord, right: WatcherPidRecord): boolean {
  return (
    left.pid === right.pid &&
    left.instanceToken === right.instanceToken &&
    left.creationIdentity === right.creationIdentity
  )
}

export function sameWatcherPidRecord(left: WatcherPidRecord, right: WatcherPidRecord): boolean {
  return (
    sameWatcherInstance(left, right) &&
    left.version === right.version &&
    left.state === right.state &&
    left.executablePath === right.executablePath &&
    left.runnerPath === right.runnerPath &&
    left.pipeName === right.pipeName
  )
}

const INSPECT_PROCESS_SCRIPT =
  "& { param([int]$targetPid);$process=Get-CimInstance Win32_Process -Filter ('ProcessId = ' + $targetPid);if($null -eq $process){'null'}else{[pscustomobject]@{pid=[int]$process.ProcessId;executablePath=[string]$process.ExecutablePath;commandLine=[string]$process.CommandLine;creationIdentity=[string]$process.CreationDate}|ConvertTo-Json -Compress} }"

export async function inspectWatcherProcess(pid: number): Promise<WatcherProcessSnapshot | null> {
  const stdout = await new Promise<string>((resolve, reject) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-Command", INSPECT_PROCESS_SCRIPT, String(pid)],
      { encoding: "utf8", windowsHide: true },
      (error, value) => {
        if (error === null) resolve(value)
        else reject(new PropertyWatcherProcessError("IDENTITY_MISMATCH", { cause: error }))
      },
    )
  })
  let parsed: unknown
  try {
    parsed = JSON.parse(stdout)
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new PropertyWatcherProcessError("IDENTITY_MISMATCH", { cause: error })
    }
    throw error
  }
  const result = processSnapshotSchema.safeParse(parsed)
  if (!result.success) throw new PropertyWatcherProcessError("IDENTITY_MISMATCH")
  return result.data
}

export type WatcherPidReservation = {
  readonly write: (value: string) => Promise<void>
  readonly sync: () => Promise<void>
  readonly close: () => Promise<void>
}

export async function reserveWatcherPidFile(path: string): Promise<WatcherPidReservation> {
  try {
    const handle = await open(path, "wx")
    return {
      write: async (value) => {
        await handle.writeFile(value)
      },
      sync: () => handle.sync(),
      close: () => handle.close(),
    }
  } catch (error) {
    if (hasErrorCode(error, "EEXIST")) {
      throw new PropertyWatcherProcessError("PID_ALREADY_RESERVED")
    }
    throw error
  }
}

export type WatcherDirectoryStat = {
  readonly isDirectory: () => boolean
}

export async function ensureWatcherArtifactDirectory(
  path = PROPERTY_WATCHER_ARTIFACT_DIRECTORY,
  readStat: (path: string) => Promise<WatcherDirectoryStat> = stat,
): Promise<void> {
  let information: WatcherDirectoryStat
  try {
    information = await readStat(path)
  } catch (error) {
    if (hasErrorCode(error, "ENOENT")) {
      throw new PropertyWatcherProcessError("ARTIFACT_DIRECTORY_MISSING", { cause: error })
    }
    throw error
  }
  if (!information.isDirectory()) {
    throw new PropertyWatcherProcessError("ARTIFACT_DIRECTORY_MISSING")
  }
}
