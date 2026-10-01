import ExcelJS from "exceljs"

const EXCEL_PATH = "C:/Users/admin/Desktop/시유재산/건물현장조사_종성.xlsx"
const SHEET_NAME = "수정본"

async function main(): Promise<void> {
  const [srcPropertyNo, dstPropertyNo] = process.argv.slice(2)
  if (srcPropertyNo === undefined || dstPropertyNo === undefined) {
    process.stderr.write("Usage: node scripts/kras-excel-copy.ts <원본재산번호> <대상재산번호>\n")
    process.exit(1)
  }
  const wb = new ExcelJS.Workbook()
  await wb.xlsx.readFile(EXCEL_PATH)
  const ws = wb.getWorksheet(SHEET_NAME)
  if (!ws) throw new Error(`시트 '${SHEET_NAME}'를 찾을 수 없습니다.`)

  const findRow = (propertyNo: string): number | undefined => {
    for (let r = 2; r <= ws.rowCount; r++) {
      if (String(ws.getRow(r).getCell(3).value ?? "").trim() === propertyNo) return r
    }
    return undefined
  }
  const srcRow = findRow(srcPropertyNo)
  const dstRow = findRow(dstPropertyNo)
  if (srcRow === undefined) throw new Error(`재산번호 ${srcPropertyNo} 행을 찾지 못했습니다.`)
  if (dstRow === undefined) throw new Error(`재산번호 ${dstPropertyNo} 행을 찾지 못했습니다.`)

  const columns: [number, string][] = [
    [55, "BC 집합건물구분"], [56, "BD 건물높이"], [57, "BE 건축일자"], [58, "BF 용적률"],
    [59, "BG 공부용도"], [60, "BH 실제용도"], [61, "BI 건물연령"], [62, "BJ 전유부분"],
    [63, "BK 건물구조"], [64, "BL 지붕구조"], [65, "BM 건폐율"], [66, "BN 연면적"],
    [67, "BO 건축면적"], [68, "BP 대지면적"], [69, "BQ 전유면적"], [70, "BR 지상층수"],
    [71, "BS 지하층수"], [72, "BT 보험가입여부"],
  ]
  process.stdout.write(`재산번호 ${srcPropertyNo}(행 ${srcRow}) → ${dstPropertyNo}(행 ${dstRow}) BC:BT 복사\n`)
  for (const [col, name] of columns) {
    const srcValue = ws.getRow(srcRow).getCell(col).value
    const dstValue = ws.getRow(dstRow).getCell(col).value
    process.stdout.write(`  ${name}: ${String(srcValue ?? "")}  (기존 ${String(dstValue ?? "")})\n`)
  }

  for (const [col] of columns) {
    ws.getRow(dstRow).getCell(col).value = ws.getRow(srcRow).getCell(col).value
  }
  await wb.xlsx.writeFile(EXCEL_PATH)
  process.stdout.write("복사 완료.\n")
}

await main()
