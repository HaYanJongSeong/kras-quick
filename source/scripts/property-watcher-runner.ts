import { fileURLToPath } from "node:url"

import { z } from "zod"

import type { PropertyWatcherEvent, PropertyWatcherLogger } from "./property-watcher.ts"
import {
  runForegroundPropertyWatcherLifecycle,
  runLiveDetachedPropertyWatcher,
} from "./property-watcher-lifecycle.ts"
import {
  PROPERTY_WATCHER_TOKEN_ENV,
  PropertyWatcherProcessError,
} from "./property-watcher-process.ts"
import {
  connectPropertyWatcherBrowser,
  runPropertyWatcherDryRun,
} from "./property-watcher-runtime.ts"
import { startDetachedPropertyWatcher } from "./property-watcher-start.ts"
import { getWatcherStatus, requestWatcherStop } from "./property-watcher-state.ts"

const commandSchema = z.union([
  z.literal("watch"),
  z.literal("dry_run"),
  z.literal("start"),
  z.literal("status"),
  z.literal("stop"),
])
const instanceTokenSchema = z.uuid()

export type PropertyWatcherCommand =
  | { readonly kind: "watch" }
  | { readonly kind: "dry_run" }
  | { readonly kind: "start" }
  | { readonly kind: "status" }
  | { readonly kind: "stop" }

export class PropertyWatcherUsageError extends Error {
  override readonly name = "PropertyWatcherUsageError"
}

function assertNever(value: never): never {
  throw new PropertyWatcherUsageError(`Unsupported command: ${String(value)}`)
}

export function parsePropertyWatcherCommand(args: readonly string[]): PropertyWatcherCommand {
  if (args.length !== 1) throw new PropertyWatcherUsageError("Expected exactly one command")
  const result = commandSchema.safeParse(args[0])
  if (!result.success) throw new PropertyWatcherUsageError("Unknown command")
  return { kind: result.data }
}

export type PropertyWatcherRunnerActions = {
  readonly watch: () => Promise<void>
  readonly dryRun: () => Promise<void>
  readonly start: () => Promise<void>
  readonly status: () => Promise<void>
  readonly stop: () => Promise<void>
}

export type PropertyWatcherCliCompletion = {
  readonly flushStdout: () => Promise<void>
  readonly exitSuccess: () => void
}

export type PropertyWatcherCliFailureCompletion = {
  readonly writeError: (line: string) => void
  readonly flushStderr: () => Promise<void>
  readonly exitFailure: () => void
}

export async function runPropertyWatcherCli(
  runCommand: () => Promise<void>,
  completion: PropertyWatcherCliFailureCompletion,
): Promise<void> {
  try {
    await runCommand()
  } catch (error) {
    const name = error instanceof Error ? error.name : "UnknownError"
    try {
      completion.writeError(JSON.stringify({ status: "error", name }))
      await completion.flushStderr()
    } finally {
      completion.exitFailure()
    }
  }
}

export async function executePropertyWatcherCommand(
  command: PropertyWatcherCommand,
  actions: PropertyWatcherRunnerActions,
  completion?: PropertyWatcherCliCompletion,
): Promise<void> {
  switch (command.kind) {
    case "watch":
      await actions.watch()
      if (completion !== undefined) {
        await completion.flushStdout()
        completion.exitSuccess()
      }
      return
    case "dry_run":
      await actions.dryRun()
      if (completion !== undefined) {
        await completion.flushStdout()
        completion.exitSuccess()
      }
      return
    case "start":
      return actions.start()
    case "status":
      return actions.status()
    case "stop":
      return actions.stop()
    default:
      return assertNever(command)
  }
}

function writeJson(value: object): void {
  process.stdout.write(`${JSON.stringify(value)}\n`)
}

const logger: PropertyWatcherLogger = {
  log: (event: PropertyWatcherEvent, count: number) => writeJson({ event, count }),
}

async function runWatch(): Promise<void> {
  const tokenValue: unknown = process.env[PROPERTY_WATCHER_TOKEN_ENV]
  if (tokenValue === undefined) return runForegroundPropertyWatcherLifecycle(logger)
  const token = instanceTokenSchema.safeParse(tokenValue)
  if (!token.success) throw new PropertyWatcherProcessError("PID_RECORD_INVALID")
  return runLiveDetachedPropertyWatcher(token.data, logger)
}

const runnerPath = fileURLToPath(import.meta.url)
const cliCompletion: PropertyWatcherCliCompletion = {
  flushStdout: () =>
    new Promise((resolve, reject) => {
      process.stdout.write("", (error) => {
        if (error === null || error === undefined) resolve()
        else reject(error)
      })
    }),
  exitSuccess: () => process.exit(0),
}
const cliFailureCompletion: PropertyWatcherCliFailureCompletion = {
  writeError: (line) => {
    process.stderr.write(`${line}\n`)
  },
  flushStderr: () =>
    new Promise((resolve, reject) => {
      process.stderr.write("", (error) => {
        if (error === null || error === undefined) resolve()
        else reject(error)
      })
    }),
  exitFailure: () => process.exit(1),
}
const actions: PropertyWatcherRunnerActions = {
  watch: runWatch,
  dryRun: () =>
    runPropertyWatcherDryRun({
      connect: connectPropertyWatcherBrowser,
      writeLine: (line) => process.stdout.write(`${line}\n`),
    }),
  start: async () => {
    const record = await startDetachedPropertyWatcher(runnerPath)
    writeJson({ status: record.state, pid: record.pid })
  },
  status: async () => writeJson(await getWatcherStatus()),
  stop: async () => {
    await requestWatcherStop()
    writeJson({ status: "stopped" })
  },
}

async function main(): Promise<void> {
  await executePropertyWatcherCommand(
    parsePropertyWatcherCommand(process.argv.slice(2)),
    actions,
    cliCompletion,
  )
}

if (process.argv[1] !== undefined && fileURLToPath(import.meta.url) === process.argv[1]) {
  void runPropertyWatcherCli(main, cliFailureCompletion).catch(() => process.exit(1))
}
