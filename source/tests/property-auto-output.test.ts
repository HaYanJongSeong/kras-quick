import assert from "node:assert/strict"
import { describe, it } from "node:test"

import {
  PROPERTY_CERTIFICATE_ROOT,
  propertyAutoOutputPaths,
} from "../scripts/property-auto-output.ts"

describe("property auto output paths", () => {
  it("places PDF at parcel root and artifacts under 기타", () => {
    const paths = propertyAutoOutputPaths("서울특별시 용산구 한남동 685-46")
    assert.equal(
      paths.directory,
      `${PROPERTY_CERTIFICATE_ROOT}\\서울특별시 용산구 한남동 685-46`,
    )
    assert.equal(paths.pdf.endsWith("\\서울특별시 용산구 한남동 685-46.pdf"), true)
    assert.equal(paths.pagePng(1).endsWith("\\기타\\서울특별시 용산구 한남동 685-46_페이지01.png"), true)
    assert.equal(paths.pageXml.endsWith("\\기타\\oz-report-data.xml"), true)
    assert.equal(paths.structureJson.endsWith("\\기타\\oz-structure.json"), true)
  })

  it("rejects invalid page numbers", () => {
    const paths = propertyAutoOutputPaths("주소")
    assert.throws(() => paths.pagePng(0), RangeError)
  })

  it("places portable output under a caller-provided root", () => {
    const paths = propertyAutoOutputPaths("서울특별시 용산구 한남동 685-46", "D:\\Portable\\KRAS")
    assert.equal(paths.directory, "D:\\Portable\\KRAS\\서울특별시 용산구 한남동 685-46")
    assert.equal(paths.pdf, "D:\\Portable\\KRAS\\서울특별시 용산구 한남동 685-46\\서울특별시 용산구 한남동 685-46.pdf")
  })
})
