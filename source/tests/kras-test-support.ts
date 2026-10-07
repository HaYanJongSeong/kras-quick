import { resolve } from "node:path"
import * as ts from "typescript/unstable/ast"
import { API } from "typescript/unstable/sync"
import type { KrasEvaluationRequest, KrasEvaluationResult } from "../scripts/kras-domain.ts"
import type { CdpConnect, KrasBrowser } from "../scripts/kras-lookup.ts"
import type { KrasPage } from "../scripts/kras-page.ts"
import type {
  PropertyWatcherBrowser,
  PropertyWatcherPage,
} from "../scripts/property-watcher-page.ts"
import type {
  PropertyPager,
  PropertyRow,
  PropertyWatcherSelectionHandle,
} from "../scripts/property-watcher-payload.ts"
import { parsePropertyPager } from "../scripts/property-watcher-payload.ts"

export const KRAS_TARGET_URL = "https://www.kras.go.kr/kras/cert/certView.do"

export type FakeKrasState = {
  readonly connectCalls: Array<{
    readonly endpoint: string
    readonly options: { readonly noDefaults: boolean; readonly timeout: number }
  }>
  readonly darkModePayloads: string[]
  readonly evaluationRequests: KrasEvaluationRequest[]
  readonly evaluationSources: string[]
  closeCalls: number
}

export function createFakeKrasState(): FakeKrasState {
  return {
    connectCalls: [],
    darkModePayloads: [],
    evaluationRequests: [],
    evaluationSources: [],
    closeCalls: 0,
  }
}

export function createFakeKrasPage(
  url: string,
  result: KrasEvaluationResult,
  state: FakeKrasState,
): KrasPage {
  return {
    url: () => url,
    evaluate: async (pageFunction, request) => {
      state.evaluationRequests.push(request)
      state.evaluationSources.push(pageFunction.toString())
      return result
    },
    applyDarkMode: async (pageFunction, css) => {
      state.darkModePayloads.push(css)
      state.evaluationSources.push(pageFunction.toString())
    },
  }
}

export function createFakeCdpConnection(
  pageGroups: readonly (readonly KrasPage[])[],
  state: FakeKrasState,
): CdpConnect {
  return async (endpoint, options) => {
    state.connectCalls.push({ endpoint, options })
    const browser: KrasBrowser = {
      contexts: () => pageGroups.map((pages) => ({ pages: () => pages })),
      close: async () => {
        state.closeCalls += 1
      },
    }
    return browser
  }
}

function propertyNameText(name: ts.PropertyName): string | undefined {
  return ts.isIdentifier(name) || ts.isStringLiteral(name) ? name.text : undefined
}

function returnsKind(node: ts.Node, kind: string): node is ts.ReturnStatement {
  if (!ts.isReturnStatement(node)) return false
  const expression = node.expression
  if (expression === undefined || !ts.isObjectLiteralExpression(expression)) return false
  return expression.properties.some(
    (property) =>
      ts.isPropertyAssignment(property) &&
      propertyNameText(property.name) === "kind" &&
      ts.isStringLiteral(property.initializer) &&
      property.initializer.text === kind,
  )
}

type LookupBusinessAction =
  | "building_read"
  | "button_click"
  | "button_discovery"
  | "cert_view"
  | "lookup_timeout"

export function fillOnlyReturnPrecedesLookupBusinessActions(): boolean {
  const configPath = resolve("tsconfig.kras.json")
  const pagePath = resolve("scripts/kras-page.ts")
  const api = new API()
  try {
    const snapshot = api.updateSnapshot({ openProjects: [configPath], openFiles: [pagePath] })
    try {
      const project = snapshot.getProject(configPath)
      const sourceFile = project?.program.getSourceFile(pagePath)
      if (sourceFile === undefined) return false
      let lookupFunction: ts.FunctionDeclaration | undefined
      sourceFile.forEachChild((node) => {
        if (ts.isFunctionDeclaration(node) && node.name?.text === "lookupEvaluation") {
          lookupFunction = node
        }
      })
      if (lookupFunction === undefined) return false
      let evaluationErrorReturnPresent = false
      let fillOnlyReturnEnd: number | undefined
      const actionStarts = new Map<LookupBusinessAction, number>()
      const recordAction = (action: LookupBusinessAction, node: ts.Node): void => {
        const start = node.getStart(sourceFile)
        const current = actionStarts.get(action)
        if (current === undefined || start < current) actionStarts.set(action, start)
      }
      const visit = (node: ts.Node): void => {
        if (returnsKind(node, "evaluation_error")) evaluationErrorReturnPresent = true
        if (
          ts.isCaseClause(node) &&
          ts.isStringLiteral(node.expression) &&
          node.expression.text === "fill_only"
        ) {
          const fillOnlyReturn = node.statements.find((statement) =>
            returnsKind(statement, "fill_only"),
          )
          if (fillOnlyReturn !== undefined) fillOnlyReturnEnd = fillOnlyReturn.end
        }
        if (ts.isIdentifier(node) && node.text === "certView") recordAction("cert_view", node)
        if (ts.isCallExpression(node)) {
          if (ts.isPropertyAccessExpression(node.expression)) {
            if (node.expression.name.text === "querySelectorAll") {
              recordAction("button_discovery", node)
            }
            if (node.expression.name.text === "click") recordAction("button_click", node)
          }
          if (
            ts.isIdentifier(node.expression) &&
            node.expression.text === "requiredSelect" &&
            node.arguments.some(
              (argument) => ts.isStringLiteral(argument) && argument.text === "#bldgSeCd",
            )
          ) {
            recordAction("building_read", node)
          }
        }
        if (
          ts.isNewExpression(node) &&
          ts.isIdentifier(node.expression) &&
          node.expression.text === "Promise" &&
          node.typeArguments?.some((argument) => argument.kind === ts.SyntaxKind.NeverKeyword) ===
            true
        ) {
          recordAction("lookup_timeout", node)
        }
        node.forEachChild(visit)
      }
      visit(lookupFunction)
      const returnEnd = fillOnlyReturnEnd
      return (
        evaluationErrorReturnPresent &&
        returnEnd !== undefined &&
        actionStarts.size === 5 &&
        [...actionStarts.values()].every((start) => start > returnEnd)
      )
    } finally {
      snapshot.dispose()
    }
  } finally {
    api.close()
  }
}

export type FakePropertyWatcherState = {
  disposeCalls: number
  installCalls: number
}

export type FakePropertyWatcherPage = {
  readonly emit: (row: PropertyRow) => void
  readonly failed: Promise<never>
  readonly page: PropertyWatcherPage
  readonly setUrl: (url: string) => void
  readonly state: FakePropertyWatcherState
}

export function createFakePropertyWatcherPage(url: string): FakePropertyWatcherPage {
  let currentUrl = url
  let onRow:
    | ((
        row: PropertyRow,
        selection: PropertyWatcherSelectionHandle,
        pager: PropertyPager,
      ) => void | Promise<void>)
    | undefined
  const failed = new Promise<never>(() => undefined)
  const state: FakePropertyWatcherState = {
    disposeCalls: 0,
    installCalls: 0,
  }
  const page: PropertyWatcherPage = {
    url: () => currentUrl,
    installPropertyWatcherChannel: async (listener) => {
      state.installCalls += 1
      onRow = listener
      return {
        failed,
        updateBuildingStatus: async () => "applied",
        dispose: async () => {
          state.disposeCalls += 1
        },
      }
    },
  }
  return {
    emit: (row) => {
      if (onRow === undefined) throw new Error("watcher channel was not installed")
      const pager = parsePropertyPager("8/16")
      if (pager === undefined) throw new Error("invalid fake watcher pager")
      void onRow(
        row,
        {
          contextId: 1,
          contextUniqueId: "context-1",
          generation: 0,
          selectionId: 1,
        },
        pager,
      )
    },
    failed,
    page,
    setUrl: (value) => (currentUrl = value),
    state,
  }
}

export function createFakePropertyWatcherBrowser(
  pageGroups: readonly (readonly PropertyWatcherPage[])[],
): PropertyWatcherBrowser {
  return { contexts: () => pageGroups.map((pages) => ({ pages: () => pages })) }
}
