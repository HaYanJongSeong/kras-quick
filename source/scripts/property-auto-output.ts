import { join } from "node:path"

export const PROPERTY_CERTIFICATE_ROOT =
  "C:\\Users\\admin\\Desktop\\시유재산\\부동산종합증명서"

export type PropertyAutoOutputPaths = {
  readonly directory: string
  readonly otherDirectory: string
  readonly pdf: string
  readonly pagePng: (page: number) => string
  readonly pageXml: string
  readonly structureJson: string
}

function safePageNumber(page: number): string {
  if (!Number.isSafeInteger(page) || page < 1) throw new RangeError("페이지 번호가 올바르지 않습니다.")
  return String(page).padStart(2, "0")
}

export function propertyAutoOutputPaths(
  address: string,
  root: string = PROPERTY_CERTIFICATE_ROOT,
): PropertyAutoOutputPaths {
  const directory = join(root, address)
  const otherDirectory = join(directory, "기타")
  return {
    directory,
    otherDirectory,
    pdf: join(directory, `${address}.pdf`),
    pagePng: (page) => join(otherDirectory, `${address}_페이지${safePageNumber(page)}.png`),
    pageXml: join(otherDirectory, "oz-report-data.xml"),
    structureJson: join(otherDirectory, "oz-structure.json"),
  }
}
