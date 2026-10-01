import { readSync, writeSync } from "node:fs"
import { createInterface } from "node:readline"

export type TerminalReader = {
  readonly question: (prompt: string) => Promise<string>
  readonly drainPending?: () => readonly string[]
  readonly close: () => void
}

export type ReadChunk = (buffer: Buffer) => number

export type BufferedLineReader = (() => string) & {
  drainPending: () => readonly string[]
}

export function createBufferedLineReader(
  readChunk: ReadChunk,
  chunkSize = 256,
): BufferedLineReader {
  let pending = Buffer.alloc(0)
  const readLine = (() => {
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
  }) as BufferedLineReader
  readLine.drainPending = () => {
    const lines: string[] = []
    while (true) {
      const newline = pending.indexOf(0x0a)
      if (newline < 0) return lines
      lines.push(pending.subarray(0, newline).toString("utf8"))
      pending = Buffer.from(pending.subarray(newline + 1))
    }
  }
  return readLine
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
  if (process.stdin.isTTY) {
    return createInteractiveTerminal(process.stdin, process.stdout)
  }
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
    drainPending: readLine.drainPending,
    close: () => undefined,
  }
}

export function createInteractiveTerminal(
  input: NodeJS.ReadableStream,
  output: NodeJS.WritableStream,
): TerminalReader {
  const readline = createInterface({ input, output, terminal: true })
    const pending: string[] = []
    let waiting:
      | { readonly resolve: (line: string) => void }
      | undefined
    readline.on("line", (line) => {
      if (waiting !== undefined) {
        const current = waiting
        waiting = undefined
        current.resolve(line)
      } else {
        pending.push(line)
      }
    })
    readline.on("close", () => {
      if (waiting !== undefined) {
        const current = waiting
        waiting = undefined
        current.resolve("")
      }
    })
    return {
      question: async (prompt) => {
        output.write(prompt)
        const line = pending.shift()
        if (line !== undefined) return line
        return await new Promise<string>((resolve) => {
          waiting = { resolve }
        })
      },
      drainPending: () => pending.splice(0),
      close: () => readline.close(),
    }
}
