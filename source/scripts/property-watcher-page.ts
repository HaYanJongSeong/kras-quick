import type { PropertyWatcherChannel } from "./property-watcher-channel.ts"
import type {
  PropertyPager,
  PropertyRow,
  PropertyWatcherSelectionHandle,
} from "./property-watcher-payload.ts"

const SCP_ORIGIN = "http://scpweb.softgraphy.biz"
const SCP_PATHNAME = "/board"
const KRAS_ORIGIN = "https://www.kras.go.kr"
const KRAS_PATHNAME = "/kras/cert/certView.do"

export type { PropertyRow } from "./property-watcher-payload.ts"

export type PropertyWatcherPage = {
  readonly url: () => string
  readonly installPropertyWatcherChannel: (
    onRow: (
      row: PropertyRow,
      selection: PropertyWatcherSelectionHandle,
      pager: PropertyPager,
    ) => void | Promise<void>,
  ) => Promise<PropertyWatcherChannel>
}

type PropertyWatcherContext<Page extends PropertyWatcherPage> = {
  readonly pages: () => readonly Page[]
}

export type PropertyWatcherBrowser<Page extends PropertyWatcherPage = PropertyWatcherPage> = {
  readonly contexts: () => readonly PropertyWatcherContext<Page>[]
}

export type PropertyWatcherTargets<Page extends PropertyWatcherPage = PropertyWatcherPage> = {
  readonly kras: Page
  readonly scp: Page
}

export type PropertyWatcherErrorCode = "TARGET_COUNT_INVALID"

const ERROR_MESSAGES = {
  TARGET_COUNT_INVALID: "SCP와 KRAS 대상 페이지는 각각 정확히 하나여야 합니다.",
} as const satisfies Record<PropertyWatcherErrorCode, string>

export class PropertyWatcherError extends Error {
  override readonly name = "PropertyWatcherError"
  readonly code: PropertyWatcherErrorCode

  constructor(code: PropertyWatcherErrorCode, message: string = ERROR_MESSAGES[code]) {
    super(message)
    this.code = code
  }
}

function matchesTarget(value: string, origin: string, pathname: string): boolean {
  if (!URL.canParse(value)) return false
  const url = new URL(value)
  return url.origin === origin && url.pathname === pathname
}

export function isScpTargetUrl(value: string): boolean {
  return matchesTarget(value, SCP_ORIGIN, SCP_PATHNAME)
}

export function isKrasWatcherTargetUrl(value: string): boolean {
  return matchesTarget(value, KRAS_ORIGIN, KRAS_PATHNAME)
}

export function discoverPropertyWatcherTargets<Page extends PropertyWatcherPage>(
  browser: PropertyWatcherBrowser<Page>,
): PropertyWatcherTargets<Page> {
  const pages = browser.contexts().flatMap((context) => context.pages())
  const scpMatches = pages.filter((page) => isScpTargetUrl(page.url()))
  const krasMatches = pages.filter((page) => isKrasWatcherTargetUrl(page.url()))
  if (scpMatches.length !== 1 || krasMatches.length !== 1) {
    throw new PropertyWatcherError("TARGET_COUNT_INVALID")
  }
  const scp = scpMatches[0]
  const kras = krasMatches[0]
  if (scp === undefined || kras === undefined) {
    throw new PropertyWatcherError("TARGET_COUNT_INVALID")
  }
  return { kras, scp }
}

export function discoverKrasTarget<Page extends PropertyWatcherPage>(
  browser: PropertyWatcherBrowser<Page>,
): Page {
  const krasMatches = browser
    .contexts()
    .flatMap((context) => context.pages())
    .filter((page) => isKrasWatcherTargetUrl(page.url()))
  if (krasMatches.length === 0) {
    throw new PropertyWatcherError("TARGET_COUNT_INVALID", "KRAS 대상 페이지를 찾지 못했습니다.")
  }
  const kras = krasMatches[0]
  if (kras === undefined) throw new PropertyWatcherError("TARGET_COUNT_INVALID")
  return kras
}

export type PropertyWatcherHandle<Page extends PropertyWatcherPage = PropertyWatcherPage> =
  PropertyWatcherTargets<Page> & PropertyWatcherChannel

export async function installPropertyWatcherPage<Page extends PropertyWatcherPage>(
  browser: PropertyWatcherBrowser<Page>,
  onRow: (
    row: PropertyRow,
    selection: PropertyWatcherSelectionHandle,
    pager: PropertyPager,
  ) => Promise<void>,
): Promise<PropertyWatcherHandle<Page>> {
  const targets = discoverPropertyWatcherTargets(browser)
  const channel = await targets.scp.installPropertyWatcherChannel(onRow)
  return { ...targets, ...channel }
}
