import { appendFile, mkdir, readFile, rename, writeFile } from "node:fs/promises"
import { dirname, resolve } from "node:path"
import { createInterface } from "node:readline/promises"
import { chromium, type Page } from "playwright-core"

type Config = { queue_file: string; cursor_file: string; log_file: string; poll_ms?: number }
type Event = {
  property_number: string
  jibun?: string
  pnu?: string
  usage?: string
  opinion?: string
  special?: string
  usage_changed?: boolean
  opinion_changed?: boolean
  special_changed?: boolean
}
type Result = { property_number: string; status: string; detail?: string }

const PLACEHOLDER = "키워드(재산명/주소/재산번호 등)"

async function loadConfig(path: string): Promise<Config> {
  const config = JSON.parse(await readFile(path, "utf8")) as Config
  const base = dirname(resolve(path))
  return {
    ...config,
    queue_file: resolve(base, config.queue_file),
    cursor_file: resolve(base, config.cursor_file),
    log_file: resolve(base, config.log_file),
  }
}

async function cursor(path: string): Promise<number> {
  try {
    const data = JSON.parse(await readFile(path, "utf8")) as { offset?: number }
    return typeof data.offset === "number" ? data.offset : 0
  } catch {
    return 0
  }
}

async function saveCursor(path: string, offset: number): Promise<void> {
  await mkdir(dirname(path), { recursive: true })
  const temp = `${path}.tmp`
  await writeFile(temp, JSON.stringify({ offset }), "utf8")
  await rename(temp, path)
}

async function readEvents(config: Config): Promise<{ event: Event; end: number }[]> {
  let text: string
  try {
    text = await readFile(config.queue_file, "utf8")
  } catch {
    return []
  }
  let offset = await cursor(config.cursor_file)
  if (offset > Buffer.byteLength(text)) offset = 0
  const bytes = Buffer.from(text, "utf8")
  const lines = bytes.subarray(offset).toString("utf8").split("\n")
  const events: { event: Event; end: number }[] = []
  let position = offset
  for (const line of lines) {
    const length = Buffer.byteLength(line) + 1
    position += length
    if (!line.trim()) continue
    try {
      const event = JSON.parse(line) as Event
      if (event.property_number && (typeof event.usage === "string" || typeof event.opinion === "string" || typeof event.special === "string")) events.push({ event, end: position })
    } catch {
      continue
    }
  }
  return events
}

async function update(page: Page, event: Event, commit: boolean): Promise<Result> {
  const clickText = async (text: string): Promise<void> => {
    await page.evaluate((buttonText) => {
      const button = [...document.querySelectorAll("button")].find((item) => item.innerText.trim() === buttonText)
      button?.click()
    }, text)
  }
  const wait = (ms: number) => page.waitForTimeout(ms)
  await clickText("닫기")
  await wait(300)
  await page.getByPlaceholder(PLACEHOLDER, { exact: true }).fill(event.property_number)
  await clickText("검색")
  await wait(900)
  const row = page.locator("tr").filter({ hasText: event.property_number }).first()
  if ((await row.count()) === 0) throw new Error(`SCP row not found: ${event.property_number}`)
  await page.evaluate((propertyNumber) => {
    const row = [...document.querySelectorAll("tr")].find((item) =>
      item.innerText.split("\t").some((value) => value.trim() === propertyNumber),
    )
    row?.click()
  }, event.property_number)
  await wait(700)
  await clickText("현장조사수정")
  await wait(700)
  const controls = page.locator("input.cell-input")
  const count = await controls.count()
  const specialTarget = controls.nth(count - 2)
  const usageTarget = controls.nth(count - 3)
  const opinionTarget = controls.nth(count - 1)
  const beforeSpecial = await specialTarget.inputValue()
  const beforeUsage = await usageTarget.inputValue()
  const beforeOpinion = await opinionTarget.inputValue()
  const updateSpecial = event.special !== undefined && event.special_changed === true
  const updateUsage = event.usage !== undefined && event.usage_changed !== false
  const updateOpinion = event.opinion !== undefined && event.opinion_changed === true
  const specialChanged = updateSpecial && beforeSpecial !== event.special
  const usageChanged = updateUsage && beforeUsage !== event.usage
  const opinionChanged = updateOpinion && beforeOpinion !== event.opinion
  if (!specialChanged && !usageChanged && !opinionChanged) {
    await clickText("닫기")
    return { property_number: event.property_number, status: "unchanged" }
  }
  const detail = [
    specialChanged ? `${beforeSpecial} -> ${event.special}` : undefined,
    usageChanged ? `${beforeUsage} -> ${event.usage}` : undefined,
    opinionChanged ? `${beforeOpinion} -> ${event.opinion}` : undefined,
  ].filter((value): value is string => value !== undefined).join("; ")
  if (!commit) {
    await clickText("취소")
    await wait(250)
    await clickText("닫기")
    return { property_number: event.property_number, status: "preview", detail }
  }
  if (specialChanged && event.special !== undefined) await specialTarget.fill(event.special)
  if (usageChanged && event.usage !== undefined) await usageTarget.fill(event.usage)
  if (opinionChanged && event.opinion !== undefined) await opinionTarget.fill(event.opinion)
  await clickText("저장")
  await wait(1500)
  await clickText("확인")
  await wait(400)
  await clickText("닫기")
  await wait(400)
  await page.evaluate((propertyNumber) => {
    const row = [...document.querySelectorAll("tr")].find((item) =>
      item.innerText.split("\t").some((value) => value.trim() === propertyNumber),
    )
    row?.click()
  }, event.property_number)
  await wait(700)
  const body = await page.locator("body").innerText()
  const verified = (!specialChanged || body.includes(`특기사항\t${event.special}`)) &&
    (!usageChanged || body.includes(`활용방안\t${event.usage}`)) &&
    (!opinionChanged || body.includes(`종합의견\t${event.opinion}`))
  await clickText("닫기")
  if (!verified) throw new Error(`SCP value verification failed: ${event.property_number}`)
  return {
    property_number: event.property_number,
    status: "saved",
    detail,
  }
}

async function log(config: Config, result: Result): Promise<void> {
  await mkdir(dirname(config.log_file), { recursive: true })
  await appendFile(config.log_file, `${JSON.stringify({ ...result, ts: new Date().toISOString() })}\n`, "utf8")
}

async function run(config: Config, commit: boolean, once: boolean): Promise<void> {
  const terminal = createInterface({ input: process.stdin, output: process.stdout })
  const browser = await chromium.connectOverCDP("http://127.0.0.1:9222", { noDefaults: true, timeout: 5000 })
  const pages = browser.contexts().flatMap((context) => context.pages()).filter((page) => new URL(page.url()).pathname === "/board")
  if (pages.length !== 1) throw new Error(`expected one SCP /board page, found ${pages.length}`)
  const page = pages[0]
  page.setDefaultTimeout(8000)
  console.log(`[scp-usage] watcher started: ${commit ? "commit" : "preview"} mode`)
  let lastHeartbeat = 0
  for (;;) {
    const events = await readEvents(config)
    if (Date.now() - lastHeartbeat >= 5000) {
      console.log(`[scp-usage] waiting: ${events.length} queued event(s)`)
      lastHeartbeat = Date.now()
    }
    for (const item of events) {
      console.log(`[scp-usage] processing ${item.event.property_number}: ${item.event.usage ?? item.event.opinion ?? item.event.special}`)
      let handled = false
      while (!handled) {
        try {
          const result = await update(page, item.event, commit)
          await log(config, result)
          console.log(`[scp-usage] ${result.property_number}: ${result.status}${result.detail === undefined ? "" : ` (${result.detail})`}`)
          if (commit || result.status === "unchanged") await saveCursor(config.cursor_file, item.end)
          handled = true
        } catch (error) {
          const result = { property_number: item.event.property_number, status: "failed", detail: String(error) }
          await log(config, result)
          console.error(`[scp-usage] ${result.property_number}: failed (${result.detail})`)
          const answer = (await terminal.question("[1] retry  [2] skip  [3] stop > ")).trim()
          if (answer === "1") continue
          if (answer === "2") {
            await saveCursor(config.cursor_file, item.end)
            await log(config, { property_number: item.event.property_number, status: "skipped", detail: "operator choice" })
            console.log(`[scp-usage] ${item.event.property_number}: skipped by operator`)
            handled = true
            continue
          }
          terminal.close()
          console.log("[scp-usage] stopped by operator")
          return
        }
      }
    }
    if (once) {
      terminal.close()
      process.exit(0)
      return
    }
    await new Promise((done) => setTimeout(done, config.poll_ms ?? 1500))
  }
}

const args = process.argv.slice(2)
const command = args[0]
if (command === "watch") {
  const configIndex = args.indexOf("--config")
  const configPath = configIndex >= 0 ? args[configIndex + 1] : undefined
  if (!configPath) throw new Error("watch requires --config path")
  run(await loadConfig(configPath), args.includes("--commit"), args.includes("--once")).catch((error) => {
    console.error(error)
    process.exitCode = 1
  })
} else {
  console.error("Usage: node scp-usage-watcher.ts watch --config PATH [--commit] [--once]")
  process.exitCode = 1
}
