import assert from "node:assert/strict"
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { describe, it, type TestContext } from "node:test"

import * as ts from "typescript/unstable/ast"
import { API } from "typescript/unstable/sync"

import type { KrasEvaluationRequest, KrasEvaluationResult } from "../scripts/kras-domain.ts"
import { KrasLookupTargetError, lookupKrasBuilding } from "../scripts/kras-page.ts"
import type {
  PropertyBuildingTerminalStatus,
  PropertyPager,
  PropertyRow,
  PropertyWatcherSelectionHandle,
} from "../scripts/property-watcher-payload.ts"
import { parsePropertyPager } from "../scripts/property-watcher-payload.ts"
import {
  adaptPropertyWatcherBrowser,
  type CdpBrowserSurface,
  type CdpPageSurface,
  runPropertyWatcherDryRun,
  startPropertyWatcherRuntime,
} from "../scripts/property-watcher-runtime.ts"

const SCP_URL = "http://scpweb.softgraphy.biz/board"
const KRAS_URL = "https://www.kras.go.kr/kras/cert/certView.do"
const ADDRESS_A = "서울특별시 강서구 개화동 255-1"
const ADDRESS_B = "서울특별시 종로구 청운동 1"
const ADDRESS_INITIAL = "서울특별시 종로구 청운동 2"

function requirePager(value: string): PropertyPager {
  const pager = parsePropertyPager(value)
  if (pager === undefined) throw new Error(`invalid test pager: ${value}`)
  return pager
}

const PAGER_8 = requirePager("8/16")
const PAGER_9 = requirePager("9/16")

type FakePageState = {
  readonly calls: string[]
  readonly requests: KrasEvaluationRequest[]
  readonly statuses: Array<{
    readonly selection: PropertyWatcherSelectionHandle
    readonly status: PropertyBuildingTerminalStatus
  }>
  emit:
    | ((
        row: PropertyRow,
        selection: PropertyWatcherSelectionHandle,
        pager: PropertyPager,
      ) => void | Promise<void>)
    | undefined
}

function fakeCdpBrowser(
  urls: readonly string[],
  evaluationResults: readonly KrasEvaluationResult[] = [],
): {
  readonly browser: CdpBrowserSurface
  readonly pages: readonly FakePageState[]
} {
  const pages = urls.map((url) => {
    const state: FakePageState = { calls: [], requests: [], statuses: [], emit: undefined }
    const surface: CdpPageSurface = {
      url: () => url,
      installPropertyWatcherChannel: async (onRow) => {
        state.calls.push("installPropertyWatcherChannel")
        state.emit = (row, selection, pager) =>
          Reflect.apply(onRow, undefined, [row, selection, pager])
        return {
          failed: new Promise<never>(() => undefined),
          dispose: async () => {
            state.calls.push("channel.dispose")
          },
          updateBuildingStatus: async (
            selection: PropertyWatcherSelectionHandle,
            status: PropertyBuildingTerminalStatus,
          ) => {
            state.statuses.push({ selection, status })
            return "applied" as const
          },
        }
      },
      evaluateKras: async (
        _pageFunction: (argument: KrasEvaluationRequest) => Promise<KrasEvaluationResult>,
        request: KrasEvaluationRequest,
      ): Promise<KrasEvaluationResult> => {
        state.calls.push(`evaluate:${request.mode}`)
        state.requests.push(request)
        return evaluationResults[state.requests.length - 1] ?? { kind: "fill_only" }
      },
    }
    return {
      state,
      surface,
    }
  })
  return {
    browser: { contexts: () => [{ pages: () => pages.map(({ surface }) => surface) }] },
    pages: pages.map(({ state }) => state),
  }
}

function requireEmitter(state: FakePageState) {
  if (state.emit === undefined) throw new Error("channel was not installed")
  return state.emit
}

function selectionHandle(selectionId: number): PropertyWatcherSelectionHandle {
  return {
    contextId: 17,
    contextUniqueId: "context-17",
    generation: 0,
    selectionId,
  }
}

async function progressFixture(context: TestContext) {
  const directory = await mkdtemp(join(tmpdir(), "property-runtime-"))
  const path = join(directory, "progress.txt")
  const source = Buffer.from(
    `머리말\r\n현재 어디까지?\r\n7/16(50개단위)(현장조사 완료여부 완료체크 후)\r\n${ADDRESS_INITIAL}\r\n\r\n`,
  )
  await writeFile(path, source)
  context.after(() => rm(directory, { force: true, recursive: true }))
  return { path, source }
}

describe("property watcher CDP runtime", () => {
  it("adapts existing contexts and forwards isolated channel resources", async () => {
    // Given
    const fixture = fakeCdpBrowser([SCP_URL, KRAS_URL])

    // When
    const browser = adaptPropertyWatcherBrowser(fixture.browser)
    const pages = browser.contexts()[0]?.pages() ?? []
    const channel = await pages[0]?.installPropertyWatcherChannel(() => undefined)
    await channel?.dispose()

    // Then
    assert.equal(pages.length, 2)
    assert.deepEqual(fixture.pages[0]?.calls, ["installPropertyWatcherChannel", "channel.dispose"])
  })

  it("dry-run inventories once and performs no page mutation or lifecycle operation", async () => {
    // Given
    const fixture = fakeCdpBrowser(["https://example.test", SCP_URL, KRAS_URL])
    const connectCalls: unknown[] = []
    const output: string[] = []

    // When
    await runPropertyWatcherDryRun({
      connect: async () => {
        connectCalls.push(true)
        return adaptPropertyWatcherBrowser(fixture.browser)
      },
      writeLine: (line) => output.push(line),
    })

    // Then
    assert.equal(connectCalls.length, 1)
    assert.deepEqual(output, ['{"scpPages":1,"krasPages":1,"status":"ready"}'])
    assert.deepEqual(
      fixture.pages.map(({ calls }) => calls),
      [[], [], []],
    )
  })

  it("runs the fake row-to-lookup-to-byte-preserving-commit scenario", async (context) => {
    // Given
    const progress = await progressFixture(context)
    const fixture = fakeCdpBrowser(
      [SCP_URL, KRAS_URL],
      [
        { kind: "lookup", hasBuilding: true },
        { kind: "lookup", hasBuilding: false },
      ],
    )
    const browser = adaptPropertyWatcherBrowser(fixture.browser)
    const watcher = await startPropertyWatcherRuntime({
      connect: async () => browser,
      lookupKras: lookupKrasBuilding,
      logger: { log: () => undefined },
      progressPath: progress.path,
    })
    const emit = requireEmitter(
      fixture.pages[0] ?? { calls: [], requests: [], statuses: [], emit: undefined },
    )
    const row = (address: string): PropertyRow => [
      "1",
      "공유재산",
      "토지",
      "구",
      "동",
      "일반",
      address,
      "미처리",
    ]

    // When
    await emit(row(ADDRESS_A), selectionHandle(1), PAGER_8)
    await emit(row(ADDRESS_A), selectionHandle(2), PAGER_8)
    await emit(row(ADDRESS_B), selectionHandle(3), PAGER_9)
    await watcher.stop()

    // Then
    const finalBytes = await readFile(progress.path)
    assert.deepEqual(
      fixture.pages[1]?.requests.map(({ address, mode }) => ({ address, mode })),
      [
        { address: ADDRESS_A, mode: "lookup" },
        { address: ADDRESS_B, mode: "lookup" },
      ],
    )
    assert.deepEqual(
      finalBytes,
      Buffer.from(
        progress.source
          .toString("utf8")
          .replace("7/16", "9/16")
          .replace(ADDRESS_INITIAL, ADDRESS_B),
      ),
    )
    assert.deepEqual(fixture.pages[1]?.calls, ["evaluate:lookup", "evaluate:lookup"])
    assert.deepEqual(fixture.pages[0]?.calls, ["installPropertyWatcherChannel", "channel.dispose"])
  })

  it("sends lookup mode and forwards true and false building results before committing", async (context) => {
    // Given
    const progress = await progressFixture(context)
    const fixture = fakeCdpBrowser(
      [SCP_URL, KRAS_URL],
      [
        { kind: "lookup", hasBuilding: true },
        { kind: "lookup", hasBuilding: false },
      ],
    )
    const dependencies = {
      connect: async () => adaptPropertyWatcherBrowser(fixture.browser),
      lookupKras: lookupKrasBuilding,
      logger: { log: () => undefined },
      progressPath: progress.path,
    }
    const watcher = await startPropertyWatcherRuntime(dependencies)
    const scp = fixture.pages[0]
    assert.ok(scp)
    const emit = requireEmitter(scp)
    const firstRow: PropertyRow = ["1", "공유재산", "토지", "구", "동", "일반", ADDRESS_A, "미처리"]
    const secondRow: PropertyRow = [
      "2",
      "공유재산",
      "토지",
      "구",
      "동",
      "일반",
      ADDRESS_B,
      "미처리",
    ]

    // When
    await emit(firstRow, selectionHandle(1), PAGER_8)
    await emit(secondRow, selectionHandle(2), PAGER_9)
    await watcher.stop()

    // Then
    assert.deepEqual(
      fixture.pages[1]?.requests.map(({ address, mode }) => ({ address, mode })),
      [
        { address: ADDRESS_A, mode: "lookup" },
        { address: ADDRESS_B, mode: "lookup" },
      ],
    )
    assert.deepEqual(
      await readFile(progress.path),
      Buffer.from(
        progress.source
          .toString("utf8")
          .replace("7/16", "9/16")
          .replace(ADDRESS_INITIAL, ADDRESS_B),
      ),
    )
    assert.deepEqual(
      scp.statuses.map(({ selection, status }) => ({ selectionId: selection.selectionId, status })),
      [
        { selectionId: 1, status: "present" },
        { selectionId: 2, status: "absent" },
      ],
    )
  })

  it("reports failed and preserves TXT bytes when lookup fails recoverably", async (context) => {
    // Given
    const progress = await progressFixture(context)
    const fixture = fakeCdpBrowser(
      [SCP_URL, KRAS_URL],
      [{ kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" }],
    )
    const dependencies = {
      connect: async () => adaptPropertyWatcherBrowser(fixture.browser),
      lookupKras: lookupKrasBuilding,
      logger: { log: () => undefined },
      progressPath: progress.path,
    }
    const watcher = await startPropertyWatcherRuntime(dependencies)
    const scp = fixture.pages[0]
    assert.ok(scp)
    const emit = requireEmitter(scp)
    const row: PropertyRow = ["1", "공유재산", "토지", "구", "동", "일반", ADDRESS_A, "미처리"]

    // When
    await emit(row, selectionHandle(1), PAGER_8)
    await watcher.stop()

    // Then
    assert.deepEqual(
      fixture.pages[1]?.requests.map(({ mode }) => mode),
      ["lookup"],
    )
    assert.deepEqual(await readFile(progress.path), progress.source)
    assert.deepEqual(scp.statuses, [{ selection: selectionHandle(1), status: "failed" }])
  })

  it("keeps an authenticated-session target change retryable without changing TXT", async (context) => {
    // Given
    const progress = await progressFixture(context)
    const fixture = fakeCdpBrowser([SCP_URL, KRAS_URL])
    const watcher = await startPropertyWatcherRuntime({
      connect: async () => adaptPropertyWatcherBrowser(fixture.browser),
      lookupKras: async () => {
        throw new KrasLookupTargetError()
      },
      logger: { log: () => undefined },
      progressPath: progress.path,
    })
    const scp = fixture.pages[0]
    assert.ok(scp)
    const emit = requireEmitter(scp)
    const row: PropertyRow = ["1", "공유재산", "토지", "구", "동", "일반", ADDRESS_A, "미처리"]

    // When
    await emit(row, selectionHandle(1), PAGER_8)
    await watcher.stop()

    // Then
    assert.deepEqual(await readFile(progress.path), progress.source)
    assert.deepEqual(scp.statuses, [{ selection: selectionHandle(1), status: "failed" }])
    assert.deepEqual(fixture.pages[1]?.requests, [])
  })

  it("has no browser lifecycle, navigation, storage, tab creation, or business-action calls", () => {
    // Given
    const configPath = resolve("tsconfig.kras.json")
    const paths = [
      resolve("scripts/property-watcher.ts"),
      resolve("scripts/property-watcher-page.ts"),
      resolve("scripts/property-watcher-runtime.ts"),
    ]
    const forbidden = new Set([
      "bringToFront",
      "click",
      "close",
      "cookies",
      "disconnect",
      "focus",
      "goto",
      "newContext",
      "newPage",
      "reload",
      "storageState",
    ])
    const api = new API()

    // When
    const calls: string[] = []
    try {
      const snapshot = api.updateSnapshot({ openProjects: [configPath], openFiles: paths })
      try {
        const project = snapshot.getProject(configPath)
        for (const path of paths) {
          const source = project?.program.getSourceFile(path)
          if (source === undefined) continue
          const visit = (node: ts.Node): void => {
            if (
              ts.isCallExpression(node) &&
              ts.isPropertyAccessExpression(node.expression) &&
              forbidden.has(node.expression.name.text)
            ) {
              calls.push(node.expression.name.text)
            }
            node.forEachChild(visit)
          }
          visit(source)
        }
      } finally {
        snapshot.dispose()
      }
    } finally {
      api.close()
    }

    // Then
    assert.deepEqual(calls, [])
  })
})
