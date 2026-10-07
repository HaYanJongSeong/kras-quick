import type { TestContext } from "node:test"
import type { KrasEvaluationResult } from "../scripts/kras-domain.ts"
import { lookupEvaluation } from "../scripts/kras-page.ts"

type TimerTask = {
  readonly callback: () => void
  readonly delay: number
  readonly id: number
}

class ControlledTimers {
  private nextId = 1
  private readonly tasks = new Map<number, TimerTask>()

  readonly setTimeout = (callback: () => void, delay: number): number => {
    const id = this.nextId
    this.nextId += 1
    this.tasks.set(id, { callback, delay, id })
    return id
  }

  readonly clearTimeout = (id: number): void => {
    this.tasks.delete(id)
  }

  pendingIds(delay: number): readonly number[] {
    return [...this.tasks.values()].filter((task) => task.delay === delay).map((task) => task.id)
  }

  pendingDelays(): readonly number[] {
    return [...this.tasks.values()].map((task) => task.delay).sort((left, right) => left - right)
  }

  run(delay: number): void {
    const ready = [...this.tasks.values()].filter((task) => task.delay === delay)
    for (const task of ready) {
      this.tasks.delete(task.id)
      task.callback()
    }
  }
}

class FakeOption {
  readonly textContent: string
  readonly value: string

  constructor(textContent: string, value: string) {
    this.textContent = textContent
    this.value = value
  }
}

class FakeSelect extends EventTarget {
  private currentValue: string
  private currentOptions: readonly FakeOption[]
  readonly assignedValues: string[] = []

  constructor(options: readonly FakeOption[], value: string) {
    super()
    this.currentOptions = options
    this.currentValue = value
  }

  get options(): readonly FakeOption[] {
    return this.currentOptions
  }

  get selectedOptions(): readonly FakeOption[] {
    const selected = this.currentOptions.find((option) => option.value === this.currentValue)
    return selected === undefined ? [] : [selected]
  }

  get value(): string {
    return this.currentValue
  }

  set value(value: string) {
    this.currentValue = value
    this.assignedValues.push(value)
  }

  replaceOptions(options: readonly FakeOption[], value: string): void {
    this.currentOptions = options
    this.currentValue = value
  }
}

class FakeInput extends EventTarget {
  value = ""
}

class FakeKrasDocument {
  private readonly elements: ReadonlyMap<string, object>

  constructor(elements: ReadonlyMap<string, object>) {
    this.elements = elements
  }

  querySelector(selector: string): object | null {
    return this.elements.get(selector) ?? null
  }
}

type FixtureObserver = {
  readonly callback: () => void
  connected: boolean
  target: object | undefined
}

export type KrasRefreshFixture = {
  readonly activeObserverCount: () => number
  readonly districtAssignedValues: () => readonly string[]
  readonly districtOptionValues: () => readonly string[]
  readonly emitFreshDistrictBatch: () => void
  readonly emitPlaceholderBatch: () => void
  readonly emitStaleDistrictBatch: () => void
  readonly pendingStabilizationTimerIds: () => readonly number[]
  readonly pendingTimerDelays: () => readonly number[]
  readonly runRefreshTimeout: () => void
  readonly runStabilizationTimers: () => void
  readonly start: () => Promise<KrasEvaluationResult>
}

export type KrasRefreshScenario = "matching_option" | "placeholder_only"

export function installKrasRefreshFixture(
  context: TestContext,
  scenario: KrasRefreshScenario = "matching_option",
): KrasRefreshFixture {
  const globalNames = ["document", "MutationObserver", "window"] as const
  const descriptors = globalNames.map(
    (name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)] as const,
  )
  context.after(() => {
    for (const [name, descriptor] of descriptors) {
      if (descriptor === undefined) Reflect.deleteProperty(globalThis, name)
      else Object.defineProperty(globalThis, name, descriptor)
    }
  })

  const placeholder = new FakeOption("선택", "")
  const staleDistrict = new FakeOption("강서구", "stale-district")
  const freshDistrict = new FakeOption("강서구", "fresh-district")
  const province = new FakeSelect(
    [placeholder, new FakeOption("서울특별시", "seoul")],
    scenario === "matching_option" ? "previous-province" : "seoul",
  )
  const district = new FakeSelect(
    [placeholder, scenario === "matching_option" ? staleDistrict : freshDistrict],
    scenario === "matching_option" ? "" : freshDistrict.value,
  )
  const neighborhood = new FakeSelect(
    [placeholder, new FakeOption("개화동", "neighborhood")],
    scenario === "matching_option" ? "neighborhood" : "previous-neighborhood",
  )
  const legalRi = new FakeSelect([placeholder], "")
  const ledger = new FakeSelect([placeholder, new FakeOption("토지", "land")], "land")
  const mainNumber = new FakeInput()
  const subNumber = new FakeInput()
  const elements = new Map<string, object>([
    ["#ctpvCd", province],
    ["#sggCd", district],
    ["#emdCd", neighborhood],
    ["#riCd", legalRi],
    ["#ldgrSeCd", ledger],
    ["#mno", mainNumber],
    ["#sno", subNumber],
  ])
  const observers = new Set<FixtureObserver>()
  class ControlledMutationObserver {
    private readonly record: FixtureObserver

    constructor(callback: () => void) {
      this.record = { callback, connected: false, target: undefined }
      observers.add(this.record)
    }

    observe(target: object): void {
      this.record.connected = true
      this.record.target = target
    }

    disconnect(): void {
      this.record.connected = false
      this.record.target = undefined
    }
  }
  const notify = (target: object): void => {
    for (const observer of observers) {
      if (observer.connected && observer.target === target) observer.callback()
    }
  }
  const timers = new ControlledTimers()
  const windowFixture = {
    clearTimeout: timers.clearTimeout,
    setTimeout: timers.setTimeout,
  }
  Object.defineProperties(globalThis, {
    document: { configurable: true, value: new FakeKrasDocument(elements) },
    MutationObserver: { configurable: true, value: ControlledMutationObserver },
    window: { configurable: true, value: windowFixture },
  })

  return {
    activeObserverCount: () => [...observers].filter((observer) => observer.connected).length,
    districtAssignedValues: () => [...district.assignedValues],
    districtOptionValues: () => district.options.map((option) => option.value),
    emitFreshDistrictBatch: () => {
      district.replaceOptions([placeholder, freshDistrict], freshDistrict.value)
      notify(district)
    },
    emitPlaceholderBatch: () => {
      legalRi.replaceOptions([placeholder], placeholder.value)
      notify(legalRi)
    },
    emitStaleDistrictBatch: () => {
      district.replaceOptions([placeholder, staleDistrict], "")
      notify(district)
    },
    pendingStabilizationTimerIds: () => timers.pendingIds(100),
    pendingTimerDelays: () => timers.pendingDelays(),
    runRefreshTimeout: () => timers.run(4000),
    runStabilizationTimers: () => timers.run(100),
    start: () =>
      lookupEvaluation({
        mode: "fill_only",
        address: "서울특별시 강서구 개화동 255-1",
        darkMode: false,
        darkModeCss: "",
        hasLegalRi: false,
        ledgerLabel: "토지",
      }),
  }
}

export async function drainKrasEvaluationMicrotasks(): Promise<void> {
  for (let turn = 0; turn < 6; turn += 1) await Promise.resolve()
}
