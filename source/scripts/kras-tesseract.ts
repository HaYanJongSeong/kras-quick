import { execFile } from "node:child_process"
import { constants } from "node:fs"
import { access, mkdtemp, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { promisify } from "node:util"

const execFileAsync = promisify(execFile)
const DEFAULT_TIMEOUT_MS = 5_000

export class TesseractBinaryMissingError extends Error {
  readonly binaryPath: string

  constructor(binaryPath: string) {
    super(`Bundled Tesseract binary missing: ${binaryPath}`)
    this.name = "TesseractBinaryMissingError"
    this.binaryPath = binaryPath
  }
}

export class TesseractTimeoutError extends Error {
  readonly timeoutMs: number

  constructor(timeoutMs: number) {
    super(`Tesseract timed out after ${timeoutMs}ms`)
    this.name = "TesseractTimeoutError"
    this.timeoutMs = timeoutMs
  }
}

export type TesseractRunOptions = {
  readonly shell: false
  readonly timeoutMs: number
  readonly env: NodeJS.ProcessEnv
}

export type TesseractRunner = (
  executable: string,
  args: readonly string[],
  options: TesseractRunOptions,
) => Promise<{ readonly stdout: string; readonly stderr: string }>

export type TesseractOptions = {
  readonly binaryPath?: string
  readonly timeoutMs?: number
  readonly tempRoot?: string
  readonly runner?: TesseractRunner
  readonly attempt?: 1 | 2 | 3
}

export function bundledTesseractPath(execPath = process.execPath): string {
  return join(dirname(dirname(execPath)), "tesseract", "tesseract.exe")
}

const runTesseract: TesseractRunner = async (executable, args, options) => {
  try {
    const result = await execFileAsync(executable, [...args], {
      encoding: "utf8",
      env: options.env,
      shell: options.shell,
      timeout: options.timeoutMs,
      windowsHide: true,
    })
    return { stdout: result.stdout, stderr: result.stderr }
  } catch (error: unknown) {
    if (error instanceof Error && "killed" in error && error.killed === true) {
      throw new TesseractTimeoutError(options.timeoutMs)
    }
    throw error
  }
}

export async function recognizeCaptchaCandidate(
  pngBase64: string,
  options: TesseractOptions = {},
): Promise<string | null> {
  const binaryPath = options.binaryPath ?? bundledTesseractPath()
  if (options.runner === undefined) {
    try {
      await access(binaryPath, constants.X_OK)
    } catch {
      throw new TesseractBinaryMissingError(binaryPath)
    }
  }
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
  const directory = await mkdtemp(join(options.tempRoot ?? tmpdir(), "kras-captcha-"))
  const inputPath = join(directory, "captcha.png")
  const attempt = options.attempt ?? 1
  try {
    await writeFile(inputPath, Buffer.from(pngBase64, "base64"))
    const { stdout } = await (options.runner ?? runTesseract)(
      binaryPath,
      [
        inputPath,
        "stdout",
        "-l",
        "eng",
        "--psm",
        attempt === 1 ? "7" : attempt === 2 ? "8" : "13",
        "--oem",
        "1",
        "-c",
        "tessedit_char_whitelist=0123456789",
        "-c",
        "classify_bln_numeric_mode=1",
        ...(attempt === 2
          ? ["-c", "thresholding_method=2"]
          : attempt === 3
            ? ["-c", "thresholding_method=1", "-c", "invert_threshold=0.5"]
            : []),
      ],
      {
        shell: false,
        timeoutMs,
        env: {
          ...process.env,
          TESSDATA_PREFIX: join(dirname(binaryPath), "tessdata"),
        },
      },
    )
    const candidate = stdout.trim()
    return /^[0-9]{5}$/.test(candidate) ? candidate : null
  } finally {
    await rm(directory, { recursive: true, force: true })
  }
}
