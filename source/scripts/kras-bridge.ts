import { execFile } from "node:child_process"
import { fileURLToPath } from "node:url"

const RUNNER_PATH = fileURLToPath(new URL("./kras-runner.ts", import.meta.url))
const CHILD_OPTIONS = { encoding: "utf8", timeout: 30000, windowsHide: true } as const

export type ChildCallOptions = {
  readonly encoding: "utf8"
  readonly timeout: number
  readonly windowsHide: boolean
}

export type ChildResult = { readonly ok: true; readonly stdout: string } | { readonly ok: false }

export type ChildExecutor = (
  executable: string,
  args: readonly string[],
  options: ChildCallOptions,
) => Promise<ChildResult>

export type KrasChildCommand =
  | { readonly kind: "lookup"; readonly address: string; readonly darkMode: boolean }
  | { readonly kind: "dark_mode" }

class KrasChildError extends Error {
  override readonly name = "KrasChildError"
}

function assertNever(value: never): never {
  throw new KrasChildError(`지원하지 않는 KRAS 실행 모드: ${String(value)}`)
}

function childArgs(command: KrasChildCommand): readonly string[] {
  switch (command.kind) {
    case "lookup":
      return [RUNNER_PATH, "lookup", command.address, String(command.darkMode)]
    case "dark_mode":
      return [RUNNER_PATH, "dark_mode"]
    default:
      return assertNever(command)
  }
}

const executeNode: ChildExecutor = (executable, args, options) =>
  new Promise((resolve) => {
    execFile(executable, [...args], options, (error, stdout) => {
      if (error !== null) {
        resolve({ ok: false })
        return
      }
      resolve({ ok: true, stdout })
    })
  })

export async function runKrasChild(
  execute: ChildExecutor,
  command: KrasChildCommand,
): Promise<string> {
  const result = await execute("node", childArgs(command), CHILD_OPTIONS)
  if (!result.ok) throw new KrasChildError("KRAS 실행에 실패했습니다.")
  const output = result.stdout.trim()
  if (output.length === 0) throw new KrasChildError("KRAS 실행 결과가 비어 있습니다.")
  return output
}

export function lookupKrasViaNode(address: string, darkMode: boolean): Promise<string> {
  return runKrasChild(executeNode, { kind: "lookup", address, darkMode })
}

export function applyKrasDarkModeViaNode(): Promise<string> {
  return runKrasChild(executeNode, { kind: "dark_mode" })
}
