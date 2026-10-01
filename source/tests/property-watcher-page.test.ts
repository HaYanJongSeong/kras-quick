import assert from "node:assert/strict"
import { describe, it } from "node:test"

import {
  discoverPropertyWatcherTargets,
  installPropertyWatcherPage,
  isKrasWatcherTargetUrl,
  isScpTargetUrl,
  PropertyWatcherError,
} from "../scripts/property-watcher-page.ts"
import {
  createFakePropertyWatcherBrowser,
  createFakePropertyWatcherPage,
} from "./kras-test-support.ts"

const SCP_URL = "http://scpweb.softgraphy.biz/board"
const KRAS_URL = "https://www.kras.go.kr/kras/cert/certView.do"
const ROW = ["1", "공유재산", "토지", "강서구", "개화동", "일반", "주소", "미처리"] as const

describe("property watcher target discovery", () => {
  it("matches only the exact SCP and KRAS origins and paths while ignoring query and hash", () => {
    // Given
    const scpUrls = [
      SCP_URL,
      `${SCP_URL}?page=2#selected`,
      "http://scpweb.softgraphy.biz.evil.test/board",
      "http://scpweb.softgraphy.biz/board/extra",
      "https://scpweb.softgraphy.biz/board",
      "not a url http://scpweb.softgraphy.biz/board",
    ]
    const krasUrls = [
      KRAS_URL,
      `${KRAS_URL}?parcel=1#result`,
      "https://www.kras.go.kr.evil.test/kras/cert/certView.do",
      "https://www.kras.go.kr/other?next=/kras/cert/certView.do",
      "http://www.kras.go.kr/kras/cert/certView.do",
      "not a url https://www.kras.go.kr/kras/cert/certView.do",
    ]

    // When / Then
    assert.deepEqual(scpUrls.map(isScpTargetUrl), [true, true, false, false, false, false])
    assert.deepEqual(krasUrls.map(isKrasWatcherTargetUrl), [true, true, false, false, false, false])
  })

  it("discovers one target of each kind across existing contexts", () => {
    // Given
    const scp = createFakePropertyWatcherPage(`${SCP_URL}?page=3`)
    const kras = createFakePropertyWatcherPage(`${KRAS_URL}#result`)
    const other = createFakePropertyWatcherPage("https://example.test/board")

    // When
    const targets = discoverPropertyWatcherTargets(
      createFakePropertyWatcherBrowser([[other.page, scp.page], [kras.page]]),
    )

    // Then
    assert.equal(targets.scp, scp.page)
    assert.equal(targets.kras, kras.page)
  })

  it("rejects missing or duplicate targets before installing page resources", async () => {
    // Given
    const firstScp = createFakePropertyWatcherPage(SCP_URL)
    const secondScp = createFakePropertyWatcherPage(`${SCP_URL}?duplicate=true`)
    const kras = createFakePropertyWatcherPage(KRAS_URL)

    // When / Then
    await assert.rejects(
      installPropertyWatcherPage(
        createFakePropertyWatcherBrowser([[firstScp.page, secondScp.page, kras.page]]),
        async () => undefined,
      ),
      (error: unknown) =>
        error instanceof PropertyWatcherError && error.code === "TARGET_COUNT_INVALID",
    )
    assert.equal(firstScp.state.installCalls, 0)
    assert.equal(secondScp.state.installCalls, 0)
    assert.equal(kras.state.installCalls, 0)
  })

  it("forwards only the selected SCP channel and preserves its fatal completion", async () => {
    // Given
    const scp = createFakePropertyWatcherPage(SCP_URL)
    const kras = createFakePropertyWatcherPage(KRAS_URL)
    const received: unknown[] = []

    // When
    const watcher = await installPropertyWatcherPage(
      createFakePropertyWatcherBrowser([[scp.page, kras.page]]),
      async (row) => {
        received.push(row)
      },
    )
    scp.emit(ROW)

    // Then
    assert.deepEqual(received, [ROW])
    assert.equal(scp.state.installCalls, 1)
    assert.equal(kras.state.installCalls, 0)
    assert.equal(watcher.failed, scp.failed)
    await watcher.dispose()
    assert.equal(scp.state.disposeCalls, 1)
  })
})
