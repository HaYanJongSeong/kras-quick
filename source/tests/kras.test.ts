import assert from "node:assert/strict"
import { describe, it, type TestContext } from "node:test"
import { KRAS_DARK_MODE_CSS } from "../scripts/kras-dark.ts"
import {
  addressHasLegalRi,
  dependentListReady,
  KRAS_EVALUATION_ERROR_CODE,
  ledgerLabelForAddress,
  longestAddressOptionLabel,
  normalizeAddress,
} from "../scripts/kras-domain.ts"
import { isKrasTargetUrl, runKrasLookup } from "../scripts/kras-lookup.ts"
import {
  fillKrasAddress,
  KrasFillError,
  type KrasFillPage,
  KrasLookupError,
  lookupEvaluation,
  lookupKrasBuilding,
} from "../scripts/kras-page.ts"
import { parseParcelAddress } from "../scripts/property-address.ts"
import { lookup } from "../tools/kras.ts"
import {
  drainKrasEvaluationMicrotasks,
  installKrasRefreshFixture,
} from "./kras-page-test-support.ts"
import {
  createFakeCdpConnection,
  createFakeKrasPage,
  createFakeKrasState,
  fillOnlyReturnPrecedesLookupBusinessActions,
  KRAS_TARGET_URL,
} from "./kras-test-support.ts"

describe("KRAS address routing", () => {
  it("normalizes whitespace-heavy Seoul district-first input", () => {
    // Given
    const address = "  강서구\t개화동   255-1  "

    // When
    const normalized = normalizeAddress(address)

    // Then
    assert.equal(normalized, "서울특별시 강서구 개화동 255-1")
  })

  it("keeps complete Seoul addresses geographically unchanged", () => {
    assert.equal(
      normalizeAddress("서울특별시  강서구 개화동 255-1"),
      "서울특별시 강서구 개화동 255-1",
    )
  })

  it("keeps fully qualified non-Seoul addresses geographically unchanged", () => {
    assert.equal(
      normalizeAddress("충청북도  제천시 백운면 평동리 123-4"),
      "충청북도 제천시 백운면 평동리 123-4",
    )
  })

  it("keeps non-Seoul legal-ri addresses unchanged", () => {
    assert.equal(
      normalizeAddress("전라남도  담양군 무정면 봉안리 12-3"),
      "전라남도 담양군 무정면 봉안리 12-3",
    )
  })

  it("does not infer Seoul when the first token does not end in gu", () => {
    assert.equal(normalizeAddress("강서 동 255-1"), "강서 동 255-1")
  })

  it("is idempotent", () => {
    const normalized = normalizeAddress("  강서구\t개화동 255-1 ")
    assert.equal(normalizeAddress(normalized), normalized)
  })

  it("rejects a stale same-named target until a changed parent refreshes", () => {
    // Given
    const state = { mutationObserved: false, parentChanged: true, targetPresent: true }

    // When
    const ready = dependentListReady(state)

    // Then
    assert.equal(ready, false)
  })

  it("accepts the target after a changed parent produces a list mutation", () => {
    // Given
    const state = { mutationObserved: true, parentChanged: true, targetPresent: true }

    // When
    const ready = dependentListReady(state)

    // Then
    assert.equal(ready, true)
  })

  it("accepts an existing target without refresh when the parent is unchanged", () => {
    // Given
    const state = { mutationObserved: false, parentChanged: false, targetPresent: true }

    // When
    const ready = dependentListReady(state)

    // Then
    assert.equal(ready, true)
  })

  it("ignores stale non-placeholder options from a previous region", () => {
    // Given
    const address = "서울특별시 종로구 청운동 1"
    const labels = ["선택", "제천시", "백운면"]

    // When
    const match = longestAddressOptionLabel(address, labels)

    // Then
    assert.equal(match, undefined)
  })

  it("chooses the longest option label contained in the requested address", () => {
    // Given
    const address = "충청북도 제천시 백운면 평동리 123-4"
    const labels = ["평동", "평동리", "선택"]

    // When
    const match = longestAddressOptionLabel(address, labels)

    // Then
    assert.equal(match, "평동리")
  })

  it("skips ri routing when the legal-dong token does not end in ri", () => {
    // Given
    const address = "서울특별시 종로구 청운동 1"

    // When
    const hasRi = addressHasLegalRi(address)

    // Then
    assert.equal(hasRi, false)
  })

  it("requires ri routing when the legal-dong token ends in ri", () => {
    // Given
    const address = "충청북도 제천시 백운면 평동리 123-4"

    // When
    const hasRi = addressHasLegalRi(address)

    // Then
    assert.equal(hasRi, true)
  })

  it("uses the exact closed land ledger label", () => {
    // Given
    const address = "서울특별시 종로구 청운동 폐쇄 1"

    // When
    const label = ledgerLabelForAddress(address)

    // Then
    assert.equal(label, "토지(폐쇄)")
  })

  it("uses the exact closed forest ledger label", () => {
    // Given
    const address = "충청북도 제천시 백운면 평동리 임야 폐쇄 산 123-4"

    // When
    const label = ledgerLabelForAddress(address)

    // Then
    assert.equal(label, "임야(폐쇄)")
  })
})

describe("kras_lookup", () => {
  it("accepts only the exact KRAS origin and certificate pathname", () => {
    // Given / When
    const valid = isKrasTargetUrl(KRAS_TARGET_URL)

    // Then
    assert.equal(valid, true)
  })

  it("rejects path lookalikes in host, query, and fragment", () => {
    // Given
    const lookalikes = [
      "https://www.kras.go.kr.evil.test/kras/cert/certView.do",
      "https://www.kras.go.kr/other?next=/kras/cert/certView.do",
      "https://www.kras.go.kr/other#/kras/cert/certView.do",
      "not a url /kras/cert/certView.do",
    ]

    // When
    const results = lookalikes.map(isKrasTargetUrl)

    // Then
    assert.deepEqual(results, [false, false, false, false])
  })

  it("serializes direct lexical certView access without a Window property", () => {
    // Given
    const pageFunction = lookupEvaluation

    // When
    const source = pageFunction.toString()

    // Then
    assert.match(source, /typeof certView === ["']undefined["']/)
    assert.doesNotMatch(source, /window\.certView/)
    assert.match(source, /new MutationObserver/)
    assert.match(source, /observer\.disconnect\(\)/)
  })

  it("returns from fill_only before every lookup business action", () => {
    // Given / When
    const isSafe = fillOnlyReturnPrecedesLookupBusinessActions()

    // Then
    assert.equal(isSafe, true)
  })

  it("keeps the serialized evaluator blocked when the first mutation retains a stale target", async (context: TestContext) => {
    // Given
    const fixture = installKrasRefreshFixture(context)
    const evaluation = fixture.start()

    // When
    fixture.emitStaleDistrictBatch()
    fixture.runStabilizationTimers()
    await drainKrasEvaluationMicrotasks()

    // Then
    assert.deepEqual(fixture.districtOptionValues(), ["", "stale-district"])
    assert.deepEqual(fixture.districtAssignedValues(), [])
    assert.deepEqual(fixture.pendingTimerDelays(), [4000])

    fixture.emitFreshDistrictBatch()
    fixture.runStabilizationTimers()
    assert.deepEqual(await evaluation, { kind: "fill_only" })
  })

  it("resets stabilization across mutation batches and resolves with only the fresh target", async (context: TestContext) => {
    // Given
    const fixture = installKrasRefreshFixture(context)
    const evaluation = fixture.start()

    // When
    fixture.emitStaleDistrictBatch()
    const firstTimerIds = fixture.pendingStabilizationTimerIds()
    fixture.emitFreshDistrictBatch()
    const secondTimerIds = fixture.pendingStabilizationTimerIds()

    // Then
    assert.deepEqual(fixture.districtOptionValues(), ["", "fresh-district"])
    assert.equal(firstTimerIds.length, 1)
    assert.equal(secondTimerIds.length, 1)
    assert.notDeepEqual(secondTimerIds, firstTimerIds)

    fixture.runStabilizationTimers()
    assert.deepEqual(await evaluation, { kind: "fill_only" })
    assert.deepEqual(fixture.districtAssignedValues(), [])
    assert.deepEqual(fixture.pendingTimerDelays(), [])
    assert.equal(fixture.activeObserverCount(), 0)
    fixture.emitStaleDistrictBatch()
    assert.deepEqual(fixture.pendingTimerDelays(), [])
  })

  it("ignores a later placeholder mutation after no-ri selection completes", async (context: TestContext) => {
    // Given
    const fixture = installKrasRefreshFixture(context, "placeholder_only")
    const evaluation = fixture.start()
    await drainKrasEvaluationMicrotasks()

    // When
    fixture.emitPlaceholderBatch()

    // Then
    assert.deepEqual(await evaluation, { kind: "fill_only" })
    fixture.runStabilizationTimers()
    assert.deepEqual(fixture.pendingTimerDelays(), [])
    assert.equal(fixture.activeObserverCount(), 0)
  })

  it("accepts an unchanged placeholder-only ri list without waiting for a mutation", async (context: TestContext) => {
    // Given
    const fixture = installKrasRefreshFixture(context, "placeholder_only")
    const evaluation = fixture.start()
    await drainKrasEvaluationMicrotasks()

    // When
    fixture.runRefreshTimeout()

    // Then
    assert.deepEqual(await evaluation, { kind: "fill_only" })
    assert.deepEqual(fixture.pendingTimerDelays(), [])
    assert.equal(fixture.activeObserverCount(), 0)
  })

  it("returns evaluation_error and clears refresh resources when the expected timeout occurs", async (context: TestContext) => {
    // Given
    const fixture = installKrasRefreshFixture(context)
    const evaluation = fixture.start()
    fixture.emitStaleDistrictBatch()
    assert.deepEqual(fixture.pendingTimerDelays(), [100, 4000])

    // When
    fixture.runRefreshTimeout()

    // Then
    assert.deepEqual(await evaluation, {
      kind: "evaluation_error",
      code: KRAS_EVALUATION_ERROR_CODE,
    })
    assert.deepEqual(fixture.pendingTimerDelays(), [])
    assert.equal(fixture.activeObserverCount(), 0)
    fixture.runStabilizationTimers()
    assert.deepEqual(fixture.pendingTimerDelays(), [])
  })

  it("fills an already-selected page with an exact fill_only request and no lifecycle calls", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "fill_only" }, state)
    const address = parseParcelAddress(" 서울특별시  강서구 개화동 255-1 ")

    // When
    await fillKrasAddress(page, address)

    // Then
    assert.deepEqual(state.evaluationRequests, [
      {
        mode: "fill_only",
        address: "서울특별시 강서구 개화동 255-1",
        darkMode: false,
        darkModeCss: KRAS_DARK_MODE_CSS,
        hasLegalRi: false,
        ledgerLabel: "토지",
      },
    ])
    assert.deepEqual(state.connectCalls, [])
    assert.equal(state.closeCalls, 0)
  })

  it("rejects a page that navigated away before evaluate without exposing the address", async () => {
    // Given
    let evaluateCalls = 0
    let currentUrl = KRAS_TARGET_URL
    const page: KrasFillPage = {
      url: () => currentUrl,
      evaluate: async () => {
        evaluateCalls += 1
        return { kind: "fill_only" }
      },
    }
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")
    currentUrl = "https://example.test/kras/cert/certView.do"

    // When / Then
    await assert.rejects(
      fillKrasAddress(page, address),
      (error: unknown) => error instanceof Error && error.name === "KrasFillTargetError",
    )
    assert.equal(evaluateCalls, 0)
  })

  it("allows query and hash changes on the exact KRAS target at use time", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(
      `${KRAS_TARGET_URL}?session=changed#certificate`,
      { kind: "fill_only" },
      state,
    )
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When
    await fillKrasAddress(page, address)

    // Then
    assert.equal(state.evaluationRequests.length, 1)
  })

  it("converts only an evaluation_error result to a recoverable fill error", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(
      KRAS_TARGET_URL,
      { kind: "evaluation_error", code: KRAS_EVALUATION_ERROR_CODE },
      state,
    )
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(fillKrasAddress(page, address), KrasFillError)
  })

  for (const [hasBuilding, expectedStatus] of [
    [true, "present"],
    [false, "absent"],
  ] as const) {
    it(`maps lookup hasBuilding=${hasBuilding} to ${expectedStatus}`, async () => {
      // Given
      const state = createFakeKrasState()
      const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding }, state)
      const address = parseParcelAddress(" 서울특별시  강서구  개화동 255-1 ")

      // When
      const status = await lookupKrasBuilding(page, address)

      // Then
      assert.equal(status, expectedStatus)
      assert.deepEqual(state.evaluationRequests, [
        {
          mode: "lookup",
          address: "서울특별시 강서구 개화동 255-1",
          darkMode: false,
          darkModeCss: KRAS_DARK_MODE_CSS,
          hasLegalRi: false,
          ledgerLabel: "토지",
        },
      ])
    })
  }

  it("converts an evaluation_error result to a recoverable lookup error", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(
      KRAS_TARGET_URL,
      { kind: "evaluation_error", code: KRAS_EVALUATION_ERROR_CODE },
      state,
    )
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(lookupKrasBuilding(page, address), KrasLookupError)
  })

  it("keeps an impossible fill-only lookup result fatal", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "fill_only" }, state)
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(
      lookupKrasBuilding(page, address),
      (error: unknown) => error instanceof Error && error.name === "KrasEvaluationResultError",
    )
  })

  it("propagates a TypeError from page evaluation unchanged as fatal", async () => {
    // Given
    const transportFailure = new TypeError("evaluation transport failed")
    const page: KrasFillPage = {
      url: () => KRAS_TARGET_URL,
      evaluate: async () => {
        throw transportFailure
      },
    }
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(
      fillKrasAddress(page, address),
      (error: unknown) => error === transportFailure,
    )
  })

  it("propagates a closed-page evaluation failure unchanged as fatal", async () => {
    // Given
    const closedPageFailure = new Error("page has been closed")
    const page: KrasFillPage = {
      url: () => KRAS_TARGET_URL,
      evaluate: async () => {
        throw closedPageFailure
      },
    }
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(
      fillKrasAddress(page, address),
      (error: unknown) => error === closedPageFailure,
    )
  })

  it("keeps an impossible fill result fatal instead of making it retryable", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: false }, state)
    const address = parseParcelAddress("서울특별시 강서구 개화동 255-1")

    // When / Then
    await assert.rejects(
      fillKrasAddress(page, address),
      (error: unknown) => error instanceof Error && error.name === "KrasEvaluationResultError",
    )
  })

  it("handles an evaluation_error lookup result as a host error and still closes once", async () => {
    // Given
    const state = createFakeKrasState()
    const connect = createFakeCdpConnection(
      [
        [
          createFakeKrasPage(
            KRAS_TARGET_URL,
            { kind: "evaluation_error", code: KRAS_EVALUATION_ERROR_CODE },
            state,
          ),
        ],
      ],
      state,
    )

    // When / Then
    await assert.rejects(
      runKrasLookup(connect, "서울특별시 종로구 청운동 1", false),
      (error: unknown) => error instanceof Error && error.name === "KrasPageError",
    )
    assert.equal(state.closeCalls, 1)
  })

  it("returns the building-present line after one target-page evaluation", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: true }, state)
    const connect = createFakeCdpConnection(
      [
        [
          createFakeKrasPage("https://example.test", { kind: "lookup", hasBuilding: false }, state),
          page,
        ],
      ],
      state,
    )

    // When
    const result = await runKrasLookup(connect, "충청북도 제천시 백운면 평동리 123-4", false)

    // Then
    assert.equal(result, "부동산 종합증명서 조회결과 건물 등장")
    assert.deepEqual(state.connectCalls, [
      {
        endpoint: "http://127.0.0.1:9222",
        options: { noDefaults: true, timeout: 5000 },
      },
    ])
    assert.deepEqual(state.evaluationRequests, [
      {
        mode: "lookup",
        address: "충청북도 제천시 백운면 평동리 123-4",
        darkMode: false,
        darkModeCss: KRAS_DARK_MODE_CSS,
        hasLegalRi: true,
        ledgerLabel: "토지",
      },
    ])
    assert.equal(state.closeCalls, 1)
  })

  it("normalizes Seoul district-first input before one target-page evaluation", async () => {
    // Given
    const state = createFakeKrasState()
    const page = createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: true }, state)
    const connect = createFakeCdpConnection([[page]], state)

    // When
    const result = await runKrasLookup(connect, "강서구 개화동 255-1", false)

    // Then
    assert.equal(result, "부동산 종합증명서 조회결과 건물 등장")
    assert.deepEqual(state.evaluationRequests, [
      {
        mode: "lookup",
        address: "서울특별시 강서구 개화동 255-1",
        darkMode: false,
        darkModeCss: KRAS_DARK_MODE_CSS,
        hasLegalRi: false,
        ledgerLabel: "토지",
      },
    ])
    assert.equal(state.connectCalls.length, 1)
    assert.equal(state.closeCalls, 1)
  })

  it("returns the unavailable line when no building option appears", async () => {
    // Given
    const state = createFakeKrasState()
    const connect = createFakeCdpConnection(
      [[createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: false }, state)]],
      state,
    )

    // When
    const result = await runKrasLookup(connect, "서울특별시 종로구 청운동 1", true)

    // Then
    assert.equal(result, "부동산 종합증명서 조회결과 건물 조회 불가")
    assert.deepEqual(state.evaluationRequests, [
      {
        mode: "lookup",
        address: "서울특별시 종로구 청운동 1",
        darkMode: true,
        darkModeCss: KRAS_DARK_MODE_CSS,
        hasLegalRi: false,
        ledgerLabel: "토지",
      },
    ])
  })

  it("rejects multiple target pages and still detaches", async () => {
    // Given
    const state = createFakeKrasState()
    const connect = createFakeCdpConnection(
      [
        [
          createFakeKrasPage(KRAS_TARGET_URL, { kind: "lookup", hasBuilding: false }, state),
          createFakeKrasPage(
            `${KRAS_TARGET_URL}?duplicate=1`,
            { kind: "lookup", hasBuilding: true },
            state,
          ),
        ],
      ],
      state,
    )

    // When / Then
    await assert.rejects(
      runKrasLookup(connect, "서울특별시 종로구 청운동 1", false),
      /KRAS 조회 페이지는 정확히 하나여야 합니다/,
    )
    assert.equal(state.evaluationRequests.length, 0)
    assert.equal(state.closeCalls, 1)
  })

  it("defines a non-empty address and false dark-mode default", () => {
    // Given / When
    const empty = lookup.args.address.safeParse("   ")
    const valid = lookup.args.darkMode.safeParse(undefined)

    // Then
    assert.equal(empty.success, false)
    assert.deepEqual(valid, { success: true, data: false })
  })
})
