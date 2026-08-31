import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { dirname, join } from "node:path"
import runtimeMeta from "../kras-quick-runtime.meta.json" with { type: "json" }
import { ensureRuntime, runProcess } from "./kras-quick-runtime.ts"
import { createTerminal } from "./terminal-reader.ts"

const QUICK_CDP_PORT = 9333
const QUICK_CDP_ENDPOINT = `http://127.0.0.1:${QUICK_CDP_PORT}`

async function cdpReady(): Promise<boolean> {
  try {
    const response = await fetch(`${QUICK_CDP_ENDPOINT}/json/version`, { signal: AbortSignal.timeout(2000) })
    return response.ok
  } catch {
    return false
  }
}

async function ensureChrome(exeDirectory: string): Promise<void> {
  if (await cdpReady()) return
  const chromePaths = [
    join(process.env["ProgramFiles"] ?? "", "Google", "Chrome", "Application", "chrome.exe"),
    join(process.env["ProgramFiles(x86)"] ?? "", "Google", "Chrome", "Application", "chrome.exe"),
    join(process.env["LOCALAPPDATA"] ?? "", "Google", "Chrome", "Application", "chrome.exe"),
  ]
  const chromePath = chromePaths.find((path) => existsSync(path))
  if (chromePath === undefined) throw new Error("Google Chrome을 찾지 못했습니다. Chrome을 설치한 후 다시 실행하세요.")
  const child = spawn(
    chromePath,
    [
      `--remote-debugging-port=${QUICK_CDP_PORT}`,
      `--user-data-dir=${join(exeDirectory, ".kras-chrome-profile")}`,
      "--disable-popup-blocking",
      "--no-first-run",
      "https://www.kras.go.kr/kras/cert/certView.do",
    ],
    { detached: true, stdio: "ignore" },
  )
  child.unref()
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await Bun.sleep(500)
    if (await cdpReady()) return
  }
  throw new Error("Chrome CDP 연결을 시작하지 못했습니다. Chrome 창과 보안 프로그램 상태를 확인하세요.")
}

try {
  const exeDirectory = dirname(process.execPath)
  const terminal = createTerminal()
  process.stdout.write(`KRAS Quick v${runtimeMeta.version}\n`)
  process.stdout.write(`PDF 저장 경로: ${join(exeDirectory, "KRAS")}\n`)
  let chromeChoice: "open" | "reuse" | "exit" = "exit"
  try {
    while (true) {
      const answer = (await terminal.question("1. Chrome 열기 동의\n2. 이미 열려 있음, 다음 단계\n3. 종료\n선택 > ")).trim()
      if (answer === "1") {
        chromeChoice = "open"
        break
      }
      if (answer === "2") {
        chromeChoice = "reuse"
        break
      }
      if (answer === "3") break
      process.stdout.write("1, 2 또는 3을 입력해 주세요.\n")
    }
  } finally {
    terminal.close()
  }
  if (chromeChoice === "exit") {
    process.stdout.write("사용자가 종료를 선택했습니다.\n")
    process.exitCode = 0
  } else {
  if (chromeChoice === "open") await ensureChrome(exeDirectory)
  if (chromeChoice === "reuse" && !(await cdpReady())) {
    throw new Error(`기존 Chrome CDP 연결(${QUICK_CDP_PORT})을 찾지 못했습니다. 다시 실행해 1번을 선택하세요.`)
  }
  const root = await ensureRuntime(runtimeMeta, process.env)
  const nodePath = join(root, "node", "node.exe")
  const scriptPath = join(root, "scripts", "property-auto-runner.ts")
  const exitCode = await runProcess(
    nodePath,
    [scriptPath, "quick", ...process.argv.slice(2)],
    root,
    { ...process.env, KRAS_CDP_ENDPOINT: QUICK_CDP_ENDPOINT, KRAS_QUICK_ROOT: exeDirectory },
  )
  process.exitCode = exitCode
  if (exitCode !== 0) await runProcess("cmd.exe", ["/c", "pause"])
  }
} catch (error: unknown) {
  process.stderr.write(`kras_quick 실행 실패: ${error instanceof Error ? error.message : String(error)}\n`)
  await runProcess("cmd.exe", ["/c", "pause"])
  process.exitCode = 1
}
