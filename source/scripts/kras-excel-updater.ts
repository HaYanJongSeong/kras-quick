export const KRAS_EXCEL_PATH = "C:/Users/admin/Desktop/시유재산/건물현장조사_종성.xlsx"
export const KRAS_SHEET_NAME = "수정본"

export type KrasXmlData = {
  readonly unqNo: string
  readonly landLocNm: string
  readonly bldgNm: string
  readonly jibn: string
  readonly jimok: string
  readonly parea: string
  readonly larea: string
  readonly barea: string
  readonly garea: string
  readonly fsi: string
  readonly blr: string
  readonly struNm: string
  readonly roofNm: string
  readonly bldgKindCd: string
  readonly hgt: string
  readonly mainUseNm: string
  readonly flr: string
  readonly useAprvYmd: string
  readonly ownGbn: string
  readonly ownerNm: string
}

export function parseKrasXml(xmlContent: string): KrasXmlData {
  const clean = xmlContent.replace(/^ozdata=?/, "")
  const tag = (t: string): string => {
    const m = clean.match(new RegExp(`<${t}>([^<]*)</${t}>`))
    return m?.[1]?.trim() ?? ""
  }
  const unqNo = tag("UNQ_NO")
  const jibnParts = unqNo.split("-")
  const jibnFromUnqNo =
    jibnParts.length >= 4 ? `${Number.parseInt(jibnParts[2] ?? "", 10)}-${Number.parseInt(jibnParts[3] ?? "", 10)}` : ""
  return {
    unqNo,
    landLocNm: tag("LAND_LOC_NM"),
    bldgNm: tag("BLDG_NM"),
    jibn: tag("JIBN") || jibnFromUnqNo,
    jimok: tag("JIMOK"),
    parea: tag("PAREA"),
    larea: tag("LAREA"),
    barea: tag("BAREA"),
    garea: tag("GAREA"),
    fsi: tag("FSI"),
    blr: tag("BLR"),
    struNm: tag("STRU_NM"),
    roofNm: tag("ROOF_NM"),
    bldgKindCd: tag("BLDG_KIND_CD"),
    hgt: tag("HGT"),
    mainUseNm: tag("MAIN_USE_NM"),
    flr: tag("FLR"),
    useAprvYmd: tag("USE_APRV_YMD"),
    ownGbn: tag("OWN_GBN"),
    ownerNm: tag("OWNER_NM"),
  }
}

export function pnuFromUnqNo(unqNo: string): string {
  return unqNo.replace(/-/g, "")
}

export function splitJibn(jibn: string): { main: string; sub: string } {
  const parts = jibn.split("-")
  return { main: parts[0] ?? "", sub: parts[1] ?? "" }
}

function parseFlr(flrcd: string): { ground: string; basement: string } {
  const parts = flrcd.split("/")
  return { ground: parts[0] ?? "0", basement: parts[1] ?? "0" }
}

function buildingAge(useAprvYmd: string): string {
  if (!useAprvYmd) return ""
  const match = useAprvYmd.match(/(\d{4})\.(\d{2})\.(\d{2})/)
  if (!match) return ""
  const year = Number(match[1])
  const now = new Date()
  return String(now.getFullYear() - year)
}

export type KrasBcBtCell = {
  readonly col: string
  readonly no: number
  readonly name: string
  readonly field: keyof KrasXmlData | "buildingAge" | "flrGround" | "flrBasement" | null
  readonly note?: string
}

export const KRAS_BCBT_FORMAT: readonly KrasBcBtCell[] = [
  { col: "BC", no: 55, name: "집합건물구분", field: "bldgKindCd" },
  { col: "BD", no: 56, name: "건물높이", field: "hgt" },
  { col: "BE", no: 57, name: "건축일자", field: "useAprvYmd" },
  { col: "BF", no: 58, name: "용적률", field: "fsi" },
  { col: "BG", no: 59, name: "공부용도", field: "mainUseNm" },
  { col: "BH", no: 60, name: "실제용도", field: null, note: "입력 안 함 (공란)" },
  { col: "BI", no: 61, name: "건물연령", field: "buildingAge", note: "건축일자에서 계산" },
  { col: "BJ", no: 62, name: "전유부분", field: null, note: "입력 안 함 (기존 유지)" },
  { col: "BK", no: 63, name: "건물구조", field: "struNm" },
  { col: "BL", no: 64, name: "지붕구조", field: "roofNm" },
  { col: "BM", no: 65, name: "건폐율", field: "blr" },
  { col: "BN", no: 66, name: "연면적", field: "garea" },
  { col: "BO", no: 67, name: "건축면적", field: "barea" },
  { col: "BP", no: 68, name: "대지면적", field: "larea" },
  { col: "BQ", no: 69, name: "전유면적", field: null, note: "입력 안 함 (기존 유지)" },
  { col: "BR", no: 70, name: "지상층수", field: "flrGround", note: "FLR 앞부분" },
  { col: "BS", no: 71, name: "지하층수", field: "flrBasement", note: "FLR 뒷부분" },
  { col: "BT", no: 72, name: "보험가입여부", field: null, note: "입력 안 함 (확인 불가)" },
]

export type KrasExcelTransform =
  | "raw"
  | "before("
  | "jibn-main"
  | "jibn-sub"
  | "flr-ground"
  | "flr-basement"
  | "building-age"
  | "building-kind"

export type KrasExcelCellRule = {
  readonly column: string
  readonly field: keyof KrasXmlData | null
  readonly transform?: KrasExcelTransform
  readonly name?: string
  readonly note?: string
}

export type KrasExcelTaskConfig = {
  readonly name: string
  readonly excelPath: string
  readonly sheetName: string
  readonly match: { readonly column: string; readonly fallbackColumn: string }
  readonly cells: readonly KrasExcelCellRule[]
}

export type KrasExcelWritePlan = {
  readonly excelPath: string
  readonly sheetName: string
  readonly cells: readonly { readonly column: string; readonly value: string | null; readonly action: "write" | "keep" }[]
}

const BCBT_CELLS: readonly KrasExcelCellRule[] = KRAS_BCBT_FORMAT.map((cell) => {
  const transform =
    cell.field === "buildingAge"
      ? ("building-age" as const)
      : cell.field === "flrGround"
        ? ("flr-ground" as const)
        : cell.field === "flrBasement"
          ? ("flr-basement" as const)
          : cell.field === "bldgKindCd"
            ? ("building-kind" as const)
            : undefined
  const field: keyof KrasXmlData | null =
    cell.field === "buildingAge"
      ? "useAprvYmd"
      : cell.field === "flrGround" || cell.field === "flrBasement"
        ? "flr"
        : cell.field
  return {
    column: cell.col,
    field,
    ...(transform === undefined ? {} : { transform }),
    name: cell.name,
    ...(cell.note === undefined ? {} : { note: cell.note }),
  }
})

export const DEFAULT_KRAS_EXCEL_CONFIG: KrasExcelTaskConfig = {
  name: "건물현장조사",
  excelPath: KRAS_EXCEL_PATH,
  sheetName: KRAS_SHEET_NAME,
  match: { column: "C", fallbackColumn: "I" },
  cells: [
    { column: "J", field: "landLocNm", transform: "before(" },
    { column: "O", field: "jibn", transform: "jibn-main" },
    { column: "P", field: "jibn", transform: "jibn-sub" },
    { column: "T", field: "jimok" },
    { column: "W", field: "garea" },
    { column: "X", field: "garea" },
    ...BCBT_CELLS,
  ],
}

export function columnToNumber(column: string): number {
  let number = 0
  for (const ch of column.toUpperCase()) number = number * 26 + (ch.charCodeAt(0) - 64)
  return number
}

function cellValue(
  cell: KrasExcelCellRule,
  xmlData: KrasXmlData,
  buildingAbsent: boolean,
): string | undefined {
  if (cell.field === null) return undefined
  const base = xmlData[cell.field] ?? ""
  switch (cell.transform) {
    case undefined:
    case "raw":
      return base
    case "before(":
      return base.split("(")[0]?.trim() ?? base
    case "jibn-main":
      return splitJibn(base).main
    case "jibn-sub":
      return splitJibn(base).sub
    case "flr-ground":
      return parseFlr(base).ground
    case "flr-basement":
      return parseFlr(base).basement
    case "building-age":
      return buildingAge(base)
    case "building-kind":
      return buildingAbsent ? "건물없음" : base
    default:
      return base
  }
}

export function createKrasExcelWritePlan(
  xmlData: KrasXmlData,
  config: KrasExcelTaskConfig = DEFAULT_KRAS_EXCEL_CONFIG,
  buildingAbsent = false,
): KrasExcelWritePlan {
  return {
    excelPath: config.excelPath,
    sheetName: config.sheetName,
    cells: config.cells.map((cell) => {
      const value = cellValue(cell, xmlData, buildingAbsent)
      return { column: cell.column, value: value ?? null, action: value === undefined ? "keep" : "write" }
    }),
  }
}

export function createOZStructure(xmlData: KrasXmlData): object {
  return {
    address: xmlData.landLocNm,
    buildingName: xmlData.bldgNm,
    owner: xmlData.ownerNm,
    landCategory: xmlData.jimok,
    area: xmlData.parea,
    registrationInfo: xmlData.unqNo,
    buildingArea: xmlData.barea,
    landArea: xmlData.larea,
    grossArea: xmlData.garea,
    floorSpaceRatio: xmlData.fsi,
    buildingCoverageRatio: xmlData.blr,
    structure: xmlData.struNm,
    roof: xmlData.roofNm,
    buildingKind: xmlData.bldgKindCd,
    height: xmlData.hgt,
    mainUse: xmlData.mainUseNm,
    floors: xmlData.flr,
    constructionDate: xmlData.useAprvYmd,
    ownershipType: xmlData.ownGbn,
  }
}

export async function updateExcelWithKrasData(
  xmlData: KrasXmlData,
  config: KrasExcelTaskConfig = DEFAULT_KRAS_EXCEL_CONFIG,
  propertyNumber?: string,
  buildingAbsent = false,
): Promise<{ updated: boolean; rowNumber: number; matchedPropertyNumber: string | null }> {
  const { default: ExcelJS } = await import("exceljs")
  const wb = new ExcelJS.Workbook()
  await wb.xlsx.readFile(config.excelPath)
  const ws = wb.getWorksheet(config.sheetName)
  if (!ws) throw new Error(`시트 '${config.sheetName}'를 찾을 수 없습니다.`)

  const lookupColumn = columnToNumber(
    propertyNumber === undefined ? config.match.fallbackColumn : config.match.column,
  )
  const lookupValue = propertyNumber ?? pnuFromUnqNo(xmlData.unqNo)
  let targetRow: number | undefined

  for (let r = 2; r <= ws.rowCount; r++) {
    const cellVal = String(ws.getRow(r).getCell(lookupColumn).value ?? "").replace(/-/g, "").trim()
    if (cellVal === lookupValue.replace(/-/g, "").trim()) {
      targetRow = r
      break
    }
  }

  if (targetRow === undefined) {
    return { updated: false, rowNumber: 0, matchedPropertyNumber: null }
  }

  const row = ws.getRow(targetRow)
  for (const cell of config.cells) {
    const value = cellValue(cell, xmlData, buildingAbsent)
    if (value !== undefined) row.getCell(columnToNumber(cell.column)).value = value
  }

  await wb.xlsx.writeFile(config.excelPath)
  const verifiedWorkbook = new ExcelJS.Workbook()
  await verifiedWorkbook.xlsx.readFile(config.excelPath)
  const verifiedSheet = verifiedWorkbook.getWorksheet(config.sheetName)
  if (verifiedSheet === undefined) throw new Error(`저장 후 시트 '${config.sheetName}'를 다시 찾을 수 없습니다.`)
  const verifiedRow = verifiedSheet.getRow(targetRow)
  for (const cell of config.cells) {
    const expected = cellValue(cell, xmlData, buildingAbsent)
    if (expected !== undefined && String(verifiedRow.getCell(columnToNumber(cell.column)).value ?? "") !== expected) {
      throw new Error(`${config.excelPath} ${config.sheetName} ${cell.column}열 저장 검증 실패`)
    }
  }
  const matchedPropertyNumber =
    propertyNumber ??
    (() => {
      const cellValue = row.getCell(columnToNumber(config.match.column)).value
      return typeof cellValue === "string" || typeof cellValue === "number"
        ? String(cellValue).trim()
        : null
    })()
  return { updated: true, rowNumber: targetRow, matchedPropertyNumber }
}

export function printKrasDataTable(xmlData: KrasXmlData): void {
  const jibnParts = splitJibn(xmlData.jibn)
  const flrParts = parseFlr(xmlData.flr)
  const table: [string, string][] = [
    ["소재지", xmlData.landLocNm.split("(")[0]?.trim() ?? xmlData.landLocNm],
    ["본번", jibnParts.main],
    ["부번", jibnParts.sub],
    ["공부지목", xmlData.jimok],
    ["연면적", xmlData.garea],
    ["공부면적", xmlData.garea],
    ["집합건물구분", xmlData.bldgKindCd],
    ["건물높이", xmlData.hgt],
    ["건축일자", xmlData.useAprvYmd],
    ["용적률", xmlData.fsi],
    ["공부용도", xmlData.mainUseNm],
    ["건물연령", buildingAge(xmlData.useAprvYmd)],
    ["건물구조", xmlData.struNm],
    ["지붕구조", xmlData.roofNm],
    ["건폐율", xmlData.blr],
    ["지상층수", flrParts.ground],
    ["지하층수", flrParts.basement],
  ]
  const col1 = Math.max(...table.map(r => r[0].length)) + 2
  for (const [k, v] of table) {
    process.stdout.write(`${k.padEnd(col1)}${v}\n`)
  }
}
