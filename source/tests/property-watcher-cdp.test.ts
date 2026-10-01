import assert from "node:assert/strict"
import { readFile } from "node:fs/promises"
import { resolve } from "node:path"
import { describe, it } from "node:test"

import {
  PropertyWatcherProtocolError,
  parsePropertyBuildingStatusEvaluation,
} from "../scripts/property-watcher-cdp.ts"

const ALLOWED_COMMANDS = new Set([
  "Page.addScriptToEvaluateOnNewDocument",
  "Page.enable",
  "Page.getFrameTree",
  "Page.removeScriptToEvaluateOnNewDocument",
  "Runtime.addBinding",
  "Runtime.enable",
  "Runtime.evaluate",
  "Runtime.removeBinding",
])

describe("property watcher CDP adapter", () => {
  for (const result of ["applied", "stale"] as const) {
    it(`accepts the isolated evaluator ${result} result unchanged`, () => {
      // Given / When
      const parsed = parsePropertyBuildingStatusEvaluation(undefined, result)

      // Then
      assert.equal(parsed, result)
    })
  }

  it("rejects isolated evaluator exception details with a typed protocol error", () => {
    // Given / When / Then
    assert.throws(
      () => parsePropertyBuildingStatusEvaluation({ text: "private exception" }, undefined),
      (error: unknown) =>
        error instanceof PropertyWatcherProtocolError &&
        error.code === "STATUS_EVALUATION_EXCEPTION" &&
        !error.message.includes("private exception"),
    )
  })

  it("rejects an unexpected isolated evaluator value with a typed protocol error", () => {
    // Given / When / Then
    assert.throws(
      () => parsePropertyBuildingStatusEvaluation(undefined, "unexpected"),
      (error: unknown) =>
        error instanceof PropertyWatcherProtocolError &&
        error.code === "STATUS_EVALUATION_RESULT_INVALID",
    )
  })

  it("uses only the fixed command allowlist and required isolated-world options", async () => {
    // Given
    const sourceText = await readFile(resolve("scripts/property-watcher-cdp.ts"), "utf8")
    // When
    const commands = [...sourceText.matchAll(/session\.send\("([^"]+)"/g)].map((match) => match[1])

    // Then
    assert.deepEqual(new Set(commands), ALLOWED_COMMANDS)
    assert.equal(sourceText.includes("executionContextName: PROPERTY_WATCHER_WORLD_NAME"), true)
    assert.equal(sourceText.includes("worldName: PROPERTY_WATCHER_WORLD_NAME"), true)
    assert.equal(sourceText.includes("runImmediately: true"), true)
  })

  it("contains no page binding, main-world init script, generic transport, or lifecycle action", async () => {
    // Given
    const paths = [
      "scripts/property-watcher-cdp.ts",
      "scripts/property-watcher-channel.ts",
      "scripts/property-watcher-listener.ts",
      "scripts/property-watcher-page.ts",
      "scripts/property-watcher-runtime.ts",
    ]

    // When
    const sources = await Promise.all(paths.map((path) => readFile(resolve(path), "utf8")))
    const joined = sources.join("\n")

    // Then
    for (const forbidden of [
      ".addInitScript(",
      ".bringToFront(",
      ".click(",
      ".close(",
      ".detach(",
      ".disconnect(",
      ".exposeBinding(",
      ".focus(",
      ".goto(",
      ".newPage(",
      ".reload(",
    ]) {
      assert.equal(joined.includes(forbidden), false, forbidden)
    }
  })
})
