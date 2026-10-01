import assert from "node:assert/strict"
import { mkdtemp, open, readdir, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { basename, join } from "node:path"
import { describe, it, type TestContext } from "node:test"

import { parsePropertyRow } from "../scripts/property-address.ts"
import {
  commitProgressUpdate,
  ProgressFileError,
  type ProgressFileErrorCode,
  type ProgressFileOperations,
  prepareProgressUpdate,
  updateProgressFile,
} from "../scripts/property-progress.ts"
import { parsePropertyPager } from "../scripts/property-watcher-payload.ts"

const UTF8_BOM = Buffer.from([0xef, 0xbb, 0xbf])
const NON_ADDRESS_CELL = "<not-address>"
const PAGER_SUFFIX = "(50개단위)(현장조사 완료여부 완료체크 후)"
const PAGER_7 = `7/16${PAGER_SUFFIX}`
const PAGER_8 = `8/16${PAGER_SUFFIX}`

function requirePager(value: string) {
  const pager = parsePropertyPager(value)
  assert.ok(pager, `invalid test pager: ${value}`)
  return pager
}

const UPDATED_PAGER = requirePager("8/16")

class SyntheticIoError extends Error {
  override readonly name = "SyntheticIoError"
}

function parcel(address: string) {
  return parsePropertyRow([
    "1",
    "공유재산",
    "토지",
    "구",
    "동",
    NON_ADDRESS_CELL,
    address,
    "미처리",
  ])
}

async function createFixture(
  context: TestContext,
  bytes: Uint8Array,
): Promise<{ readonly directory: string; readonly path: string }> {
  const directory = await mkdtemp(join(tmpdir(), "property-progress-"))
  const path = join(directory, "progress.txt")
  await writeFile(path, bytes)
  context.after(() => rm(directory, { force: true, recursive: true }))
  return { directory, path }
}

function progressOperations(
  compareAndWriteLocked: ProgressFileOperations["compareAndWriteLocked"],
): ProgressFileOperations {
  return {
    openExclusive: async (path) => {
      const handle = await open(path, "wx")
      return {
        writeFile: (data) => handle.writeFile(data),
        sync: () => handle.sync(),
        close: () => handle.close(),
      }
    },
    compareAndWriteLocked,
    remove: (path) => rm(path, { force: true }),
  }
}

describe("property progress file updater", () => {
  it("replaces the pager and address together while preserving BOM, CRLF, suffix, and blanks", async (context) => {
    // Given
    const sourceText = [
      "현장조사 진행 상황",
      "기존 진행 문구는 그대로",
      "현재 어디까지?",
      PAGER_7,
      "서울특별시 종로구 효자동 130-7",
      "",
      "",
      "",
    ].join("\r\n")
    const source = Buffer.concat([UTF8_BOM, Buffer.from(sourceText)])
    const fixture = await createFixture(context, source)

    // When
    await updateProgressFile(
      fixture.path,
      parcel("서울특별시 강서구 개화동 산 300-2"),
      UPDATED_PAGER,
    )

    // Then
    const expectedText = sourceText
      .replace(PAGER_7, PAGER_8)
      .replace("서울특별시 종로구 효자동 130-7", "서울특별시 강서구 개화동 산 300-2")
    assert.deepEqual(
      await readFile(fixture.path),
      Buffer.concat([UTF8_BOM, Buffer.from(expectedText)]),
    )
    assert.deepEqual(await readdir(fixture.directory), ["progress.txt"])
  })

  it("preserves LF files without a BOM byte for byte outside the address", async (context) => {
    // Given
    const source = Buffer.from(
      `머리말\n현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n\n\n`,
    )
    const fixture = await createFixture(context, source)

    // When
    await updateProgressFile(fixture.path, parcel("서울특별시 강서구 개화동 255-1"), UPDATED_PAGER)

    // Then
    assert.deepEqual(
      await readFile(fixture.path),
      Buffer.from(`머리말\n현재 어디까지?\n${PAGER_8}\n서울특별시 강서구 개화동 255-1\n\n\n`),
    )
  })

  it("rejects invalid UTF-8, non-unique markers, and invalid address lines with stable errors", async (context) => {
    // Given
    const cases: readonly {
      readonly bytes: Uint8Array
      readonly code: ProgressFileErrorCode
      readonly secret?: string
    }[] = [
      {
        bytes: Buffer.concat([
          Buffer.from(`현재 어디까지?\n${PAGER_7}\n`),
          Buffer.from([0xc3, 0x28]),
        ]),
        code: "INVALID_ENCODING",
      },
      {
        bytes: Buffer.from(
          `현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n현재 어디까지?\n`,
        ),
        code: "MARKER_NOT_UNIQUE",
      },
      {
        bytes: Buffer.from("현재 어디까지?\n\n"),
        code: "ADDRESS_LINE_MISSING",
      },
      {
        bytes: Buffer.from(`현재 어디까지?\n${PAGER_7}\n민감한 메모 payload-42\n`),
        code: "INVALID_ADDRESS_LINE",
        secret: "payload-42",
      },
      {
        bytes: Buffer.from("현재 어디까지?\n서울특별시 종로구 청운동 1\n"),
        code: "PAGER_LINE_MISSING",
      },
      {
        bytes: Buffer.from("현재 어디까지?\n잘못된 진행 payload-43\n서울특별시 종로구 청운동 1\n"),
        code: "INVALID_PAGER_LINE",
        secret: "payload-43",
      },
    ]

    // When / Then
    for (const testCase of cases) {
      const fixture = await createFixture(context, testCase.bytes)
      await assert.rejects(
        prepareProgressUpdate(
          fixture.path,
          parcel("서울특별시 강서구 개화동 255-1"),
          UPDATED_PAGER,
        ),
        (error: unknown) =>
          error instanceof ProgressFileError &&
          error.code === testCase.code &&
          (testCase.secret === undefined || !error.message.includes(testCase.secret)),
      )
    }
  })

  it("stages exclusive synced snapshots before invoking the locked writer", async (context) => {
    // Given
    const source = Buffer.from(`현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n`)
    const fixture = await createFixture(context, source)
    const update = await prepareProgressUpdate(
      fixture.path,
      parcel("서울특별시 강서구 개화동 255-1"),
      UPDATED_PAGER,
    )
    const states: Array<{ path: string; synced: boolean; closed: boolean }> = []
    const operations: ProgressFileOperations = {
      openExclusive: async (path) => {
        const handle = await open(path, "wx")
        const state = { path, synced: false, closed: false }
        states.push(state)
        return {
          writeFile: (data) => handle.writeFile(data),
          sync: async () => {
            await handle.sync()
            state.synced = true
          },
          close: async () => {
            await handle.close()
            state.closed = true
          },
        }
      },
      compareAndWriteLocked: async (targetPath, expectedPath, replacementPath) => {
        assert.equal(states.length, 2)
        assert.equal(
          states.every(({ synced, closed }) => synced && closed),
          true,
        )
        assert.match(basename(expectedPath), /\.expected\./)
        assert.match(basename(replacementPath), /\.replacement\./)
        assert.deepEqual(await readFile(targetPath), await readFile(expectedPath))
        await writeFile(targetPath, await readFile(replacementPath))
        return "written"
      },
      remove: (path) => rm(path, { force: true }),
    }

    // When
    await commitProgressUpdate(update, operations)

    // Then
    assert.equal(states.length, 2)
    assert.deepEqual(
      await readFile(fixture.path),
      Buffer.from(`현재 어디까지?\n${PAGER_8}\n서울특별시 강서구 개화동 255-1\n`),
    )
    assert.deepEqual(await readdir(fixture.directory), ["progress.txt"])
  })

  it("preserves a change made inside the locked primitive immediately before comparison", async (context) => {
    // Given
    const source = Buffer.from(`현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n`)
    const fixture = await createFixture(context, source)
    const update = await prepareProgressUpdate(
      fixture.path,
      parcel("서울특별시 강서구 개화동 255-1"),
      UPDATED_PAGER,
    )
    const changed = Buffer.concat([source, Buffer.from("동시 변경\n")])
    const operations = progressOperations(async (targetPath, expectedPath) => {
      await writeFile(targetPath, changed)
      const targetBytes = await readFile(targetPath)
      const expectedBytes = await readFile(expectedPath)
      return targetBytes.equals(expectedBytes) ? "written" : "source_changed"
    })

    // When / Then
    await assert.rejects(
      commitProgressUpdate(update, operations),
      (error: unknown) => error instanceof ProgressFileError && error.code === "SOURCE_CHANGED",
    )
    assert.deepEqual(await readFile(fixture.path), changed)
    assert.deepEqual(await readdir(fixture.directory), ["progress.txt"])
  })

  it("maps locked writer sharing or IO failures without modifying the target", async (context) => {
    // Given
    const source = Buffer.from(`현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n`)
    const fixture = await createFixture(context, source)
    const update = await prepareProgressUpdate(
      fixture.path,
      parcel("서울특별시 강서구 개화동 255-1"),
      UPDATED_PAGER,
    )
    const operations = progressOperations(async () => {
      throw new SyntheticIoError("sharing violation")
    })

    // When / Then
    await assert.rejects(
      commitProgressUpdate(update, operations),
      (error: unknown) =>
        error instanceof ProgressFileError &&
        error.code === "IO_FAILURE" &&
        !error.message.includes(fixture.path),
    )
    assert.deepEqual(await readFile(fixture.path), source)
    assert.deepEqual(await readdir(fixture.directory), ["progress.txt"])
  })

  it("closes and removes both staging paths when fsync fails", async (context) => {
    // Given
    const source = Buffer.from(`현재 어디까지?\n${PAGER_7}\n서울특별시 종로구 청운동 1\n`)
    const fixture = await createFixture(context, source)
    const update = await prepareProgressUpdate(
      fixture.path,
      parcel("서울특별시 강서구 개화동 255-1"),
      UPDATED_PAGER,
    )
    const unlinkedPaths: string[] = []
    let lockedWriterCalled = false
    const operations: ProgressFileOperations = {
      openExclusive: async (path) => {
        const handle = await open(path, "wx")
        return {
          writeFile: (data) => handle.writeFile(data),
          sync: async () => {
            throw new SyntheticIoError("synthetic fsync failure")
          },
          close: () => handle.close(),
        }
      },
      compareAndWriteLocked: async () => {
        lockedWriterCalled = true
        return "written"
      },
      remove: async (path) => {
        unlinkedPaths.push(path)
        await rm(path, { force: true })
      },
    }

    // When / Then
    await assert.rejects(
      commitProgressUpdate(update, operations),
      (error: unknown) => error instanceof ProgressFileError && error.code === "IO_FAILURE",
    )
    assert.equal(lockedWriterCalled, false)
    assert.equal(unlinkedPaths.length, 2)
    assert.equal(
      unlinkedPaths.some((path) => basename(path).includes(".expected.")),
      true,
    )
    assert.equal(
      unlinkedPaths.some((path) => basename(path).includes(".replacement.")),
      true,
    )
    assert.deepEqual(await readFile(fixture.path), source)
    assert.deepEqual(await readdir(fixture.directory), ["progress.txt"])
  })
})
