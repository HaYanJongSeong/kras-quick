import { type Browser, chromium } from "playwright-core"

import { darkModeEvaluation, KRAS_DARK_MODE_CSS } from "./kras-dark.ts"
import {
  addressHasLegalRi,
  isKrasTargetUrl,
  ledgerLabelForAddress,
  normalizeAddress,
} from "./kras-domain.ts"
import { type KrasPage, lookupEvaluation } from "./kras-page.ts"

export { isKrasTargetUrl } from "./kras-domain.ts"

const CDP_ENDPOINT = "http://127.0.0.1:9222"
const CDP_OPTIONS = { noDefaults: true, timeout: 5000 } as const
const BUILDING_PRESENT = "부동산 종합증명서 조회결과 건물 등장"
const BUILDING_UNAVAILABLE = "부동산 종합증명서 조회결과 건물 조회 불가"

type CdpOptions = {
  readonly noDefaults: boolean
  readonly timeout: number
}

type KrasContext = {
  readonly pages: () => readonly KrasPage[]
}

export type KrasBrowser = {
  readonly contexts: () => readonly KrasContext[]
  readonly close: () => Promise<void>
}

export type CdpConnect = (endpoint: string, options: CdpOptions) => Promise<KrasBrowser>

class KrasPageError extends Error {
  override readonly name = "KrasPageError"
}

function assertNeverResult(result: never): never {
  throw new KrasPageError(`처리할 수 없는 KRAS 평가 결과: ${String(result)}`)
}

function adaptBrowser(browser: Browser): KrasBrowser {
  return {
    contexts: () =>
      browser.contexts().map((context) => ({
        pages: () =>
          context.pages().map((page) => ({
            url: () => page.url(),
            evaluate: (pageFunction, request) => page.evaluate(pageFunction, request),
            applyDarkMode: (pageFunction, css) => page.evaluate(pageFunction, css),
          })),
      })),
    close: () => browser.close(),
  }
}

const connectOverCdp: CdpConnect = async (endpoint, options) => {
  const browser = await chromium.connectOverCDP(endpoint, options)
  return adaptBrowser(browser)
}

function targetPage(browser: KrasBrowser): KrasPage {
  const matches = browser
    .contexts()
    .flatMap((context) => context.pages())
    .filter((page) => isKrasTargetUrl(page.url()))
  if (matches.length !== 1) {
    throw new KrasPageError("KRAS 조회 페이지는 정확히 하나여야 합니다.")
  }
  const page = matches[0]
  if (page === undefined) {
    throw new KrasPageError("KRAS 조회 페이지를 찾지 못했습니다.")
  }
  return page
}

export async function runKrasLookup(
  connect: CdpConnect,
  address: string,
  darkMode: boolean,
): Promise<string> {
  const normalizedAddress = normalizeAddress(address)
  const browser = await connect(CDP_ENDPOINT, CDP_OPTIONS)
  try {
    const result = await targetPage(browser).evaluate(lookupEvaluation, {
      mode: "lookup",
      address: normalizedAddress,
      darkMode,
      darkModeCss: KRAS_DARK_MODE_CSS,
      hasLegalRi: addressHasLegalRi(normalizedAddress),
      ledgerLabel: ledgerLabelForAddress(normalizedAddress),
    })
    switch (result.kind) {
      case "lookup":
        return result.hasBuilding ? BUILDING_PRESENT : BUILDING_UNAVAILABLE
      case "evaluation_error":
        throw new KrasPageError("KRAS 페이지 평가에 실패했습니다.")
      case "fill_only":
        throw new KrasPageError("KRAS 조회 평가 결과가 아닙니다.")
      default:
        return assertNeverResult(result)
    }
  } finally {
    await browser.close()
  }
}

export async function runKrasDarkMode(connect: CdpConnect): Promise<string> {
  const browser = await connect(CDP_ENDPOINT, CDP_OPTIONS)
  try {
    await targetPage(browser).applyDarkMode(darkModeEvaluation, KRAS_DARK_MODE_CSS)
    return "다크 모드 적용."
  } finally {
    await browser.close()
  }
}

export function lookupKras(address: string, darkMode = false): Promise<string> {
  return runKrasLookup(connectOverCdp, address, darkMode)
}

export function applyKrasDarkMode(): Promise<string> {
  return runKrasDarkMode(connectOverCdp)
}
