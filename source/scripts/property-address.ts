import { z } from "zod"

const MAX_ROW_CELL_LENGTH = 500
const MAX_ADDRESS_LENGTH = 120
const PARCEL_ADDRESS =
  /^[가-힣·]+(?:특별시|광역시|특별자치시|특별자치도|도)\s+[가-힣·]+(?:시|군|구)(?:\s+[가-힣·]+구)?\s+(?:[가-힣·]+(?:동|읍|면|리)|[가-힣·]+[1-9]?\d?가|세종로)(?:\s+[가-힣·]+(?:동|읍|면|리))?\s+(?:산\s*)?[1-9]\d{0,3}(?:-[1-9]\d{0,3})?$/u

const rowCellSchema = z.string().max(MAX_ROW_CELL_LENGTH)
const propertyRowSchema = z.tuple([
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
  rowCellSchema,
])
const parcelAddressSchema = z
  .string()
  .max(MAX_ADDRESS_LENGTH)
  .transform((value) => value.replace(/\s+/gu, " ").trim())
  .transform((value) => value.replace(/ 산\s*(?=\d)/u, " 산 "))
  .pipe(z.string().regex(PARCEL_ADDRESS))
  .brand<"ParcelAddress">()

export type ParcelAddress = z.infer<typeof parcelAddressSchema>

export type PropertyAddressErrorCode = "INVALID_ROW" | "INVALID_ADDRESS"

export class PropertyAddressError extends Error {
  override readonly name = "PropertyAddressError"
  readonly code: PropertyAddressErrorCode

  constructor(code: PropertyAddressErrorCode) {
    super("SCP 행의 지번 주소가 올바르지 않습니다.")
    this.code = code
  }
}

export function parseParcelAddress(value: unknown): ParcelAddress {
  const result = parcelAddressSchema.safeParse(value)
  if (!result.success) throw new PropertyAddressError("INVALID_ADDRESS")
  return result.data
}

export function parsePropertyRow(row: unknown): ParcelAddress {
  const result = propertyRowSchema.safeParse(row)
  if (!result.success) throw new PropertyAddressError("INVALID_ROW")
  return parseParcelAddress(result.data[6])
}
