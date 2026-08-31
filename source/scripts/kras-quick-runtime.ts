import { type SpawnOptions, spawn } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync } from "node:fs"
import { mkdir, open, readFile, rename, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"

export type RuntimeMeta = {
  readonly version: string
  readonly url: string
  readonly sha256: string
}

export const RUNTIME_DOWNLOAD_TIMEOUT_MS = 300_000

export function runtimeZipName(version: string): string {
  return `kras-quick-runtime-v${version}.zip`
}

export function runtimeCacheDir(env: NodeJS.ProcessEnv): string {
  return env["KRAS_QUICK_CACHE_DIR"] ?? join(env["LOCALAPPDATA"] ?? tmpdir(), "kras-quick")
}

export async function runProcess(
  command: string,
  args: readonly string[],
  cwd?: string,
  env?: NodeJS.ProcessEnv,
): Promise<number> {
  return await new Promise((resolve, reject) => {
    const options: SpawnOptions = { stdio: "inherit", windowsHide: false }
    if (cwd !== undefined) options.cwd = cwd
    if (env !== undefined) options.env = env
    const child = spawn(command, args, options)
    child.once("error", reject)
    child.once("close", (code) => resolve(code ?? 1))
  })
}

export async function sha256File(path: string): Promise<string> {
  const hash = createHash("sha256")
  const file = await open(path, "r")
  try {
    const buffer = Buffer.alloc(1 << 20)
    while (true) {
      const { bytesRead } = await file.read(buffer, 0, buffer.length, null)
      if (bytesRead === 0) break
      hash.update(buffer.subarray(0, bytesRead))
    }
  } finally {
    await file.close()
  }
  return hash.digest("hex")
}

async function downloadZip(url: string, tmpPath: string, expectedSha: string): Promise<void> {
  const response = await fetch(url, { signal: AbortSignal.timeout(RUNTIME_DOWNLOAD_TIMEOUT_MS) })
  if (!response.ok) throw new Error(`런타임 ZIP 다운로드 실패 (HTTP ${response.status}): ${url}`)
  if (response.body === null)
    throw new Error(`런타임 ZIP 다운로드 실패: 응답 본문이 없습니다: ${url}`)
  const hash = createHash("sha256")
  const file = await open(tmpPath, "w")
  const reader = response.body.getReader()
  try {
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      hash.update(value)
      await file.write(value)
    }
  } finally {
    reader.releaseLock()
    await file.close()
  }
  const digest = hash.digest("hex")
  if (digest.toLowerCase() !== expectedSha.toLowerCase()) {
    throw new Error(`런타임 ZIP SHA-256 불일치 (기대: ${expectedSha}, 실제: ${digest})`)
  }
}

async function extractZip(zipPath: string, extractDir: string): Promise<void> {
  const escapedZip = zipPath.replaceAll("'", "''")
  const escapedDir = extractDir.replaceAll("'", "''")
  const code = `$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath '${escapedZip}' -DestinationPath '${escapedDir}' -Force`
  const exitCode = await runProcess("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-Command",
    code,
  ])
  if (exitCode !== 0) throw new Error(`런타임 ZIP 압축 해제 실패 (exit ${exitCode})`)
}

export async function ensureRuntime(meta: RuntimeMeta, env: NodeJS.ProcessEnv): Promise<string> {
  const cacheDir = runtimeCacheDir(env)
  const url = env["KRAS_QUICK_RUNTIME_URL"] ?? meta.url
  const expectedSha = env["KRAS_QUICK_RUNTIME_SHA256"] ?? meta.sha256
  const zipPath = join(cacheDir, runtimeZipName(meta.version))
  const extractDir = join(cacheDir, `kras-quick-runtime-v${meta.version}`)
  const nodeMarker = join(extractDir, "node", "node.exe")
  const scriptMarker = join(extractDir, "scripts", "property-auto-runner.ts")
  const extractHashMarker = join(extractDir, ".runtime-sha256")

  await mkdir(cacheDir, { recursive: true })

  // 재사용 검증: 캐시된 ZIP의 해시가 기대값과 일치하면 재다운로드 없이 사용.
  let zipReady = false
  if (existsSync(zipPath)) {
    if ((await sha256File(zipPath)).toLowerCase() === expectedSha.toLowerCase()) {
      zipReady = true
    } else {
      await rm(zipPath, { force: true })
    }
  }

  if (!zipReady) {
    const tmpPath = `${zipPath}.tmp`
    await rm(tmpPath, { force: true })
    try {
      await downloadZip(url, tmpPath, expectedSha)
      // 검증 완료된 바이트만 최종 이름으로 원자적 이동.
      await rename(tmpPath, zipPath)
    } finally {
      await rm(tmpPath, { force: true })
    }
  }

  let extractedHash = ""
  try {
    extractedHash = (await readFile(extractHashMarker, "utf8")).trim().toLowerCase()
  } catch {
    extractedHash = ""
  }
  const extractedReady =
    existsSync(nodeMarker) &&
    existsSync(scriptMarker) &&
    extractedHash === expectedSha.toLowerCase()
  if (!extractedReady) {
    await rm(extractDir, { recursive: true, force: true })
    await extractZip(zipPath, extractDir)
    await writeFile(extractHashMarker, expectedSha.toLowerCase(), "utf8")
  }
  return extractDir
}
