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
  KRAS_QUICK_RUNTIME_PATH: resolve(`dist/kras-quick-runtime-v${meta.version}.zip`),
})
const node = join(root, "node", "node.exe")
const script = join(root, "scripts", "property-auto-runner.ts")
const code = `await import(${JSON.stringify(pathToFileURL(script).href)})`
const imported = spawnSync(node, ["--input-type=module", "-e", code], { encoding: "utf8" })
assert.equal(imported.status, 0, imported.stderr)
assert.equal(spawnSync(join(root, "tesseract", "tesseract.exe"), ["--version"]).status, 0)
assert.equal(await ensureRuntime(meta, { ...process.env, KRAS_QUICK_CACHE_DIR: cache }), root)
console.log("PASS: runtime hash, extraction, all imports, Tesseract, cached reuse")

const exe = spawn(resolve("dist/kras-quick.exe"), [], {
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, KRAS_QUICK_CACHE_DIR: cache },
})
let output = ""
const exitCode = await new Promise((resolveExit, reject) => {
  const timer = setTimeout(() => {
    exe.kill()
    reject(new Error("EXE startup timeout"))
  }, 30000)
  exe.stdout.on("data", (data) => {
    output += data.toString("utf8")
  })
  exe.on("error", reject)
  exe.on("close", (code) => { clearTimeout(timer); resolveExit(code) })
  exe.stdin.end("0\n")
})
assert.equal(exitCode, 0, output)
assert.ok(output.includes(`kras-quick v${meta.version}`), output)
assert.ok(output.includes("브라우저 번호 >"), output)
console.log("PASS: EXE reaches browser selection and cancels cleanly; no browser or KRAS query started")
