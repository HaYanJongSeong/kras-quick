import { readSync, writeSync } from "node:fs"
import { createInterface } from "node:readline/promises"

export type TerminalReader = {
  readonly question: (prompt: string) => Promise<string>
  readonly close: () => void
}

export type ReadChunk = (buffer: Buffer) => number

export function createBufferedLineReader(readChunk: ReadChunk, chunkSize = 256): () => string {
  let pending = Buffer.alloc(0)
  return () => {
    while (true) {
      const newline = pending.indexOf(0x0a)
      if (newline >= 0) {
        const line = pending.subarray(0, newline)
        pending = Buffer.from(pending.subarray(newline + 1))
        return line.toString("utf8")
      }
      const chunk = Buffer.alloc(chunkSize)
      const bytesRead = readChunk(chunk)
      if (bytesRead <= 0) {
        const line = pending
        pending = Buffer.alloc(0)
        return line.toString("utf8")
      }
      pending = Buffer.concat([pending, chunk.subarray(0, bytesRead)])
    }
  }
}

function syncWrite(fd: number | undefined, text: string): void {
  if (typeof fd === "number") {
    try {
      writeSync(fd, text)
      return
    } catch (error: unknown) {
      if (!(error instanceof Error)) throw error
    }
  }
  process.stdout.write(text)
}

export function createTerminal(): TerminalReader {
  if (process.stdin.isTTY) return createInterface({ input: process.stdin, output: process.stdout })
  const readLine = createBufferedLineReader((buffer) => readSync(0, buffer, 0, buffer.length, null))
  return {
    question: async (prompt) => {
      const state = (process.stdout as { _writableState?: { length?: number } })._writableState
      if (state !== undefined) {
        for (let attempt = 0; attempt < 100 && (state.length ?? 0) > 0; attempt += 1) {
          await new Promise((resolve) => setTimeout(resolve, 5))
        }
      }
      syncWrite(process.stdout.fd, prompt)
      return readLine()
    },
    close: () => undefined,
  }
}
