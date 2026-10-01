import assert from "node:assert/strict"
import { once } from "node:events"
import { createConnection } from "node:net"
import { describe, it } from "node:test"

import { openWatcherStopServer, sendWatcherStop } from "../scripts/property-watcher-control.ts"

describe("property watcher stop pipe shutdown", () => {
  it("closes idle and malformed clients before server shutdown completes", async () => {
    // Given
    const pipeName = `\\\\.\\pipe\\opencode-property-watcher-hostile-${crypto.randomUUID()}`
    const server = await openWatcherStopServer(pipeName, async () => undefined)
    const idle = createConnection(pipeName)
    await once(idle, "connect")
    const idleClosed = once(idle, "close")
    const malformed = createConnection(pipeName)
    await once(malformed, "connect")
    const malformedClosed = once(malformed, "close")
    malformed.write("x".repeat(1024))

    // When
    await sendWatcherStop(pipeName)
    await server.stopped
    const closed = await Promise.allSettled([idleClosed, malformedClosed])

    // Then
    assert.deepEqual(
      closed.map(({ status }) => status),
      ["fulfilled", "fulfilled"],
    )
  })
})
