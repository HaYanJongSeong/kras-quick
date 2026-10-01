import { chromium, type Page } from "playwright-core"

import {
  addressHasLegalRi,
  isKrasTargetUrl,
  ledgerLabelForAddress,
  normalizeAddress,
  type KrasEvaluationResult,
} from "./kras-domain.ts"
import {
  createKrasAutoStage2Result,
  createKrasManualStage2Result,
  isViewerUrl,
  KRAS_AUTO_STAGE_VERSION,
  propertyAutoStagePaths,
  type KrasAutoStage1Result,
  type KrasAutoStage2Result,
  type KrasAutoStageResult,
  writeKrasAutoStageResult,
} from "./kras-auto-stages.ts"
import { KRAS_DARK_MODE_CSS } from "./kras-dark.ts"
import { lookupEvaluation, type KrasPage } from "./kras-page.ts"
import { parseParcelAddress, type ParcelAddress } from "./property-address.ts"
import { isKrasWatcherTargetUrl } from "./property-watcher-page.ts"
import type { KrasBuildingOption } from "./kras-domain.ts"

const CDP_ENDPOINT = process.env["KRAS_CDP_ENDPOINT"] ?? "http://127.0.0.1:9222"

export type KrasWorkflowPage = {
  readonly url: () => string
  readonly lookup: (address: ParcelAddress) => Promise<KrasEvaluationResult>
  readonly readBuildingOptions: () => Promise<KrasBuildingOption[]>
  readonly selectBuilding: (value: string) => Promise<readonly KrasBuildingOption[]>
  readonly selectFloorRoom: (option: KrasBuildingOption) => Promise<void | KrasBuildingOption>
  readonly showCaptchaPrompt: () => Promise<void>
  readonly awaitOzViewer: () => Promise<boolean>
  readonly captureCaptchaImage: () => Promise<string | null>
  readonly renderCaptchaTerminal: () => Promise<string | null>
  readonly fillCaptchaAndSubmit: (
    captchaText: string,
  ) => Promise<"success" | "need_lookup" | "login_expired" | "error">
  readonly closeOzViewer: () => Promise<boolean>
  readonly disconnect?: () => Promise<void>
}

export class KrasFloorRoomLoadTimeoutError extends Error {
  constructor() {
    super("층-호명칭 목록 로딩 시간이 초과되었습니다.")
    this.name = "KrasFloorRoomLoadTimeoutError"
  }
}

export type KrasDialogKind = "confirm" | "need_lookup" | "login_expired" | "error"

export function classifyKrasDialogText(text: string): KrasDialogKind | null {
  if (text.includes("도면정보") || text.includes("도호가 불일치") || text.includes("출력축척"))
    return "confirm"
  if (text.includes("신청물건을 먼저 조회")) return "need_lookup"
  if (text.includes("로그인이 만료되었습니다")) return "login_expired"
  if (/보안문자|일치하지 않|오류|실패/.test(text)) return "error"
  return null
}

export function isKrasLoginUrl(url: string): boolean {
  if (!URL.canParse(url)) return false
  return /\/login(?:\/|$)/i.test(new URL(url).pathname)
}

export type KrasWorkflowResult = {
  readonly stage1: KrasAutoStage1Result
  readonly stage2?: KrasAutoStage2Result
  readonly awaitingUser: boolean
  readonly returnToAddress?: boolean
}

export type KrasWorkflowHooks = {
  readonly writeResult?: (
    path: string,
    result: KrasAutoStageResult,
  ) => Promise<void>
  readonly onStage1?: (result: KrasAutoStage1Result) => void
  readonly chooseBuilding?: (options: readonly KrasBuildingOption[]) => Promise<number | "address" | undefined>
  readonly chooseFloorRoom?: (
    options: readonly KrasBuildingOption[],
    building: KrasBuildingOption,
  ) => Promise<number | "none" | undefined>
  // chooseFloorRoom이 "none"을 반환하면 층-호를 선택하지 않고 진행한다 —
  // 집합건물 총괄 열람에 사용한다. undefined(미반환)는 기존 의미(blocked)를 유지한다.
  readonly handleFloorRoomOptions?: (
    building: KrasBuildingOption,
    status: "empty" | "timeout",
  ) => Promise<"retry" | "continue">
  readonly handleLookupError?: () => Promise<"retry" | "manual">
  // true면 조회(lookup)를 건너뛰고 이미 로드된 #bldgSeCd 옵션을 재사용한다.
  // 주소 재입력 없이 현재 페이지의 건물 목록으로 건물 선택부터 다시 시작할 때 쓴다.
  readonly reuseLoadedOptions?: boolean
  readonly outputRoot?: string
}

function selectFloorRoomEvaluation(value: string): void {
  const select = document.querySelector<HTMLSelectElement>("#flrSeCd")
  if (select === null) throw new Error("층-호명칭 필드를 찾지 못했습니다.")
  const option = Array.from(select.options).find((candidate) => candidate.value === value)
  if (option === undefined) throw new Error("선택한 층-호명칭을 찾지 못했습니다.")
  select.value = value
  select.dispatchEvent(new MouseEvent("click", { bubbles: true }))
  select.dispatchEvent(new Event("input", { bubbles: true }))
  select.dispatchEvent(new Event("change", { bubbles: true }))
  if (select.value !== value) throw new Error("층-호명칭 선택값 검증에 실패했습니다.")
}

export function shouldWaitForFloorRoomOptions(buildingLabel: string, options: readonly KrasBuildingOption[]): boolean {
  return buildingLabel.includes("(집합)") || options.length > 0
}

async function selectBuildingAndReadFloorRoomOptionsEvaluation(value: string): Promise<KrasBuildingOption[]> {
  const building = document.querySelector<HTMLSelectElement>("#bldgSeCd")
  if (building === null) throw new Error("건물구분 필드를 찾지 못했습니다.")
  const buildingOption = Array.from(building.options).find((candidate) => candidate.value === value)
  if (buildingOption === undefined) throw new Error("Stage 1 건물 옵션을 찾지 못했습니다.")
  const readOptions = (): KrasBuildingOption[] => {
    const floorRoom = document.querySelector<HTMLSelectElement>("#flrSeCd")
    if (floorRoom === null) return []
    const normalize = (text: string | null): string => (text ?? "").replace(/\s+/g, " ").trim()
    return Array.from(floorRoom.options)
      .map((option, index) => ({ index, value: option.value, label: normalize(option.textContent) }))
      .filter(({ label, value }) => value !== "" && label !== "" && label !== "선택" && label !== "선택사항없음")
  }
  const initialFingerprint = JSON.stringify(readOptions())
  building.value = value
  building.dispatchEvent(new MouseEvent("click", { bubbles: true }))
  building.dispatchEvent(new Event("input", { bubbles: true }))
  building.dispatchEvent(new Event("change", { bubbles: true }))
  const initialOptions = readOptions()
  if (!(buildingOption.textContent ?? "").includes("(집합)") && initialOptions.length === 0) return []
  let previousFingerprint = initialFingerprint
  let stableSince = Date.now()
  const deadline = Date.now() + 4_000
  while (Date.now() < deadline) {
    const options = readOptions()
    const fingerprint = JSON.stringify(options)
    if (fingerprint !== previousFingerprint) {
      previousFingerprint = fingerprint
      stableSince = Date.now()
    }
    if (options.length > 0 && Date.now() - stableSince >= 800) return options
    await new Promise((resolve) => window.setTimeout(resolve, 250))
  }
  // This function runs in the browser context, so host-side error classes are unavailable.
  throw new Error("층-호명칭 목록 로딩 시간이 초과되었습니다.")
}

function isFloorRoomLoadTimeoutError(error: unknown): boolean {
  return (
    error instanceof KrasFloorRoomLoadTimeoutError ||
    (error instanceof Error && error.message.includes("층-호명칭 목록 로딩 시간이 초과되었습니다."))
  )
}

export function renderCaptchaTerminalPixels(
  data: ArrayLike<number>,
  width: number,
  height: number,
): string {
  const lines: string[] = []
  for (let y = 0; y < height; y += 2) {
    let line = ""
    for (let x = 0; x < width; x += 1) {
      const top = (y * width + x) * 4
      const hasBottom = y + 1 < height
      const bottom = hasBottom ? ((y + 1) * width + x) * 4 : top
      const r1 = data[top] ?? 0
      const g1 = data[top + 1] ?? 0
      const b1 = data[top + 2] ?? 0
      const a1 = data[top + 3] ?? 0
      const r2 = hasBottom ? (data[bottom] ?? 0) : 0
      const g2 = hasBottom ? (data[bottom + 1] ?? 0) : 0
      const b2 = hasBottom ? (data[bottom + 2] ?? 0) : 0
      const a2 = hasBottom ? (data[bottom + 3] ?? 0) : 0
      if (a1 === 0 && a2 === 0) {
        line += " "
        continue
      }
      const background = a2 === 0 ? "\x1b[40m" : `\x1b[48;2;${r2};${g2};${b2}m`
      const foreground = a1 === 0 ? "\x1b[37m" : `\x1b[38;2;${r1};${g1};${b1}m`
      line += `${foreground}${background}▀\x1b[0m`
    }
    lines.push(line)
  }
  return lines.join("\n")
}

// 조회 버튼을 누르지 않고, 현재 페이지의 건물 구분(#bldgSeCd)에 이미 로드된 옵션만 다시 읽는다.
// 조회 실패 후 "지금 다시 확인" 선택 시 사용한다.
function readBuildingOptionsEvaluation(): KrasBuildingOption[] {
  const building = document.querySelector<HTMLSelectElement>("#bldgSeCd")
  if (building === null) return []
  const normalize = (value: string | null): string => (value ?? "").replace(/\s+/g, " ").trim()
  return Array.from(building.options)
    .map((option, index) => ({
      index,
      value: option.value,
      label: normalize(option.textContent),
    }))
    .filter(({ label, value }) => {
      return value !== "" && label !== "" && label !== "선택" && label !== "선택사항없음"
    })
}

function createPlaywrightPage(page: Page, disconnect: () => Promise<void>): KrasWorkflowPage {
  const evaluateKras: KrasPage["evaluate"] = (pageFunction, request) =>
    page.evaluate(pageFunction, request)
  return {
    url: () => page.url(),
    lookup: async (address) =>
      evaluateKras(lookupEvaluation, {
        mode: "lookup",
        address: normalizeAddress(address),
        darkMode: false,
        darkModeCss: KRAS_DARK_MODE_CSS,
        hasLegalRi: addressHasLegalRi(address),
        ledgerLabel: ledgerLabelForAddress(address),
      }),
    readBuildingOptions: () => page.evaluate(readBuildingOptionsEvaluation),
    selectBuilding: (value) => page.evaluate(selectBuildingAndReadFloorRoomOptionsEvaluation, value),
    selectFloorRoom: (option) => page.evaluate(selectFloorRoomEvaluation, option.value),
    showCaptchaPrompt: async () => undefined,
    awaitOzViewer: async () => {
      const deadline = Date.now() + 60_000
      while (Date.now() < deadline) {
        const contexts = page.context().browser()?.contexts() ?? []
        for (const ctx of contexts) {
          for (const p of ctx.pages()) {
            if (isViewerUrl(p.url())) return true
          }
        }
        await new Promise(r => setTimeout(r, 1000))
      }
      return false
    },
    captureCaptchaImage: async () => {
      try {
        return await page.evaluate(async () => {
          const img = document.querySelector<HTMLImageElement>("img[id*='captcha' i], img[src*='captcha' i], img[alt*='보안' i]")
          if (!img) return null
          if (!img.complete || img.naturalWidth === 0) {
            await new Promise(resolve => { img.onload = img.onerror = resolve })
          }
          const canvas = document.createElement("canvas")
          canvas.width = img.naturalWidth * 2
          canvas.height = img.naturalHeight * 2
          const ctx = canvas.getContext("2d")
          if (!ctx) return null
          ctx.imageSmoothingEnabled = false
          ctx.drawImage(img, 0, 0, canvas.width, canvas.height)
          return canvas.toDataURL("image/png").split(",")[1] || null
        })
      } catch {
        return null
      }
    },
    renderCaptchaTerminal: async () => {
      try {
        const rendered = await page.evaluate(async () => {
          const img = document.querySelector<HTMLImageElement>("img[id*='captcha' i], img[src*='captcha' i], img[alt*='보안' i]")
          if (!img) return null
          if (!img.complete || img.naturalWidth === 0) {
            await new Promise(resolve => { img.onload = img.onerror = resolve })
          }
           const maxWidth = 80
          const scale = Math.min(1, maxWidth / img.naturalWidth)
          const width = Math.max(1, Math.round(img.naturalWidth * scale))
          const height = Math.max(1, Math.round(img.naturalHeight * scale))
          const canvas = document.createElement("canvas")
          canvas.width = width
          canvas.height = height
          const ctx = canvas.getContext("2d")
          if (!ctx) return null
          ctx.drawImage(img, 0, 0, width, height)
          const data = ctx.getImageData(0, 0, width, height).data
          return { data: Array.from(data), width, height }
        })
        return rendered === null ? null : renderCaptchaTerminalPixels(rendered.data, rendered.width, rendered.height)
      } catch {
        return null
      }
    },
    fillCaptchaAndSubmit: async (captchaText: string) => {
      try {
        if (isKrasLoginUrl(page.url())) return "login_expired"
        const captcha = page.locator("#captcha:visible")
        const viewButton = page.locator("button[onclick='certView.fnProcessCert();']:visible")
        const clickView = async (): Promise<boolean> => {
          if (await captcha.count() !== 1 || await viewButton.count() !== 1) return false
          await captcha.fill(captchaText)
          if (await captcha.inputValue() !== captchaText) return false
          await viewButton.click()
          return true
        }
        if (!(await clickView())) return "error"
        const deadline = Date.now() + 20_000
        while (Date.now() < deadline) {
          if (isKrasLoginUrl(page.url())) return "login_expired"
          const dialog = await page
            .evaluate(() => {
              const el = document.querySelector(".swal2-container.swal2-backdrop-show")
              if (el === null) return null
              return (el.textContent ?? "").replace(/\s+/g, " ").trim()
            })
            .catch(() => null)
          const dialogKind = dialog === null ? null : classifyKrasDialogText(dialog)
          if (dialogKind === null) continue
          if (dialogKind === "need_lookup") {
            // ledger 미선택 — 이 dialog의 확인(OK) 버튼도 swal2-confirm이라 도면정보 확인과
            // 구분해야 한다. 닫고 호출자에게 "조회 필요"를 알려 수동 조회를 유도한다.
            await page
              .evaluate(() =>
                document
                  .querySelector<HTMLElement>(".swal2-container.swal2-backdrop-show .swal2-confirm")
                  ?.click(),
              )
              .catch(() => undefined)
            await page.waitForTimeout(300)
            return "need_lookup"
          }
          if (dialogKind === "login_expired") {
            await page
              .evaluate(() =>
                document
                  .querySelector<HTMLElement>('.swal2-container.swal2-backdrop-show .swal2-confirm')
                  ?.click(),
              )
              .catch(() => undefined)
            await page.waitForTimeout(300)
            return "login_expired"
          }
          if (dialogKind === "confirm") {
            const confirmBtn = await page
              .waitForSelector(".swal2-container.swal2-backdrop-show .swal2-confirm", { timeout: 1000 })
              .catch(() => null)
            if (confirmBtn !== null) {
              await confirmBtn.click()
              await page.waitForTimeout(1500)
              const stillOpen = await page
                .evaluate(() => document.querySelector(".swal2-container.swal2-backdrop-show") !== null)
                .catch(() => false)
              if (!stillOpen) return "success"
              continue
            }
          }
          if (dialogKind === "error") {
            await page
              .evaluate(() =>
                document
                  .querySelector<HTMLElement>(".swal2-container.swal2-backdrop-show .swal2-cancel, .swal2-container.swal2-backdrop-show .swal2-close")
                  ?.click(),
              )
              .catch(() => undefined)
            return "error"
          }
        }
        return "error"
      } catch {
        return isKrasLoginUrl(page.url()) ? "login_expired" : "error"
      }
    },
    closeOzViewer: async () => {
      const contexts = page.context().browser()?.contexts() ?? []
      for (const ctx of contexts) {
        for (const p of ctx.pages()) {
          if (isViewerUrl(p.url())) {
            await p.close().catch(() => undefined)
            return true
          }
        }
      }
      return false
    },
    disconnect,
  }
}

export async function runKrasWorkflowWatcher(
  rawAddress: string,
  page: KrasWorkflowPage,
  hooks: KrasWorkflowHooks = {},
): Promise<KrasWorkflowResult> {
  const address = parseParcelAddress(rawAddress)
  if (!isKrasTargetUrl(page.url())) throw new Error("KRAS 대상 페이지가 아닙니다.")
  let lookup: KrasEvaluationResult
  if (hooks.reuseLoadedOptions === true) {
    // 조회 버튼을 다시 누르지 않고 현재 페이지에 로드된 건물 구분 옵션을 그대로 쓴다.
    // 워크플로 완료 후 페이지 상태가 바뀌어 옵션이 비어 있으면 신선한 조회로 대체한다.
    const loadedOptions = await page.readBuildingOptions()
    lookup =
      loadedOptions.length > 0
        ? {
            kind: "lookup",
            hasBuilding: true,
            buildingOptions: loadedOptions,
          }
        : await page.lookup(address)
  } else {
    lookup = await page.lookup(address)
  }
  while (lookup.kind === "evaluation_error") {
    if (hooks.handleLookupError === undefined) throw new Error(`Stage 1 조회 실패: ${lookup.code}`)
    const action = await hooks.handleLookupError()
    if (action === "manual") {
      const stage1: KrasAutoStage1Result = {
        version: KRAS_AUTO_STAGE_VERSION,
        stage: 1,
        status: "blocked",
        address: normalizeAddress(address),
        reason: "no_building",
        hasBuilding: false,
        buildingOptions: [],
      }
      const paths = propertyAutoStagePaths(stage1.address, hooks.outputRoot)
      const stage2 = createKrasManualStage2Result(stage1)
      const writeResult = hooks.writeResult ?? writeKrasAutoStageResult
      await writeResult(paths.stage1Json, stage1)
      await writeResult(paths.stage2Json, stage2)
      hooks.onStage1?.(stage1)
      await page.showCaptchaPrompt()
      return { stage1, stage2, awaitingUser: false }
    }
    // "지금 다시 확인": 조회 버튼을 다시 누르지 않고, 이미 로드된 건물 구분 옵션을 재읽는다
    const reReadOptions = await page.readBuildingOptions()
    lookup = {
      kind: "lookup",
      hasBuilding: reReadOptions.length > 0,
      buildingOptions: reReadOptions,
    }
  }
  if (lookup.kind !== "lookup") throw new Error("Stage 1 조회 결과가 아닙니다.")
  const options = lookup.buildingOptions ?? []
  const stage1: KrasAutoStage1Result =
    lookup.noParcel === true
      ? {
          version: KRAS_AUTO_STAGE_VERSION,
          stage: 1,
          status: "blocked",
          address: normalizeAddress(address),
          reason: "no_parcel",
          hasBuilding: false,
          buildingOptions: [],
        }
      : !lookup.hasBuilding || options.length === 0
      ? {
          version: KRAS_AUTO_STAGE_VERSION,
          stage: 1,
          status: "blocked",
          address: normalizeAddress(address),
          reason: "no_building",
          hasBuilding: false,
          buildingOptions: [],
        }
      : {
          version: KRAS_AUTO_STAGE_VERSION,
          stage: 1,
          status: "completed",
          address: normalizeAddress(address),
          hasBuilding: true,
          buildingOptions: options,
        }
  const paths = propertyAutoStagePaths(stage1.address, hooks.outputRoot)
  await (hooks.writeResult ?? writeKrasAutoStageResult)(paths.stage1Json, stage1)
  hooks.onStage1?.(stage1)
  if (stage1.status !== "completed") return { stage1, awaitingUser: false }
  if (stage1.buildingOptions.length === 0) return { stage1, awaitingUser: false }
  if (hooks.chooseBuilding === undefined) return { stage1, awaitingUser: true }
  const selectedIndex = await hooks.chooseBuilding(stage1.buildingOptions)
  if (selectedIndex === "address") return { stage1, awaitingUser: false, returnToAddress: true }
  if (selectedIndex === undefined) {
    // 사용자가 건물 자동 선택을 거부(수동 열람)한 경우 — 빈 건물명 stage2를 기록하고 수동 열람으로 진행한다
    const stage2 = createKrasManualStage2Result(stage1)
    await (hooks.writeResult ?? writeKrasAutoStageResult)(paths.stage2Json, stage2)
    await page.showCaptchaPrompt()
    return { stage1, stage2, awaitingUser: false }
  }
  const building = stage1.buildingOptions.find(({ index }) => index === selectedIndex)
  if (building === undefined) throw new Error("선택한 건물 번호가 유효하지 않습니다.")
  let floorRoomOptions: readonly KrasBuildingOption[] = []
  while (true) {
    try {
      floorRoomOptions = await page.selectBuilding(building.value)
      if (floorRoomOptions.length === 0 && hooks.handleFloorRoomOptions !== undefined) {
        if ((await hooks.handleFloorRoomOptions(building, "empty")) === "retry") continue
      }
      break
    } catch (error) {
      if (!isFloorRoomLoadTimeoutError(error)) throw error
      if (hooks.handleFloorRoomOptions === undefined) break
      if ((await hooks.handleFloorRoomOptions(building, "timeout")) === "retry") continue
      break
    }
  }
  if (floorRoomOptions.length === 0 && building.label.includes("(집합)") && hooks.handleFloorRoomOptions === undefined) {
    throw new Error("집합건물 층-호명칭 목록을 불러오지 못했습니다. 세션 상태를 확인한 뒤 다시 시도하세요.")
  }
  let floorRoom: KrasBuildingOption | undefined
  if (floorRoomOptions.length === 1) {
    floorRoom = floorRoomOptions[0]
    if (floorRoom !== undefined) await page.selectFloorRoom(floorRoom)
  } else if (floorRoomOptions.length > 1) {
    if (hooks.chooseFloorRoom === undefined) {
      const blocked: KrasAutoStage2Result = {
        version: KRAS_AUTO_STAGE_VERSION,
        stage: 2,
        status: "blocked",
        address: stage1.address,
        reason: "floor_room_selection_required",
        building,
        floorRoomOptions,
      }
      await (hooks.writeResult ?? writeKrasAutoStageResult)(paths.stage2Json, blocked)
      return { stage1, stage2: blocked, awaitingUser: true }
    }
    const floorRoomIndex = await hooks.chooseFloorRoom(floorRoomOptions, building)
    if (floorRoomIndex === undefined) {
      const blocked: KrasAutoStage2Result = {
        version: KRAS_AUTO_STAGE_VERSION,
        stage: 2,
        status: "blocked",
        address: stage1.address,
        reason: "floor_room_selection_required",
        building,
        floorRoomOptions,
      }
      await (hooks.writeResult ?? writeKrasAutoStageResult)(paths.stage2Json, blocked)
      return { stage1, stage2: blocked, awaitingUser: true }
    }
    if (floorRoomIndex !== "none") {
      floorRoom = floorRoomOptions.find(({ index }) => index === floorRoomIndex)
      if (floorRoom === undefined) throw new Error("선택한 층-호명칭 번호가 유효하지 않습니다.")
      await page.selectFloorRoom(floorRoom)
    }
  }
  const stage2 = createKrasAutoStage2Result(stage1, building.index, floorRoom)
  await (hooks.writeResult ?? writeKrasAutoStageResult)(paths.stage2Json, stage2)
  await page.showCaptchaPrompt()
  return { stage1, stage2, awaitingUser: false }
}

async function connectKrasBrowser(): Promise<ReturnType<typeof chromium.connectOverCDP> extends Promise<infer T> ? T : never> {
  return await chromium.connectOverCDP(CDP_ENDPOINT, { noDefaults: true, timeout: 5000 })
}

export async function connectKrasWorkflowWatcher(): Promise<KrasWorkflowPage> {
  const browser = await connectKrasBrowser()
  try {
    const pages = browser.contexts().flatMap((context) => context.pages())
    const page = pages.find((candidate) => isKrasWatcherTargetUrl(candidate.url()))
    if (page === undefined) throw new Error("KRAS 대상 페이지를 찾지 못했습니다.")
    return createPlaywrightPage(page, () => browser.close())
  } catch (error: unknown) {
    await browser.close()
    throw error
  }
}

export async function connectOrOpenKrasWorkflowWatcher(): Promise<KrasWorkflowPage> {
  const browser = await connectKrasBrowser()
  try {
    const pages = browser.contexts().flatMap((context) => context.pages())
    const existing = pages.find((candidate) => isKrasWatcherTargetUrl(candidate.url()))
    if (existing !== undefined) return createPlaywrightPage(existing, () => browser.close())
    const loginPage = pages.find((candidate) => {
      if (!URL.canParse(candidate.url())) return false
      const url = new URL(candidate.url())
      return (
        url.origin === "https://www.kras.go.kr" &&
        url.pathname === "/login/loginView.do" &&
        url.searchParams.get("uri") === "/kras/cert/certView.do"
      )
    })
    if (loginPage !== undefined) return createPlaywrightPage(loginPage, () => browser.close())
    const context = browser.contexts()[0] ?? await browser.newContext()
    const page = context.pages()[0] ?? await context.newPage()
    await page.goto("https://www.kras.go.kr/kras/cert/certView.do")
    return createPlaywrightPage(page, () => browser.close())
  } catch (error: unknown) {
    await browser.close()
    throw error
  }
}
