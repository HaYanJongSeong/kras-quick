import assert from "node:assert/strict"
import { it } from "node:test"

import {
  executePropertyWatcherCommand,
  type PropertyWatcherCliFailureCompletion,
  type PropertyWatcherRunnerActions,
  runPropertyWatcherCli,
} from "../scripts/property-watcher-runner.ts"

class SyntheticRunnerFailure extends Error {
  override readonly name = "SyntheticRunnerError"
}

it("writes a concise error, flushes stderr, and exits one for every rejected command", async () => {
  for (const command of ["watch", "dry_run", "start", "status", "stop"] as const) {
    // Given
    const events: string[] = []
    const failure = new SyntheticRunnerFailure("서울특별시 중구 태평로1가 31")
    const rejectCommand = async (): Promise<void> => {
      events.push("action:reject")
      throw failure
    }
    const actions: PropertyWatcherRunnerActions = {
      watch: rejectCommand,
      dryRun: rejectCommand,
      start: rejectCommand,
      status: rejectCommand,
      stop: rejectCommand,
    }
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
      () => executePropertyWatcherCommand({ kind: command }, actions),
      failureCompletion,
    )

    // Then
    assert.deepEqual(events, [
      "action:reject",
      'error:{"status":"error","name":"SyntheticRunnerError"}',
      "stderr:flush",
      "exit:1",
    ])
    assert.equal(events.join("\n").includes(failure.message), false)
  }
})
