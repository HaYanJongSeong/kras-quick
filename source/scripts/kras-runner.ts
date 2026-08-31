import { applyKrasDarkMode, lookupKras } from "./kras-lookup.ts"

type RunnerMode = "lookup" | "dark_mode"

type RunnerCommand =
  | { readonly kind: "lookup"; readonly address: string; readonly darkMode: boolean }
  | { readonly kind: "dark_mode" }

class KrasRunnerUsageError extends Error {
  override readonly name = "KrasRunnerUsageError"
}

function assertNever(value: never): never {
  throw new KrasRunnerUsageError(`지원하지 않는 KRAS 실행 모드: ${String(value)}`)
}

function parseMode(value: string | undefined): RunnerMode {
  if (value === "lookup" || value === "dark_mode") return value
  throw new KrasRunnerUsageError("KRAS 실행 모드가 올바르지 않습니다.")
}

function parseDarkMode(value: string | undefined): boolean {
  switch (value) {
    case "true":
      return true
    case "false":
      return false
    default:
      throw new KrasRunnerUsageError("다크 모드 인수가 올바르지 않습니다.")
  }
}

function parseCommand(args: readonly string[]): RunnerCommand {
  const mode = parseMode(args[0])
  switch (mode) {
    case "lookup": {
      const address = args[1]
      if (args.length !== 3 || address === undefined || address.trim().length === 0) {
        throw new KrasRunnerUsageError("조회 주소 인수가 올바르지 않습니다.")
      }
      return { kind: "lookup", address, darkMode: parseDarkMode(args[2]) }
    }
    case "dark_mode":
      if (args.length !== 1) throw new KrasRunnerUsageError("다크 모드 인수가 너무 많습니다.")
      return { kind: "dark_mode" }
    default:
      return assertNever(mode)
  }
}

async function executeCommand(command: RunnerCommand): Promise<string> {
  switch (command.kind) {
    case "lookup":
      return lookupKras(command.address, command.darkMode)
    case "dark_mode":
      return applyKrasDarkMode()
    default:
      return assertNever(command)
  }
}

const output = await executeCommand(parseCommand(process.argv.slice(2)))
process.stdout.write(output)
