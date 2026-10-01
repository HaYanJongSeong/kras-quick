import assert from "node:assert/strict"
import { describe, it } from "node:test"

import { installPropertyWatcherChannel } from "../scripts/property-watcher-channel.ts"
import {
  PROPERTY_WATCHER_BINDING_NAME,
  PROPERTY_WATCHER_WORLD_NAME,
} from "../scripts/property-watcher-listener.ts"
import type {
  PropertyBuildingTerminalStatus,
  PropertyWatcherSelectionHandle,
} from "../scripts/property-watcher-payload.ts"
import {
  FakePropertyWatcherProtocol,
  isolatedContext,
  SCP_ORIGIN,
  SCP_URL,
  validPayload,
} from "./property-watcher-channel-test-support.ts"

const ROW = ["1", "공유재산", "토지", "강서구", "개화동", "일반", "주소", "미처리"] as const

async function updateBuildingStatus(
  channel: object,
  selection: PropertyWatcherSelectionHandle,
  status: PropertyBuildingTerminalStatus,
): Promise<unknown> {
  const update = Reflect.get(channel, "updateBuildingStatus")
  assert.ok(typeof update === "function", "channel.updateBuildingStatus was not installed")
  return Promise.resolve(Reflect.apply(update, channel, [selection, status]))
}

describe("isolated-world property watcher channel", () => {
  it("registers every handler before enabling domains and installs the named world", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()

    // When
    await installPropertyWatcherChannel(protocol, () => undefined)

    // Then
    assert.deepEqual(protocol.state.calls.slice(0, 7), [
      "on:Runtime.bindingCalled",
      "on:close",
      "on:Runtime.executionContextsCleared",
      "on:Runtime.executionContextCreated",
      "on:Runtime.executionContextDestroyed",
      "on:Page.frameNavigated",
      "on:Page.navigatedWithinDocument",
    ])
    assert.deepEqual(protocol.state.calls.slice(7), [
      "Page.enable",
      "Runtime.enable",
      "Page.getFrameTree",
      "Runtime.addBinding",
      "Page.addScriptToEvaluateOnNewDocument",
    ])
    assert.equal(protocol.state.bindingName, PROPERTY_WATCHER_BINDING_NAME)
    assert.equal(protocol.state.worldName, PROPERTY_WATCHER_WORLD_NAME)
  })

  it("accepts only a strict payload from the live root isolated context", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row, _selection, pager) => {
      received.push([row, pager])
    })
    protocol.emitContextCreated(isolatedContext(7, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitBinding({
      contextId: 7,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [[ROW, "8/16"]])
  })

  it("delivers the row with its exact context, generation, and selection handle", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[][] = []
    await installPropertyWatcherChannel(protocol, (...arguments_) => {
      received.push(arguments_)
    })
    protocol.emitContextCreated(isolatedContext(7, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitBinding({
      contextId: 7,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW, 41),
    })

    // Then
    assert.deepEqual(received, [
      [
        ROW,
        {
          contextId: 7,
          contextUniqueId: "context-7",
          generation: 0,
          selectionId: 41,
        },
        "8/16",
      ],
    ])
  })

  it("applies a terminal status only to the exact current selection", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const selections: PropertyWatcherSelectionHandle[] = []
    const receiveSelection = (...arguments_: unknown[]): void => {
      const selection = arguments_[1]
      if (isSelectionHandle(selection)) selections.push(selection)
    }
    const channel = await installPropertyWatcherChannel(protocol, receiveSelection)
    protocol.emitContextCreated(isolatedContext(8, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitBinding({
      contextId: 8,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW, 1),
    })
    protocol.emitBinding({
      contextId: 8,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW, 2),
    })
    const first = selections[0]
    const second = selections[1]
    assert.ok(first)
    assert.ok(second)

    // When
    const staleResult = await updateBuildingStatus(channel, first, "present")
    const appliedResult = await updateBuildingStatus(channel, second, "absent")

    // Then
    assert.equal(staleResult, "stale")
    assert.equal(appliedResult, "applied")
    assert.deepEqual(protocol.state.statusEvaluations, [
      { contextId: 8, selectionId: 2, status: "absent" },
    ])
  })

  it("propagates the isolated evaluator stale result for an exact channel handle", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    protocol.setStatusResult("stale")
    let selection: PropertyWatcherSelectionHandle | undefined
    const receiveSelection = (_row: unknown, handle: PropertyWatcherSelectionHandle): void => {
      selection = handle
    }
    const channel = await installPropertyWatcherChannel(protocol, receiveSelection)
    protocol.emitContextCreated(isolatedContext(9, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitBinding({
      contextId: 9,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW, 3),
    })
    const current = selection
    assert.ok(current)

    // When
    const result = await channel.updateBuildingStatus(current, "present")

    // Then
    assert.equal(result, "stale")
    assert.deepEqual(protocol.state.statusEvaluations, [
      { contextId: 9, selectionId: 3, status: "present" },
    ])
  })

  it("makes a current selection stale before evaluation when the channel is disposed", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    let selection: PropertyWatcherSelectionHandle | undefined
    const receiveSelection = (_row: unknown, handle: PropertyWatcherSelectionHandle): void => {
      selection = handle
    }
    const channel = await installPropertyWatcherChannel(protocol, receiveSelection)
    protocol.emitContextCreated(isolatedContext(10, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitBinding({
      contextId: 10,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW, 4),
    })
    const current = selection
    assert.ok(current)
    await channel.dispose()

    // When
    const result = await channel.updateBuildingStatus(current, "present")

    // Then
    assert.equal(result, "stale")
    assert.deepEqual(protocol.state.statusEvaluations, [])
  })

  it("ignores main-world, subframe, stale-name, wrong-origin, and wrong-binding sources", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    const contexts = [
      { ...isolatedContext(1, PROPERTY_WATCHER_WORLD_NAME), type: "default" },
      isolatedContext(2, PROPERTY_WATCHER_WORLD_NAME, "child-frame"),
      isolatedContext(3, "other-world"),
      { ...isolatedContext(4, PROPERTY_WATCHER_WORLD_NAME), origin: "http://evil.test" },
    ]
    for (const context of contexts) protocol.emitContextCreated(context)

    // When
    for (const contextId of [1, 2, 3, 4]) {
      protocol.emitBinding({
        contextId,
        name: PROPERTY_WATCHER_BINDING_NAME,
        payload: validPayload(ROW),
      })
    }
    protocol.emitContextCreated(isolatedContext(5, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitBinding({ contextId: 5, name: "wrong-binding", payload: validPayload(ROW) })

    // Then
    assert.deepEqual(received, [])
  })

  it("rejects v2, missing, malformed, zero, reversed, extra-key, wrong-tuple, and oversized payloads", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(9, PROPERTY_WATCHER_WORLD_NAME))
    const payloads = [
      "not-json",
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW, pager: "8/16", extra: true }),
      JSON.stringify({ version: 2, selectionId: 1, cells: ROW, pager: "8/16" }),
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW }),
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW, pager: "8 / 16" }),
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW, pager: "0/16" }),
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW, pager: "8/0" }),
      JSON.stringify({ version: 3, selectionId: 1, cells: ROW, pager: "17/16" }),
      JSON.stringify({
        version: 3,
        selectionId: 1,
        cells: ROW,
        pager: "8/9007199254740992",
      }),
      JSON.stringify({ version: 3, cells: ROW, pager: "8/16" }),
      JSON.stringify({ version: 3, selectionId: 0, cells: ROW, pager: "8/16" }),
      JSON.stringify({
        version: 3,
        selectionId: Number.MAX_SAFE_INTEGER + 1,
        cells: ROW,
        pager: "8/16",
      }),
      validPayload(ROW.slice(0, 7)),
      validPayload([...ROW.slice(0, 7), "x".repeat(4097)]),
      "x".repeat(65_537),
    ]

    // When
    for (const payload of payloads) {
      protocol.emitBinding({ contextId: 9, name: PROPERTY_WATCHER_BINDING_NAME, payload })
    }

    // Then
    assert.deepEqual(received, [])
  })

  it("disables on history-away and reactivates the same context on history-return", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(11, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitHistory("http://scpweb.softgraphy.biz/board/detail/1")
    protocol.emitBinding({
      contextId: 11,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })
    protocol.emitHistory(`${SCP_URL}?page=2#selected`)
    protocol.emitBinding({
      contextId: 11,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [ROW])
  })

  it("requires a context-created event after each cross-document root navigation", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitFrameNavigated("http://scpweb.softgraphy.biz/board/detail/1")
    protocol.emitContextsCleared()
    protocol.emitContextCreated(isolatedContext(14, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitFrameNavigated(`${SCP_URL}?reload=1`)
    protocol.emitBinding({
      contextId: 14,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })
    protocol.emitContextCreated(isolatedContext(15, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitBinding({
      contextId: 15,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [ROW])
  })

  it("clearing or destroying a context disables intake until a new live context exists", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(17, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitContextDestroyed(17)
    protocol.emitBinding({
      contextId: 17,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })
    protocol.emitContextCreated(isolatedContext(18, PROPERTY_WATCHER_WORLD_NAME))
    protocol.emitContextsCleared()
    protocol.emitBinding({
      contextId: 18,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [])
  })

  it("disposes inert-first, removes handlers, cleans the active world, script, and binding", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    const channel = await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(21, PROPERTY_WATCHER_WORLD_NAME))

    // When
    await channel.dispose()
    protocol.emitBinding({
      contextId: 21,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [])
    assert.deepEqual(protocol.state.evaluations, [21])
    assert.deepEqual(protocol.state.calls.slice(-10), [
      "off:Runtime.bindingCalled",
      "off:close",
      "off:Runtime.executionContextsCleared",
      "off:Runtime.executionContextCreated",
      "off:Runtime.executionContextDestroyed",
      "off:Page.frameNavigated",
      "off:Page.navigatedWithinDocument",
      "Runtime.evaluate",
      "Page.removeScriptToEvaluateOnNewDocument:watcher-script",
      `Runtime.removeBinding:${PROPERTY_WATCHER_BINDING_NAME}`,
    ])
  })

  it("surfaces session close as a stable fatal completion without payload data", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const channel = await installPropertyWatcherChannel(protocol, () => undefined)

    // When
    protocol.emitClose()

    // Then
    await assert.rejects(channel.failed, (error: unknown) => {
      if (!(error instanceof Error)) return false
      return error.name === "PropertyWatcherChannelError" && !error.message.includes(SCP_ORIGIN)
    })
  })

  it("surfaces row callback failure through fatal completion", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const failure = new Error("callback failure")
    const channel = await installPropertyWatcherChannel(protocol, () => {
      throw failure
    })
    protocol.emitContextCreated(isolatedContext(23, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitBinding({
      contextId: 23,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })
    protocol.emitBinding({
      contextId: 23,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: JSON.stringify({ version: 1, cells: ROW }),
    })

    // Then
    await assert.rejects(channel.failed, failure)
    await channel.dispose()
    assert.deepEqual(protocol.state.evaluations, [23])
  })

  it("ignores subframe navigation events", async () => {
    // Given
    const protocol = new FakePropertyWatcherProtocol()
    const received: unknown[] = []
    await installPropertyWatcherChannel(protocol, (row) => {
      received.push(row)
    })
    protocol.emitContextCreated(isolatedContext(25, PROPERTY_WATCHER_WORLD_NAME))

    // When
    protocol.emitHistory("http://evil.test", "child-frame")
    protocol.emitBinding({
      contextId: 25,
      name: PROPERTY_WATCHER_BINDING_NAME,
      payload: validPayload(ROW),
    })

    // Then
    assert.deepEqual(received, [ROW])
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
