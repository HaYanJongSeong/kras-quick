import assert from "node:assert/strict"
import { mkdtemp, readFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn, spawnSync } from "node:child_process"

const home = await mkdtemp(join(tmpdir(), "kras-npx-v203-"))
const child = spawn("cmd.exe", ["/d", "/c", "npx --yes @hayanjongseong/kras-quick@latest"], {
  cwd: home,
  env: { ...process.env, USERPROFILE: home },
  stdio: ["pipe", "pipe", "pipe"],
})
child.stdin.end("0\n")
let output = ""
let errors = ""
try {
  await new Promise((ready, reject) => {
    const timer = setTimeout(() => reject(new Error(`npx timeout: ${errors}`)), 90000)
    child.stderr.on("data", (data) => { errors += data.toString("utf8") })
    child.stdout.on("data", (data) => {
      output += data.toString("utf8")
      if (output.includes("Chrome을 열까요?")) { clearTimeout(timer); ready() }
    })
    child.on("error", reject)
    child.on("exit", (code) => {
      clearTimeout(timer)
      if (output.includes("Chrome을 열까요?")) ready()
      else reject(new Error(`npx early exit ${code}: ${output} ${errors}`))
    })
  })
  assert.match(output, /kras-quick v2\.0\.3/)
  const bytes = await readFile(join(home, "Downloads", "kras-quick.exe"))
  const sum = await readFile(resolve("dist/kras-quick.exe.sha256"), "utf8")
  assert.equal(createHash("sha256").update(bytes).digest("hex").toUpperCase(), sum.split(/\s+/)[0])
  console.log("PASS: registry npx@latest downloads verified v2.0.3 EXE and reaches Chrome prompt")
} finally {
  spawnSync("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"])
}
