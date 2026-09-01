import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from "node:fs/promises"
import { createServer } from "node:http"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { describe, it, type TestContext } from "node:test"

import { ensureRuntime, runtimeZipName } from "../scripts/kras-quick-runtime.ts"

const VERSION = "1.0.3"
const ZIP_NAME = runtimeZipName(VERSION)
const EXTRACT_DIR = `kras-quick-runtime-v${VERSION}`
const NODE_EXE = "fake node binary\n"

function sha256Of(data: Buffer): string {
  return createHash("sha256").update(data).digest("hex")
}

function makeEnv(cacheDir: string, overrides?: Record<string, string>): NodeJS.ProcessEnv {
  return { ...overrides, KRAS_QUICK_CACHE_DIR: cacheDir }
}

async function makeRuntimeZip(dir: string): Promise<Buffer> {
  const staging = join(dir, "staging")
  await mkdir(join(staging, "node"), { recursive: true })
  await writeFile(join(staging, "node", "node.exe"), NODE_EXE)
  await mkdir(join(staging, "scripts"), { recursive: true })
  await writeFile(join(staging, "scripts", "property-auto-runner.ts"), 'console.log("quick")\n')
  await mkdir(join(staging, "tesseract"), { recursive: true })
  await writeFile(join(staging, "tesseract", "tesseract.exe"), "fake tesseract binary\n")
  const zipPath = join(dir, "fixture.zip")
  const escaped = staging.replaceAll("'", "''")
  const result = spawnSync(
    "powershell.exe",
    [
      "-NoProfile",
      "-NonInteractive",
      "-Command",
      `Compress-Archive -Path '${escaped}\\*' -DestinationPath '${zipPath}'`,
    ],
    { encoding: "utf8" },
  )
  assert.equal(result.status, 0, result.stderr)
  return await readFile(zipPath)
}

async function serveZip(
  bytes: Buffer,
): Promise<{ url: string; hits: () => number; close: () => Promise<void> }> {
  let hitCount = 0
  const server = createServer((_request, response) => {
    hitCount += 1
    response.writeHead(200, { "content-type": "application/zip" })
    response.end(bytes)
  })
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(0, "127.0.0.1", resolve)
  })
  const address = server.address()
  if (address === null || typeof address === "string")
    throw new Error("test server has no TCP port")
  return {
    url: `http://127.0.0.1:${address.port}/kras-quick-runtime.zip`,
    hits: () => hitCount,
    close: () =>
      new Promise<void>((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      ),
  }
}

describe("ensureRuntime", () => {
  it("reuses verified cached zip and extracted dir without network", async (t: TestContext) => {
    const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"))
    t.after(() => rm(dir, { recursive: true, force: true }))
    const zip = await makeRuntimeZip(dir)
    const cacheDir = join(dir, "cache")
    await mkdir(join(cacheDir, EXTRACT_DIR, "node"), { recursive: true })
    await writeFile(join(cacheDir, EXTRACT_DIR, "node", "node.exe"), NODE_EXE)
    await mkdir(join(cacheDir, EXTRACT_DIR, "scripts"), { recursive: true })
    await writeFile(
      join(cacheDir, EXTRACT_DIR, "scripts", "property-auto-runner.ts"),
      'console.log("quick")\n',
    )
    await mkdir(join(cacheDir, EXTRACT_DIR, "tesseract"), { recursive: true })
    await writeFile(
      join(cacheDir, EXTRACT_DIR, "tesseract", "tesseract.exe"),
      "fake tesseract binary\n",
    )
    await writeFile(join(cacheDir, EXTRACT_DIR, ".runtime-sha256"), sha256Of(zip))
    await writeFile(join(cacheDir, ZIP_NAME), zip)
    const meta = {
      version: VERSION,
      url: "http://127.0.0.1:1/unreachable",
      sha256: sha256Of(zip).toUpperCase(),
    }
    const result = await ensureRuntime(meta, makeEnv(cacheDir))
    assert.equal(result, join(cacheDir, EXTRACT_DIR))
    assert.equal(await readFile(join(result, "node", "node.exe"), "utf8"), NODE_EXE)
  })

  it("downloads via env URL seam, verifies, and extracts into cache dir", async (t: TestContext) => {
    const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"))
    t.after(() => rm(dir, { recursive: true, force: true }))
    const zip = await makeRuntimeZip(dir)
    const server = await serveZip(zip)
    t.after(() => server.close())
    const cacheDir = join(dir, "cache")
    const meta = {
      version: VERSION,
      url: "http://127.0.0.1:1/wrong-url",
      sha256: sha256Of(zip).toUpperCase(),
    }
    const result = await ensureRuntime(
      meta,
      makeEnv(cacheDir, { KRAS_QUICK_RUNTIME_URL: server.url }),
    )
    assert.equal(server.hits(), 1)
    assert.equal(result, join(cacheDir, EXTRACT_DIR))
    assert.equal(await readFile(join(result, "node", "node.exe"), "utf8"), NODE_EXE)
    assert.equal(sha256Of(await readFile(join(cacheDir, ZIP_NAME))), sha256Of(zip))
    assert.deepEqual(
      (await readdir(cacheDir)).filter((name) => name.endsWith(".tmp")),
      [],
    )
  })

  it("rejects on SHA-256 mismatch via env hash seam and leaves nothing cached", async (t: TestContext) => {
    const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"))
    t.after(() => rm(dir, { recursive: true, force: true }))
    const zip = await makeRuntimeZip(dir)
    const server = await serveZip(zip)
    t.after(() => server.close())
    const cacheDir = join(dir, "cache")
    const meta = { version: VERSION, url: server.url, sha256: sha256Of(zip) }
    await assert.rejects(
      ensureRuntime(meta, makeEnv(cacheDir, { KRAS_QUICK_RUNTIME_SHA256: "0".repeat(64) })),
      /SHA-256/,
    )
    assert.equal(server.hits(), 1)
    assert.deepEqual(await readdir(cacheDir), [])
  })

  it("re-downloads when the cached zip hash does not match", async (t: TestContext) => {
    const dir = await mkdtemp(join(tmpdir(), "kras-quick-runtime-test-"))
    t.after(() => rm(dir, { recursive: true, force: true }))
    const zip = await makeRuntimeZip(dir)
    const server = await serveZip(zip)
    t.after(() => server.close())
    const cacheDir = join(dir, "cache")
    await mkdir(cacheDir, { recursive: true })
    await writeFile(join(cacheDir, ZIP_NAME), Buffer.from("stale bytes"))
    const meta = { version: VERSION, url: server.url, sha256: sha256Of(zip) }
    const result = await ensureRuntime(meta, makeEnv(cacheDir))
    assert.equal(server.hits(), 1)
    assert.equal(sha256Of(await readFile(join(cacheDir, ZIP_NAME))), sha256Of(zip))
    assert.equal(await readFile(join(result, "node", "node.exe"), "utf8"), NODE_EXE)
  })
})
