import assert from "node:assert/strict"
import { describe, it } from "node:test"

import {
  executePropertyWatcherCommand,
  type PropertyWatcherCliFailureCompletion,
  type PropertyWatcherRunnerActions,
  PropertyWatcherUsageError,
  parsePropertyWatcherCommand,
  runPropertyWatcherCli,
} from "../scripts/property-watcher-runner.ts"

class SyntheticRunnerError extends Error {
  override readonly name = "SyntheticRunnerError"
}

describe("property watcher command runner", () => {
  it("parses every exact command and rejects missing, unknown, or extra arguments", () => {
    // Given
    const commands = ["watch", "dry_run", "start", "status", "stop"] as const

    // When / Then
    assert.deepEqual(
      commands.map((command) => parsePropertyWatcherCommand([command])),
      [
        { kind: "watch" },
        { kind: "dry_run" },
        { kind: "start" },
        { kind: "status" },
        { kind: "stop" },
      ],
    )
    for (const args of [[], ["unknown"], ["watch", "extra"], ["stop", "extra"]]) {
      assert.throws(
        () => parsePropertyWatcherCommand(args),
        (error: unknown) => error instanceof PropertyWatcherUsageError,
      )
    }
  })

  it("routes each command exactly once", async () => {
    // Given
    const calls: string[] = []
    const actions: PropertyWatcherRunnerActions = {
      dryRun: async () => {
        calls.push("dry_run")
      },
      start: async () => {
        calls.push("start")
      },
      status: async () => {
        calls.push("status")
      },
      stop: async () => {
        calls.push("stop")
      },
      watch: async () => {
        calls.push("watch")
      },
    }

    // When
    for (const command of ["watch", "dry_run", "start", "status", "stop"] as const) {
      await executePropertyWatcherCommand({ kind: command }, actions)
    }

    // Then
    assert.deepEqual(calls, ["watch", "dry_run", "start", "status", "stop"])
  })

  it("flushes stdout and exits successfully only after dry-run output completes", async () => {
    // Given
    const calls: string[] = []
    const actions: PropertyWatcherRunnerActions = {
      dryRun: async () => {
        calls.push("output")
      },
      start: async () => undefined,
      status: async () => undefined,
      stop: async () => undefined,
      watch: async () => undefined,
    }
    const completion = {
      flushStdout: async () => {
        calls.push("flush")
      },
      exitSuccess: () => {
        calls.push("exit:0")
      },
    }

    // When
    await executePropertyWatcherCommand({ kind: "dry_run" }, actions, completion)

    // Then
    assert.deepEqual(calls, ["output", "flush", "exit:0"])
  })

  it("flushes stdout and exits successfully only after watch resolves", async () => {
    // Given
    const calls: string[] = []
    const active = Promise.withResolvers<void>()
    const release = Promise.withResolvers<void>()
    const actions: PropertyWatcherRunnerActions = {
      dryRun: async () => undefined,
      start: async () => undefined,
      status: async () => undefined,
      stop: async () => undefined,
      watch: async () => {
        calls.push("watch:active")
        active.resolve()
        await release.promise
        calls.push("watch:resolved")
      },
    }
    const completion = {
      flushStdout: async () => {
        calls.push("flush")
      },
      exitSuccess: () => {
        calls.push("exit:0")
      },
    }

    // When
    const execution = executePropertyWatcherCommand({ kind: "watch" }, actions, completion)
    await active.promise
    assert.deepEqual(calls, ["watch:active"])
    release.resolve()
    await execution

    // Then
    assert.deepEqual(calls, ["watch:active", "watch:resolved", "flush", "exit:0"])
  })

  it("does not invoke watcher completion for start, status, or stop", async () => {
    // Given
    const completed: string[] = []
    const actions: PropertyWatcherRunnerActions = {
      dryRun: async () => undefined,
      start: async () => undefined,
      status: async () => undefined,
      stop: async () => undefined,
      watch: async () => undefined,
    }
    const completion = {
      flushStdout: async () => {
        completed.push("flush")
      },
      exitSuccess: () => {
        completed.push("exit:0")
      },
    }

    // When
    for (const command of ["start", "status", "stop"] as const) {
      await executePropertyWatcherCommand({ kind: command }, actions, completion)
    }

    // Then
    assert.deepEqual(completed, [])
  })

  it("completes detached lifecycle cleanup before reporting and exiting failure", async () => {
    // Given
    const events: string[] = []
    const failure = new SyntheticRunnerError("fatal watcher failure")
    const actions: PropertyWatcherRunnerActions = {
      watch: async () => {
        events.push("lifecycle:control-close")
        events.push("lifecycle:watcher-stop")
        events.push("lifecycle:pid-remove")
        throw failure
      },
      dryRun: async () => undefined,
      start: async () => undefined,
      status: async () => undefined,
      stop: async () => undefined,
    }
    const failureCompletion: PropertyWatcherCliFailureCompletion = {
      writeError: () => {
        events.push("error")
      },
      flushStderr: async () => {
        events.push("stderr:flush")
      },
      exitFailure: () => {
        events.push("exit:1")
      },
    }

    // When
    await runPropertyWatcherCli(
      () => executePropertyWatcherCommand({ kind: "watch" }, actions),
      failureCompletion,
    )

    // Then
    assert.deepEqual(events, [
      "lifecycle:control-close",
      "lifecycle:watcher-stop",
      "lifecycle:pid-remove",
      "error",
      "stderr:flush",
      "exit:1",
    ])
  })

  it("reports command parse errors through the same concise failure boundary", async () => {
    // Given
    const events: string[] = []
    const failureCompletion: PropertyWatcherCliFailureCompletion = {
      writeError: (line) => {
        events.push(`error:${line}`)
      },
      flushStderr: async () => {
        events.push("stderr:flush")
      },
      exitFailure: () => {
        events.push("exit:1")
      },
    }

    // When
    await runPropertyWatcherCli(
      () =>
        Promise.resolve().then(() => {
          parsePropertyWatcherCommand([])
        }),
      failureCompletion,
    )

    // Then
    assert.deepEqual(events, [
      'error:{"status":"error","name":"PropertyWatcherUsageError"}',
      "stderr:flush",
      "exit:1",
    ])
  })

  it("still exits one when stderr failure completion rejects", async () => {
    // Given
    const events: string[] = []
    const commandFailure = new SyntheticRunnerError("command failed")
    const flushFailure = new SyntheticRunnerError("stderr flush failed")
    const failureCompletion: PropertyWatcherCliFailureCompletion = {
      writeError: () => {
        events.push("error")
      },
      flushStderr: async () => {
        events.push("stderr:flush")
        throw flushFailure
      },
      exitFailure: () => {
        events.push("exit:1")
      },
    }

    // When / Then
    await assert.rejects(
      runPropertyWatcherCli(() => Promise.reject(commandFailure), failureCompletion),
      (error: unknown) => error === flushFailure,
    )
    assert.deepEqual(events, ["error", "stderr:flush", "exit:1"])
  })
})
