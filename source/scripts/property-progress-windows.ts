import { execFile } from "node:child_process"
import { lstat } from "node:fs/promises"

export type LockedWriteResult = "written" | "source_changed"

type PowerShellResult = { readonly ok: true; readonly stdout: string } | { readonly ok: false }

function createEncodedCommand(
  targetPath: string,
  expectedSnapshotPath: string,
  replacementPath: string,
): string {
  const targetPathBase64 = Buffer.from(targetPath, "utf8").toString("base64")
  const expectedSnapshotPathBase64 = Buffer.from(expectedSnapshotPath, "utf8").toString("base64")
  const replacementPathBase64 = Buffer.from(replacementPath, "utf8").toString("base64")
  const script = `
& {
  $ErrorActionPreference = "Stop"
  try {
    [string]$TargetPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("${targetPathBase64}"))
    [string]$ExpectedSnapshotPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("${expectedSnapshotPathBase64}"))
    [string]$ReplacementPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("${replacementPathBase64}"))
    $attributes = [System.IO.File]::GetAttributes($TargetPath)
    if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "reparse target"
    }
    [byte[]]$expected = [System.IO.File]::ReadAllBytes($ExpectedSnapshotPath)
    [byte[]]$replacement = [System.IO.File]::ReadAllBytes($ReplacementPath)
    $target = [System.IO.FileStream]::new(
      $TargetPath,
      [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::ReadWrite,
      [System.IO.FileShare]::Read
    )
    try {
      $matches = $target.Length -eq $expected.LongLength
      if ($matches) {
        for ($index = 0; $index -lt $expected.Length; $index += 1) {
          if ($target.ReadByte() -ne $expected[$index]) {
            $matches = $false
            break
          }
        }
      }
      if (-not $matches) {
        [Console]::Out.Write("source_changed")
        return
      }
      $target.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
      $target.Write($replacement, 0, $replacement.Length)
      $target.SetLength($replacement.LongLength)
      $target.Flush($true)
      [Console]::Out.Write("written")
    } finally {
      $target.Dispose()
    }
  } catch {
    [Console]::Error.Write("io_failure")
    exit 1
  }
}
`
  return Buffer.from(script, "utf16le").toString("base64")
}

const POWERSHELL_OPTIONS = {
  encoding: "utf8",
  shell: false,
  timeout: 30000,
  windowsHide: true,
} as const

export class LockedWriteError extends Error {
  override readonly name = "LockedWriteError"

  constructor() {
    super("Windows locked progress write failed.")
  }
}

function executePowerShell(encodedCommand: string): Promise<PowerShellResult> {
  return new Promise((resolve) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-EncodedCommand", encodedCommand],
      POWERSHELL_OPTIONS,
      (error, stdout) => {
        if (error !== null) {
          resolve({ ok: false })
          return
        }
        resolve({ ok: true, stdout })
      },
    )
  })
}

export async function compareAndWriteLocked(
  targetPath: string,
  expectedSnapshotPath: string,
  replacementPath: string,
): Promise<LockedWriteResult> {
  if (process.platform !== "win32") throw new LockedWriteError()

  try {
    const target = await lstat(targetPath)
    if (!target.isFile() || target.isSymbolicLink()) throw new LockedWriteError()
  } catch (error) {
    if (error instanceof LockedWriteError) throw error
    throw new LockedWriteError()
  }

  const result = await executePowerShell(
    createEncodedCommand(targetPath, expectedSnapshotPath, replacementPath),
  )
  if (!result.ok) throw new LockedWriteError()

  switch (result.stdout.trim()) {
    case "written":
      return "written"
    case "source_changed":
      return "source_changed"
    default:
      throw new LockedWriteError()
  }
}
