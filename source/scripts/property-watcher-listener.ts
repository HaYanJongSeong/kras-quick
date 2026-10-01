import {
  PROPERTY_GUIDANCE_ABSENT_PROMPT,
  PROPERTY_GUIDANCE_COPY_FAILURE,
  PROPERTY_GUIDANCE_CSS,
  PROPERTY_GUIDANCE_ITEMS,
} from "./property-watcher-guidance.ts"

export const PROPERTY_WATCHER_BINDING_NAME = "__opencodePropertyWatcherRowV3" as const
export const PROPERTY_WATCHER_WORLD_NAME = "opencode-property-watcher-v1" as const

export const PROPERTY_WATCHER_LISTENER_SOURCE = `(() => {
  const stateKey = "__opencodePropertyWatcherListenerV3";
  if (globalThis[stateKey] !== undefined || window.top !== window) return;
  const focusPreservingScroll = (element) => {
    Reflect.apply(HTMLElement.prototype.focus, element, [{ preventScroll: true }]);
  };
  const fallbackCopy = (result) => {
    const activeElement = document.activeElement;
    const previousFocus =
      activeElement instanceof HTMLElement && activeElement.isConnected ? activeElement : null;
    const textarea = document.createElement("textarea");
    textarea.value = result;
    textarea.setAttribute("readonly", "");
    textarea.setAttribute("aria-hidden", "true");
    textarea.setAttribute("tabindex", "-1");
    textarea.setAttribute(
      "style",
      "position:fixed;inset-block-start:0;inset-inline-start:-10000px;",
    );
    try {
      document.body.append(textarea);
      focusPreservingScroll(textarea);
      textarea.select();
      return document.execCommand("copy") === true;
    } catch (_error) {
      return false;
    } finally {
      textarea.remove();
      if (previousFocus !== null) focusPreservingScroll(previousFocus);
    }
  };
  const copyResult = async (result) => {
    try {
      const clipboard = navigator.clipboard;
      if (typeof clipboard?.writeText === "function") {
        try {
          await clipboard.writeText(result);
          return true;
        } catch (_error) {
          return fallbackCopy(result);
        }
      }
      return fallbackCopy(result);
    } catch (_error) {
      return false;
    }
  };
  const selectionOverlaps = (control) => {
    const selection = window.getSelection();
    if (selection === null || selection.isCollapsed || selection.rangeCount === 0) return false;
    try {
      for (let index = 0; index < selection.rangeCount; index += 1) {
        if (selection.getRangeAt(index).intersectsNode(control)) return true;
      }
      return false;
    } catch (_error) {
      return true;
    }
  };
  const currentPager = () => {
    const positions = [];
    for (const select of document.querySelectorAll("select")) {
      const parent = select.parentElement;
      if (parent === null) continue;
      const children = Array.from(parent.children);
      if (children.filter((child) => child.tagName === "SELECT").length !== 1) continue;
      const buttons = children
        .filter((child) => child.tagName === "BUTTON")
        .map((button) => (button.textContent ?? "").trim());
      if (buttons.length !== 2 || !buttons.includes("이전") || !buttons.includes("다음")) continue;
      const spans = children.filter((child) => child.tagName === "SPAN");
      if (spans.length !== 1) continue;
      const match = (spans[0].textContent ?? "").trim().match(/^(\\d+)\\s*\\/\\s*(\\d+)$/);
      if (match === null) {
        positions.push(null);
        continue;
      }
      const current = Number(match[1]);
      const total = Number(match[2]);
      positions.push(
        Number.isSafeInteger(current) &&
          Number.isSafeInteger(total) &&
          current > 0 &&
          total > 0 &&
          current <= total
          ? current + "/" + total
          : null,
      );
    }
    return positions.length === 1 ? positions[0] : null;
  };
  const buildingStatusPresentations = new Map([
    ["present", {
      className: "opencode-property-guidance-building-status--present",
      feedback: "",
      label: "건물 있음",
    }],
    ["absent", {
      className: "opencode-property-guidance-building-status--absent",
      feedback: ${JSON.stringify(PROPERTY_GUIDANCE_ABSENT_PROMPT)},
      label: "건물 없음",
    }],
    ["failed", {
      className: "opencode-property-guidance-building-status--failed",
      feedback: "",
      label: "건물 확인 실패",
    }],
  ]);
  const updateBuildingStatus = (selectionId, status) => {
    if (selectionId !== state.latestSelectionId) return "stale";
    const presentation = buildingStatusPresentations.get(status);
    if (presentation === undefined) return "stale";
    state.buildingStatus.setAttribute("class", presentation.className);
    state.buildingStatus.textContent = presentation.label;
    state.feedback.textContent = presentation.feedback;
    return "applied";
  };
  const listener = (event) => {
    if (!event.isTrusted) return;
    const url = new URL(window.location.href);
    if (url.origin !== "http://scpweb.softgraphy.biz" || url.pathname !== "/board") return;
    if (!(event.target instanceof Element)) return;
    const control = event.target.closest("#opencode-property-guidance-panel button");
    if (control !== null) {
      const result = state.guidanceResults.get(control);
      if (result === undefined || selectionOverlaps(control)) return;
      void copyResult(result).then(
        (copied) => {
          state.feedback.textContent = copied
            ? "복사됨: " + result
            : ${JSON.stringify(PROPERTY_GUIDANCE_COPY_FAILURE)};
        },
        (_error) => {
          state.feedback.textContent = ${JSON.stringify(PROPERTY_GUIDANCE_COPY_FAILURE)};
        },
      );
      return;
    }
    const row = event.target.closest("tbody tr.cursor-pointer");
    if (row === null || row.children.length !== 8) return;
    const cells = [];
    for (const cell of row.children) {
      if (!(cell instanceof HTMLTableCellElement) || cell.tagName !== "TD") return;
      const value = (cell.textContent ?? "").trim();
      if (value.length > 4096) return;
      cells.push(value);
    }
    const pager = currentPager();
    if (pager === null) return;
    const selectionId = state.latestSelectionId + 1;
    if (!Number.isSafeInteger(selectionId) || selectionId <= 0) return;
    const payload = JSON.stringify({ version: 3, selectionId, cells, pager });
    if (new TextEncoder().encode(payload).byteLength > 65536) return;
    if (state.panel === null) {
      const textElement = (tagName, text) => {
        const element = document.createElement(tagName);
        element.textContent = text;
        return element;
      };
      const style = document.createElement("style");
      style.setAttribute("id", "opencode-property-guidance-style");
      style.textContent = ${JSON.stringify(PROPERTY_GUIDANCE_CSS)};
      const panel = document.createElement("aside");
      panel.setAttribute("id", "opencode-property-guidance-panel");
      panel.setAttribute("aria-labelledby", "opencode-property-guidance-heading");
      const label = textElement("p", "현재 지번 주소");
      label.setAttribute("id", "opencode-property-guidance-label");
      const address = textElement("p", "");
      address.setAttribute("id", "opencode-property-guidance-address");
      address.setAttribute("role", "status");
      address.setAttribute("aria-live", "polite");
      address.setAttribute("aria-atomic", "true");
      const buildingStatus = textElement("p", "");
      buildingStatus.setAttribute("id", "opencode-property-guidance-building-status");
      buildingStatus.setAttribute("role", "status");
      buildingStatus.setAttribute("aria-live", "polite");
      buildingStatus.setAttribute("aria-atomic", "true");
      const heading = textElement(
        "h2",
        "건물이 있는곳이면 일사편리로 부동산 종합증명서 조회를 해보고,",
      );
      heading.setAttribute("id", "opencode-property-guidance-heading");
      const list = document.createElement("ul");
      for (const [itemText, result] of ${JSON.stringify(PROPERTY_GUIDANCE_ITEMS)}) {
        const item = document.createElement("li");
        const button = textElement("button", itemText);
        button.setAttribute("type", "button");
        state.guidanceResults.set(button, result);
        item.append(button);
        list.append(item);
      }
      const feedback = textElement("p", "");
      feedback.setAttribute("id", "opencode-property-guidance-feedback");
      feedback.setAttribute("role", "status");
      feedback.setAttribute("aria-live", "polite");
      feedback.setAttribute("aria-atomic", "true");
      panel.append(label, address, buildingStatus, heading, list, feedback);
      document.head.append(style);
      document.body.append(panel);
      state.style = style;
      state.panel = panel;
      state.label = label;
      state.address = address;
      state.buildingStatus = buildingStatus;
      state.feedback = feedback;
    }
    state.latestSelectionId = selectionId;
    state.label.textContent = "현재 지번 주소 (" + pager + ")";
    state.address.textContent = cells[6].trim();
    state.buildingStatus.setAttribute(
      "class",
      "opencode-property-guidance-building-status--pending",
    );
    state.buildingStatus.textContent = "건물 확인 대기중";
    state.feedback.textContent = "";
    globalThis.__opencodePropertyWatcherRowV3(payload);
  };
  const state = {
    listener,
    style: null,
    panel: null,
    label: null,
    address: null,
    buildingStatus: null,
    feedback: null,
    latestSelectionId: 0,
    updateBuildingStatus,
    guidanceResults: new WeakMap(),
  };
  globalThis[stateKey] = state;
  document.addEventListener("click", listener, true);
})()`

export const PROPERTY_WATCHER_CLEANUP_SOURCE = `(() => {
  const stateKey = "__opencodePropertyWatcherListenerV3";
  const state = globalThis[stateKey];
  if (state === undefined) return;
  document.removeEventListener("click", state.listener, true);
  state.panel?.remove();
  state.style?.remove();
  delete globalThis[stateKey];
})()`
