import assert from "node:assert/strict"
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { describe, it } from "node:test"
import { join } from "node:path"

import {
  appendDoneBuildingIndex,
  assertKrasAutoStageTransition,
  createKrasAutoStage2Result,
  createKrasManualStage2Result,
  isKrasStage3OutputComplete,
  isViewerUrl,
  nextKrasAutoAction,
  parseKrasAutoStage2Result,
  parseKrasAutoStageResult,
  propertyAutoStagePaths,
  readDoneBuildingIndexes,
  runKrasAutoStage3,
  runKrasAutoStage1,
  type KrasAutoStage1Result,
  type KrasAutoStage2Result,
  withBuildingSuffix,
} from "../scripts/kras-auto-stages.ts"
import { parseParcelAddress } from "../scripts/property-address.ts"

const stage1: KrasAutoStage1Result = {
  version: 1,
  stage: 1,
  status: "completed",
  address: "서울특별시 용산구 용산동2가 5-227",
  hasBuilding: true,
  buildingOptions: [{ index: 1, value: "A", label: "주건축물" }],
}

const stage2: KrasAutoStage2Result = {
  version: 1,
  stage: 2,
  status: "completed",
  address: stage1.address,
  building: { index: 1, value: "A", label: "주건축물" },
}

const VALID_XML = `<?xml version='1.0' encoding='euc-kr'?><REAL_LAND_BLDG><LAND_BASE_SET><LAND_BASE><UNQ_NO>1117011000-2-0005-0227</UNQ_NO><LAND_LOC_NM>서울특별시 용산구 용산동2가 5-227</LAND_LOC_NM><BLDG_NM>주건축물</BLDG_NM><JIBN>5-227</JIBN><JIMOK>대</JIMOK><PAREA>123.45</PAREA><LAREA>200.0</LAREA><BAREA>80.0</BAREA><GAREA>150.0</GAREA><FSI>150</FSI><BLR>40</BLR><STRU_NM>철근콘크리트</STRU_NM><ROOF_NM>평슬라브</ROOF_NM><BLDG_KIND_CD>주거용</BLDG_KIND_CD><HGT>15</HGT><MAIN_USE_NM>단독주택</MAIN_USE_NM><FLR>2/1</FLR><USE_APRV_YMD>2000.01.01</USE_APRV_YMD><OWN_GBN>소유</OWN_GBN><OWNER_NM>홍길동</OWNER_NM></LAND_BASE></LAND_BASE_SET></REAL_LAND_BLDG>`

const MISMATCH_XML = `<?xml version='1.0' encoding='euc-kr'?><REAL_LAND_BLDG><LAND_BASE_SET><LAND_BASE><UNQ_NO>1117011000-2-0005-0999</UNQ_NO><LAND_LOC_NM>서울특별시 노원구 월계동 392-19</LAND_LOC_NM><BLDG_NM>주건축물</BLDG_NM><JIBN>392-19</JIBN><JIMOK>대</JIMOK><PAREA>0</PAREA><LAREA>0</LAREA><BAREA>0</BAREA><GAREA>0</GAREA><FSI>0</FSI><BLR>0</BLR><STRU_NM>철근콘크리트</STRU_NM><ROOF_NM>평슬라브</ROOF_NM><BLDG_KIND_CD>주거용</BLDG_KIND_CD><HGT>15</HGT><MAIN_USE_NM>단독주택</MAIN_USE_NM><FLR>2/1</FLR><USE_APRV_YMD>2000.01.01</USE_APRV_YMD><OWN_GBN>소유</OWN_GBN><OWNER_NM>홍길동</OWNER_NM></LAND_BASE></LAND_BASE_SET></REAL_LAND_BLDG>`

describe("KRAS automatic stage state", () => {
  it("recognizes only the exact OZ Viewer target", () => {
    assert.equal(isViewerUrl("https://kras.go.kr/oz80/ozhviewer/report.jsp"), true)
    assert.equal(isViewerUrl("https://www.kras.go.kr/oz80/ozhviewer/report.jsp"), false)
    assert.equal(isViewerUrl("https://kras.go.kr/oz80/ozhviewer/report.jsp?x=1"), true)
  })

  it("accepts OZ address metadata with trailing parcel details", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-stage3-address-"))
    const paths = propertyAutoStagePaths(stage2.address, root)
    try {
      const result = await runKrasAutoStage3(stage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => false,
        capturePdf: async () => Buffer.from("pdf"),
        capturePagePngs: async () => [Buffer.from("png")],
        readReportXml: async () => VALID_XML.replace(
          "<LAND_LOC_NM>서울특별시 용산구 용산동2가 5-227</LAND_LOC_NM>",
          "<LAND_LOC_NM>서울특별시 용산구 용산동2가 5-227 (용산동2가)</LAND_LOC_NM>",
        ),
      }, paths)

      assert.equal(result.status, "completed")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("writes Stage 3 artifacts and persists completed state", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-stage3-"))
    const paths = {
      directory: root,
      otherDirectory: join(root, "기타"),
      pdf: join(root, "certificate.pdf"),
      pagePng: (page: number) => join(root, "기타", `page-${page}.png`),
      pageXml: join(root, "기타", "report.xml"),
      structureJson: join(root, "기타", "oz-structure.json"),
      stage1Json: join(root, "기타", "stage1.json"),
      stage2Json: join(root, "기타", "stage2.json"),
      stage3Json: join(root, "기타", "stage3.json"),
      doneBuildingsJson: join(root, "기타", "done-buildings.json"),
    }
    try {
      const result = await runKrasAutoStage3(stage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => false,
        capturePdf: async () => Buffer.from("pdf"),
        capturePagePngs: async () => [Buffer.from("png")],
        readReportXml: async () => VALID_XML,
      }, paths)

      const pdfPath = join(root, "certificate-주건축물.pdf")
      const pngPath = join(root, "기타", "page-1-주건축물.png")
      const xmlPath = join(root, "기타", "report-주건축물.xml")
      const structureJsonPath = join(root, "기타", "oz-structure-주건축물.json")
      assert.equal(result.status, "completed")
      if (result.status !== "completed") {
        console.error("DEBUG result:", JSON.stringify(result, null, 2));
        throw new Error("Stage 3 should have completed");
      }
      assert.equal(result.output.pdf, pdfPath)
      assert.equal((await readFile(pdfPath, "utf8")), "pdf")
      assert.equal(result.output.pagePngs[0], pngPath)
      assert.equal((await readFile(pngPath, "utf8")), "png")
      assert.equal(result.output.pageXml, xmlPath)
      assert.equal((await readFile(xmlPath, "utf8")), VALID_XML)
      assert.equal(result.output.structureJson, structureJsonPath)
      const structureJson = JSON.parse(await readFile(structureJsonPath, "utf8"))
      assert.equal(structureJson.address, "서울특별시 용산구 용산동2가 5-227")
      assert.equal(structureJson.buildingName, "주건축물")
      assert.equal(JSON.parse(await readFile(paths.stage3Json, "utf8")).status, "completed")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("includes building and floor/unit labels in Stage 3 artifact suffixes", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-floor-room-"))
    const paths = propertyAutoStagePaths(stage2.address, root)
    const unitStage2: KrasAutoStage2Result = {
      ...stage2,
      floorRoom: { index: 1, value: "101", label: "1층-101호" },
    }
    try {
      const result = await runKrasAutoStage3(unitStage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => false,
        capturePdf: async () => Buffer.from("pdf"),
        capturePagePngs: async () => [Buffer.from("png")],
        readReportXml: async () => VALID_XML,
      }, paths)

      assert.equal(result.status, "completed")
      if (result.status !== "completed") throw new Error("Stage 3 should complete")
      assert.match(result.output.pdf, /-주건축물-1층-101호\.pdf$/u)
      assert.match(result.output.pageXml, /-주건축물-1층-101호\.xml$/u)
      assert.match(result.output.pagePngs[0] ?? "", /-주건축물-1층-101호\.png$/u)
      assert.match(result.output.structureJson, /-주건축물-1층-101호\.json$/u)
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("recognizes only complete exact building and floor/unit artifact sets", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-output-complete-"))
    const paths = propertyAutoStagePaths(stage2.address, root)
    const building = stage2.building
    const floorRoom = { index: 1, value: "101", label: "1층-101호" }
    const sibling = { index: 2, value: "102", label: "1층-102호" }
    try {
      await mkdir(paths.otherDirectory, { recursive: true })
      await writeFile(withBuildingSuffix(paths.pdf, paths.directory, building.label), "pdf")
      assert.equal(isKrasStage3OutputComplete(paths, building), false)

      await writeFile(withBuildingSuffix(paths.pagePng(1), paths.otherDirectory, building.label), "png")
      await writeFile(withBuildingSuffix(paths.pageXml, paths.otherDirectory, building.label), "xml")
      await writeFile(withBuildingSuffix(paths.structureJson, paths.otherDirectory, building.label), "json")
      assert.equal(isKrasStage3OutputComplete(paths, building), true)
      assert.equal(isKrasStage3OutputComplete(paths, building, floorRoom), false)

      const floorRoomLabel = `${building.label}-${floorRoom.label}`
      await writeFile(withBuildingSuffix(paths.pdf, paths.directory, floorRoomLabel), "pdf")
      await writeFile(withBuildingSuffix(paths.pagePng(1), paths.otherDirectory, floorRoomLabel), "png")
      await writeFile(withBuildingSuffix(paths.pageXml, paths.otherDirectory, floorRoomLabel), "xml")
      await writeFile(withBuildingSuffix(paths.structureJson, paths.otherDirectory, floorRoomLabel), "json")
      assert.equal(isKrasStage3OutputComplete(paths, building, floorRoom), true)
      assert.equal(isKrasStage3OutputComplete(paths, building, sibling), false)
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("writes portable quick artifacts under KRAS root without changing an Excel file", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-quick-capture-"))
    const portableRoot = join(root, "KRAS")
    const excelPath = join(root, "workbook.xlsx")
    const paths = propertyAutoStagePaths(stage2.address, portableRoot)
    try {
      await writeFile(excelPath, "excel-sentinel", "utf8")

      const result = await runKrasAutoStage3(stage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => false,
        capturePdf: async () => Buffer.from("portable-pdf"),
        capturePagePngs: async () => [Buffer.from("portable-png")],
        readReportXml: async () => VALID_XML,
      }, paths)

      assert.equal(result.status, "completed")
      if (result.status !== "completed") throw new Error("Portable capture should complete")
      assert.equal(result.output.pdf.startsWith(portableRoot), true)
      assert.equal(result.output.pageXml.startsWith(portableRoot), true)
      assert.equal(await readFile(result.output.pdf, "utf8"), "portable-pdf")
      assert.equal(await readFile(result.output.pagePngs[0] ?? "", "utf8"), "portable-png")
      assert.equal(result.output.structureJson.startsWith(portableRoot), true)
      assert.equal(await readFile(excelPath, "utf8"), "excel-sentinel")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("asks for a suffix when the PDF or XML artifact already exists", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-stage3-"))
    const paths = {
      directory: root,
      otherDirectory: join(root, "기타"),
      pdf: join(root, "certificate.pdf"),
      pagePng: (page: number) => join(root, "기타", `page-${page}.png`),
      pageXml: join(root, "기타", "report.xml"),
      structureJson: join(root, "기타", "oz-structure.json"),
      stage1Json: join(root, "기타", "stage1.json"),
      stage2Json: join(root, "기타", "stage2.json"),
      stage3Json: join(root, "기타", "stage3.json"),
      doneBuildingsJson: join(root, "기타", "done-buildings.json"),
    }
    await mkdir(join(root, "기타"), { recursive: true })
    await writeFile(join(root, "certificate-주건축물.pdf"), "old-pdf")
    await writeFile(join(root, "기타", "report-주건축물.xml"), "old-xml")
    const conflicts: string[] = []
    try {
      const result = await runKrasAutoStage3(stage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => false,
        capturePdf: async () => Buffer.from("pdf"),
        capturePagePngs: async () => [Buffer.from("png")],
        readReportXml: async () => VALID_XML,
      }, paths, async (existingPath) => {
        conflicts.push(existingPath)
        return "-재발급"
      })

      assert.equal(result.status, "completed")
      if (result.status !== "completed") throw new Error("Stage 3 should have completed")
      const renamedPdf = join(root, "certificate-주건축물-재발급.pdf")
      const renamedXml = join(root, "기타", "report-주건축물-재발급.xml")
      assert.equal(result.output.pdf, renamedPdf)
      assert.equal(result.output.pageXml, renamedXml)
      assert.equal((await readFile(renamedPdf, "utf8")), "pdf")
      assert.equal((await readFile(renamedXml, "utf8")), VALID_XML)
      assert.equal((await readFile(join(root, "certificate-주건축물.pdf"), "utf8")), "old-pdf")
      assert.equal((await readFile(join(root, "기타", "report-주건축물.xml"), "utf8")), "old-xml")
      assert.deepEqual(conflicts, [
        join(root, "certificate-주건축물.pdf"),
        join(root, "기타", "report-주건축물.xml"),
      ])
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("rejects a conflict suffix that escapes the artifact directory", async () => {
    // Given
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-suffix-escape-"))
    const paths = propertyAutoStagePaths(stage2.address, root)
    await mkdir(paths.otherDirectory, { recursive: true })
    await writeFile(withBuildingSuffix(paths.pdf, paths.directory, stage2.building.label), "old-pdf")

    try {
      // When
      const execution = runKrasAutoStage3(
        stage2,
        {
          url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
          hasVisibleCaptcha: async () => false,
          capturePdf: async () => Buffer.from("pdf"),
          capturePagePngs: async () => [Buffer.from("png")],
          readReportXml: async () => VALID_XML,
        },
        paths,
        async () => "..\\..\\outside",
      )

      // Then
      await assert.rejects(execution)
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("blocks artifacts when the OZ parcel address differs from Stage 2", async () => {
    // Given
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-identity-"))
    const paths = propertyAutoStagePaths(stage2.address, root)

    try {
      // When
      const result = await runKrasAutoStage3(
        stage2,
        {
          url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
          hasVisibleCaptcha: async () => false,
          capturePdf: async () => Buffer.from("pdf"),
          capturePagePngs: async () => [Buffer.from("png")],
          readReportXml: async () => MISMATCH_XML,
        },
        paths,
      )

      // Then
      assert.equal(result.status, "blocked")
      if (result.status === "blocked") assert.equal(result.reason, "document_mismatch")
      await assert.rejects(readFile(withBuildingSuffix(paths.pdf, paths.directory, stage2.building.label)))
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("creates a manual 열람 Stage 2 with an empty building label", () => {
    const manual = createKrasManualStage2Result(stage1)
    assert.equal(manual.status, "completed")
    if (manual.status !== "completed") throw new Error("manual Stage 2 should complete")
    assert.equal(manual.address, stage1.address)
    assert.deepEqual(manual.building, { index: 0, value: "", label: "" })
    assert.deepEqual(parseKrasAutoStage2Result(JSON.parse(JSON.stringify(manual))), manual)
  })

  it("blocks Stage 3 without interacting with CAPTCHA", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-stage3-"))
    const paths = {
      directory: root,
      otherDirectory: join(root, "기타"),
      pdf: join(root, "certificate.pdf"),
      pagePng: (page: number) => join(root, "기타", `page-${page}.png`),
      pageXml: join(root, "기타", "report.xml"),
      structureJson: join(root, "기타", "oz-structure.json"),
      stage1Json: join(root, "기타", "stage1.json"),
      stage2Json: join(root, "기타", "stage2.json"),
      stage3Json: join(root, "기타", "stage3.json"),
      doneBuildingsJson: join(root, "기타", "done-buildings.json"),
    }
    let captureCalled = false
    try {
      const result = await runKrasAutoStage3(stage2, {
        url: () => "https://kras.go.kr/oz80/ozhviewer/report.jsp",
        hasVisibleCaptcha: async () => true,
        capturePdf: async () => {
          captureCalled = true
          return Buffer.from("pdf")
        },
        capturePagePngs: async () => [],
        readReportXml: async () => "",
      }, paths)

      assert.deepEqual(result, {
        version: 1,
        stage: 3,
        status: "blocked",
        address: stage2.address,
        reason: "captcha_required",
      })
      assert.equal(captureCalled, false)
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("uses the shared certificate directory for stage artifacts", () => {
    const paths = propertyAutoStagePaths(stage1.address)
    assert.match(paths.stage1Json, /\\기타\\stage1\.json$/u)
    assert.match(paths.stage2Json, /\\기타\\stage2\.json$/u)
    assert.match(paths.pdf, new RegExp(`${stage1.address}\\.pdf$`, "u"))
  })

  it("keeps Stage 1 building options instead of reducing them to a status", async () => {
    const page = {
      url: () => "https://www.kras.go.kr/kras/cert/certView.do",
      evaluate: async () => ({
        kind: "lookup" as const,
        hasBuilding: true,
        buildingOptions: stage1.buildingOptions,
      }),
    }
    assert.deepEqual(await runKrasAutoStage1(page, parseParcelAddress(stage1.address)), stage1)
  })

  it("parses persisted stage results and reports next action", () => {
    assert.deepEqual(parseKrasAutoStageResult(stage1), stage1)
    assert.deepEqual(nextKrasAutoAction([]), { kind: "ready", stage: 1 })
    assert.deepEqual(nextKrasAutoAction([stage1]), { kind: "ready", stage: 2 })
    assert.deepEqual(nextKrasAutoAction([stage1, stage2]), { kind: "ready", stage: 3 })
  })

  it("requires user choice when stage 1 returns multiple buildings", () => {
    const multiple: KrasAutoStage1Result = {
      ...stage1,
      buildingOptions: [
        ...stage1.buildingOptions,
        { index: 2, value: "B", label: "부속건축물" },
      ],
    }

    assert.deepEqual(nextKrasAutoAction([multiple]), {
      kind: "await_user",
      stage: 2,
      reason: "building_selection_required",
    })
  })

  it("creates Stage 2 only for an explicitly offered building", () => {
    assert.deepEqual(createKrasAutoStage2Result(stage1, 1), stage2)
    assert.throws(() => createKrasAutoStage2Result(stage1, 9))
  })

  it("rejects address mismatch and selections absent from stage 1", () => {
    assert.throws(() =>
      assertKrasAutoStageTransition(stage1, { ...stage2, address: "다른 주소" }),
    )
    assert.throws(() =>
      assertKrasAutoStageTransition(stage1, {
        version: 1,
        stage: 2,
        status: "completed",
        address: stage1.address,
        building: { index: 9, value: "X", label: "없는 건물" },
      }),
    )
  })

  it("stops after no-building result", () => {
    const blocked: KrasAutoStage1Result = {
      version: 1,
      stage: 1,
      status: "blocked",
      address: stage1.address,
      reason: "no_building",
      hasBuilding: false,
      buildingOptions: [],
    }

    assert.deepEqual(nextKrasAutoAction([blocked]), {
      kind: "stop",
      reason: "no_building",
    })
  })
})

describe("KRAS done-building history", () => {
  it("appends indices without duplicates and reads them back", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-done-"))
    const paths = {
      directory: root,
      otherDirectory: join(root, "기타"),
      pdf: join(root, "certificate.pdf"),
      pagePng: (page: number) => join(root, "기타", `page-${page}.png`),
      pageXml: join(root, "기타", "report.xml"),
      structureJson: join(root, "기타", "oz-structure.json"),
      stage1Json: join(root, "기타", "stage1.json"),
      stage2Json: join(root, "기타", "stage2.json"),
      stage3Json: join(root, "기타", "stage3.json"),
      doneBuildingsJson: join(root, "기타", "done-buildings.json"),
    }
    try {
      assert.deepEqual(readDoneBuildingIndexes(paths), [])
      appendDoneBuildingIndex(paths, 1)
      appendDoneBuildingIndex(paths, 2)
      appendDoneBuildingIndex(paths, 1)
      assert.deepEqual(readDoneBuildingIndexes(paths), [1, 2])
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("ignores a corrupted history file", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-done-"))
    const paths = {
      directory: root,
      otherDirectory: join(root, "기타"),
      pdf: join(root, "certificate.pdf"),
      pagePng: (page: number) => join(root, "기타", `page-${page}.png`),
      pageXml: join(root, "기타", "report.xml"),
      structureJson: join(root, "기타", "oz-structure.json"),
      stage1Json: join(root, "기타", "stage1.json"),
      stage2Json: join(root, "기타", "stage2.json"),
      stage3Json: join(root, "기타", "stage3.json"),
      doneBuildingsJson: join(root, "기타", "done-buildings.json"),
    }
    try {
      await mkdir(join(root, "기타"), { recursive: true })
      await writeFile(paths.doneBuildingsJson, "not json", "utf8")
      assert.deepEqual(readDoneBuildingIndexes(paths), [])
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })
})
