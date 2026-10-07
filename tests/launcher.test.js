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
  assert.match(text, /const VERSION = "2\.0\.2"/)
  assert.match(text, /download\/v\$\{VERSION\}\/kras-quick\.exe/)
  assert.doesNotMatch(text, /kras_quick_v/)
})
