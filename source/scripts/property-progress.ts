import { randomUUID } from "node:crypto"
import { open, readFile, rm } from "node:fs/promises"
import { basename, dirname, join } from "node:path"

import { type ParcelAddress, PropertyAddressError, parseParcelAddress } from "./property-address.ts"
import { compareAndWriteLocked, type LockedWriteResult } from "./property-progress-windows.ts"
import { type PropertyPager, parsePropertyPager } from "./property-watcher-payload.ts"

export const PROPERTY_PROGRESS_FILE =
  "C:\\Users\\admin\\Desktop\\시유재산\\현장조사_분류_활용방안.txt"
export const PROPERTY_PROGRESS_MARKER = "현재 어디까지?"

export type ProgressFileErrorCode =
  | "INVALID_ENCODING"
  | "MARKER_NOT_UNIQUE"
  | "ADDRESS_LINE_MISSING"
  | "INVALID_ADDRESS_LINE"
  | "PAGER_LINE_MISSING"
  | "INVALID_PAGER_LINE"
  | "SOURCE_CHANGED"
  | "IO_FAILURE"

const ERROR_MESSAGES = {
  INVALID_ENCODING: "진행 파일은 올바른 UTF-8이어야 합니다.",
  MARKER_NOT_UNIQUE: "진행 파일의 현재 위치 표시는 정확히 하나여야 합니다.",
  ADDRESS_LINE_MISSING: "진행 파일에 갱신할 지번 주소가 없습니다.",
  INVALID_ADDRESS_LINE: "진행 파일의 현재 지번 주소가 올바르지 않습니다.",
  PAGER_LINE_MISSING: "진행 파일에 갱신할 페이지 진행 표시가 없습니다.",
  INVALID_PAGER_LINE: "진행 파일의 페이지 진행 표시가 올바르지 않습니다.",
  SOURCE_CHANGED: "준비 후 진행 파일이 변경되어 갱신하지 않았습니다.",
  IO_FAILURE: "진행 파일을 안전하게 갱신할 수 없습니다.",
} as const satisfies Record<ProgressFileErrorCode, string>

export class ProgressFileError extends Error {
  override readonly name = "ProgressFileError"
  readonly code: ProgressFileErrorCode

  constructor(code: ProgressFileErrorCode, options?: ErrorOptions) {
    super(ERROR_MESSAGES[code], options)
    this.code = code
  }
}

export type ProgressFileHandle = {
  readonly writeFile: (data: Uint8Array) => Promise<void>
  readonly sync: () => Promise<void>
  readonly close: () => Promise<void>
}

export type ProgressFileOperations = {
  readonly openExclusive: (path: string) => Promise<ProgressFileHandle>
  readonly compareAndWriteLocked: (
    targetPath: string,
    expectedSnapshotPath: string,
    replacementPath: string,
  ) => Promise<LockedWriteResult>
  readonly remove: (path: string) => Promise<void>
}

export type PreparedProgressUpdate = {
  readonly path: string
  readonly sourceBytes: Uint8Array
  readonly updatedBytes: Uint8Array
}

type LineSpan = {
  readonly start: number
  readonly end: number
}

type ProgressSpans = {
  readonly address: LineSpan
  readonly pagerValue: LineSpan
}

type CommitOutcome =
  | { readonly kind: "result"; readonly result: LockedWriteResult }
  | { readonly kind: "error"; readonly error: unknown }

const UTF8_BOM = Buffer.from([0xef, 0xbb, 0xbf])
const UTF8_DECODER = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true })

const nodeOperations = {
  openExclusive: async (path: string): Promise<ProgressFileHandle> => {
    const handle = await open(path, "wx")
    return {
      writeFile: (data) => handle.writeFile(data),
      sync: () => handle.sync(),
      close: () => handle.close(),
    }
  },
  compareAndWriteLocked,
  remove: (path: string) => rm(path, { force: true }),
} satisfies ProgressFileOperations

function decodeUtf8(bytes: Uint8Array): string {
  try {
    return UTF8_DECODER.decode(bytes)
  } catch (error) {
    if (error instanceof TypeError) throw new ProgressFileError("INVALID_ENCODING")
    throw error
  }
}

function findLines(bytes: Uint8Array, start: number): readonly LineSpan[] {
  const lines: LineSpan[] = []
  let lineStart = start
  for (let index = start; index < bytes.length; index += 1) {
    if (bytes[index] !== 0x0a) continue
    const end = index > lineStart && bytes[index - 1] === 0x0d ? index - 1 : index
    lines.push({ start: lineStart, end })
    lineStart = index + 1
  }
  lines.push({ start: lineStart, end: bytes.length })
  return lines
}

function progressSpans(sourceBytes: Uint8Array): ProgressSpans {
  const contentStart = Buffer.from(sourceBytes.subarray(0, UTF8_BOM.length)).equals(UTF8_BOM)
    ? UTF8_BOM.length
    : 0
  decodeUtf8(sourceBytes.subarray(contentStart))
  const lines = findLines(sourceBytes, contentStart)
  const markerIndexes = lines
    .map((line, index) => ({ index, text: decodeUtf8(sourceBytes.subarray(line.start, line.end)) }))
    .filter(({ text }) => text.trim() === PROPERTY_PROGRESS_MARKER)
    .map(({ index }) => index)
  if (markerIndexes.length !== 1) throw new ProgressFileError("MARKER_NOT_UNIQUE")
  const markerIndex = markerIndexes[0]
  if (markerIndex === undefined) throw new ProgressFileError("MARKER_NOT_UNIQUE")

  let addressIndex: number | undefined
  for (let index = markerIndex + 1; index < lines.length; index += 1) {
    const line = lines[index]
    if (line === undefined) continue
    const text = decodeUtf8(sourceBytes.subarray(line.start, line.end))
    if (text.trim().length > 0) addressIndex = index
  }
  if (addressIndex === undefined) throw new ProgressFileError("ADDRESS_LINE_MISSING")
  const addressLine = lines[addressIndex]
  if (addressLine === undefined) throw new ProgressFileError("ADDRESS_LINE_MISSING")

  const pagerIndex = addressIndex - 1
  if (pagerIndex <= markerIndex) throw new ProgressFileError("PAGER_LINE_MISSING")
  const pagerLine = lines[pagerIndex]
  if (pagerLine === undefined) throw new ProgressFileError("PAGER_LINE_MISSING")
  const pagerLineText = decodeUtf8(sourceBytes.subarray(pagerLine.start, pagerLine.end))
  if (pagerLineText.trim().length === 0) throw new ProgressFileError("PAGER_LINE_MISSING")
  const pagerMatch = /^(\d+\/\d+)/u.exec(pagerLineText)
  const existingPagerText = pagerMatch?.[1]
  if (existingPagerText === undefined || parsePropertyPager(existingPagerText) === undefined) {
    throw new ProgressFileError("INVALID_PAGER_LINE")
  }

  const existingAddress = decodeUtf8(sourceBytes.subarray(addressLine.start, addressLine.end))
  try {
    parseParcelAddress(existingAddress)
  } catch (error) {
    if (error instanceof PropertyAddressError) {
      throw new ProgressFileError("INVALID_ADDRESS_LINE")
    }
    throw error
  }
  return {
    address: addressLine,
    pagerValue: {
      start: pagerLine.start,
      end: pagerLine.start + Buffer.byteLength(existingPagerText),
    },
  }
}

export async function prepareProgressUpdate(
  path: string,
  address: ParcelAddress,
  pager: PropertyPager,
): Promise<PreparedProgressUpdate> {
  let source: Uint8Array
  try {
    source = await readFile(path)
  } catch {
    throw new ProgressFileError("IO_FAILURE")
  }
  const sourceBytes = Buffer.from(source)
  const spans = progressSpans(sourceBytes)
  const updatedBytes = Buffer.concat([
    sourceBytes.subarray(0, spans.pagerValue.start),
    Buffer.from(pager),
    sourceBytes.subarray(spans.pagerValue.end, spans.address.start),
    Buffer.from(address),
    sourceBytes.subarray(spans.address.end),
  ])
  return { path, sourceBytes, updatedBytes }
}

function stagingPath(targetPath: string, kind: "expected" | "replacement"): string {
  return join(
    dirname(targetPath),
    `.${basename(targetPath)}.${kind}.${process.pid}.${randomUUID()}.tmp`,
  )
}

async function stageFile(
  path: string,
  bytes: Uint8Array,
  operations: ProgressFileOperations,
): Promise<void> {
  const handle = await operations.openExclusive(path)
  try {
    await handle.writeFile(bytes)
    await handle.sync()
  } finally {
    await handle.close()
  }
}

export async function commitProgressUpdate(
  update: PreparedProgressUpdate,
  operations: ProgressFileOperations = nodeOperations,
): Promise<void> {
  const expectedSnapshotPath = stagingPath(update.path, "expected")
  const replacementPath = stagingPath(update.path, "replacement")
  let outcome: CommitOutcome
  try {
    await stageFile(expectedSnapshotPath, update.sourceBytes, operations)
    await stageFile(replacementPath, update.updatedBytes, operations)
    outcome = {
      kind: "result",
      result: await operations.compareAndWriteLocked(
        update.path,
        expectedSnapshotPath,
        replacementPath,
      ),
    }
  } catch (error) {
    outcome = { kind: "error", error }
  }

  const cleanup = await Promise.allSettled([
    operations.remove(expectedSnapshotPath),
    operations.remove(replacementPath),
  ])
  if (cleanup.some(({ status }) => status === "rejected")) {
    throw new ProgressFileError("IO_FAILURE")
  }

  switch (outcome.kind) {
    case "result":
      switch (outcome.result) {
        case "written":
          return
        case "source_changed":
          throw new ProgressFileError("SOURCE_CHANGED")
        default:
          throw new ProgressFileError("IO_FAILURE")
      }
    case "error":
      if (outcome.error instanceof ProgressFileError) throw outcome.error
      throw new ProgressFileError("IO_FAILURE")
  }
}

export async function updateProgressFile(
  path: string,
  address: ParcelAddress,
  pager: PropertyPager,
): Promise<void> {
  const update = await prepareProgressUpdate(path, address, pager)
  await commitProgressUpdate(update)
}
