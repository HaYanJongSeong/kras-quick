import { createHash } from "node:crypto"

import { PDFDocument } from "pdf-lib"
import { chromium, type Page } from "playwright-core"

import { isViewerUrl, type KrasOzViewerSurface } from "./kras-auto-stages.ts"

const CDP_ENDPOINT = process.env["KRAS_CDP_ENDPOINT"] ?? "http://127.0.0.1:9222"

function visibleCaptchaSelector(page: Page): Promise<boolean> {
  return page
    .locator(
      "input[id*='captcha' i], input[name*='captcha' i], textarea[id*='captcha' i], textarea[name*='captcha' i]",
    )
    .first()
    .isVisible()
}

// OZ Viewer HTML5 툴바 셀렉터는 라이브 report.jsp 실측값. 버튼은 title 속성으로 선택한다(클래스 btnNEXT/btnNEXT2가 뷰어 상태에 따라 바뀜)
async function setOZZoomTo300(page: Page): Promise<void> {
  await page.evaluate(() => {
    const select = document.querySelector<HTMLSelectElement>("select.oz_ui_layout")
    if (select === null) return
    select.value = "300%"
    select.dispatchEvent(new Event("change", { bubbles: true }))
  })
  await page.waitForFunction(
    () => {
      const canvas = document.querySelector<HTMLCanvasElement>("canvas")
      return canvas !== null && canvas.width > 3000
    },
    { timeout: 15_000 },
  )
}

async function readOZTotalPages(page: Page): Promise<number> {
  const total = Number(
    await page.evaluate(() => {
      const match = (document.body.innerText ?? "").match(/(\d+)\s*\/\s*(\d+)/)
      return match?.[2] ?? "0"
    }),
  )
  if (!Number.isSafeInteger(total) || total <= 0) throw new Error("OZ 뷰어 총 페이지 수를 읽지 못했습니다.")
  return total
}

async function captureStableCanvas(page: Page): Promise<Uint8Array | undefined> {
  const capture = (): Promise<Uint8Array | undefined> =>
    page.evaluate(async () => {
      const canvas = document.querySelector<HTMLCanvasElement>("canvas")
      if (canvas === null) return undefined
      const blob = await new Promise<Blob | undefined>((resolve) =>
        canvas.toBlob((value) => resolve(value ?? undefined), "image/png"),
      )
      return blob === undefined ? undefined : new Uint8Array(await blob.arrayBuffer())
    })
  let previous: Uint8Array | undefined
  let previousFingerprint = ""
  for (let attempt = 0; attempt < 40; attempt += 1) {
    const current = await capture()
    if (current !== undefined) {
      const fingerprint = createHash("sha1").update(current).digest("hex")
      if (previousFingerprint === fingerprint) return current
      previousFingerprint = fingerprint
      previous = current
    }
    await new Promise((resolve) => setTimeout(resolve, 300))
  }
  return previous
}

async function resetOZToFirstPage(page: Page): Promise<void> {
  for (let attempt = 0; attempt < 50; attempt += 1) {
    const current = await page.evaluate(
      () => document.querySelector<HTMLInputElement>("input[title='현재 페이지']")?.value ?? "",
    )
    if (current === "1" || current === "") return
    await page.evaluate(() => {
      document.querySelector<HTMLElement>("input[title='한 페이지 전으로 이동']")?.click()
    })
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
}

async function captureOZPages(page: Page): Promise<readonly Uint8Array[]> {
  await setOZZoomTo300(page)
  await resetOZToFirstPage(page)
  const total = await readOZTotalPages(page)
  const pages: Uint8Array[] = []
  for (let index = 1; index <= total; index += 1) {
    const png = await captureStableCanvas(page)
    if (png === undefined) throw new Error(`${index}페이지 캔버스 캡처에 실패했습니다.`)
    pages.push(png)
    if (index < total) {
      await page.evaluate(() => {
        document.querySelector<HTMLElement>("input[title='한 페이지 다음으로 이동']")?.click()
      })
      await page.waitForFunction(
        (target) =>
          document.querySelector<HTMLInputElement>("input[title='현재 페이지']")?.value === String(target),
        index + 1,
        { timeout: 15_000 },
      )
    }
  }
  return pages
}

async function buildPdfFromPngs(pngs: readonly Uint8Array[]): Promise<Uint8Array> {
  const doc = await PDFDocument.create()
  for (const png of pngs) {
    const image = await doc.embedPng(png)
    const page = doc.addPage([image.width, image.height])
    page.drawImage(image, { x: 0, y: 0, width: image.width, height: image.height })
  }
  return doc.save()
}

function createSurface(page: Page, disconnect: () => Promise<void>): KrasOzViewerSurface {
  let capturedPngs: readonly Uint8Array[] | undefined
  const captureOnce = async (): Promise<readonly Uint8Array[]> => {
    if (capturedPngs !== undefined) return capturedPngs
    const pngs = await captureOZPages(page)
    capturedPngs = pngs
    return pngs
  }
  return {
    url: () => page.url(),
    disconnect,
    hasVisibleCaptcha: () => visibleCaptchaSelector(page),
    capturePdf: async () => buildPdfFromPngs(await captureOnce()),
    capturePagePngs: () => captureOnce(),
    readReportXml: async () => {
      try {
        // 1. Try to get XML from global JavaScript variables
        const xmlFromVar = await page.evaluate(() => {
          const win = window as any
          const candidates = [
            win.ozData,
            win.OZDATA,
            win._ozData,
            win.__ozData,
            win.ozdata,
            win.OZ_DATA,
            win._OZ_DATA,
            win.reportData,
            win.REPORT_DATA,
          ]
          for (const val of candidates) {
            if (typeof val === "string" && val.trim().startsWith("<")) return val
          }
          return undefined
        })
        if (xmlFromVar) return xmlFromVar

        // 2. Extract from page source (content)
        const content = await page.content()

        // 2a. New OZ format (2026): var strOZDataXML = "<?xml ...?><REAL_LAND>...</REAL_LAND>"
        const ozVar = content.match(/var\s+strOZDataXML\s*=\s*"((?:[^"\\]|\\.)*)"/)
        const ozXml = ozVar?.[1]?.trim()
        if (ozXml !== undefined && ozXml.startsWith("<")) {
          return ozXml.replace(/\\"/g, '"').replace(/\\\\/g, "\\")
        }

        // 2b. Old format: ozdata=<...REAL_LAND_BLDG...>
        const match = content.match(/ozdata=(<[\s\S]*?)(?=<\/REAL_LAND_BLDG>)/i)
        if (match && match[1]) {
          return `<?xml version='1.0' encoding='euc-kr'?>${match[1]}</REAL_LAND_BLDG>`
        }

        // 3. Fallback: look for XML in DOM elements
        const candidates = await page.locator("pre, textarea, [id*='xml' i], [class*='xml' i]").allTextContents()
        const found = candidates.find((value) => value.trim().startsWith("<"))?.trim()
        if (found) return found

        return ""
      } catch (_) {
        return ""
      }
    },
  }
}

export async function connectKrasOzViewer(): Promise<KrasOzViewerSurface | undefined> {
  const browser = await chromium.connectOverCDP(CDP_ENDPOINT, { noDefaults: true, timeout: 5000 })
  const pages = browser.contexts().flatMap((context) => context.pages())
  const matches = pages.filter((page) => isViewerUrl(page.url()))
  const exact = matches.length === 1 ? matches[0] : undefined
  if (exact === undefined) {
    await browser.close()
    return undefined
  }
  const page = exact
  return createSurface(page, () => browser.close())
}
