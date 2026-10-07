import assert from "node:assert/strict"
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn, spawnSync } from "node:child_process"
import { chromium } from "../source/node_modules/playwright-core/index.mjs"

// 실제 설치 명령을 실행하되 로그인·필지 조회는 하지 않는다.
// 전용 테스트 Chrome과 이 테스트의 자식 프로세스만 종료한다.
const mode = process.argv[2]
assert.ok(mode === "npm" || mode === "npx")
const home = await mkdtemp(join(tmpdir(), `kras-${mode}-runtime-`))
const local = join(home, "AppData", "Local")
await mkdir(local, { recursive: true })
const command = mode === "npm"
  ? join(resolve(process.argv[3]), "kras-quick.cmd")
  : "npx --yes @hayanjongseong/kras-quick@latest"
const browser = await chromium.launchPersistentContext(join(home, "chrome-test-profile"), {
  executablePath: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  headless: true,
  args: ["--remote-debugging-port=9333"],
})
let child
let output = ""
let errors = ""
try {
  const response = await fetch("http://127.0.0.1:9333/json/version")
  assert.equal(response.ok, true)
  child = spawn("cmd.exe", ["/d", "/c", command], {
    cwd: home,
    env: { ...process.env, USERPROFILE: home, LOCALAPPDATA: local },
    stdio: ["pipe", "pipe", "pipe"],
  })
  await new Promise((ready, reject) => {
    let chromeAnswered = false
    const timer = setTimeout(() => reject(new Error(`runtime timeout: ${output}\n${errors}`)), 150000)
    child.stderr.on("data", (data) => { errors += data.toString("utf8") })
    child.stdout.on("data", (data) => {
      output += data.toString("utf8")
      if (!chromeAnswered && output.includes("Chrome을 열까요?")) {
        chromeAnswered = true
        child.stdin.write("0\n")
      }
      if (output.includes("Chrome의 KRAS 페이지에서 로그인해주세요.")) {
        clearTimeout(timer)
        ready()
      }
    })
    child.on("error", reject)
    child.on("exit", (code) => {
      clearTimeout(timer)
      reject(new Error(`runtime early exit ${code}: ${output}\n${errors}`))
    })
  })
  assert.match(output, /kras-quick v2\.0\.3/)
  const exeBytes = await readFile(join(home, "Downloads", "kras-quick.exe"))
  const sum = await readFile(resolve("dist/kras-quick.exe.sha256"), "utf8")
  assert.equal(createHash("sha256").update(exeBytes).digest("hex").toUpperCase(), sum.split(/\s+/)[0])
  const root = join(local, "kras-quick", "kras-quick-runtime-v2.0.3")
  const marker = await readFile(join(root, ".runtime-sha256"), "utf8")
  const meta = JSON.parse(await readFile(resolve("kras-quick-runtime.meta.json"), "utf8"))
  assert.equal(marker.trim().toUpperCase(), meta.sha256.toUpperCase())
  assert.equal(spawnSync(join(root, "tesseract", "tesseract.exe"), ["--version"]).status, 0)
  await writeFile(join(home, "execution.json"), JSON.stringify({
    mode, command, version: "2.0.3", exeHash: sum.split(/\s+/)[0],
    runtimeHash: meta.sha256, reachedLoginPrompt: true, submittedQuery: false,
    output, errors,
  }, null, 2))
  console.log(`PASS ${mode}: actual command, EXE, Chrome CDP, runtime download/extraction, login prompt, Tesseract`)
  console.log(`Evidence: ${join(home, "execution.json")}`)
} finally {
  if (child) spawnSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"])
  await browser.close()
}
