import { type Browser, chromium, type Page } from "playwright-core"

import {
  type KrasFillPage,
  KrasLookupError,
  KrasLookupTargetError,
  lookupKrasBuilding,
} from "./kras-page.ts"
import {
  commitProgressUpdate,
  PROPERTY_PROGRESS_FILE,
  ProgressFileError,
  prepareProgressUpdate,
} from "./property-progress.ts"
import {
  type PropertyWatcher,
  type PropertyWatcherLogger,
  type PropertyWatcherOperation,
  startPropertyWatcher,
} from "./property-watcher.ts"
import { createPropertyWatcherProtocol } from "./property-watcher-cdp.ts"
import { installPropertyWatcherChannel } from "./property-watcher-channel.ts"
import {
  discoverPropertyWatcherTargets,
  installPropertyWatcherPage,
  type PropertyWatcherBrowser,
  type PropertyWatcherPage,
} from "./property-watcher-page.ts"

const CDP_ENDPOINT = "http://127.0.0.1:9222"
const CDP_OPTIONS = { noDefaults: true, timeout: 5000 } as const
const DRY_RUN_RESULT = '{"scpPages":1,"krasPages":1,"status":"ready"}'

class PropertyWatcherRuntimeInvariantError extends Error {
  override readonly name = "PropertyWatcherRuntimeInvariantError"
}

function assertNeverOperation(operation: never): never {
  throw new PropertyWatcherRuntimeInvariantError(
    `Unexpected watcher operation: ${String(operation)}`,
  )
}

function isRecoverableRuntimeError(operation: PropertyWatcherOperation, error: unknown): boolean {
  switch (operation) {
    case "prepare":
    case "commit":
      return error instanceof ProgressFileError
    case "lookup":
      return error instanceof KrasLookupError || error instanceof KrasLookupTargetError
    default:
      return assertNeverOperation(operation)
  }
}

export type CdpPageSurface = {
  readonly url: () => string
  readonly installPropertyWatcherChannel: PropertyWatcherPage["installPropertyWatcherChannel"]
  readonly evaluateKras: KrasFillPage["evaluate"]
}

type CdpContextSurface = {
  readonly pages: () => readonly CdpPageSurface[]
}

export type CdpBrowserSurface = {
  readonly contexts: () => readonly CdpContextSurface[]
}

export type PropertyWatcherRuntimePage = PropertyWatcherPage & KrasFillPage
export type PropertyWatcherRuntimeBrowser = PropertyWatcherBrowser<PropertyWatcherRuntimePage>
export type PropertyWatcherConnect = () => Promise<PropertyWatcherRuntimeBrowser>

function adaptPage(page: CdpPageSurface): PropertyWatcherRuntimePage {
  const adapted: PropertyWatcherRuntimePage = {
    url: () => page.url(),
    installPropertyWatcherChannel: (onRow) => page.installPropertyWatcherChannel(onRow),
    evaluate: (pageFunction, request) => page.evaluateKras(pageFunction, request),
  }
  return adapted
}

export function adaptPropertyWatcherBrowser(
  browser: CdpBrowserSurface,
): PropertyWatcherRuntimeBrowser {
  return {
    contexts: () =>
      browser.contexts().map((context) => ({
        pages: () => context.pages().map(adaptPage),
      })),
  }
}

function adaptPlaywrightBrowser(browser: Browser): PropertyWatcherRuntimeBrowser {
  const adaptPlaywrightPage = (page: Page): CdpPageSurface => ({
    url: () => page.url(),
    installPropertyWatcherChannel: async (onRow) => {
      const session = await page.context().newCDPSession(page)
      return installPropertyWatcherChannel(createPropertyWatcherProtocol(session), onRow)
    },
    evaluateKras: (pageFunction, request) => page.evaluate(pageFunction, request),
  })
  return adaptPropertyWatcherBrowser({
    contexts: () =>
      browser
        .contexts()
        .map((context) => ({ pages: () => context.pages().map(adaptPlaywrightPage) })),
  })
}

export async function connectPropertyWatcherBrowser(): Promise<PropertyWatcherRuntimeBrowser> {
  const browser = await chromium.connectOverCDP(CDP_ENDPOINT, CDP_OPTIONS)
  return adaptPlaywrightBrowser(browser)
}

export type DryRunDependencies = {
  readonly connect: PropertyWatcherConnect
  readonly writeLine: (line: string) => void
}

export async function runPropertyWatcherDryRun(dependencies: DryRunDependencies): Promise<void> {
  const browser = await dependencies.connect()
  discoverPropertyWatcherTargets(browser)
  dependencies.writeLine(DRY_RUN_RESULT)
}

export type PropertyWatcherRuntimeDependencies = {
  readonly connect: PropertyWatcherConnect
  readonly lookupKras: (
    page: KrasFillPage,
    address: Parameters<typeof lookupKrasBuilding>[1],
  ) => ReturnType<typeof lookupKrasBuilding>
  readonly logger: PropertyWatcherLogger
  readonly progressPath: string
}

export async function startPropertyWatcherRuntime(
  dependencies: PropertyWatcherRuntimeDependencies,
): Promise<PropertyWatcher> {
  const browser = await dependencies.connect()
  return startPropertyWatcher({
    installTargetListener: async (onRow) => {
      const handle = await installPropertyWatcherPage(browser, async (row, selection, pager) =>
        onRow(row, selection, pager),
      )
      return {
        target: handle.kras,
        dispose: handle.dispose,
        failed: handle.failed,
        updateBuildingStatus: handle.updateBuildingStatus,
      }
    },
    prepareProgress: (address, pager) =>
      prepareProgressUpdate(dependencies.progressPath, address, pager),
    lookupKras: dependencies.lookupKras,
    commitProgress: commitProgressUpdate,
    isRecoverableError: isRecoverableRuntimeError,
    logger: dependencies.logger,
  })
}

export function startLivePropertyWatcher(
  _instanceToken: string,
  logger: PropertyWatcherLogger,
): Promise<PropertyWatcher> {
  return startPropertyWatcherRuntime({
    connect: connectPropertyWatcherBrowser,
    lookupKras: lookupKrasBuilding,
    logger,
    progressPath: PROPERTY_PROGRESS_FILE,
  })
}
