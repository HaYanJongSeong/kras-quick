import assert from "node:assert/strict"
import { mkdtemp, readFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn, spawnSync } from "node:child_process"

const prefix = resolve(process.argv[2])
const home = await mkdtemp(join(tmpdir(), "kras-npm-smoke-"))
const command = join(prefix, "kras-quick.cmd")
const child = spawn("cmd.exe", ["/d", "/c", command], {
  env: { ...process.env, USERPROFILE: home },
  stdio: ["pipe", "pipe", "pipe"],
})
let output = ""
let errors = ""
child.stderr.on("data", (data) => { errors += data.toString("utf8") })
try {
  await new Promise((resolveReady, reject) => {
    const timer = setTimeout(() => reject(new Error(`startup timeout: ${errors}`)), 60000)
    child.on("error", reject)
    child.on("exit", (code) => { clearTimeout(timer); reject(new Error(`early exit ${code}: ${errors}`)) })
    child.stdout.on("data", (data) => {
      output += data.toString("utf8")
      if (output.includes("Chrome을 열까요?")) {
        clearTimeout(timer)
        resolveReady()
      }
    })
  })
  const bytes = await readFile(join(home, "Downloads", "kras-quick.exe"))
  const sum = await readFile(resolve("dist/kras-quick.exe.sha256"), "utf8")
  assert.equal(createHash("sha256").update(bytes).digest("hex").toUpperCase(), sum.split(/\s+/)[0])
  assert.match(output, /kras-quick v2\.0\.3/)
  console.log("PASS: installed npm command downloads verified EXE and reaches Chrome prompt")
} finally {
  // 이 테스트가 띄운 명령과 EXE만 종료한다. Chrome은 시작하지 않았다.
  spawnSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"])
}
