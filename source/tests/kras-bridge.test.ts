import assert from "node:assert/strict"
import { basename, isAbsolute } from "node:path"
import { describe, it } from "node:test"

import {
  type ChildCallOptions,
  type ChildExecutor,
  type ChildResult,
  runKrasChild,
} from "../scripts/kras-bridge.ts"

type ChildCall = {
  readonly executable: string
  readonly args: readonly string[]
  readonly options: ChildCallOptions
}

function fakeChildExecutor(calls: ChildCall[], result: ChildResult): ChildExecutor {
  return async (executable, args, options) => {
    calls.push({ executable, args, options })
    return result
  }
}

describe("KRAS Node child bridge", () => {
  it("routes lookup arguments without shell interpolation and trims stdout", async () => {
    // Given
    const calls: ChildCall[] = []
    const address = "충청북도 제천시 백운면 평동리 123-4"
    const execute = fakeChildExecutor(calls, {
      ok: true,
      stdout: "부동산 종합증명서 조회결과 건물 등장\r\n",
    })

    // When
    const output = await runKrasChild(execute, { kind: "lookup", address, darkMode: true })

    // Then
    assert.equal(output, "부동산 종합증명서 조회결과 건물 등장")
    const call = calls[0]
    assert.ok(call)
    const runnerPath = call.args[0]
    assert.ok(runnerPath)
    assert.equal(call.executable, "node")
    assert.equal(isAbsolute(runnerPath), true)
    assert.equal(basename(runnerPath), "kras-runner.ts")
    assert.deepEqual(call.args.slice(1), ["lookup", address, "true"])
    assert.deepEqual(call.options, { encoding: "utf8", timeout: 30000, windowsHide: true })
  })

  it("routes dark mode with one child argument", async () => {
    // Given
    const calls: ChildCall[] = []
    const execute = fakeChildExecutor(calls, { ok: true, stdout: "다크 모드 적용." })

    // When
    const output = await runKrasChild(execute, { kind: "dark_mode" })

    // Then
    assert.equal(output, "다크 모드 적용.")
    assert.deepEqual(calls[0]?.args.slice(1), ["dark_mode"])
  })

  it("converts an expected child failure to a concise error", async () => {
    // Given
    const execute = fakeChildExecutor([], { ok: false })

    // When / Then
    await assert.rejects(runKrasChild(execute, { kind: "dark_mode" }), /KRAS 실행에 실패했습니다\./)
  })
})
