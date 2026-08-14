#!/usr/bin/env node
import { createHash } from "node:crypto"
import { mkdir, open, rename, rm } from "node:fs/promises"
import { homedir } from "node:os"
import { join } from "node:path"
import { spawn } from "node:child_process"

const VERSION = "1.0.5"
const SHA256 = "F56BC2452A5BF23A68FB7585C475E53528011E07A46A057A1D0B0A99CEBF7AE6"
const URL = `https://github.com/HaYanJongSeong/kras-quick/releases/download/v${VERSION}/kras_quick_v${VERSION}.exe`

function cacheDir() {
  return join(process.env.LOCALAPPDATA ?? join(homedir(), "AppData", "Local"), "kras-quick", "npm")
}

async function sha256File(path) {
  const hash = createHash("sha256")
  const file = await open(path, "r")
  try {
    const buffer = Buffer.alloc(1024 * 1024)
    while (true) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null)
      if (bytesRead === 0) break
      hash.update(buffer.subarray(0, bytesRead))
    }
  } finally {
    await file.close()
  }
  return hash.digest("hex").toUpperCase()
}

async function ensureExe() {
  const dir = cacheDir()
  const exe = join(dir, `kras_quick_v${VERSION}.exe`)
  await mkdir(dir, { recursive: true })
  if (await sha256File(exe).catch(() => "") === SHA256) return exe

  const temp = `${exe}.${process.pid}.tmp`
  await rm(temp, { force: true })
  try {
    console.error(`Downloading KRAS Quick launcher v${VERSION}...`)
    const response = await fetch(URL)
    if (!response.ok || response.body === null) throw new Error(`download failed (HTTP ${response.status})`)
    const file = await open(temp, "w")
    try {
      for await (const chunk of response.body) await file.write(chunk)
    } finally {
      await file.close()
    }
    if (await sha256File(temp) !== SHA256) throw new Error("SHA-256 mismatch")
    await rename(temp, exe)
    return exe
  } finally {
    await rm(temp, { force: true })
  }
}

if (process.platform !== "win32") {
  console.error("kras-quick supports Windows only.")
  process.exit(1)
}

try {
  const exe = await ensureExe()
  const child = spawn(exe, process.argv.slice(2), { stdio: "inherit", windowsHide: false })
  child.once("error", (error) => {
    console.error(`kras-quick launch failed: ${error.message}`)
    process.exitCode = 1
  })
  child.once("exit", (code, signal) => {
    if (signal !== null) process.exitCode = 1
    else process.exitCode = code ?? 1
  })
} catch (error) {
  console.error(`kras-quick failed: ${error instanceof Error ? error.message : String(error)}`)
  process.exitCode = 1
}
