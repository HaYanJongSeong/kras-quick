import assert from "node:assert/strict"
import { type ChildProcessWithoutNullStreams, execFile, spawn } from "node:child_process"
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { createInterface } from "node:readline"
import { describe, it, type TestContext } from "node:test"

import { parseParcelAddress } from "../scripts/property-address.ts"
import { updateProgressFile } from "../scripts/property-progress.ts"
import { compareAndWriteLocked, LockedWriteError } from "../scripts/property-progress-windows.ts"
import { parsePropertyPager } from "../scripts/property-watcher-payload.ts"

it("invokes PowerShell through an encoded non-shell boundary without GUI handlers or path arguments", async () => {
  // Given
  const writerUrl = new URL("../scripts/property-progress-windows.ts", import.meta.url)

  // When
  const source = await readFile(writerUrl, "utf8")

  // Then
  assert.doesNotMatch(source, /["']-Command["']/)
  assert.doesNotMatch(source, /GetCommandLineArgs/i)
  assert.doesNotMatch(
    source,
    /Start-Process|Invoke-Item|explorer(?:\.exe)?|ShellExecute|AppActivate|SetForegroundWindow/i,
  )
  assert.match(source, /function executePowerShell\(encodedCommand: string\)/)
  assert.match(
    source,
    /execFile\(\s*"powershell\.exe",\s*\[\s*"-NoProfile",\s*"-NonInteractive",\s*"-EncodedCommand",\s*encodedCommand\s*\]/s,
  )
  assert.match(source, /shell:\s*false/)
  assert.match(source, /windowsHide:\s*true/)
  assert.doesNotMatch(
    source,
    /"-EncodedCommand",\s*encodedCommand\s*,\s*(?:targetPath|expectedSnapshotPath|replacementPath)/s,
  )
})

function encodePowerShell(script: string): string {
  return Buffer.from(script, "utf16le").toString("base64")
}

function exclusiveLockScript(targetPath: string): string {
  const targetPathBase64 = Buffer.from(targetPath, "utf8").toString("base64")
  return `
& {
  [string]$TargetPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("${targetPathBase64}"))
  $stream = [System.IO.FileStream]::new(
    $TargetPath,
    [System.IO.FileMode]::Open,
    [System.IO.FileAccess]::ReadWrite,
    [System.IO.FileShare]::None
  )
  try {
    [Console]::Out.WriteLine("locked")
    [Console]::Out.Flush()
    [Console]::In.ReadLine() | Out-Null
  } finally {
    $stream.Dispose()
  }
}
`
}

type LockedFixture = {
  readonly targetPath: string
  readonly expectedPath: string
  readonly replacementPath: string
}

async function createFixture(
  context: TestContext,
  target: Uint8Array,
  replacement: Uint8Array = Buffer.from("replacement"),
): Promise<LockedFixture> {
  const directory = await mkdtemp(join(tmpdir(), "재산 진행 & (잠금) O'Brien-"))
  const targetPath = join(directory, "대상 진행 & (현재) O'Brien.txt")
  const expectedPath = join(directory, "예상 & (원본) O'Brien.tmp")
  const replacementPath = join(directory, "교체 & (새 파일) O'Brien.tmp")
  await Promise.all([
    writeFile(targetPath, target),
    writeFile(expectedPath, target),
    writeFile(replacementPath, replacement),
  ])
  context.after(() => rm(directory, { force: true, recursive: true }))
  return { targetPath, expectedPath, replacementPath }
}

async function firstOutputLine(child: ChildProcessWithoutNullStreams): Promise<string> {
  const lines = createInterface({ input: child.stdout, crlfDelay: Number.POSITIVE_INFINITY })
  for await (const line of lines) return line
  throw new LockedWriteError()
}

function waitForExit(child: ChildProcessWithoutNullStreams): Promise<number | null> {
  return new Promise((resolve, reject) => {
    child.once("error", reject)
    child.once("exit", (code) => resolve(code))
  })
}

function notepadProcessIds(): Promise<readonly number[]> {
  const script = encodePowerShell(`
[System.Diagnostics.Process]::GetProcessesByName("notepad") |
  ForEach-Object { [Console]::Out.WriteLine($_.Id) }
`)
  return new Promise((resolve, reject) => {
    execFile(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-EncodedCommand", script],
      { encoding: "utf8", shell: false, windowsHide: true },
      (error, stdout) => {
        if (error !== null) {
          reject(error)
          return
        }
        const processIds = stdout
          .split(/\r?\n/u)
          .filter((line) => line.length > 0)
          .map((line) => Number.parseInt(line, 10))
          .sort((left, right) => left - right)
        resolve(processIds)
      },
    )
  })
}

describe("Windows locked progress writer", { skip: process.platform !== "win32" }, () => {
  it("writes the replacement when target bytes match expected bytes", async (context) => {
    // Given
    const fixture = await createFixture(context, Buffer.from("expected"))

    // When
    const result = await compareAndWriteLocked(
      fixture.targetPath,
      fixture.expectedPath,
      fixture.replacementPath,
    )

    // Then
    assert.equal(result, "written")
    assert.deepEqual(await readFile(fixture.targetPath), Buffer.from("replacement"))
  })

  it("truncates a stale tail when the replacement is shorter", async (context) => {
    // Given
    const fixture = await createFixture(
      context,
      Buffer.from("expected-with-stale-tail"),
      Buffer.from("short"),
    )

    // When
    const result = await compareAndWriteLocked(
      fixture.targetPath,
      fixture.expectedPath,
      fixture.replacementPath,
    )

    // Then
    assert.equal(result, "written")
    assert.deepEqual(await readFile(fixture.targetPath), Buffer.from("short"))
  })

  it("returns source_changed and preserves mismatched target bytes", async (context) => {
    // Given
    const fixture = await createFixture(context, Buffer.from("expected"))
    const changed = Buffer.from("changed-by-another-writer")
    await writeFile(fixture.targetPath, changed)

    // When
    const result = await compareAndWriteLocked(
      fixture.targetPath,
      fixture.expectedPath,
      fixture.replacementPath,
    )

    // Then
    assert.equal(result, "source_changed")
    assert.deepEqual(await readFile(fixture.targetPath), changed)
  })

  it("fails without retrying when another handle denies sharing", async (context) => {
    // Given
    const fixture = await createFixture(context, Buffer.from("expected"))
    const lockProcess = spawn(
      "powershell.exe",
      [
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        encodePowerShell(exclusiveLockScript(fixture.targetPath)),
      ],
      { shell: false, windowsHide: true },
    )
    assert.equal(await firstOutputLine(lockProcess), "locked")

    // When / Then
    try {
      await assert.rejects(
        compareAndWriteLocked(fixture.targetPath, fixture.expectedPath, fixture.replacementPath),
        (error: unknown) => error instanceof LockedWriteError,
      )
    } finally {
      const exit = waitForExit(lockProcess)
      lockProcess.stdin.end("\n")
      assert.equal(await exit, 0)
    }
    assert.deepEqual(await readFile(fixture.targetPath), Buffer.from("expected"))
  })

  it("updates a temporary progress TXT without changing the Notepad PID set", async (context) => {
    // Given
    const directory = await mkdtemp(join(tmpdir(), "재산 진행 & (메모장 없음) O'Brien-"))
    const progressPath = join(directory, "현장 진행 & (현재) O'Brien.txt")
    const sourceText =
      "\uFEFF현재 어디까지?\r\n7/16(50개단위)(현장조사 완료여부 완료체크 후)\r\n서울특별시 종로구 청운동 1\r\n\r\n"
    await writeFile(progressPath, Buffer.from(sourceText))
    context.after(() => rm(directory, { force: true, recursive: true }))
    const processIdsBefore = await notepadProcessIds()
    const pager = parsePropertyPager("8/16")
    assert.ok(pager)

    // When
    await updateProgressFile(
      progressPath,
      parseParcelAddress("서울특별시 강서구 개화동 255-1"),
      pager,
    )
    const processIdsAfter = await notepadProcessIds()

    // Then
    assert.deepEqual(processIdsAfter, processIdsBefore)
    assert.deepEqual(
      await readFile(progressPath),
      Buffer.from(
        "\uFEFF현재 어디까지?\r\n8/16(50개단위)(현장조사 완료여부 완료체크 후)\r\n서울특별시 강서구 개화동 255-1\r\n\r\n",
      ),
    )
  })
})
