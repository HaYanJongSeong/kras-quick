import { LAW_DONG_ENTRIES } from "./kras-lawdong.ts"
import { parseParcelAddress } from "./property-address.ts"

// PNU(19자리) → 지번 주소 변환.
// 구조: 법정동코드(10) + 대지구분(1) + 본번(4) + 부번(4). 대지구분 1=일반, 2=산.
// 법정동코드 맵은 kras-lawdong.ts(국토부 "법정동코드 전체자료.txt" 기반 생성)를 사용한다.

const LAW_DONG: ReadonlyMap<string, string> = new Map(LAW_DONG_ENTRIES)

export type PnuParts = {
  readonly legalDongCode: string
  readonly landType: "plain" | "mountain"
  readonly mainNumber: number
  readonly subNumber: number
}

const PNU_PATTERN = /^\d{19}$/

export function isPnuInput(value: string): boolean {
  return PNU_PATTERN.test(value)
}

export function parsePnu(pnu: string): PnuParts {
  if (!isPnuInput(pnu)) {
    throw new Error(
      `PNU 형식이 올바르지 않습니다. 19자리 숫자여야 합니다. (입력: ${pnu})`,
    )
  }
  const legalDongCode = pnu.slice(0, 10)
  const landTypeDigit = pnu[10]
  const mainNumber = Number.parseInt(pnu.slice(11, 15), 10)
  const subNumber = Number.parseInt(pnu.slice(15, 19), 10)
  if (!Number.isSafeInteger(mainNumber) || mainNumber < 0 || mainNumber > 9999) {
    throw new Error(`PNU 본번이 올바르지 않습니다. (PNU: ${pnu})`)
  }
  if (!Number.isSafeInteger(subNumber) || subNumber < 0 || subNumber > 9999) {
    throw new Error(`PNU 부번이 올바르지 않습니다. (PNU: ${pnu})`)
  }
  if (mainNumber === 0) {
    throw new Error(`PNU 본번이 0입니다. 변환할 수 없습니다. (PNU: ${pnu})`)
  }
  return {
    legalDongCode,
    landType: landTypeDigit === "2" ? "mountain" : "plain",
    mainNumber,
    subNumber,
  }
}

export function pnuToAddress(pnu: string): string {
  const parts = parsePnu(pnu)
  const dongName = LAW_DONG.get(parts.legalDongCode)
  if (dongName === undefined) {
    throw new Error(
      `알 수 없는 법정동코드입니다. (코드: ${parts.legalDongCode}, PNU: ${pnu})`,
    )
  }
  const parcel =
    parts.landType === "mountain" ? `산 ${parts.mainNumber}` : String(parts.mainNumber)
  const address = `${dongName} ${parcel}${parts.subNumber > 0 ? `-${parts.subNumber}` : ""}`
  try {
    parseParcelAddress(address)
  } catch (error) {
    throw new Error(
      `PNU 변환 주소가 지번 형식에 맞지 않습니다. (${address}, PNU: ${pnu}) — ${
        error instanceof Error ? error.message : String(error)
      }`,
    )
  }
  return address
}