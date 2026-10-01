import assert from "node:assert/strict"
import { mkdtemp, rm } from "node:fs/promises"
import { join } from "node:path"
import { describe, it } from "node:test"
import ExcelJS from "exceljs"

import {
  DEFAULT_KRAS_EXCEL_CONFIG,
  type KrasExcelTaskConfig,
  type KrasXmlData,
  updateExcelWithKrasData,
} from "../scripts/kras-excel-updater.ts"

const xmlData: KrasXmlData = {
  unqNo: "",
  landLocNm: "테스트 소재지",
  bldgNm: "",
  jibn: "1-2",
  jimok: "대",
  parea: "",
  larea: "100",
  barea: "40",
  garea: "80",
  fsi: "80",
  blr: "40",
  struNm: "철근콘크리트구조",
  roofNm: "슬래브",
  bldgKindCd: "일반건축물",
  hgt: "12",
  mainUseNm: "업무시설",
  flr: "3/1",
  useAprvYmd: "2020.01.01",
  ownGbn: "",
  ownerNm: "",
}

describe("KRAS Excel BC:BT transfer", () => {
  it("updates a manual capture row by property number in column C", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-excel-"))
    const path = join(root, "book.xlsx")
    try {
      const workbook = new ExcelJS.Workbook()
      const sheet = workbook.addWorksheet("수정본")
      sheet.getCell("C2").value = "67608"
      await workbook.xlsx.writeFile(path)

      const result = await updateExcelWithKrasData(xmlData, { ...DEFAULT_KRAS_EXCEL_CONFIG, excelPath: path }, "67608")

      assert.deepEqual(result, { updated: true, rowNumber: 2, matchedPropertyNumber: "67608" })
      const saved = new ExcelJS.Workbook()
      await saved.xlsx.readFile(path)
      const row = saved.getWorksheet("수정본")?.getRow(2)
      assert.equal(row?.getCell(55).value, "일반건축물")
      assert.equal(row?.getCell(66).value, "80")
      assert.equal(row?.getCell(70).value, "3")
      assert.equal(row?.getCell(71).value, "1")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("writes 건물없음 to BC when Stage 1 confirmed no building", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-excel-"))
    const path = join(root, "book.xlsx")
    try {
      const workbook = new ExcelJS.Workbook()
      const sheet = workbook.addWorksheet("수정본")
      sheet.getCell("C2").value = "67608"
      await workbook.xlsx.writeFile(path)

      const result = await updateExcelWithKrasData(
        xmlData,
        { ...DEFAULT_KRAS_EXCEL_CONFIG, excelPath: path },
        "67608",
        true,
      )

      assert.deepEqual(result, { updated: true, rowNumber: 2, matchedPropertyNumber: "67608" })
      const saved = new ExcelJS.Workbook()
      await saved.xlsx.readFile(path)
      assert.equal(saved.getWorksheet("수정본")?.getCell("BC2").value, "건물없음")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })

  it("writes to a custom sheet and mapping from a task config", async () => {
    const root = await mkdtemp(join(process.env["TEMP"] ?? ".", "kras-excel-"))
    const path = join(root, "custom.xlsx")
    const config: KrasExcelTaskConfig = {
      name: "다른 작업",
      excelPath: path,
      sheetName: "조사표",
      match: { column: "A", fallbackColumn: "B" },
      cells: [
        { column: "B", field: "garea" },
        { column: "C", field: "jimok" },
      ],
    }
    try {
      const workbook = new ExcelJS.Workbook()
      const sheet = workbook.addWorksheet("조사표")
      sheet.getCell("A2").value = "67608"
      await workbook.xlsx.writeFile(path)

      const result = await updateExcelWithKrasData(xmlData, config, "67608")

      assert.deepEqual(result, { updated: true, rowNumber: 2, matchedPropertyNumber: "67608" })
      const saved = new ExcelJS.Workbook()
      await saved.xlsx.readFile(path)
      const row = saved.getWorksheet("조사표")?.getRow(2)
      assert.equal(row?.getCell(2).value, "80")
      assert.equal(row?.getCell(3).value, "대")
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })
})
