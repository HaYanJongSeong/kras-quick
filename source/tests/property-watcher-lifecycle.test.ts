import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { openWatcherStopServer, sendWatcherStop } from "../scripts/property-watcher-control.ts"
import {
  runDetachedPropertyWatcherLifecycle,
  runForegroundPropertyWatcherLifecycle,
  type WatcherLifecycleRuntimeDependencies,
} from "../scripts/property-watcher-lifecycle.ts"
import type { WatcherPidRecord } from "../scripts/property-watcher-process.ts"
import {
  executePropertyWatcherCommand,
  type PropertyWatcherRunnerActions,
} from "../scripts/property-watcher-runner.ts"

const TOKEN = "91f2c793-e7c4-49f0-b394-962599aea376"
const WATCHER_RUNNING = new Promise<void>(() => undefined)

class SyntheticLifecycleError extends Error {
  override readonly name = "SyntheticLifecycleError"
}

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

describe("detached property watcher lifecycle", () => {
  it("removes the matching starting PID when live watcher startup rejects", async () => {
    // Given
    const expected = startingRecord()
    const failure = new SyntheticLifecycleError("startup failed")
    const actions: string[] = []
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => {
        actions.push("record")
        return expected
      },
      startWatcher: async () => {
        actions.push("start")
        throw failure
      },
      openControl: async () => {
        actions.push("control")
        return { stopped: Promise.resolve(), close: async () => undefined }
      },
      markReady: async (record) => record,
      removePid: async (_path, record) => {
        assert.equal(record, expected)
        actions.push("remove")
      },
    }

    // When / Then
    await assert.rejects(runDetachedPropertyWatcherLifecycle(TOKEN, dependencies), failure)
    assert.deepEqual(actions, ["record", "start", "remove"])
  })

  it("acknowledges drained stop, removes PID, then lets CLI flush and self-exit", async () => {
    // Given
    const expected = startingRecord()
    const ready = { ...expected, state: "ready" } as const
    const actions: string[] = []
    const stopRequested = Promise.withResolvers<void>()
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => {
        actions.push("record")
        return expected
      },
      startWatcher: async () => {
        actions.push("start")
        return {
          stopped: WATCHER_RUNNING,
          stop: async () => {
            actions.push("drain")
          },
        }
      },
      openControl: async (_pipeName, onStop) => {
        actions.push("control")
        return {
          stopped: stopRequested.promise.then(onStop).then(() => {
            actions.push("ack")
          }),
          close: async () => undefined,
        }
      },
      markReady: async () => {
        actions.push("ready")
        stopRequested.resolve()
        return ready
      },
      removePid: async (_path, record) => {
        assert.equal(record, ready)
        actions.push("remove")
      },
    }
    const runnerActions: PropertyWatcherRunnerActions = {
      dryRun: async () => undefined,
      start: async () => undefined,
      status: async () => undefined,
      stop: async () => undefined,
      watch: () => runDetachedPropertyWatcherLifecycle(TOKEN, dependencies),
    }

    // When
    await executePropertyWatcherCommand({ kind: "watch" }, runnerActions, {
      flushStdout: async () => {
        actions.push("flush")
      },
      exitSuccess: () => {
        actions.push("exit:0")
      },
    })

    // Then
    assert.deepEqual(actions, [
      "record",
      "start",
      "control",
      "ready",
      "drain",
      "ack",
      "remove",
      "flush",
      "exit:0",
    ])
  })

  it("attempts matching PID removal when watcher disposal rejects and preserves the error", async () => {
    // Given
    const expected = startingRecord()
    const disposalFailure = new SyntheticLifecycleError("dispose failed")
    const actions: string[] = []
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => expected,
      startWatcher: async () => ({
        stopped: WATCHER_RUNNING,
        stop: async () => {
          actions.push("dispose")
          throw disposalFailure
        },
      }),
      openControl: async (_pipeName, onStop) => ({
        stopped: Promise.resolve().then(onStop),
        close: async () => undefined,
      }),
      markReady: async () => ({ ...expected, state: "ready" }),
      removePid: async () => {
        actions.push("remove")
      },
    }

    // When / Then
    await assert.rejects(runDetachedPropertyWatcherLifecycle(TOKEN, dependencies), disposalFailure)
    assert.equal(actions.includes("remove"), true)
    assert.equal(actions[actions.length - 1], "remove")
  })

  it("closes control intake before draining and removing PID when ready marking fails", async () => {
    // Given
    const expected = startingRecord()
    const readyFailure = new SyntheticLifecycleError("ready failed")
    const actions: string[] = []
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => expected,
      startWatcher: async () => ({
        stopped: WATCHER_RUNNING,
        stop: async () => {
          actions.push("drain")
        },
      }),
      openControl: async () => ({
        stopped: new Promise(() => undefined),
        close: async () => {
          actions.push("control:close")
        },
      }),
      markReady: async () => {
        throw readyFailure
      },
      removePid: async () => {
        actions.push("remove")
      },
    }

    // When / Then
    await assert.rejects(runDetachedPropertyWatcherLifecycle(TOKEN, dependencies), readyFailure)
    assert.deepEqual(actions, ["control:close", "drain", "remove"])
  })

  it("retains an early control failure while ready marking is pending without an unhandled rejection", async () => {
    // Given
    const expected = startingRecord()
    const ready = { ...expected, state: "ready" } as const
    const controlFailure = new SyntheticLifecycleError("control drain failed")
    const controlStopped = Promise.withResolvers<void>()
    const readyMarkEntered = Promise.withResolvers<void>()
    const releaseReadyMark = Promise.withResolvers<void>()
    const actions: string[] = []
    const unhandledReasons: unknown[] = []
    const observeUnhandled = (reason: unknown): void => {
      unhandledReasons.push(reason)
    }
    process.on("unhandledRejection", observeUnhandled)
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => {
        actions.push("record")
        return expected
      },
      startWatcher: async () => {
        actions.push("start")
        return {
          stopped: WATCHER_RUNNING,
          stop: async () => {
            actions.push("watcher:stop")
          },
        }
      },
      openControl: async () => {
        actions.push("control")
        return {
          stopped: controlStopped.promise,
          close: async () => {
            actions.push("control:close")
          },
        }
      },
      markReady: async () => {
        actions.push("ready:pending")
        readyMarkEntered.resolve()
        await releaseReadyMark.promise
        actions.push("ready:released")
        return ready
      },
      removePid: async (_path, record) => {
        assert.equal(record, ready)
        actions.push("remove")
      },
    }

    try {
      const lifecycle = runDetachedPropertyWatcherLifecycle(TOKEN, dependencies)
      const lifecycleOutcome = lifecycle.then(
        () => undefined,
        (error: unknown) => error,
      )
      await readyMarkEntered.promise

      // When
      controlStopped.reject(controlFailure)
      await new Promise<void>((resolve) => setImmediate(resolve))
      releaseReadyMark.resolve()
      const outcome = await lifecycleOutcome

      // Then
      assert.equal(outcome, controlFailure)
      assert.deepEqual(unhandledReasons, [])
      assert.deepEqual(actions, [
        "record",
        "start",
        "control",
        "ready:pending",
        "ready:released",
        "control:close",
        "watcher:stop",
        "remove",
      ])
    } finally {
      releaseReadyMark.resolve()
      process.off("unhandledRejection", observeUnhandled)
    }
  })

  it("rejects on watcher fatal completion without waiting for a control request", async () => {
    // Given
    const expected = startingRecord()
    const fatal = new TypeError("watcher defect")
    const watcherStopped = Promise.withResolvers<void>()
    const controlStopped = Promise.withResolvers<void>()
    let stopCalls = 0
    void watcherStopped.promise.then(
      () => undefined,
      () => undefined,
    )
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => expected,
      startWatcher: async () => ({
        stopped: watcherStopped.promise,
        stop: async () => {
          stopCalls += 1
          throw fatal
        },
      }),
      openControl: async () => ({
        stopped: controlStopped.promise,
        close: async () => undefined,
      }),
      markReady: async () => {
        watcherStopped.reject(fatal)
        return { ...expected, state: "ready" }
      },
      removePid: async () => undefined,
    }
    const lifecycle = runDetachedPropertyWatcherLifecycle(TOKEN, dependencies)
    const pending = Symbol("pending")

    // When
    const firstOutcome = await Promise.race([
      lifecycle.then(
        () => undefined,
        (error: unknown) => error,
      ),
      new Promise<typeof pending>((resolve) => setImmediate(() => resolve(pending))),
    ])
    controlStopped.resolve()

    // Then
    await assert.rejects(lifecycle, (error: unknown) => error === fatal)
    assert.equal(firstOutcome, fatal)
    assert.equal(stopCalls, 1)
  })

  it("delivers the pipe acknowledgement before control close and final cleanup", async () => {
    // Given
    const instanceToken = crypto.randomUUID()
    const expected = {
      ...startingRecord(),
      instanceToken,
      pipeName: `\\\\.\\pipe\\opencode-property-watcher-ack-${instanceToken}`,
    }
    const ready = { ...expected, state: "ready" } as const
    const watcherStopped = Promise.withResolvers<void>()
    const drained = Promise.withResolvers<void>()
    const releaseAcknowledgement = Promise.withResolvers<void>()
    const actions: string[] = []
    let client: Promise<void> | undefined
    const dependencies: WatcherLifecycleRuntimeDependencies = {
      waitForRecord: async () => expected,
      startWatcher: async () => ({
        stopped: watcherStopped.promise,
        stop: async () => {
          actions.push("drain")
          watcherStopped.resolve()
        },
      }),
      openControl: async (pipeName, onStop) => {
        const server = await openWatcherStopServer(pipeName, async () => {
          await onStop()
          actions.push("drained")
          drained.resolve()
          await releaseAcknowledgement.promise
        })
        return {
          stopped: server.stopped,
          close: async () => {
            actions.push("control:close")
            await server.close()
          },
        }
      },
      markReady: async () => {
        client = sendWatcherStop(expected.pipeName).then(() => {
          actions.push("client:ack")
        })
        return ready
      },
      removePid: async () => {
        actions.push("remove")
      },
    }
    const lifecycle = runDetachedPropertyWatcherLifecycle(instanceToken, dependencies).then(() => {
      actions.push("lifecycle:return")
    })

    // When
    await drained.promise
    await Promise.resolve()
    const controlClosedBeforeAcknowledgement = actions.includes("control:close")
    releaseAcknowledgement.resolve()
    const clientCompletion = client
    if (clientCompletion === undefined) throw new SyntheticLifecycleError("client not started")
    const outcomes = await Promise.allSettled([clientCompletion, lifecycle])

    // Then
    assert.equal(controlClosedBeforeAcknowledgement, false)
    assert.deepEqual(
      outcomes.map(({ status }) => status),
      ["fulfilled", "fulfilled"],
    )
    assert.equal(actions.indexOf("client:ack") < actions.indexOf("control:close"), true)
    assert.equal(actions.indexOf("control:close") < actions.indexOf("remove"), true)
    assert.equal(actions.indexOf("remove") < actions.indexOf("lifecycle:return"), true)
  })
})

describe("foreground property watcher lifecycle", () => {
  it("drains and disposes the watcher before returning", async () => {
    // Given
    const actions: string[] = []

    // When
    await runForegroundPropertyWatcherLifecycle(
      { log: () => undefined },
      {
        startWatcher: async () => {
          actions.push("start")
          return {
            stopped: WATCHER_RUNNING,
            stop: async () => {
              actions.push("drain")
            },
          }
        },
        waitForStop: async () => {
          actions.push("signal")
        },
      },
    )

    // Then
    assert.deepEqual(actions, ["start", "signal", "drain"])
  })

  it("rejects on watcher fatal completion without waiting for an external signal", async () => {
    // Given
    const fatal = new TypeError("watcher defect")
    const watcherStopped = Promise.withResolvers<void>()
    const externalStop = Promise.withResolvers<void>()
    void watcherStopped.promise.then(
      () => undefined,
      () => undefined,
    )
    const lifecycle = runForegroundPropertyWatcherLifecycle(
      { log: () => undefined },
      {
        startWatcher: async () => ({
          stopped: watcherStopped.promise,
          stop: async () => {
            throw fatal
          },
        }),
        waitForStop: () => externalStop.promise,
      },
    )
    const pending = Symbol("pending")
    watcherStopped.reject(fatal)

    // When
    const firstOutcome = await Promise.race([
      lifecycle.then(
        () => undefined,
        (error: unknown) => error,
      ),
      new Promise<typeof pending>((resolve) => setImmediate(() => resolve(pending))),
    ])
    externalStop.resolve()

    // Then
    await assert.rejects(lifecycle, (error: unknown) => error === fatal)
    assert.equal(firstOutcome, fatal)
  })
})
