export type LedgerLabel = "토지" | "임야" | "토지(폐쇄)" | "임야(폐쇄)"

export const KRAS_EVALUATION_ERROR_CODE = "KRAS_EVALUATION_ERROR"

export type KrasEvaluationMode = "lookup" | "fill_only"

const KRAS_TARGET_ORIGIN = "https://www.kras.go.kr"
const KRAS_TARGET_PATHNAME = "/kras/cert/certView.do"

type KrasEvaluationRequestFields = {
  readonly address: string
  readonly darkModeCss: string
  readonly hasLegalRi: boolean
  readonly ledgerLabel: LedgerLabel
}

export type KrasEvaluationRequest =
  | (KrasEvaluationRequestFields & {
      readonly mode: "lookup"
      readonly darkMode: boolean
    })
  | (KrasEvaluationRequestFields & {
      readonly mode: "fill_only"
      readonly darkMode: false
    })

export type KrasBuildingOption = {
  readonly index: number
  readonly value: string
  readonly label: string
}

export type KrasEvaluationResult =
  | {
      readonly kind: "lookup"
      readonly hasBuilding: boolean
      readonly buildingOptions?: readonly KrasBuildingOption[]
    }
  | { readonly kind: "fill_only" }
  | { readonly kind: "evaluation_error"; readonly code: typeof KRAS_EVALUATION_ERROR_CODE }

export type DependentRefreshState = {
  readonly mutationObserved: boolean
  readonly parentChanged: boolean
  readonly targetPresent: boolean
}

function normalizeWhitespace(address: string): string {
  return address.replace(/\s+/g, " ").trim()
}

export function normalizeAddress(address: string): string {
  const normalized = normalizeWhitespace(address)
  const firstToken = normalized.split(" ")[0]
  return firstToken?.endsWith("구") ? `서울특별시 ${normalized}` : normalized
}

export function isKrasTargetUrl(value: string): boolean {
  if (!URL.canParse(value)) return false
  const url = new URL(value)
  return url.origin === KRAS_TARGET_ORIGIN && url.pathname === KRAS_TARGET_PATHNAME
}

export function dependentListReady(state: DependentRefreshState): boolean {
  return state.targetPresent && (!state.parentChanged || state.mutationObserved)
}

export function addressHasLegalRi(address: string): boolean {
  return normalizeAddress(address)
    .split(" ")
    .some((token) => /^[가-힣·]+리$/.test(token))
}

export function ledgerLabelForAddress(address: string): LedgerLabel {
  const normalized = normalizeAddress(address)
  const mountain = normalized.includes("임야") || /(?:^|\s)산\s*\d/.test(normalized)
  if (normalized.includes("폐쇄")) return mountain ? "임야(폐쇄)" : "토지(폐쇄)"
  return mountain ? "임야" : "토지"
}

export function longestAddressOptionLabel(
  address: string,
  labels: readonly string[],
): string | undefined {
  const normalized = normalizeAddress(address)
  return labels
    .map(normalizeWhitespace)
    .filter(
      (label) =>
        label !== "" && label !== "선택" && label !== "선택사항없음" && normalized.includes(label),
    )
    .sort((left, right) => right.length - left.length)[0]
}
