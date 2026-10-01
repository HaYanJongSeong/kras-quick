import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { installPropertyWatcherChannel } from "../scripts/property-watcher-channel.ts"
import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_WORLD_NAME,
} from "../scripts/property-watcher-listener.ts"
import type { PropertyWatcherSelectionHandle } from "../scripts/property-watcher-payload.ts"
import {
  FakePropertyWatcherProtocol,
  isolatedContext,
  SCP_URL,
  validPayload,
} from "./property-watcher-channel-test-support.ts"

const ROW = ["1", "공유재산", "토지", "강서구", "개화동", "일반", "주소", "미처리"] as const

function emitValid(protocol: FakePropertyWatcherProtocol, contextId: number): void {
  protocol.emitBinding({
    contextId,
    name: PROPERTY_WATCHER_BINDING_NAME,
    payload: validPayload(ROW),
  })
}

describe("isolated-world channel navigation ordering", () => {
  it("makes a captured selection stale after history generation and context replacement", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    let captured: PropertyWatcherSelectionHandle | undefined
    const receiveSelection = (...arguments_: unknown[]): void => {
      const candidate = arguments_[1]
      if (isSelectionHandle(candidate)) captured = candidate
    }
    const channel = await installPropertyWatcherChannel(protocol, receiveSelection)
    protocol.emitContextCreated(isolatedContext(30, PROPERTY_WATCHER_WORLD_NAME))
    emitValid(protocol, 30)
    const selection = captured
    assert.ok(selection)

    // When
    protocol.emitHistory(`${SCP_URL}?page=2`)
    protocol.emitContextDestroyed(30)
    protocol.emitContextCreated(isolatedContext(31, PROPERTY_WATCHER_WORLD_NAME))
    const update = Reflect.get(channel, "updateBuildingStatus")
    assert.ok(typeof update === "function", "channel.updateBuildingStatus was not installed")
    const result = await Promise.resolve(Reflect.apply(update, channel, [selection, "present"]))

    // Then
    assert.equal(result, "stale")
    assert.deepEqual(protocol.state.statusEvaluations, [])
  })

  it("makes a handle stale when the same context id is replaced by a new unique id", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    let selection: PropertyWatcherSelectionHandle | undefined
    const receiveSelection = (_row: unknown, handle: PropertyWatcherSelectionHandle): void => {
      selection = handle
    }
    const channel = await installPropertyWatcherChannel(protocol, receiveSelection)
    protocol.emitContextCreated(isolatedContext(30, PROPERTY_WATCHER_WORLD_NAME))
    emitValid(protocol, 30)
    const captured = selection
    assert.ok(captured)

    // When
    protocol.emitContextCreated({
      ...isolatedContext(30, PROPERTY_WATCHER_WORLD_NAME),
      uniqueId: "replacement-context-30",
    })
    const result = await channel.updateBuildingStatus(captured, "absent")

    // Then
    assert.equal(result, "stale")
    assert.deepEqual(protocol.state.statusEvaluations, [])
  })

  it("disables on document navigation away and reactivates only a new context after return", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(31, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitFrameNavigated("http://scpweb.softgraphy.biz/board/detail/1")
    emitValid(protocol, 31)
    protocol.emitContextsCleared()
    protocol.emitFrameNavigated(SCP_URL)
    emitValid(protocol, 31)
    protocol.emitContextCreated(isolatedContext(32, PROPERTY_WATCHER_WORLD_NAME))
    emitValid(protocol, 32)

    // Then
    assert.deepEqual(received, [ROW])
  })

  it("accepts a named context reported during Runtime.enable before the frame tree reply", async () => {
    // Given
    class ContextDuringEnableProtocol extends FakePropertyWatcherProtocol {
      override runtimeEnable(): Promise<void> {
        this.state.calls.push("Runtime.enable")
        this.emitContextCreated(isolatedContext(33, PROPERTY_WATCHER_WORLD_NAME))
        return Promise.resolve()
      }
    }
    const protocol = new ContextDuringEnableProtocol()
    const received: unknown[] = []

    // When
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    emitValid(protocol, 33)

    // Then
    assert.deepEqual(received, [ROW])
  })

  it("removes the binding when listener script installation fails", async () => {
    // Given
    class ScriptFailureProtocol extends FakePropertyWatcherProtocol {
      override addListenerScript(): Promise<string> {
        this.state.calls.push("Page.addScriptToEvaluateOnNewDocument")
        return Promise.reject(new TypeError("protocol failed"))
      }
    }
    const protocol = new ScriptFailureProtocol()

    // When / Then
    await assert.rejects(
      installPropertyWatcherChannel(protocol, () => undefined),
      TypeError,
    )
    assert.equal(
      protocol.state.calls.at(-1),
      `Runtime.removeBinding:${PROPERTY_WATCHER_BINDING_NAME}`,
    )
  })

  it("continues script and binding removal when active-context cleanup fails", async () => {
    // Given
    class CleanupFailureProtocol extends FakePropertyWatcherProtocol {
      override evaluateCleanup(contextId: number): Promise<void> {
        this.state.calls.push("Runtime.evaluate")
        this.state.evaluations.push(contextId)
        return Promise.reject(new TypeError("cleanup failed"))
      }
    }
    const protocol = new CleanupFailureProtocol()
    const channel = await installPropertyWatcherChannel(protocol, () => undefined)
    protocol.emitContextCreated(isolatedContext(35, PROPERTY_WATCHER_WORLD_NAME))

    // When / Then
    await assert.rejects(channel.dispose(), TypeError)
    assert.deepEqual(protocol.state.calls.slice(-3), [
      "Runtime.evaluate",
      "Page.removeScriptToEvaluateOnNewDocument:watcher-script",
      `Runtime.removeBinding:${PROPERTY_WATCHER_BINDING_NAME}`,
    ])
  })

  it("cleans matching isolated contexts while history is away before reinstall", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const channel = await installPropertyWatcherChannel(protocol, () => undefined)
    protocol.emitContextCreated(isolatedContext(37, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitHistory("http://scpweb.softgraphy.biz/board/detail/1")

    // When
    await channel.dispose()
    protocol.emitHistory(SCP_URL)
    const replacement = await installPropertyWatcherChannel(protocol, () => undefined)
    protocol.emitContextCreated(isolatedContext(38, PROPERTY_WATCHER_WORLD_NAME))
    await replacement.dispose()

    // Then
    assert.deepEqual(protocol.state.evaluations, [37, 38])
  })
})

function isSelectionHandle(value: unknown): value is PropertyWatcherSelectionHandle {
  if (typeof value !== "object" || value === null) return false
  return (
    typeof Reflect.get(value, "contextId") === "number" &&
    typeof Reflect.get(value, "contextUniqueId") === "string" &&
    typeof Reflect.get(value, "generation") === "number" &&
    typeof Reflect.get(value, "selectionId") === "number"
  )
}
