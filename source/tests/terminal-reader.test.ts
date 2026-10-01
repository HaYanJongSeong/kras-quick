import assert from "node:assert/strict"
import { describe, it } from "node:test"
import { PassThrough, Writable } from "node:stream"

import { createBufferedLineReader, createInteractiveTerminal } from "../scripts/terminal-reader.ts"

describe("buffered terminal input", () => {
  it("preserves every line from one input chunk", () => {
    // Given
    const chunks = [Buffer.from("first\nsecond\n")]
    const readLine = createBufferedLineReader((buffer) => {
      const chunk = chunks.shift()
      if (chunk === undefined) return 0
      chunk.copy(buffer)
      return chunk.length
    })

    // When
    const first = readLine()
    const second = readLine()

    // Then
    assert.equal(first, "first")
    assert.equal(second, "second")
  })

  it("drains complete pasted lines without consuming a partial line", () => {
    const chunks = [Buffer.from("first\nsecond\nthird")]
    const readLine = createBufferedLineReader((buffer) => {
      const chunk = chunks.shift()
      if (chunk === undefined) return 0
      chunk.copy(buffer)
      return chunk.length
    })

    assert.equal(readLine(), "first")
    assert.deepEqual(readLine.drainPending(), ["second"])
    assert.equal(readLine(), "third")
  })

  it("accepts interactive input and exposes pasted overflow", async () => {
    const input = new PassThrough()
    const output = new Writable({ write: (_chunk, _encoding, callback) => callback() })
    const terminal = createInteractiveTerminal(input, output)
    input.end("first\nsecond\n")

    assert.equal(await terminal.question("입력 > "), "first")
    assert.deepEqual(terminal.drainPending?.(), ["second"])
    terminal.close()
  })
})
