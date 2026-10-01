import assert from "node:assert/strict"
import { it } from "node:test"

import { startPropertyWatcher } from "../scripts/property-watcher.ts"

it("stops and disposes when the installed click channel reports a fatal failure", async () => {
  // Given
  const failed = Promise.withResolvers<never>()
  const fatal = new Error("channel failed")
  let disposeCalls = 0
  const watcher = await startPropertyWatcher({
    installTargetListener: async () => ({
      target: Symbol("target"),
      failed: failed.promise,
      updateBuildingStatus: async () => "applied",
      dispose: async () => {
        disposeCalls += 1
      },
    }),
    prepareProgress: async () => undefined,
    lookupKras: async () => "present",
    commitProgress: async () => undefined,
    isRecoverableError: () => false,
    logger: { log: () => undefined },
  })

  // When
  failed.reject(fatal)

  // Then
  await assert.rejects(watcher.stopped, fatal)
  assert.equal(disposeCalls, 1)
})
