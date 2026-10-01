import assert from "node:assert/strict"
import { dirname, join } from "node:path"
import { describe, it } from "node:test"

import type { WatcherPidRecord } from "../scripts/property-watcher-process.ts"
import {
  markWatcherPidReady,
  type WatcherReadyStateOperations,
} from "../scripts/property-watcher-state.ts"

const TOKEN = "91f2c793-e7c4-49f0-b394-962599aea376"

function startingRecord(): WatcherPidRecord {
  return {
    version: 1,
    state: "starting",
    pid: 4242,
    executablePath: "C:\\Program Files\\nodejs\\node.exe",
    runnerPath: "C:\\watcher\\property-watcher-runner.ts",
    instanceToken: TOKEN,
    creationIdentity: "created",
    pipeName: `\\\\.\\pipe\\opencode-property-watcher-${TOKEN}`,
  }
}

describe("watcher ready PID replacement", () => {
  it("writes, syncs, closes, rechecks identity, and atomically renames an exclusive temp", async () => {
    // Given
    assert.equal(markWatcherPidReady.length, 3)
    const expected = startingRecord()
    const pidPath = join("C:\\temp", "watcher.pid.json")
    const actions: string[] = []
    let temporaryPath = ""
    const operations: WatcherReadyStateOperations = {
      randomUuid: () => "446c43c4-f434-412b-9a6e-48da61614954",
      readRecord: async () => {
        actions.push("read")
        return expected
      },
      openExclusive: async (path) => {
        temporaryPath = path
        actions.push("open")
        return {
          write: async (value) => {
            assert.equal(JSON.parse(value).state, "ready")
            actions.push("write")
          },
          sync: async () => {
            actions.push("sync")
          },
          close: async () => {
            actions.push("close")
          },
        }
      },
      rename: async (from, to) => {
        assert.equal(from, temporaryPath)
        assert.equal(to, pidPath)
        actions.push("rename")
      },
      unlink: async () => {
        actions.push("unlink")
      },
    }

    // When
    const ready = await markWatcherPidReady(expected, pidPath, operations)

    // Then
    assert.equal(ready.state, "ready")
    assert.equal(dirname(temporaryPath), dirname(pidPath))
    assert.notEqual(temporaryPath, pidPath)
    assert.deepEqual(actions, ["read", "open", "write", "sync", "close", "read", "rename"])
  })

  it("rejects a concurrent identity change and removes only the temporary file", async () => {
    // Given
    assert.equal(markWatcherPidReady.length, 3)
    const expected = startingRecord()
    const changed = { ...expected, instanceToken: "446c43c4-f434-412b-9a6e-48da61614954" }
    const actions: string[] = []
    let reads = 0
    let temporaryPath = ""
    const operations: WatcherReadyStateOperations = {
      randomUuid: () => "446c43c4-f434-412b-9a6e-48da61614954",
      readRecord: async () => {
        reads += 1
        return reads === 1 ? expected : changed
      },
      openExclusive: async (path) => {
        temporaryPath = path
        return {
          write: async () => undefined,
          sync: async () => undefined,
          close: async () => undefined,
        }
      },
      rename: async () => {
        actions.push("rename")
      },
      unlink: async (path) => {
        assert.equal(path, temporaryPath)
        actions.push("unlink")
      },
    }

    // When / Then
    await assert.rejects(markWatcherPidReady(expected, "C:\\temp\\watcher.pid.json", operations))
    assert.deepEqual(actions, ["unlink"])
  })
})
