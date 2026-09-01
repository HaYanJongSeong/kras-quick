import assert from "node:assert/strict"
import { access, mkdtemp, readdir, rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { describe, it, type TestContext } from "node:test"

import {
  bundledTesseractPath,
  recognizeCaptchaCandidate,
  TesseractBinaryMissingError,
  TesseractTimeoutError,
  type TesseractRunner,
} from "../scripts/kras-tesseract.ts"

const PNG_BASE64 = Buffer.from("png fixture").toString("base64")

async function withTempRoot(t: TestContext): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "kras-tesseract-test-"))
  t.after(() => rm(root, { recursive: true, force: true }))
  return root
}

describe("recognizeCaptchaCandidate", () => {
  it("resolves bundled Tesseract beside the portable runtime, not beside node", () => {
    const nodePath = "C:\\runtime\\node\\node.exe"
    assert.equal(bundledTesseractPath(nodePath), "C:\\runtime\\tesseract\\tesseract.exe")
  })

  it("returns only an exact five-ASCII-digit candidate and invokes process without shell", async (t) => {
    const root = await withTempRoot(t)
    const binaryPath = join(root, "tesseract.exe")
    const runner: TesseractRunner = async (executable, args, options) => {
      assert.equal(executable, binaryPath)
      assert.equal(options.shell, false)
      assert.deepEqual(args.slice(1), ["stdout", "-l", "eng", "--psm", "7", "-c", "tessedit_char_whitelist=0123456789"])
      await access(args[0] ?? "")
      return { stdout: "12345\n", stderr: "" }
    }

    assert.equal(await recognizeCaptchaCandidate(PNG_BASE64, { binaryPath, runner }), "12345")
  })

  it("rejects candidates that are not exactly five ASCII digits", async (t) => {
    const root = await withTempRoot(t)
    const outputs = ["1234", "123456", "12 345", "１２３４５", "abcde"]
    for (const stdout of outputs) {
      const runner: TesseractRunner = async () => ({ stdout, stderr: "" })
      assert.equal(
        await recognizeCaptchaCandidate(PNG_BASE64, {
          binaryPath: join(root, "tesseract.exe"),
          runner,
        }),
        null,
      )
    }
  })

  it("reports a missing bundled binary before invoking OCR", async (t) => {
    const root = await withTempRoot(t)
    await assert.rejects(
      recognizeCaptchaCandidate(PNG_BASE64, { binaryPath: join(root, "missing.exe") }),
      TesseractBinaryMissingError,
    )
  })

  it("times out through the bounded runner contract", async (t) => {
    const root = await withTempRoot(t)
    const runner: TesseractRunner = async (_executable, _args, options) => {
      assert.equal(options.timeoutMs, 25)
      throw new TesseractTimeoutError(options.timeoutMs)
    }
    await assert.rejects(
      recognizeCaptchaCandidate(PNG_BASE64, {
        binaryPath: join(root, "tesseract.exe"),
        timeoutMs: 25,
        runner,
      }),
      TesseractTimeoutError,
    )
  })

  it("removes the temporary PNG after OCR completes", async (t) => {
    const root = await withTempRoot(t)
    let inputPath = ""
    const runner: TesseractRunner = async (_executable, args) => {
      inputPath = args[0] ?? ""
      await access(inputPath)
      return { stdout: "54321", stderr: "" }
    }

    await recognizeCaptchaCandidate(PNG_BASE64, {
      binaryPath: join(root, "tesseract.exe"),
      tempRoot: root,
      runner,
    })

    await assert.rejects(access(inputPath))
    assert.deepEqual(await readdir(root), [])
  })
})
