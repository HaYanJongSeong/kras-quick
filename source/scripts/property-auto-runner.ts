import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { readFile } from "node:fs/promises"
import { dirname, join } from "node:path"

import {
  appendDoneBuildingIndex,
  createKrasAutoStage2Result,
  createKrasManualStage2Result,
  isKrasStage3OutputComplete,
  type KrasAutoStage1Result,
  type KrasAutoStage3Result,
  type KrasAutoStageResult,
  parseKrasAutoStage1Result,
  parseKrasAutoStage2Result,
  parseKrasAutoStage3Result,
  propertyAutoStagePaths,
  readDoneBuildingIndexes,
  runKrasAutoStage1,
  runKrasAutoStage3,
  writeKrasAutoStageResult,
} from "./kras-auto-stages.ts"
import { normalizeAddress } from "./kras-domain.ts"
import {
  createKrasExcelWritePlan,
  type KrasExcelTaskConfig,
  type KrasXmlData,
  parseKrasXml,
  printKrasDataTable,
  updateExcelWithKrasData,
} from "./kras-excel-updater.ts"
import { connectKrasOzViewer } from "./kras-oz.ts"
import {
  connectKrasWorkflowWatcher,
  connectOrOpenKrasWorkflowWatcher,
  type KrasWorkflowHooks,
  type KrasWorkflowPage,
  runKrasWorkflowWatcher,
} from "./kras-workflow-watcher.ts"
import { parseParcelAddress } from "./property-address.ts"
import { discoverKrasTarget } from "./property-watcher-page.ts"
import {
  connectPropertyWatcherBrowser,
  type PropertyWatcherRuntimeBrowser,
} from "./property-watcher-runtime.ts"
import { createTerminal, type TerminalReader } from "./terminal-reader.ts"

const watcherRunnerPath = join(
  dirname(process.argv[1] ?? process.execPath),
  "property-watcher-runner.ts",
)

export type PropertyAutoDependencies = {
  readonly runCli: (args: readonly string[]) => Promise<number>
  readonly runWatcher: (command: "start" | "stop") => Promise<void>
}

export type PropertyAutoStage1Dependencies = {
  readonly connect: () => Promise<PropertyWatcherRuntimeBrowser>
  readonly writeResult: (path: string, result: KrasAutoStageResult) => Promise<void>
}

export type PropertyAutoStage2Dependencies = {
  readonly readResult: (path: string) => Promise<string>
  readonly writeResult: (path: string, result: KrasAutoStageResult) => Promise<void>
}

export type PropertyAutoStage3Dependencies = {
  readonly readResult: (path: string) => Promise<string>
  readonly connect: () => Promise<Awaited<ReturnType<typeof connectKrasOzViewer>>>
}

function spawnInherited(executable: string, args: readonly string[]): Promise<number> {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { shell: false, stdio: "inherit" })
    child.once("error", reject)
    child.once("exit", (code, signal) => {
      if (signal !== null) {
        reject(new Error(`${executable} terminated by ${signal}`))
        return
      }
      resolve(code ?? 1)
    })
  })
}

const nodeDependencies: PropertyAutoDependencies = {
  runCli: (args) => spawnInherited("opencode", args),
  runWatcher: async (command) => {
    const exitCode = await spawnInherited(process.execPath, [watcherRunnerPath, command])
    if (exitCode !== 0)
      throw new Error(`property watcher ${command} failed with exit code ${exitCode}`)
  },
}

const nodeStage1Dependencies: PropertyAutoStage1Dependencies = {
  connect: connectPropertyWatcherBrowser,
  writeResult: writeKrasAutoStageResult,
}

const nodeStage2Dependencies: PropertyAutoStage2Dependencies = {
  readResult: (path) => readFile(path, "utf8"),
  writeResult: writeKrasAutoStageResult,
}

const nodeStage3Dependencies: PropertyAutoStage3Dependencies = {
  readResult: (path) => readFile(path, "utf8"),
  connect: connectKrasOzViewer,
}

export async function writeManualStage2ForRoot(
  stage1: KrasAutoStage1Result,
  outputRoot: string | undefined,
  writeResult: (
    path: string,
    result: KrasAutoStageResult,
  ) => Promise<void> = writeKrasAutoStageResult,
): Promise<void> {
  const stage2 = createKrasManualStage2Result(stage1)
  await writeResult(propertyAutoStagePaths(stage1.address, outputRoot).stage2Json, stage2)
}

export async function runPropertyAutoStage1(
  rawAddress: string,
  dependencies: PropertyAutoStage1Dependencies = nodeStage1Dependencies,
): Promise<string> {
  const address = parseParcelAddress(rawAddress)
  const browser = await dependencies.connect()
  const kras = discoverKrasTarget(browser)
  const result = await runKrasAutoStage1(kras, address)
  const path = propertyAutoStagePaths(result.address).stage1Json
  await dependencies.writeResult(path, result)
  return path
}

export async function runPropertyAutoStage2(
  rawAddress: string,
  buildingIndex?: number,
  dependencies: PropertyAutoStage2Dependencies = nodeStage2Dependencies,
): Promise<string> {
  const address = parseParcelAddress(rawAddress)
  const paths = propertyAutoStagePaths(address)
  const stage1 = parseKrasAutoStage1Result(
    JSON.parse(await dependencies.readResult(paths.stage1Json)),
  )
  if (stage1.status !== "completed") {
    throw new Error(`Stage 2 not started: Stage 1 blocked (${stage1.reason})`)
  }
  const selectedBuildingIndex =
    buildingIndex ??
    (stage1.buildingOptions.length === 1 ? stage1.buildingOptions[0]?.index : undefined)
  if (selectedBuildingIndex === undefined) {
    const options = stage1.buildingOptions.map(({ index, label }) => `${index}:${label}`).join(", ")
    throw new Error(`Stage 2 not started: multiple buildings require selection (${options})`)
  }
  const result = createKrasAutoStage2Result(stage1, selectedBuildingIndex)
  await dependencies.writeResult(paths.stage2Json, result)
  return paths.stage2Json
}

export async function runPropertyAutoStage3(
  rawAddress: string,
  dependencies: PropertyAutoStage3Dependencies = nodeStage3Dependencies,
  options: {
    readonly capture?: boolean
    readonly excel?: boolean
    readonly propertyNumber?: string
    readonly buildingAbsent?: boolean
    readonly resolveNameConflict?: (existingPath: string) => Promise<string>
    readonly excelConfig?: KrasExcelTaskConfig
    readonly outputRoot?: string
  } = {},
): Promise<string> {
  const doCapture = options.capture ?? true
  const doExcel = options.excel ?? true
  const address = parseParcelAddress(rawAddress)
  const paths = propertyAutoStagePaths(address, options.outputRoot)
  const stage2 = parseKrasAutoStage2Result(
    JSON.parse(await dependencies.readResult(paths.stage2Json)),
  )
  const surface = await dependencies.connect()
  if (surface === undefined) {
    await writeKrasAutoStageResult(paths.stage3Json, {
      version: 1,
      stage: 3,
      status: "blocked",
      address: stage2.address,
      reason: "viewer_not_open",
    })
    process.stdout.write("Stage 3: OZ Viewer 창이 감지되지 않았습니다.\n")
  } else {
    try {
      let stage3Result: KrasAutoStage3Result | undefined
      if (doCapture) {
        stage3Result = await runKrasAutoStage3(stage2, surface, paths, options.resolveNameConflict)
        if (stage3Result.status === "completed") {
          process.stdout.write(
            `저장 완료. PDF: ${stage3Result.output.pdf}\n스크린샷 ${stage3Result.output.pagePngs.length}장: ${paths.otherDirectory}\n`,
          )
        } else {
          process.stdout.write(`Stage 3: 저장하지 못했습니다. (${stage3Result.reason})\n`)
        }
      }
      if (doExcel) {
        let xmlData: KrasXmlData | undefined
        try {
          let xmlContent: string
          if (doCapture) {
            xmlContent =
              stage3Result !== undefined && stage3Result.status === "completed"
                ? await readFile(stage3Result.output.pageXml, "utf8")
                : await readFile(paths.pageXml, "utf8")
          } else {
            xmlContent = await surface.readReportXml()
          }
          xmlData = parseKrasXml(xmlContent)
          process.stdout.write("\n=== XML 추출값 ===\n")
          printKrasDataTable(xmlData)
        } catch (xmlErr) {
          process.stdout.write(
            `\nXML 읽기/파싱 실패: ${xmlErr instanceof Error ? xmlErr.message : String(xmlErr)}\n`,
          )
          xmlData = undefined
        }
        if (xmlData !== undefined) {
          try {
            const writePlan = createKrasExcelWritePlan(
              xmlData,
              options.excelConfig,
              options.buildingAbsent,
            )
            const excelResult = await updateExcelWithKrasData(
              xmlData,
              options.excelConfig,
              options.propertyNumber,
              options.buildingAbsent,
            )
            if (excelResult.updated) {
              process.stdout.write(
                `\n엑셀 파일: ${writePlan.excelPath}\n시트: ${writePlan.sheetName}\n수정 행: ${excelResult.rowNumber}행\n`,
              )
              for (const cell of writePlan.cells) {
                if (cell.action === "write")
                  process.stdout.write(`${cell.column}열 = ${cell.value}\n`)
              }
              process.stdout.write(
                `Excel 입력 완료. 재산번호: ${excelResult.matchedPropertyNumber ?? "없음"}\n`,
              )
            } else {
              process.stdout.write(
                `\n엑셀 입력 실패: 대상 파일 ${writePlan.excelPath}, 시트 ${writePlan.sheetName}에서 일치 행을 찾지 못했습니다.\n`,
              )
            }
          } catch (excelErr) {
            process.stdout.write(
              `\nExcel 처리 실패: ${excelErr instanceof Error ? excelErr.message : String(excelErr)}\n`,
            )
          }
        }
      }
    } finally {
      await surface.disconnect?.()
    }
  }
  return paths.stage3Json
}

async function printQuickStage3Result(stage3Path: string): Promise<void> {
  const stage3 = parseKrasAutoStage3Result(JSON.parse(await readFile(stage3Path, "utf8")))
  if (stage3.status !== "completed") {
    process.stdout.write(`캡처 실패: ${stage3.reason}. PDF/JSON이 저장되지 않았습니다.\n`)
    return
  }
  process.stdout.write(
    `저장 완료\nPDF: ${stage3.output.pdf}\nXML: ${stage3.output.pageXml}\n구조 JSON: ${stage3.output.structureJson}\n`,
  )
  for (const pagePng of stage3.output.pagePngs) process.stdout.write(`PNG: ${pagePng}\n`)
}

export class ReturnToAddressError extends Error {
  constructor() {
    super("Return to address input")
    this.name = "ReturnToAddressError"
  }
}

export function createAddressReturningTerminal(terminal: TerminalReader): TerminalReader {
  return {
    question: async (prompt) => {
      const answer = await terminal.question(prompt)
      if (answer.trim() === "/주소") throw new ReturnToAddressError()
      return answer
    },
    close: () => terminal.close(),
  }
}

async function askCloseOzViewer(
  terminal: TerminalReader,
  page: Pick<KrasWorkflowPage, "closeOzViewer">,
): Promise<void> {
  while (true) {
    const answer = (
      await terminal.question(
        "오즈뷰어를 종료할까요? (1: 종료하고 주소 입력 / 2: 유지하고 주소 입력)\n",
      )
    ).trim()
    if (answer === "1") {
      const closed = await page.closeOzViewer()
      process.stdout.write(
        closed ? "오즈뷰어 종료 완료.\n" : "오즈뷰어 탭을 찾지 못했습니다. (수동 종료 필요)\n",
      )
      return
    }
    if (answer === "2") return
    process.stdout.write("1 또는 2를 입력해 주세요.\n")
  }
}

async function askOzViewerLoaded(terminal: TerminalReader): Promise<boolean> {
  while (true) {
    const answer = (
      await terminal.question("오즈뷰어가 로딩되었나요? (1: 예 / 2: 다음 주소로 건너뛰기)\n")
    ).trim()
    if (answer === "1") return true
    if (answer === "2") return false
    process.stdout.write("1 또는 2를 입력해 주세요.\n")
  }
}

type QuickStage3Runner = (
  address: string,
  options: NonNullable<Parameters<typeof runPropertyAutoStage3>[2]>,
) => Promise<string>

export async function finishQuickOzWorkflow(
  page: Pick<KrasWorkflowPage, "closeOzViewer">,
  address: string,
  terminal: TerminalReader | undefined,
  outputRoot: string,
  buildingAbsent = false,
  runStage3: QuickStage3Runner = (stage3Address, options) =>
    runPropertyAutoStage3(stage3Address, undefined, options),
): Promise<void> {
  let capture = true
  let returningToAddress = false
  if (terminal !== undefined) {
    while (true) {
      const answer = (
        await terminal.question("OZ 뷰어 작업을 선택하세요. (1: 캡처 / 2: 아무것도 안 함)\n")
      ).trim()
      if (answer === "1") break
      if (answer === "2") {
        capture = false
        break
      }
      process.stdout.write("1 또는 2를 입력해 주세요.\n")
    }
  }
  try {
    if (capture) {
      process.stdout.write(`저장 위치: ${outputRoot}\nPDF/JSON 캡처 시작...\n`)
      let stage3Path: string
      try {
        stage3Path = await runStage3(address, {
          capture: true,
          excel: false,
          buildingAbsent,
          outputRoot,
          ...(terminal === undefined
            ? {}
            : {
                resolveNameConflict: async (existingPath: string): Promise<string> =>
                  (
                    await terminal.question(
                      `${existingPath}\n파일이 이미 있습니다. 이름에 추가할 텍스트를 입력하세요 (예: -재발급, 빈 칸: 덮어쓰기)\n`,
                    )
                  ).trim(),
              }),
        })
      } catch (error) {
        if (error instanceof ReturnToAddressError) {
          returningToAddress = true
          throw error
        }
        process.stdout.write(
          `캡처 실패: ${error instanceof Error ? error.message : String(error)}\n`,
        )
        return
      }
      await printQuickStage3Result(stage3Path)
    }
  } finally {
    if (terminal !== undefined && !returningToAddress) await askCloseOzViewer(terminal, page)
  }
}

async function askStage3Mode(
  terminal: TerminalReader,
): Promise<{ capture: boolean; excel: boolean }> {
  while (true) {
    const answer = (
      await terminal.question(
        "Stage 3 처리 방식을 선택하세요. (1: 캡처만 / 2: 엑셀입력만 / 3: 둘 다 / 4: 아무것도 안 함)\n",
      )
    ).trim()
    if (answer === "1") return { capture: true, excel: false }
    if (answer === "2") return { capture: false, excel: true }
    if (answer === "3") return { capture: true, excel: true }
    if (answer === "4") return { capture: false, excel: false }
    process.stdout.write("1~4 중 하나를 입력해 주세요.\n")
  }
}

type PropertyKrasWorkflowOptions = {
  readonly reuseLoadedOptions?: boolean
  readonly openViewerOnly?: boolean
  readonly excelConfig?: KrasExcelTaskConfig
  readonly quickOutputRoot?: string
}

export async function withKrasWorkflowPage<T>(
  connect: () => Promise<KrasWorkflowPage>,
  action: (page: KrasWorkflowPage) => Promise<T>,
): Promise<T> {
  const page = await connect()
  try {
    return await action(page)
  } finally {
    await page.disconnect?.()
  }
}

export async function runPropertyKrasWorkflow(
  rawAddress: string,
  terminal?: TerminalReader,
  options: PropertyKrasWorkflowOptions = {},
): Promise<boolean> {
  return await withKrasWorkflowPage(connectKrasWorkflowWatcher, (page) =>
    runPropertyKrasWorkflowOnPage(rawAddress, page, terminal, options),
  )
}

async function runPropertyKrasWorkflowOnPage(
  rawAddress: string,
  page: KrasWorkflowPage,
  terminal: TerminalReader | undefined,
  options: PropertyKrasWorkflowOptions,
): Promise<boolean> {
  const workflowPaths = propertyAutoStagePaths(
    normalizeAddress(parseParcelAddress(rawAddress)),
    options.quickOutputRoot,
  )
  const onStage1 = (stage1: Parameters<NonNullable<KrasWorkflowHooks["onStage1"]>>[0]): void => {
    if (stage1.status !== "completed") {
      process.stdout.write(`Stage 1: 건물을 찾지 못했습니다. (${stage1.reason})\n`)
      return
    }
    const doneIndexes = new Set(readDoneBuildingIndexes(workflowPaths))
    process.stdout.write(`Stage 1: 건물구분 ${stage1.buildingOptions.length}개 감지\n1. 선택\n`)
    stage1.buildingOptions.forEach((option, index) => {
      if (isKrasStage3OutputComplete(workflowPaths, option)) {
        process.stdout.write(`${index + 2}.\x1b[32m${option.label} [출력됨]\x1b[0m\n`)
        return
      }
      if (doneIndexes.has(option.index)) {
        process.stdout.write(`${index + 2}.\x1b[32m${option.label} [출력됨]\x1b[0m\n`)
        return
      }
      process.stdout.write(`${index + 2}.${option.label}\n`)
    })
  }
  const result = await (terminal === undefined
    ? runKrasWorkflowWatcher(rawAddress, page, {
        onStage1,
        ...(options.quickOutputRoot === undefined ? {} : { outputRoot: options.quickOutputRoot }),
        ...(options.reuseLoadedOptions === true ? { reuseLoadedOptions: true } : {}),
      })
    : runKrasWorkflowWatcher(rawAddress, page, {
        onStage1,
        ...(options.quickOutputRoot === undefined ? {} : { outputRoot: options.quickOutputRoot }),
        ...(options.reuseLoadedOptions === true ? { reuseLoadedOptions: true } : {}),
        handleLookupError: async () => {
          while (true) {
            const answer = (
              await terminal.question("조회 실패. 1: 지금 다시 확인 / 2: 무시하고 수동 열람 진행\n")
            ).trim()
            if (answer === "1") return "retry"
            if (answer === "2") return "manual"
            process.stdout.write("1 또는 2를 입력해 주세요.\n")
          }
        },
        handleFloorRoomOptions: async (building, status) => {
          while (true) {
            const prompt =
              status === "empty"
                ? `${building.label}에 층-호명칭 정보가 없습니다.\n1: 건물 전체로 계속 / 2: 다시 로드\n`
                : `${building.label}의 층-호명칭 정보가 4초 동안 바뀌지 않았습니다.\n1: 건물정보 및 층-호명칭 다시 로드 / 2: 층-호명칭 없이 계속\n`
            const answer = (await terminal.question(prompt)).trim()
            if (status === "empty") {
              if (answer === "1") return "continue"
              if (answer === "2") return "retry"
            } else {
              if (answer === "1") return "retry"
              if (answer === "2") return "continue"
            }
            process.stdout.write("1 또는 2를 입력해 주세요.\n")
          }
        },
        chooseBuilding: async (buildingOptions) => {
          while (true) {
            const answer = await terminal.question(
              options.quickOutputRoot === undefined
                ? "몇 번의 건물을 선택할까요? (0: 수동 열람, -1: 주소 입력으로 돌아가기)\n"
                : "몇 번의 건물을 선택할까요? (-1: 수동 열람, /주소: 주소 입력으로 돌아가기)\n",
            )
            const displayIndex = Number(answer.trim())
            if (options.quickOutputRoot !== undefined && displayIndex === -1) return undefined
            if (options.quickOutputRoot === undefined && displayIndex === -1) return "address"
            if (options.quickOutputRoot === undefined && displayIndex === 0) return undefined
            const option = buildingOptions[displayIndex - 2]
            if (Number.isSafeInteger(displayIndex) && option !== undefined) return option.index
            process.stdout.write("건물 번호가 올바르지 않습니다. 목록 번호를 입력해 주세요.\n")
          }
        },
        chooseFloorRoom: async (floorRoomOptions, building) => {
          process.stdout.write(`Stage 2: 층-호명칭 ${floorRoomOptions.length}개 감지\n`)
          floorRoomOptions.forEach((option, index) => {
            if (isKrasStage3OutputComplete(workflowPaths, building, option)) {
              process.stdout.write(`${index + 1}.\x1b[32m${option.label} [출력됨]\x1b[0m\n`)
              return
            }
            process.stdout.write(`${index + 1}.${option.label}\n`)
          })
          while (true) {
            const answer = await terminal.question("몇 번의 층-호명칭을 선택할까요? ")
            const displayIndex = Number(answer.trim())
            const option = floorRoomOptions[displayIndex - 1]
            if (Number.isSafeInteger(displayIndex) && option !== undefined) return option.index
            process.stdout.write("층-호명칭 번호가 올바르지 않습니다. 목록 번호를 입력해 주세요.\n")
          }
        },
      }))
  if (result.awaitingUser) {
    throw new Error(
      `Stage 2 not started: multiple buildings require selection (${result.stage1.buildingOptions.map(({ index, label }) => `${index}:${label}`).join(", ")})`,
    )
  }
  if (result.returnToAddress) return false
  let stage2 = result.stage2
  let propertyNumber: string | undefined
  const buildingAbsent =
    result.stage1.status === "blocked" && result.stage1.reason === "no_building"
  if (stage2 === undefined) {
    if (terminal === undefined) throw new Error("Stage 2 not started: no building found")
    while (true) {
      const answer = (
        await terminal.question(
          "건물이 없습니다. 토지로 열람할까요? (1: 네 / 2: 아니오, 다른 주소 입력)\n",
        )
      ).trim()
      if (answer === "1") {
        stage2 = createKrasManualStage2Result(result.stage1)
        await writeManualStage2ForRoot(result.stage1, options.quickOutputRoot)
        break
      }
      if (answer === "2") {
        process.stdout.write("다른 주소를 입력해 주세요.\n")
        return false
      }
      process.stdout.write("1 또는 2를 입력해 주세요.\n")
    }
  }
  const reusable = stage2.status === "completed"
  if (
    terminal !== undefined &&
    stage2.status === "completed" &&
    options.openViewerOnly !== true &&
    options.quickOutputRoot === undefined
  ) {
    propertyNumber =
      (
        await terminal.question(
          "엑셀 반영용 재산번호(C열)를 입력하세요 (빈 칸: 엑셀 반영 시 PNU 사용)\n",
        )
      ).trim() || undefined
  }
  process.stdout.write("Stage 2: 보안문자를 입력한 후 열람버튼을 눌러주세요.\n")
  // CAPTCHA 자동 처리: 이미지 캡처 -> 터미널 ANSI 렌더링 -> 입력 -> 열람 버튼 자동 클릭
  if (terminal !== undefined) {
    const captchaAnsi = await page.renderCaptchaTerminal()
    if (captchaAnsi) {
      process.stdout.write("\n보안문자 이미지:\n")
      process.stdout.write(`${captchaAnsi}\n`)
      while (true) {
        const captchaText = (await terminal.question("\n보안문자를 입력하세요: ")).trim()
        if (captchaText.length >= 4) {
          const submitted = await page.fillCaptchaAndSubmit(captchaText)
          if (submitted === "success") {
            process.stdout.write("보안문자 입력 및 열람 버튼 클릭 완료\n")
            break
          }
          if (submitted === "need_lookup") {
            process.stdout.write(
              "건물 조회가 필요합니다. 브라우저에서 직접 조회한 뒤 열람 버튼을 눌러주세요.\n",
            )
            await terminal.question("조회 및 열람 완료 후 Enter를 누르세요: ")
            break
          }
          process.stdout.write("자동 제출 실패. 다시 시도해 주세요.\n")
          await new Promise((resolve) => setTimeout(resolve, 500))
          const refreshedCaptchaAnsi = await page.renderCaptchaTerminal()
          if (refreshedCaptchaAnsi)
            process.stdout.write(`\n새 보안문자 이미지:\n${refreshedCaptchaAnsi}\n`)
        } else {
          process.stdout.write("보안문자가 너무 짧습니다. 다시 입력해 주세요.\n")
        }
      }
    } else {
      process.stdout.write(
        "보안문자 이미지를 자동으로 찾을 수 없습니다. 수동으로 입력 후 열람 버튼을 눌러주세요.\n",
      )
      await terminal.question("수동 완료 후 Enter를 누르세요: ")
    }
  }
  process.stdout.write("OZ 뷰어 팝업 대기 중...\n")
  const ozDetected = await page.awaitOzViewer()
  if (!ozDetected) {
    process.stdout.write("OZ Viewer를 감지하지 못했습니다. 60초 내 팝업을 확인해 주세요.\n")
    return reusable
  }
  process.stdout.write("OZ Viewer 감지됨.\n")
  if (stage2.status === "completed" && stage2.building.index > 0) {
    appendDoneBuildingIndex(
      propertyAutoStagePaths(stage2.address, options.quickOutputRoot),
      stage2.building.index,
    )
  }
  if (options.openViewerOnly === true) {
    process.stdout.write("오즈뷰어가 열렸습니다. 창에서 직접 확인해 주세요.\n")
    return reusable
  }
  if (options.quickOutputRoot !== undefined) {
    if (terminal !== undefined) {
      const loaded = await askOzViewerLoaded(terminal)
      if (!loaded) return reusable
    }
    await finishQuickOzWorkflow(
      page,
      stage2.address,
      terminal,
      options.quickOutputRoot,
      buildingAbsent,
    )
    return reusable
  }
  if (terminal !== undefined) {
    const loaded = await askOzViewerLoaded(terminal)
    if (!loaded) return reusable
  }
  let capture = true
  let excel = true
  if (terminal !== undefined) {
    const mode = await askStage3Mode(terminal)
    capture = mode.capture
    excel = mode.excel
  }
  if (capture) process.stdout.write("캡처 시작...\n")
  const stage3Options: {
    capture: boolean
    excel: boolean
    propertyNumber?: string
    buildingAbsent?: boolean
    resolveNameConflict?: (existingPath: string) => Promise<string>
    excelConfig?: KrasExcelTaskConfig
    outputRoot?: string
  } = { capture, excel }
  if (propertyNumber !== undefined) stage3Options.propertyNumber = propertyNumber
  if (buildingAbsent) stage3Options.buildingAbsent = true
  if (options.excelConfig !== undefined) stage3Options.excelConfig = options.excelConfig
  if (terminal !== undefined) {
    stage3Options.resolveNameConflict = async (existingPath: string): Promise<string> => {
      const answer = (
        await terminal.question(
          `${existingPath}\n파일이 이미 있습니다. 이름에 추가할 텍스트를 입력하세요 (예: -재발급, 빈 칸: 덮어쓰기)\n`,
        )
      ).trim()
      return answer
    }
  }
  await runPropertyAutoStage3(stage2.address, undefined, stage3Options)
  if (capture) {
    const stage3 = parseKrasAutoStage3Result(
      JSON.parse(await readFile(propertyAutoStagePaths(stage2.address).stage3Json, "utf8")),
    )
    if (stage3.status !== "completed") {
      process.stdout.write(`Stage 3 실패: ${stage3.reason}. PDF/캡처가 저장되지 않았습니다.\n`)
      return reusable
    }
  }
  process.stdout.write("완료. 다음 주소를 입력해 주세요.\n")
  if (terminal !== undefined) await askCloseOzViewer(terminal, page)
  return reusable
}

export function nextPreviousAddress(
  previousAddress: string | undefined,
  currentAddress: string,
  reusable: boolean,
): string | undefined {
  return reusable ? currentAddress : previousAddress
}

export async function runPropertyAutoPipeline(
  args: readonly string[],
  dependencies: PropertyAutoDependencies = nodeDependencies,
): Promise<number> {
  await dependencies.runWatcher("start")
  return dependencies.runCli(args)
}

async function main(): Promise<void> {
  try {
    const args = process.argv.slice(2)
    if (args[0] === "stage1") {
      if (args.length < 2) throw new Error("Usage: property-auto-runner.ts stage1 <address>")
      const rawAddress = args.slice(1).join(" ")
      const stage1Path = await runPropertyAutoStage1(rawAddress)
      const stage1 = parseKrasAutoStage1Result(await readFile(stage1Path, "utf8").then(JSON.parse))
      if (stage1.status !== "completed") {
        throw new Error(`Stage 2 not started: Stage 1 blocked (${stage1.reason})`)
      }
      if (stage1.buildingOptions.length !== 1) {
        const options = stage1.buildingOptions
          .map(({ index, label }) => `${index}:${label}`)
          .join(", ")
        throw new Error(`Stage 2 not started: multiple buildings require selection (${options})`)
      }
      const stage2Path = await runPropertyAutoStage2(stage1.address)
      const stage3Path = await runPropertyAutoStage3(stage1.address)
      process.stdout.write(`${stage1Path}\n${stage2Path}\n${stage3Path}\n`)
      process.exit(0)
      return
    }
    if (args[0] === "stage2") {
      if (args.length < 2 || args.length > 3) {
        throw new Error("Usage: property-auto-runner.ts stage2 <address> [building-index]")
      }
      const rawAddress = args[1]
      const rawBuildingIndex = args[2]
      if (rawAddress === undefined) {
        throw new Error("Usage: property-auto-runner.ts stage2 <address> [building-index]")
      }
      let buildingIndex: number | undefined
      if (rawBuildingIndex !== undefined) {
        buildingIndex = Number(rawBuildingIndex)
        if (!Number.isSafeInteger(buildingIndex) || buildingIndex < 0) {
          throw new Error("building-index must be a non-negative integer")
        }
      }
      const path = await runPropertyAutoStage2(rawAddress, buildingIndex)
      process.stdout.write(`${path}\n`)
      return
    }
    if (args[0] === "stage3") {
      if (args.length < 2) throw new Error("Usage: property-auto-runner.ts stage3 <address>")
      const path = await runPropertyAutoStage3(args.slice(1).join(" "))
      process.stdout.write(`${path}\n`)
      return
    }
    if (args[0] === "quick") {
      const terminal = createTerminal()
      try {
        await withKrasWorkflowPage(connectOrOpenKrasWorkflowWatcher, async () => undefined)
        while (true) {
          const loginAnswer = (
            await terminal.question(
              "Chrome의 KRAS 페이지에서 로그인해주세요. 로그인이 끝났나요?\n1. 예\n2. 아니오\n선택 > ",
            )
          ).trim()
          if (loginAnswer === "2") {
            process.stdout.write("로그인을 완료한 후 1을 입력해 주세요.\n")
            continue
          }
          if (loginAnswer !== "1") {
            process.stdout.write("1 또는 2를 입력해 주세요.\n")
            continue
          }
          try {
            await withKrasWorkflowPage(connectKrasWorkflowWatcher, async () => undefined)
            break
          } catch {
            process.stdout.write(
              "로그인 후 KRAS 증명서 열람 페이지가 아직 확인되지 않습니다. 다시 확인해 주세요.\n",
            )
          }
        }
        const quickRoot = process.env["KRAS_QUICK_ROOT"] ?? dirname(process.execPath)
        const quickOutputRoot = join(quickRoot, "KRAS")
        const quickTerminal = createAddressReturningTerminal(terminal)
        const quickAddressPrompt =
          "\n주소 입력 또는 작업 선택\n0: 기본 주소(서울특별시 노원구 월계동 392-19)\n주소: 새 주소 조회\n1: 현재 건물 목록에서 다시 선택\n3: OZ 뷰어를 기다린 뒤 직전 주소로 저장\n작업 중 /주소: 이 화면으로 돌아오기\nEnter: 종료\n입력 > "
        let address =
          args.length >= 2
            ? args.slice(1).join(" ")
            : (await terminal.question(quickAddressPrompt)).trim()
        let previousAddress: string | undefined
        while (address !== "") {
          if (address === "/주소") {
            address = (await terminal.question(quickAddressPrompt)).trim()
            continue
          }
          if (address === "0") address = "서울특별시 노원구 월계동 392-19"
          if (address === "1") {
            if (previousAddress === undefined) {
              process.stdout.write("직전 주소가 없습니다. 먼저 주소를 입력해 주세요.\n")
            } else {
              try {
                await runPropertyKrasWorkflow(previousAddress, quickTerminal, {
                  reuseLoadedOptions: true,
                  quickOutputRoot,
                })
              } catch (error: unknown) {
                if (error instanceof ReturnToAddressError) {
                  address = (await terminal.question(quickAddressPrompt)).trim()
                  continue
                }
                process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
              }
            }
            address = (await terminal.question(quickAddressPrompt)).trim()
            continue
          }
          if (address === "3") {
            try {
              const targetAddress =
                previousAddress ??
                (await quickTerminal.question("저장할 주소를 입력하세요 > ")).trim()
              if (targetAddress === "") {
                process.stdout.write("주소가 비어 있습니다. 먼저 주소를 입력해 주세요.\n")
              } else {
                const paths = propertyAutoStagePaths(
                  parseParcelAddress(targetAddress),
                  quickOutputRoot,
                )
                if (!existsSync(paths.stage2Json)) {
                  const stage1: KrasAutoStage1Result = {
                    version: 1,
                    stage: 1,
                    status: "blocked",
                    address: normalizeAddress(targetAddress),
                    reason: "no_building",
                    hasBuilding: false,
                    buildingOptions: [],
                  }
                  await writeKrasAutoStageResult(paths.stage1Json, stage1)
                  await writeKrasAutoStageResult(
                    paths.stage2Json,
                    createKrasManualStage2Result(stage1),
                  )
                  process.stdout.write("수동 열람 모드로 stage2를 생성했습니다.\n")
                }
                await withKrasWorkflowPage(connectKrasWorkflowWatcher, async (page) => {
                  process.stdout.write("OZ 뷰어 팝업 대기 중...\n")
                  const ozDetected = await page.awaitOzViewer()
                  if (!ozDetected) {
                    process.stdout.write(
                      "OZ Viewer를 감지하지 못했습니다. 60초 내 팝업을 확인해 주세요.\n",
                    )
                  } else {
                    await finishQuickOzWorkflow(page, targetAddress, quickTerminal, quickOutputRoot)
                  }
                })
              }
            } catch (error: unknown) {
              if (error instanceof ReturnToAddressError) {
                address = (await terminal.question(quickAddressPrompt)).trim()
                continue
              }
              process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
            }
            address = (await terminal.question(quickAddressPrompt)).trim()
            continue
          }
          try {
            const reusable = await runPropertyKrasWorkflow(address, quickTerminal, {
              quickOutputRoot,
            })
            previousAddress = nextPreviousAddress(previousAddress, address, reusable)
          } catch (error: unknown) {
            if (error instanceof ReturnToAddressError) {
              address = (await terminal.question(quickAddressPrompt)).trim()
              continue
            }
            process.stderr.write(
              `작업 실패: ${error instanceof Error ? error.message : String(error)}\n`,
            )
            process.stdout.write("다시 주소를 입력하거나 다음 작업을 선택해 주세요.\n")
          }
          address = (await terminal.question(quickAddressPrompt)).trim()
        }
        process.stdout.write("종료합니다.\n")
      } catch (error: unknown) {
        process.stderr.write(
          `kras_quick 오류: ${error instanceof Error ? error.message : String(error)}\n`,
        )
        await terminal.question("오류 내용을 확인한 후 Enter를 누르면 종료합니다.")
        process.exitCode = 1
      } finally {
        terminal.close()
      }
      return
    }
    process.exitCode = await runPropertyAutoPipeline(args)
  } catch (error: unknown) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
    process.exitCode = 1
  }
}

if (
  process.argv[0] === "bun" ||
  process.argv[1] === process.execPath ||
  process.argv[1]?.endsWith("property-auto-runner.ts")
) {
  void main().then(() => process.exit(process.exitCode ?? 0))
}
