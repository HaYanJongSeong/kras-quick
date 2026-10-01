import { createConnection, createServer, type Socket } from "node:net"

const MAX_STOP_REQUEST_BYTES = 64

export type PropertyWatcherControlErrorCode = "PIPE_UNRESPONSIVE"

export class PropertyWatcherControlError extends Error {
  override readonly name = "PropertyWatcherControlError"
  readonly code: PropertyWatcherControlErrorCode

  constructor(code: PropertyWatcherControlErrorCode, options?: ErrorOptions) {
    super(code, options)
    this.code = code
  }
}

export type WatcherStopServer = {
  readonly stopped: Promise<void>
  readonly close: () => Promise<void>
}

export async function openWatcherStopServer(
  pipeName: string,
  onStop: () => Promise<void>,
): Promise<WatcherStopServer> {
  const stopped = Promise.withResolvers<void>()
  const closed = Promise.withResolvers<void>()
  const sockets = new Set<Socket>()
  let accepted = false
  let outcome:
    | { readonly kind: "pending" }
    | { readonly kind: "success" }
    | { readonly kind: "failed"; readonly error: unknown } = { kind: "pending" }
  const server = createServer((socket) => {
    if (accepted) {
      socket.destroy()
      return
    }
    sockets.add(socket)
    socket.once("close", () => sockets.delete(socket))
    socket.setEncoding("utf8")
    let request = ""
    let ownsStop = false
    let acknowledgementFlushed = false
    let clientEnded = false
    const completeAcknowledgement = (): void => {
      if (!acknowledgementFlushed || !clientEnded || outcome.kind !== "pending") return
      outcome = { kind: "success" }
      socket.setTimeout(0)
      socket.end()
    }
    socket.once("end", () => {
      clientEnded = true
      completeAcknowledgement()
    })
    socket.on("error", (error) => {
      if (ownsStop && outcome.kind === "pending") {
        outcome = {
          kind: "failed",
          error: new PropertyWatcherControlError("PIPE_UNRESPONSIVE", { cause: error }),
        }
      }
      socket.destroy()
    })
    socket.on("data", (chunk) => {
      request += chunk
      if (Buffer.byteLength(request) > MAX_STOP_REQUEST_BYTES || !"stop\n".startsWith(request)) {
        socket.destroy()
        return
      }
      if (request !== "stop\n") return
      ownsStop = true
      accepted = true
      server.close()
      for (const connected of sockets) {
        if (connected !== socket) connected.destroy()
      }
      Promise.resolve()
        .then(onStop)
        .then(
          () => {
            socket.setTimeout(5000, () => {
              outcome = {
                kind: "failed",
                error: new PropertyWatcherControlError("PIPE_UNRESPONSIVE"),
              }
              socket.destroy()
            })
            socket.write("stopped\n", () => {
              acknowledgementFlushed = true
              completeAcknowledgement()
            })
          },
          (error: unknown) => {
            outcome = { kind: "failed", error }
            socket.end("failed\n", () => socket.destroy())
          },
        )
    })
  })
  server.once("close", () => {
    closed.resolve()
    switch (outcome.kind) {
      case "success":
        stopped.resolve()
        return
      case "failed":
        stopped.reject(outcome.error)
        return
      case "pending":
        stopped.reject(new PropertyWatcherControlError("PIPE_UNRESPONSIVE"))
    }
  })
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(pipeName, resolve)
  })
  return {
    stopped: stopped.promise,
    close: () => {
      if (!accepted) {
        accepted = true
        outcome = { kind: "success" }
      }
      for (const socket of sockets) socket.destroy()
      if (server.listening) server.close()
      return closed.promise
    },
  }
}

export async function sendWatcherStop(pipeName: string): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const socket = createConnection(pipeName)
    let response = ""
    let settled = false
    const fail = (cause?: unknown): void => {
      if (settled) return
      settled = true
      reject(new PropertyWatcherControlError("PIPE_UNRESPONSIVE", { cause }))
    }
    socket.setEncoding("utf8")
    socket.setTimeout(5000)
    socket.once("connect", () => socket.write("stop\n"))
    socket.on("data", (chunk) => {
      response += chunk
      if (Buffer.byteLength(response) > MAX_STOP_REQUEST_BYTES) {
        socket.destroy()
        fail()
        return
      }
      if (response === "stopped\n") {
        settled = true
        resolve()
        socket.end()
        return
      }
      if (response === "failed\n") {
        socket.end()
        fail()
        return
      }
      if (!"stopped\n".startsWith(response) && !"failed\n".startsWith(response)) {
        socket.destroy()
        fail()
      }
    })
    socket.once("end", () => {
      if (!settled) fail()
    })
    socket.once("error", fail)
    socket.once("timeout", () => {
      socket.destroy()
      fail()
    })
  })
}
