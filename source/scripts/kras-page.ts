import { type darkModeEvaluation, KRAS_DARK_MODE_CSS } from "./kras-dark.ts"
import {
  addressHasLegalRi,
  isKrasTargetUrl,
  type KrasBuildingOption,
  type KrasEvaluationRequest,
  type KrasEvaluationResult,
  ledgerLabelForAddress,
  normalizeAddress,
} from "./kras-domain.ts"
import type { ParcelAddress } from "./property-address.ts"
import type { PropertyBuildingTerminalStatus } from "./property-watcher-payload.ts"

export type KrasPage = {
  readonly url: () => string
  readonly evaluate: (
    pageFunction: typeof lookupEvaluation,
    request: KrasEvaluationRequest,
  ) => Promise<KrasEvaluationResult>
  readonly applyDarkMode: (pageFunction: typeof darkModeEvaluation, css: string) => Promise<void>
}

export type KrasFillPage = Pick<KrasPage, "url" | "evaluate">

class KrasEvaluationResultError extends Error {
  override readonly name = "KrasEvaluationResultError"
}

export class KrasFillError extends Error {
  override readonly name = "KrasFillError"

  constructor(options?: ErrorOptions) {
    super("KRAS 주소 입력 평가에 실패했습니다.", options)
  }
}

export class KrasFillTargetError extends Error {
  override readonly name = "KrasFillTargetError"

  constructor() {
    super("KRAS 주소 입력 대상 페이지가 아닙니다.")
  }
}

export class KrasLookupError extends Error {
  override readonly name = "KrasLookupError"

  constructor(options?: ErrorOptions) {
    super("KRAS 건물 조회 평가에 실패했습니다.", options)
  }
}

export class KrasLookupTargetError extends Error {
  override readonly name = "KrasLookupTargetError"

  constructor() {
    super("KRAS 건물 조회 대상 페이지가 아닙니다.")
  }
}

function assertNeverResult(result: never): never {
  throw new KrasEvaluationResultError(`처리할 수 없는 KRAS 평가 결과: ${String(result)}`)
}

// allow: SIZE_OK — Playwright requires this DOM transaction to serialize without module closures.
export async function lookupEvaluation(
  request: KrasEvaluationRequest,
): Promise<KrasEvaluationResult> {
  const {
    address: rawAddress,
    darkMode: useDarkMode,
    darkModeCss,
    hasLegalRi,
    ledgerLabel,
  } = request
  class KrasEvaluationError extends Error {
    override readonly name = "KrasEvaluationError"
  }

  const normalize = (value: string | null): string => (value ?? "").replace(/\s+/g, " ").trim()
  const assertNeverMode = (unhandled: never): never => {
    throw new KrasEvaluationError(`처리할 수 없는 KRAS 평가 모드: ${String(unhandled)}`)
  }
  const normalizedAddress = normalize(rawAddress)
  const requiredSelect = (selector: string, field: string): HTMLSelectElement => {
    const select = document.querySelector<HTMLSelectElement>(selector)
    if (select === null) throw new KrasEvaluationError(`${field} 필드를 찾지 못했습니다.`)
    return select
  }
  const matchingAddressOption = (select: HTMLSelectElement): HTMLOptionElement | undefined =>
    Array.from(select.options)
      .filter((option) => {
        const label = normalize(option.textContent)
        return (
          label !== "" &&
          label !== "선택" &&
          label !== "선택사항없음" &&
          normalizedAddress.includes(label)
        )
      })
      .sort(
        (left, right) => normalize(right.textContent).length - normalize(left.textContent).length,
      )[0]
  const placeholderOption = (select: HTMLSelectElement): HTMLOptionElement | undefined =>
    Array.from(select.options).find((option) => {
      const label = normalize(option.textContent)
      return label === "" || label === "선택" || label === "선택사항없음"
    })
  const optionFingerprint = (select: HTMLSelectElement): string =>
    JSON.stringify(
      Array.from(select.options).map((option) => [option.value, normalize(option.textContent)]),
    )
  type OptionFinder = (select: HTMLSelectElement) => HTMLOptionElement | undefined
  type RefreshExpectation = {
    readonly initialFingerprint: string
    readonly requireFingerprintChange: boolean
    readonly target: OptionFinder
  }
  const observeRefresh = (select: HTMLSelectElement, expectation: RefreshExpectation) => {
    let cancel = (): void => undefined
    const done = new Promise<void>((resolve, reject) => {
      let settled = false
      let quietId: number | undefined
      let timeoutId: number | undefined
      const observer = new MutationObserver(() => {
        if (settled) return
        if (quietId !== undefined) window.clearTimeout(quietId)
        quietId = window.setTimeout(() => {
          quietId = undefined
          const fingerprintChanged = optionFingerprint(select) !== expectation.initialFingerprint
          if (
            settled ||
            expectation.target(select) === undefined ||
            (expectation.requireFingerprintChange && !fingerprintChanged)
          ) {
            return
          }
          settled = true
          observer.disconnect()
          if (timeoutId !== undefined) window.clearTimeout(timeoutId)
          resolve()
        }, 100)
      })
      observer.observe(select, { childList: true, subtree: true })
      timeoutId = window.setTimeout(() => {
        if (settled) return
        settled = true
        observer.disconnect()
        if (quietId !== undefined) window.clearTimeout(quietId)
        reject(new KrasEvaluationError("종속 주소 선택지가 갱신되지 않았습니다."))
      }, 4000)
      cancel = () => {
        if (settled) return
        settled = true
        observer.disconnect()
        if (quietId !== undefined) window.clearTimeout(quietId)
        if (timeoutId !== undefined) window.clearTimeout(timeoutId)
      }
    })
    return { done, cancel: () => cancel() }
  }
  const selectedFields: Array<{ readonly select: HTMLSelectElement; readonly label: string }> = []
  const applyOption = (
    select: HTMLSelectElement,
    option: HTMLOptionElement,
    label: string,
  ): void => {
    select.value = option.value
    select.dispatchEvent(new Event("input", { bubbles: true }))
    select.dispatchEvent(new Event("change", { bubbles: true }))
    selectedFields.push({ select, label })
  }
  const selectAddress = (selector: string, field: string): void => {
    const select = requiredSelect(selector, field)
    const match = matchingAddressOption(select)
    if (match === undefined) {
      throw new KrasEvaluationError(`${field} 주소를 선택하지 못했습니다.`)
    }
    applyOption(select, match, normalize(match.textContent))
  }
  const selectPlaceholder = (selector: string): void => {
    const select = document.querySelector<HTMLSelectElement>(selector)
    if (select === null) return
    const placeholder = placeholderOption(select)
    if (placeholder === undefined) {
      throw new KrasEvaluationError(`${selector} 기본 선택지를 찾지 못했습니다.`)
    }
    applyOption(select, placeholder, normalize(placeholder.textContent))
  }
  const exactOption = (select: HTMLSelectElement, label: string): void => {
    const option = Array.from(select.options).find(
      (candidate) => normalize(candidate.textContent) === label,
    )
    if (option === undefined) {
      throw new KrasEvaluationError(`지목 ${label}을 선택하지 못했습니다.`)
    }
    applyOption(select, option, label)
  }
  type CascadeStep = {
    readonly awaitDependentRefresh: boolean
    readonly dependentSelector: string
    readonly parentField: string
    readonly parentSelector: string
    readonly requireFingerprintChange: boolean
    readonly target: OptionFinder
  }
  const selectParent = async (step: CascadeStep): Promise<void> => {
    const parent = requiredSelect(step.parentSelector, step.parentField)
    const option = matchingAddressOption(parent)
    if (option === undefined) {
      throw new KrasEvaluationError(`${step.parentField} 주소를 선택하지 못했습니다.`)
    }
    const label = normalize(option.textContent)
    if (parent.value === option.value) {
      selectedFields.push({ select: parent, label })
      return
    }
    if (!step.awaitDependentRefresh) {
      applyOption(parent, option, label)
      return
    }
    const dependent = requiredSelect(step.dependentSelector, step.dependentSelector)
    const refresh = observeRefresh(dependent, {
      initialFingerprint: optionFingerprint(dependent),
      requireFingerprintChange: step.requireFingerprintChange,
      target: step.target,
    })
    try {
      applyOption(parent, option, label)
      await refresh.done
    } finally {
      refresh.cancel()
    }
  }

  try {
    if (useDarkMode) {
      const existing = document.getElementById("opencode-dark-mode")
      const style = existing ?? document.createElement("style")
      style.id = "opencode-dark-mode"
      style.textContent = darkModeCss
      if (existing === null) document.head.append(style)
    }

    const numberMatch = normalizedAddress.match(/(?:^|\s)(산\s*)?(\d+)(?:-(\d+))?$/)
    const mainNumber = numberMatch?.[2]
    if (mainNumber === undefined) throw new KrasEvaluationError("본번을 확인하지 못했습니다.")
    const subNumber = numberMatch?.[3] ?? ""
    await selectParent({
      awaitDependentRefresh: true,
      dependentSelector: "#sggCd",
      parentField: "시도",
      parentSelector: "#ctpvCd",
      requireFingerprintChange: true,
      target: matchingAddressOption,
    })
    await selectParent({
      awaitDependentRefresh: true,
      dependentSelector: "#emdCd",
      parentField: "시군구",
      parentSelector: "#sggCd",
      requireFingerprintChange: true,
      target: matchingAddressOption,
    })
    await selectParent({
      awaitDependentRefresh: hasLegalRi,
      dependentSelector: "#riCd",
      parentField: "읍면동",
      parentSelector: "#emdCd",
      requireFingerprintChange: hasLegalRi,
      target: hasLegalRi ? matchingAddressOption : placeholderOption,
    })
    if (hasLegalRi) {
      selectAddress("#riCd", "리")
    } else {
      selectPlaceholder("#riCd")
    }

    const landType = requiredSelect("#ldgrSeCd", "지목")
    exactOption(landType, ledgerLabel)

    const mainInput = document.querySelector<HTMLInputElement>("#mno")
    const subInput = document.querySelector<HTMLInputElement>("#sno")
    if (mainInput === null || subInput === null) {
      throw new KrasEvaluationError("번지 필드를 찾지 못했습니다.")
    }
    for (const [input, value] of [
      [mainInput, mainNumber],
      [subInput, subNumber],
    ] as const) {
      input.value = value
      input.dispatchEvent(new Event("input", { bubbles: true }))
      input.dispatchEvent(new Event("change", { bubbles: true }))
    }
    for (const field of selectedFields) {
      if (normalize(field.select.selectedOptions[0]?.textContent ?? null) !== field.label) {
        throw new KrasEvaluationError(`${field.label} 선택값 검증에 실패했습니다.`)
      }
    }
    if (mainInput.value !== mainNumber || subInput.value !== subNumber) {
      throw new KrasEvaluationError("번지 입력값 검증에 실패했습니다.")
    }

    switch (request.mode) {
      case "fill_only":
        return { kind: "fill_only" }
      case "lookup":
        break
      default:
        return assertNeverMode(request)
    }

    const buttons = Array.from(
      document.querySelectorAll<HTMLElement>("button,input[type='button'],input[type='submit']"),
    ).filter((element) => {
      const label =
        element instanceof HTMLInputElement ? element.value.trim() : normalize(element.textContent)
      const enabled =
        element instanceof HTMLButtonElement || element instanceof HTMLInputElement
          ? !element.disabled
          : true
      return enabled && label === "조회"
    })
    if (buttons.length !== 1) {
      throw new KrasEvaluationError("조회 버튼은 정확히 하나여야 합니다.")
    }
    const button = buttons[0]
    if (button === undefined || typeof certView === "undefined") {
      throw new KrasEvaluationError("조회 실행 함수를 찾지 못했습니다.")
    }

    let complete = (): void => {
      throw new KrasEvaluationError("조회 완료 신호가 준비되지 않았습니다.")
    }
    const completion = new Promise<void>((resolve) => {
      complete = resolve
    })
    const originalCheckLedger = certView.fnCheckLedger
    const originalEmptyRequest = certView.fnEmptyRequestInfo
    certView.fnCheckLedger = function wrappedCheckLedger(...args): unknown {
      try {
        return originalCheckLedger.apply(this, args)
      } finally {
        complete()
      }
    }
    certView.fnEmptyRequestInfo = function wrappedEmptyRequest(...args): unknown {
      try {
        return originalEmptyRequest.apply(this, args)
      } finally {
        complete()
      }
    }
    let timeoutId: number | undefined
    const timeout = new Promise<never>((_resolve, reject) => {
      timeoutId = window.setTimeout(
        () => reject(new KrasEvaluationError("조회 완료 시간 초과")),
        300_000,
      )
    })
    try {
      button.click()
      await Promise.race([completion, timeout])
    } finally {
      certView.fnCheckLedger = originalCheckLedger
      certView.fnEmptyRequestInfo = originalEmptyRequest
      if (timeoutId !== undefined) window.clearTimeout(timeoutId)
    }

    const readBuildingOptions = (): KrasBuildingOption[] => {
      const building = requiredSelect("#bldgSeCd", "건물 구분")
      return Array.from(building.options)
        .map((option, index) => ({
          index,
          value: option.value,
          label: normalize(option.textContent),
        }))
        .filter(
          ({ label, value }) =>
            value !== "" && label !== "" && label !== "선택" && label !== "선택사항없음",
        )
    }
    let previousFingerprint = ""
    let stableSince = Date.now()
    const deadline = Date.now() + 30_000
    const ledgerState = certView as unknown as {
      readonly ledger?: { readonly child?: readonly unknown[] }
    }
    const expectedBuildingCount = Array.isArray(ledgerState.ledger?.child)
      ? ledgerState.ledger.child.length
      : undefined
    while (Date.now() < deadline) {
      const buildingOptions = readBuildingOptions()
      const fingerprint = JSON.stringify(buildingOptions)
      if (fingerprint !== previousFingerprint) {
        previousFingerprint = fingerprint
        stableSince = Date.now()
      }
      const emptyPlaceholder =
        document.querySelector<HTMLSelectElement>("#bldgSeCd")?.options.length === 1 &&
        normalize(
          document.querySelector<HTMLSelectElement>("#bldgSeCd")?.options[0]?.textContent ?? null,
        ) === "선택사항없음"
      const countComplete =
        expectedBuildingCount === undefined || buildingOptions.length >= expectedBuildingCount
      if (
        Date.now() - stableSince >= 800 &&
        countComplete &&
        (buildingOptions.length > 0 || emptyPlaceholder)
      ) {
        const hasBuilding = buildingOptions.length > 0
        return { kind: "lookup", hasBuilding, buildingOptions }
      }
      await new Promise((resolve) => window.setTimeout(resolve, 100))
    }
    const buildingOptions = readBuildingOptions()
    if (expectedBuildingCount !== undefined && buildingOptions.length < expectedBuildingCount) {
      throw new KrasEvaluationError("건물구분 목록이 끝까지 로딩되지 않았습니다.")
    }
    const hasBuilding = buildingOptions.length > 0
    return { kind: "lookup", hasBuilding, buildingOptions }
  } catch (error) {
    if (error instanceof KrasEvaluationError) {
      return { kind: "evaluation_error", code: "KRAS_EVALUATION_ERROR" }
    }
    throw error
  }
}

export async function fillKrasAddress(page: KrasFillPage, address: ParcelAddress): Promise<void> {
  const normalizedAddress = normalizeAddress(address)
  if (!isKrasTargetUrl(page.url())) throw new KrasFillTargetError()
  const result = await page.evaluate(lookupEvaluation, {
    mode: "fill_only",
    address: normalizedAddress,
    darkMode: false,
    darkModeCss: KRAS_DARK_MODE_CSS,
    hasLegalRi: addressHasLegalRi(normalizedAddress),
    ledgerLabel: ledgerLabelForAddress(normalizedAddress),
  })
  switch (result.kind) {
    case "fill_only":
      return
    case "evaluation_error":
      throw new KrasFillError()
    case "lookup":
      throw new KrasEvaluationResultError("KRAS 주소 입력 평가 결과가 아닙니다.")
    default:
      return assertNeverResult(result)
  }
}

export async function lookupKrasBuilding(
  page: KrasFillPage,
  address: ParcelAddress,
): Promise<PropertyBuildingTerminalStatus> {
  const normalizedAddress = normalizeAddress(address)
  if (!isKrasTargetUrl(page.url())) throw new KrasLookupTargetError()
  const result = await page.evaluate(lookupEvaluation, {
    mode: "lookup",
    address: normalizedAddress,
    darkMode: false,
    darkModeCss: KRAS_DARK_MODE_CSS,
    hasLegalRi: addressHasLegalRi(normalizedAddress),
    ledgerLabel: ledgerLabelForAddress(normalizedAddress),
  })
  switch (result.kind) {
    case "lookup":
      return result.hasBuilding ? "present" : "absent"
    case "evaluation_error":
      throw new KrasLookupError()
    case "fill_only":
      throw new KrasEvaluationResultError("KRAS 건물 조회 평가 결과가 아닙니다.")
    default:
      return assertNeverResult(result)
  }
}
