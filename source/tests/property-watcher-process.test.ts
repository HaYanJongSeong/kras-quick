import assert from "node:assert/strict"
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { describe, it, type TestContext } from "node:test"

import {
  openWatcherStopServer,
  PropertyWatcherControlError,
  sendWatcherStop,
} from "../scripts/property-watcher-control.ts"
import {
  inspectWatcherProcess,
  matchesWatcherProcessIdentity,
  PropertyWatcherProcessError,
  parseWatcherPidRecord,
  reserveWatcherPidFile,
  type WatcherPidRecord,
} from "../scripts/property-watcher-process.ts"
import {
  type DetachedStartDependencies,
  startDetachedPropertyWatcher,
} from "../scripts/property-watcher-start.ts"
import {
  getWatcherStatus,
  removeMatchingWatcherPidRecord,
  requestWatcherStop,
} from "../scripts/property-watcher-state.ts"

const TOKEN = "91f2c793-e7c4-49f0-b394-962599aea376"
const OTHER_TOKEN = "446c43c4-f434-412b-9a6e-48da61614954"
const EXECUTABLE = "C:\\Program Files\\nodejs\\node.exe"
const RUNNER = "C:\\Users\\admin\\.config\\opencode\\scripts\\property-watcher-runner.ts"
const CREATED = "20260721140531.000000+540"

class SyntheticFsError extends Error {
  override readonly name = "SyntheticFsError"
  readonly code = "ENOENT"
}

function record(overrides: Partial<WatcherPidRecord> = {}): WatcherPidRecord {
  return {
    version: 1,
    state: "ready",
    pid: 4242,
    executablePath: EXECUTABLE,
    runnerPath: RUNNER,
    instanceToken: TOKEN,
    creationIdentity: CREATED,
    pipeName: `\\\\.\\pipe\\opencode-property-watcher-${TOKEN}`,
    ...overrides,
  }
}

async function fixtureDirectory(context: TestContext): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "property-process-"))
  context.after(() => rm(directory, { force: true, recursive: true }))
  return directory
}

describe("property watcher process identity", () => {
  it("inspects the current Windows process as exactly one process snapshot", {
    skip: process.platform !== "win32",
  }, async () => {
    // Given / When
    const snapshot = await inspectWatcherProcess(process.pid)

    // Then
    assert.notEqual(snapshot, null)
    if (snapshot === null) throw new SyntheticFsError("current process was not found")
    assert.equal(snapshot.pid, process.pid)
    assert.equal(
      snapshot.executablePath.replaceAll("/", "\\").toLocaleLowerCase("en-US"),
      process.execPath.replaceAll("/", "\\").toLocaleLowerCase("en-US"),
    )
    assert.notEqual(snapshot.creationIdentity, "")
  })

  it("returns null for a missing Windows process", {
    skip: process.platform !== "win32",
  }, async () => {
    // Given / When
    const snapshot = await inspectWatcherProcess(2_147_483_647)

    // Then
    assert.equal(snapshot, null)
  })

  it("parses the PID JSON boundary and rejects malformed or extra data", () => {
    // Given
    const valid = record()

    // When / Then
    assert.deepEqual(parseWatcherPidRecord(valid), valid)
    for (const invalid of [
      { ...valid, pid: -1 },
      { ...valid, instanceToken: "not-a-uuid" },
      { ...valid, extra: true },
      JSON.stringify(valid),
    ]) {
      assert.throws(
        () => parseWatcherPidRecord(invalid),
        (error: unknown) => error instanceof PropertyWatcherProcessError,
      )
    }
  })

  it("rejects a stale or PID-reused process identity", () => {
    // Given
    const expected = record()
    const matching = {
      pid: expected.pid,
      executablePath: expected.executablePath,
      commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
      creationIdentity: expected.creationIdentity,
    }

    // When / Then
    assert.equal(matchesWatcherProcessIdentity(expected, matching), true)
    assert.equal(
      matchesWatcherProcessIdentity(expected, { ...matching, creationIdentity: "reused" }),
      false,
    )
    assert.equal(
      matchesWatcherProcessIdentity(expected, {
        ...matching,
        commandLine: matching.commandLine.replace(TOKEN, OTHER_TOKEN),
      }),
      false,
    )
  })

  it("reports stale identity and refuses to signal a reused PID", async () => {
    // Given
    const expected = record()
    const sent: string[] = []
    const dependencies = {
      readRecord: async () => expected,
      inspectProcess: async () => ({
        pid: expected.pid,
        executablePath: expected.executablePath,
        commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
        creationIdentity: "reused",
      }),
      sendStop: async (pipeName: string) => {
        sent.push(pipeName)
      },
    }

    // When
    const status = await getWatcherStatus(dependencies)

    // Then
    assert.deepEqual(status, { status: "stale", pid: expected.pid })
    await assert.rejects(
      requestWatcherStop(dependencies),
      (error: unknown) =>
        error instanceof PropertyWatcherProcessError && error.code === "IDENTITY_MISMATCH",
    )
    assert.deepEqual(sent, [])
  })

  it("reserves the PID file exclusively under a deterministic start race", async (context) => {
    // Given
    const directory = await fixtureDirectory(context)
    const pidPath = join(directory, "watcher.pid.json")

    // When
    const results = await Promise.allSettled([
      reserveWatcherPidFile(pidPath),
      reserveWatcherPidFile(pidPath),
    ])

    // Then
    const successes = results.filter((result) => result.status === "fulfilled")
    const failures = results.filter((result) => result.status === "rejected")
    assert.equal(successes.length, 1)
    assert.equal(failures.length, 1)
    for (const success of successes) {
      if (success.status === "fulfilled") await success.value.close()
    }
  })

  it("spawns Node detached with fixed safe options and writes a starting identity", async () => {
    // Given
    const writes: string[] = []
    const spawnRequests: Parameters<DetachedStartDependencies["spawn"]>[0][] = []
    const dependencies: DetachedStartDependencies = {
      ensureArtifactDirectory: async () => undefined,
      inspectProcess: async () => ({
        pid: 4242,
        executablePath: EXECUTABLE,
        commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
        creationIdentity: CREATED,
      }),
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => record(),
      removeReservation: async () => undefined,
      reservePid: async () => ({
        close: async () => undefined,
        sync: async () => undefined,
        write: async (value) => {
          writes.push(value)
        },
      }),
      spawn: (request) => {
        spawnRequests.push(request)
        return { pid: 4242, unref: () => undefined }
      },
    }

    // When
    const started = await startDetachedPropertyWatcher(RUNNER, dependencies)

    // Then
    assert.equal(started.state, "starting")
    assert.equal(parseWatcherPidRecord(JSON.parse(writes[0] ?? "null")).instanceToken, TOKEN)
    assert.deepEqual(spawnRequests, [
      {
        executable: process.execPath,
        arguments: [`--title=opencode-property-watcher:${TOKEN}`, RUNNER, "watch"],
        options: {
          detached: true,
          env: { ...process.env, PROPERTY_WATCHER_INSTANCE_TOKEN: TOKEN },
          shell: false,
          stdio: ["ignore", 72, 72],
          windowsHide: true,
        },
      },
    ])
  })

  it("maps a missing artifact directory to the typed process error", async () => {
    // Given
    const dependencies: DetachedStartDependencies = {
      ensureArtifactDirectory: async () => {
        throw new SyntheticFsError("missing")
      },
      inspectProcess: async () => null,
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => record(),
      removeReservation: async () => undefined,
      reservePid: async () => {
        throw new Error("reservation must not run")
      },
      spawn: () => ({ pid: 4242, unref: () => undefined }),
    }

    // When / Then
    await assert.rejects(
      startDetachedPropertyWatcher(RUNNER, dependencies),
      (error: unknown) =>
        error instanceof PropertyWatcherProcessError && error.code === "ARTIFACT_DIRECTORY_MISSING",
    )
  })

  it("removes a verified absent stale record and retries exclusive reservation", async () => {
    // Given
    const stale = record()
    const actions: string[] = []
    let reserveCalls = 0
    let inspectionCalls = 0
    const dependencies = {
      ensureArtifactDirectory: async () => undefined,
      inspectProcess: async () => {
        inspectionCalls += 1
        if (inspectionCalls === 1) return null
        return {
          pid: stale.pid,
          executablePath: stale.executablePath,
          commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
          creationIdentity: stale.creationIdentity,
        }
      },
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => {
        actions.push("read")
        return stale
      },
      removeReservation: async () => {
        actions.push("remove")
      },
      reservePid: async () => {
        reserveCalls += 1
        actions.push(`reserve:${reserveCalls}`)
        if (reserveCalls === 1) throw new PropertyWatcherProcessError("PID_ALREADY_RESERVED")
        return {
          close: async () => undefined,
          sync: async () => undefined,
          write: async () => undefined,
        }
      },
      spawn: () => ({ pid: 4242, unref: () => undefined }),
    }

    // When
    await startDetachedPropertyWatcher(RUNNER, dependencies)

    // Then
    assert.deepEqual(actions.slice(0, 5), ["reserve:1", "read", "read", "remove", "reserve:2"])
  })

  it("removes a verified nonmatching stale record and retries exclusive reservation", async () => {
    // Given
    const stale = record()
    let inspectionCalls = 0
    let reserveCalls = 0
    let removalCalls = 0
    const dependencies = {
      ensureArtifactDirectory: async () => undefined,
      inspectProcess: async () => {
        inspectionCalls += 1
        return {
          pid: stale.pid,
          executablePath: stale.executablePath,
          commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
          creationIdentity: inspectionCalls === 1 ? "reused" : stale.creationIdentity,
        }
      },
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => stale,
      removeReservation: async () => {
        removalCalls += 1
      },
      reservePid: async () => {
        reserveCalls += 1
        if (reserveCalls === 1) throw new PropertyWatcherProcessError("PID_ALREADY_RESERVED")
        return {
          close: async () => undefined,
          sync: async () => undefined,
          write: async () => undefined,
        }
      },
      spawn: () => ({ pid: stale.pid, unref: () => undefined }),
    }

    // When
    await startDetachedPropertyWatcher(RUNNER, dependencies)

    // Then
    assert.equal(removalCalls, 1)
    assert.equal(reserveCalls, 2)
  })

  it("does not remove a stale record when the recorded process identity still matches", async () => {
    // Given
    const live = record()
    const actions: string[] = []
    const dependencies = {
      ensureArtifactDirectory: async () => undefined,
      inspectProcess: async () => ({
        pid: live.pid,
        executablePath: live.executablePath,
        commandLine: `"${EXECUTABLE}" --title=opencode-property-watcher:${TOKEN} "${RUNNER}" watch`,
        creationIdentity: live.creationIdentity,
      }),
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => live,
      removeReservation: async () => {
        actions.push("remove")
      },
      reservePid: async () => {
        throw new PropertyWatcherProcessError("PID_ALREADY_RESERVED")
      },
      spawn: () => ({ pid: 4242, unref: () => undefined }),
    }

    // When / Then
    await assert.rejects(
      startDetachedPropertyWatcher(RUNNER, dependencies),
      (error: unknown) =>
        error instanceof PropertyWatcherProcessError && error.code === "PID_ALREADY_RESERVED",
    )
    assert.deepEqual(actions, [])
  })

  it("does not remove a stale record that changes during the recovery recheck", async () => {
    // Given
    const stale = record()
    const changed = record({ instanceToken: OTHER_TOKEN })
    const actions: string[] = []
    let reads = 0
    const dependencies = {
      ensureArtifactDirectory: async () => undefined,
      inspectProcess: async () => null,
      openAppendLog: async () => ({ fd: 72, close: async () => undefined }),
      randomUuid: () => TOKEN,
      readRecord: async () => {
        reads += 1
        return reads === 1 ? stale : changed
      },
      removeReservation: async () => {
        actions.push("remove")
      },
      reservePid: async () => {
        throw new PropertyWatcherProcessError("PID_ALREADY_RESERVED")
      },
      spawn: () => ({ pid: 4242, unref: () => undefined }),
    }

    // When / Then
    await assert.rejects(
      startDetachedPropertyWatcher(RUNNER, dependencies),
      (error: unknown) =>
        error instanceof PropertyWatcherProcessError && error.code === "IDENTITY_MISMATCH",
    )
    assert.deepEqual(actions, [])
  })
})

describe("property watcher named-pipe lifecycle", () => {
  it("sends stop only through the instance pipe and waits for a drained acknowledgement", async () => {
    // Given
    const pipeName = `\\\\.\\pipe\\opencode-property-watcher-test-${crypto.randomUUID()}`
    const actions: string[] = []
    const server = await openWatcherStopServer(pipeName, async () => {
      actions.push("drained")
    })

    // When
    await sendWatcherStop(pipeName)
    await server.stopped

    // Then
    assert.deepEqual(actions, ["drained"])
  })

  it("returns a typed error for a missing pipe without a kill fallback", async () => {
    // Given
    const missing = `\\\\.\\pipe\\opencode-property-watcher-missing-${crypto.randomUUID()}`

    // When / Then
    await assert.rejects(
      sendWatcherStop(missing),
      (error: unknown) =>
        error instanceof PropertyWatcherControlError && error.code === "PIPE_UNRESPONSIVE",
    )
  })

  it("removes only a PID record that still matches the stopped instance", async (context) => {
    // Given
    const directory = await fixtureDirectory(context)
    const pidPath = join(directory, "watcher.pid.json")
    const expected = record()
    await writeFile(pidPath, JSON.stringify(expected))

    // When
    await removeMatchingWatcherPidRecord(pidPath, record({ instanceToken: OTHER_TOKEN }))

    // Then
    assert.deepEqual(parseWatcherPidRecord(JSON.parse(await readFile(pidPath, "utf8"))), expected)
    await removeMatchingWatcherPidRecord(pidPath, expected)
    await assert.rejects(readFile(pidPath))
  })

  it("contains no process termination or shell fallback", async () => {
    // Given
    const sources = await Promise.all([
      readFile(new URL("../scripts/property-watcher-process.ts", import.meta.url), "utf8"),
      readFile(new URL("../scripts/property-watcher-start.ts", import.meta.url), "utf8"),
    ])

    // When / Then
    for (const forbidden of [
      /taskkill/iu,
      /Stop-Process/iu,
      /process\.kill/u,
      /cmd\.exe/iu,
      /shell\s*:\s*true/u,
    ]) {
      for (const source of sources) assert.doesNotMatch(source, forbidden)
    }
  })
})
