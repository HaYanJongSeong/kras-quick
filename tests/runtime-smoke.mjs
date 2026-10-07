import assert from "node:assert/strict"
import { mkdtemp, readFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { spawn, spawnSync } from "node:child_process"
import { pathToFileURL } from "node:url"
import { ensureRuntime } from "../source/scripts/kras-quick-runtime.ts"

const cache = await mkdtemp(join(tmpdir(), "kras-runtime-smoke-"))
const meta = JSON.parse(await readFile(new URL("../kras-quick-runtime.meta.json", import.meta.url), "utf8"))
const root = await ensureRuntime(meta, {
  ...process.env,
  KRAS_QUICK_CACHE_DIR: cache,
  KRAS_QUICK_RUNTIME_PATH: resolve("dist/kras-quick-runtime-v2.0.2.zip"),
})
const node = join(root, "node", "node.exe")
const script = join(root, "scripts", "property-auto-runner.ts")
const code = `await import(${JSON.stringify(pathToFileURL(script).href)})`
const imported = spawnSync(node, ["--input-type=module", "-e", code], { encoding: "utf8" })
assert.equal(imported.status, 0, imported.stderr)
assert.equal(spawnSync(join(root, "tesseract", "tesseract.exe"), ["--version"]).status, 0)
assert.equal(await ensureRuntime(meta, { ...process.env, KRAS_QUICK_CACHE_DIR: cache }), root)
console.log("PASS: runtime hash, extraction, all imports, Tesseract, cached reuse")

const exe = spawn(resolve("dist/kras-quick.exe"), [], { stdio: ["pipe", "pipe", "pipe"] })
let output = ""
const ready = await new Promise((resolveReady, reject) => {
  const timer = setTimeout(() => reject(new Error("EXE startup timeout")), 15000)
  exe.stdout.on("data", (data) => {
    output += data.toString("utf8")
    if (output.includes("Chrome을 열까요?")) {
      clearTimeout(timer)
      resolveReady(true)
    }
  })
  exe.on("error", reject)
  exe.on("exit", () => { clearTimeout(timer); resolveReady(false) })
})
exe.kill()
assert.equal(ready, true, output)
assert.match(output, /kras-quick v2\.0\.2/)
console.log("PASS: EXE starts and reaches the Chrome prompt; no KRAS query submitted")
