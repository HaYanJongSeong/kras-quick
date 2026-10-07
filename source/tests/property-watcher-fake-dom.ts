export class FakeElement extends EventTarget {
  private readonly attributes = new Map<string, string>()
  private readonly childElements: FakeElement[]
  private ownerDocument: FakeDocument | null = null
  private parent: FakeElement | null = null
  readonly tagName: string
  textContent: string | null
  value = ""
  private readonly row: FakeElement | null

  constructor(
    children: readonly FakeElement[] = [],
    tagName = "DIV",
    textContent: string | null = null,
    row: FakeElement | null = null,
  ) {
    super()
    this.childElements = [...children]
    for (const child of this.childElements) child.parent = this
    this.tagName = tagName
    this.textContent = textContent
    this.row = row
  }

  get children(): readonly FakeElement[] {
    return this.childElements
  }

  get isConnected(): boolean {
    if (this.ownerDocument === null) return false
    if (this === this.ownerDocument.body || this === this.ownerDocument.head) return true
    return this.parent?.isConnected === true
  }

  get parentElement(): FakeElement | null {
    return this.parent
  }

  append(...elements: readonly FakeElement[]): void {
    for (const element of elements) {
      element.parent = this
      element.attachDocument(this.ownerDocument)
      this.childElements.push(element)
    }
  }

  attachDocument(document: FakeDocument | null): void {
    this.ownerDocument = document
    for (const child of this.childElements) child.attachDocument(document)
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null
  }

  remove(): void {
    const document = this.ownerDocument
    this.parent?.removeChild(this)
    this.parent = null
    document?.disconnectElement(this)
  }

  focus(options?: FocusOptions): void {
    this.ownerDocument?.setActiveElement(this, options)
  }

  select(): void {
    this.ownerDocument?.selectElement(this)
  }

  setAttribute(name: string, value: string): void {
    this.attributes.set(name, value)
  }

  closest(selector: string): FakeElement | null {
    if (selector === "tbody tr.cursor-pointer") return this.row
    if (selector === "#opencode-property-guidance-panel button") {
      if (this.tagName !== "BUTTON") return this.parent?.closest(selector) ?? null
      let ancestor = this.parent
      while (ancestor !== null) {
        if (ancestor.getAttribute("id") === "opencode-property-guidance-panel") return this
        ancestor = ancestor.parent
      }
      return null
    }
    throw new Error(`unexpected selector: ${selector}`)
  }

  private removeChild(element: FakeElement): void {
    const index = this.childElements.indexOf(element)
    if (index >= 0) this.childElements.splice(index, 1)
  }
}

export class FakeTableCellElement extends FakeElement {
  constructor(text: string, tagName = "TD") {
    super([], tagName, text)
  }
}

export class FakeDocument {
  readonly body: FakeElement
  readonly head: FakeElement
  readonly listeners = new Set<EventListenerOrEventListenerObject>()
  readonly addCaptures: boolean[] = []
  readonly removeCaptures: boolean[] = []
  readonly execCommands: string[] = []
  readonly focusCalls: Array<{ readonly element: FakeElement; readonly preventScroll: boolean }> =
    []
  activeElement: FakeElement | null = null
  addCalls = 0
  removeCalls = 0
  private readonly executeCopy: (document: FakeDocument) => boolean
  private selectedElement: FakeElement | null = null
  private selectionCollapsed = true
  private selectionTarget: FakeElement | null = null

  constructor(executeCopy: (document: FakeDocument) => boolean = () => false) {
    this.executeCopy = executeCopy
    this.body = new FakeElement([], "BODY")
    this.head = new FakeElement([], "HEAD")
    this.body.attachDocument(this)
    this.head.attachDocument(this)
  }

  addEventListener(
    _type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: boolean | AddEventListenerOptions,
  ): void {
    if (listener === null) return
    this.addCalls += 1
    this.addCaptures.push(
      options === true || (typeof options === "object" && options.capture === true),
    )
    this.listeners.add(listener)
  }

  removeEventListener(
    _type: string,
    listener: EventListenerOrEventListenerObject | null,
    options?: boolean | EventListenerOptions,
  ): void {
    if (listener === null) return
    this.removeCalls += 1
    this.removeCaptures.push(
      options === true || (typeof options === "object" && options.capture === true),
    )
    this.listeners.delete(listener)
  }

  createElement(tagName: string): FakeElement {
    const element = new FakeElement([], tagName.toUpperCase())
    element.attachDocument(this)
    return element
  }

  execCommand(command: string): boolean {
    this.execCommands.push(command)
    return this.executeCopy(this)
  }

  disconnectElement(element: FakeElement): void {
    if (this.activeElement === element) this.activeElement = null
    if (this.selectedElement === element) this.selectedElement = null
  }

  elementsById(id: string): readonly FakeElement[] {
    return this.elements().filter((element) => element.getAttribute("id") === id)
  }

  getElementById(id: string): FakeElement | null {
    return this.elementsById(id)[0] ?? null
  }

  querySelectorAll(selector: string): readonly FakeElement[] {
    const tagName = selector.toUpperCase()
    return this.elements().filter((element) => element.tagName === tagName)
  }

  dispatchClick(target: FakeElement, isTrusted: boolean): Event {
    const event = new Event("click", { bubbles: true, cancelable: true })
    Object.defineProperties(event, {
      isTrusted: { configurable: true, value: isTrusted },
      target: { configurable: true, value: target },
    })
    for (const listener of this.listeners) {
      if (typeof listener === "function") listener(event)
      else listener.handleEvent(event)
    }
    return event
  }

  getSelection(): {
    readonly getRangeAt: (index: number) => {
      readonly intersectsNode: (node: FakeElement) => boolean
    }
    readonly isCollapsed: boolean
    readonly rangeCount: number
  } {
    return {
      getRangeAt: (index) => {
        if (index !== 0) throw new RangeError("selection range index out of bounds")
        return { intersectsNode: (node) => node === this.selectionTarget }
      },
      isCollapsed: this.selectionCollapsed,
      rangeCount: this.selectionCollapsed ? 0 : 1,
    }
  }

  selectElement(element: FakeElement): void {
    this.selectedElement = element
  }

  getSelectedElement(): FakeElement | null {
    return this.selectedElement
  }

  setActiveElement(element: FakeElement, options?: FocusOptions): void {
    this.activeElement = element
    this.focusCalls.push({ element, preventScroll: options?.preventScroll === true })
  }

  setSelection(target: FakeElement | null, collapsed: boolean): void {
    this.selectionTarget = target
    this.selectionCollapsed = collapsed
  }

  private elements(): readonly FakeElement[] {
    const found: FakeElement[] = []
    const visit = (element: FakeElement): void => {
      found.push(element)
      for (const child of element.children) visit(child)
    }
    visit(this.head)
    visit(this.body)
    return found
  }
}
