import assert from "node:assert/strict"
import { describe, it } from "node:test"
import { darkModeEvaluation, KRAS_DARK_MODE_CSS } from "../scripts/kras-dark.ts"
import { runKrasDarkMode, runKrasLookup } from "../scripts/kras-lookup.ts"
import { dark_mode } from "../tools/kras.ts"
import {
  createFakeCdpConnection,
  createFakeKrasPage,
  createFakeKrasState,
  KRAS_TARGET_URL,
} from "./kras-test-support.ts"

describe("kras_dark_mode", () => {
  it("defines the complete soft neutral theme without changing KRAS geometry", () => {
    // Given
    const css = KRAS_DARK_MODE_CSS
    const compactCss = css.replace(/\s+/g, "")

    // When / Then
    assert.match(compactCss, /html,body\{[^}]*var\(--kras-page\)/)
    assert.match(
      compactCss,
      /(?:main,)?section,article,header,footer,nav,form,fieldset,table,tbody,tr\{[^}]*var\(--kras-panel\)/,
    )
    assert.match(
      compactCss,
      /th,\[role=["']rowheader["']\],\[role=["']columnheader["']\]\{[^}]*var\(--kras-raised\)/,
    )
    assert.match(compactCss, /td\{[^}]*var\(--kras-panel\)/)
    assert.match(compactCss, /input,select,textarea,button\{[^}]*var\(--kras-control\)/)
    assert.match(
      compactCss,
      /caption,\.radio__name,\.con_tit\{[^}]*color:var\(--kras-text\)!important/,
    )
    assert.match(compactCss, /\.con_tit\{[^}]*color:var\(--kras-text\)!important/)
    assert.match(compactCss, /\.surely\{[^}]*color:var\(--kras-muted\)!important/)
    assert.match(compactCss, /::placeholder\{[^}]*var\(--kras-muted\)[^}]*opacity:1/)
    assert.match(compactCss, /:disabled[^}]*\{[^}]*var\(--kras-muted\)[^}]*opacity:1/)
    assert.match(compactCss, /border-color:var\(--kras-border\)/)
    assert.match(compactCss, /:focus-visible\{[^}]*var\(--kras-focus\)/)
    assert.doesNotMatch(css, /(?:backdrop-)?filter\s*:/i)
    assert.doesNotMatch(css, /@media|\b(?:img|canvas|video|iframe)\b/i)
    assert.doesNotMatch(css, /body\s+div/i)
    assert.doesNotMatch(
      css,
      /(?:^|[;{])\s*(?:display|position|(?:min-|max-)?(?:inline-|block-)?(?:width|height)|margin(?:-[a-z]+)?|padding(?:-[a-z]+)?|font(?:-[a-z]+)?|line-height)\s*:/im,
    )
  })

  it("allows only color and focus declarations", () => {
    // Given
    const allowedProperties = new Set([
      "accent-color",
      "background-color",
      "border-color",
      "color",
      "opacity",
      "outline",
      "outline-offset",
    ])

    // When
    const declarationProperties = Array.from(
      KRAS_DARK_MODE_CSS.matchAll(/(?:^|[;{])\s*([-\w]+)\s*:/g),
      (match) => match[1],
    ).filter((property): property is string => property !== undefined)
    const rejectedProperties = declarationProperties.filter(
      (property) => !property.startsWith("--kras-") && !allowedProperties.has(property),
    )
    const opacityValues = Array.from(
      KRAS_DARK_MODE_CSS.matchAll(/opacity\s*:\s*([^;]+);/g),
      (match) => match[1]?.trim(),
    ).filter((value): value is string => value !== undefined)

    // Then
    assert.deepEqual(rejectedProperties, [])
    assert.deepEqual(new Set(opacityValues), new Set(["1 !important"]))
  })

  it("documents one distinct raw color for every runtime theme token", () => {
    // Given
    const expectedColors = [
      "#202124",
      "#292b2f",
      "#33363b",
      "#303338",
      "#e8eaed",
      "#b6bbc2",
      "#555b63",
      "#9fc5e8",
      "#a8c7fa",
    ]

    // When
    const rawColors = KRAS_DARK_MODE_CSS.match(/#[\da-f]{6}/gi) ?? []

    // Then
    assert.deepEqual(rawColors, expectedColors)
    assert.equal(new Set(rawColors).size, expectedColors.length)
  })

  it("keeps dark-mode injection serialization-safe and idempotent", () => {
    // Given
    const source = darkModeEvaluation.toString()

    // When / Then
    assert.equal(darkModeEvaluation.length, 1)
    assert.match(source, /getElementById\(["']opencode-dark-mode["']\)/)
    assert.match(source, /style\.textContent\s*=\s*css/)
    assert.match(source, /if\s*\(existing\s*===\s*null\)\s*document\.head\.append\(style\)/)
    assert.doesNotMatch(source, /KRAS_DARK_MODE_CSS/)
  })

  it("applies dark mode in one evaluation and returns the exact line", async () => {
    // Given
    const state = createFakeKrasState()
    const connect = createFakeCdpConnection(
      [[createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: false }, state)]],
      state,
    )

    // When
    const result = await runKrasDarkMode(connect)

    // Then
    assert.equal(result, "다크 모드 적용.")
    assert.equal(state.evaluationSources.length, 1)
    assert.deepEqual(state.darkModePayloads, [KRAS_DARK_MODE_CSS])
    assert.equal(state.closeCalls, 1)
  })

  it("passes the same canonical payload through combined lookup dark mode", async () => {
    // Given
    const state = createFakeKrasState()
    const connect = createFakeCdpConnection(
      [[createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: false }, state)]],
      state,
    )

    // When
    await runKrasLookup(connect, "서울특별시 종로구 청운동 1", true)

    // Then
    assert.equal(state.evaluationRequests[0]?.mode, "lookup")
    assert.equal(state.evaluationRequests[0]?.darkModeCss, KRAS_DARK_MODE_CSS)
  })

  it("exposes a no-argument dark-mode tool", () => {
    // Given
    const args = dark_mode.args

    // When / Then
    assert.deepEqual(Object.keys(args), [])
    assert.match(dark_mode.description, /dark mode/i)
    assert.equal(typeof dark_mode.execute, "function")
  })
})
