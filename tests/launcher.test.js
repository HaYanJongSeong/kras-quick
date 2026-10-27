import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { spawnSync } from "node:child_process"
import { test } from "node:test"
import { fileURLToPath } from "node:url"

test("launcher is BOM-free and points to the current release asset", () => {
  const file = new URL("../bin/kras-quick.js", import.meta.url)
  const bytes = readFileSync(file)
  assert.equal(bytes.subarray(0, 2).toString(), "#!")
  assert.equal(spawnSync(process.execPath, ["--check", fileURLToPath(file)]).status, 0)
  const text = bytes.toString("utf8")
  const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"))
  const lock = JSON.parse(readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"))
  assert.equal(manifest.name, "kras-quick", "npx kras-quick must resolve the public package")
  assert.equal(lock.name, manifest.name)
  assert.equal(lock.packages[""].name, manifest.name)
  assert.equal(manifest.bin["kras-quick"], "bin/kras-quick.js")
  assert.equal(text.match(/const VERSION = "([^"]+)"/)?.[1], manifest.version)
  assert.match(text, /download\/v\$\{VERSION\}\/kras-quick\.exe/)
  assert.doesNotMatch(text, /kras_quick_v/)
})
