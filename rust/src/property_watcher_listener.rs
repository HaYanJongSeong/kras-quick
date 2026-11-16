// TS scripts/property-watcher-listener.ts + property-watcher-guidance.ts — 페이지 주입 소스.
// 브라우저 안에서 실행되는 JS이므로 원문을 보존하고 가이드 상수만 치환한다.

use std::sync::LazyLock;

pub const PROPERTY_WATCHER_BINDING_NAME: &str = "__opencodePropertyWatcherRowV3";
pub const PROPERTY_WATCHER_WORLD_NAME: &str = "opencode-property-watcher-v1";

pub const PROPERTY_GUIDANCE_COPY_FAILURE: &str =
	"복사하지 못했습니다. 다시 시도하거나 문장을 직접 선택해 주세요.";

pub const PROPERTY_GUIDANCE_ABSENT_PROMPT: &str =
	"아래 첫 번째 안내를 눌러 ‘부동산 종합증명서 조회결과 건물 조회 불가’를 복사하세요.";

pub const PROPERTY_GUIDANCE_ITEMS: &[(&str, &str); 4] = &[
	(
		"건물구분에 건물이 안뜨면 - 부동산 종합증명서 조회결과 건물 조회 불가",
		"부동산 종합증명서 조회결과 건물 조회 불가",
	),
	(
		"건물이 있는데 서울시 소유가 아니면 - 부동산 종합증명서 조회결과 건물 시유재산 아님",
		"부동산 종합증명서 조회결과 건물 시유재산 아님",
	),
	(
		"소유자가 아예 안 보이면 - 부동산 종합증명서 조회결과 건물 소유 확인 불명",
		"부동산 종합증명서 조회결과 건물 소유 확인 불명",
	),
	(
		"서울시 소유면 - 부동산 종합증명서 조회결과 건물 시유재산 확인",
		"부동산 종합증명서 조회결과 건물 시유재산 확인",
	),
];

pub const PROPERTY_GUIDANCE_CSS: &str = r##"#opencode-property-guidance-panel {
  --opencode-guidance-light-panel: #fff;
  --opencode-guidance-light-raised: #f6f8fa;
  --opencode-guidance-light-text: #1f2328;
  --opencode-guidance-light-muted: #57606a;
  --opencode-guidance-light-border: #d0d7de;
  --opencode-guidance-light-present: #087f5b;
  --opencode-guidance-dark-panel: #292b2f;
  --opencode-guidance-dark-raised: #33363b;
  --opencode-guidance-dark-text: #e8eaed;
  --opencode-guidance-dark-muted: #b6bbc2;
  --opencode-guidance-dark-border: #555b63;
  --opencode-guidance-dark-present: #7ee2c0;
  --opencode-guidance-panel: var(--opencode-guidance-light-panel);
  --opencode-guidance-raised: var(--opencode-guidance-light-raised);
  --opencode-guidance-text: var(--opencode-guidance-light-text);
  --opencode-guidance-muted: var(--opencode-guidance-light-muted);
  --opencode-guidance-border: var(--opencode-guidance-light-border);
  --opencode-guidance-present: var(--opencode-guidance-light-present);
  position: fixed;
  inset-block-end: max(16px, env(safe-area-inset-bottom));
  inset-inline-start: max(16px, env(safe-area-inset-left));
  box-sizing: border-box;
  inline-size: min(420px, calc(100vw - 32px));
  max-block-size: calc(100dvh - 32px);
  overflow-y: auto;
  pointer-events: auto;
  z-index: 2147483000;
  padding: 16px;
  border: 1px solid var(--opencode-guidance-border);
  border-radius: 8px;
  background-color: var(--opencode-guidance-panel);
  color: var(--opencode-guidance-text);
  font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans KR", "Malgun Gothic", "Apple SD Gothic Neo", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  word-break: keep-all;
  overflow-wrap: break-word;
  user-select: text;
  -webkit-user-select: text;
}

#opencode-property-guidance-panel * {
  box-sizing: border-box;
  user-select: text;
  -webkit-user-select: text;
}

#opencode-property-guidance-panel #opencode-property-guidance-label {
  margin: 0 0 4px;
  color: var(--opencode-guidance-muted);
}

#opencode-property-guidance-panel #opencode-property-guidance-address {
  margin: 0 0 12px;
  padding: 8px;
  border-radius: 4px;
  background-color: var(--opencode-guidance-raised);
  font-weight: 600;
}

#opencode-property-guidance-panel #opencode-property-guidance-building-status {
  margin: 0 0 12px;
  color: var(--opencode-guidance-muted);
}

#opencode-property-guidance-panel #opencode-property-guidance-building-status.opencode-property-guidance-building-status--present {
  color: var(--opencode-guidance-present);
  font-size: 18px;
  font-weight: 700;
}

#opencode-property-guidance-panel #opencode-property-guidance-heading {
  margin: 0 0 8px;
  font-size: 14px;
  line-height: 1.5;
  text-wrap: balance;
}

#opencode-property-guidance-panel ul {
  margin: 0;
  padding-inline-start: 16px;
}

#opencode-property-guidance-panel li + li {
  margin-block-start: 4px;
}

#opencode-property-guidance-panel button {
  display: block;
  inline-size: 100%;
  border: 0;
  padding: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: inherit;
  text-decoration-line: underline;
  text-decoration-style: dotted;
  text-decoration-color: var(--opencode-guidance-muted);
  text-underline-offset: 4px;
  cursor: copy;
}

#opencode-property-guidance-panel button:focus-visible {
  outline: 2px solid var(--opencode-guidance-text);
  outline-offset: 2px;
  border-radius: 4px;
}

#opencode-property-guidance-panel #opencode-property-guidance-feedback:empty {
  display: none;
}

#opencode-property-guidance-panel #opencode-property-guidance-feedback:not(:empty) {
  margin: 0;
  margin-block-start: 8px;
  padding: 8px;
  border-radius: 4px;
  background-color: var(--opencode-guidance-raised);
  color: var(--opencode-guidance-text);
  font-weight: 600;
  text-wrap: balance;
}

@media (prefers-color-scheme: dark) {
  #opencode-property-guidance-panel {
    --opencode-guidance-panel: var(--opencode-guidance-dark-panel);
    --opencode-guidance-raised: var(--opencode-guidance-dark-raised);
    --opencode-guidance-text: var(--opencode-guidance-dark-text);
    --opencode-guidance-muted: var(--opencode-guidance-dark-muted);
    --opencode-guidance-border: var(--opencode-guidance-dark-border);
    --opencode-guidance-present: var(--opencode-guidance-dark-present);
  }
}

:root:has(#opencode-dark-mode) #opencode-property-guidance-panel {
  --opencode-guidance-panel: var(--opencode-guidance-dark-panel);
  --opencode-guidance-raised: var(--opencode-guidance-dark-raised);
  --opencode-guidance-text: var(--opencode-guidance-dark-text);
  --opencode-guidance-muted: var(--opencode-guidance-dark-muted);
  --opencode-guidance-border: var(--opencode-guidance-dark-border);
}

@media (max-width: 767px) {
  #opencode-property-guidance-panel {
    inset-block-end: max(8px, env(safe-area-inset-bottom));
    inset-inline-start: max(8px, env(safe-area-inset-left));
    inset-inline-end: max(8px, env(safe-area-inset-right));
    inline-size: auto;
    max-block-size: calc(100dvh - 16px);
  }
}"##;

// TS 템플릿 리터럴의 `${JSON.stringify(...)}` 치환을 그대로 재현한다.
fn json_string<T: serde::Serialize + ?Sized>(value: &T) -> String {
	serde_json::to_string(value).expect("상수 직렬화는 실패하지 않는다")
}

fn substitute_listener() -> String {
	let raw = r##"(() => {
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
      const match = (spans[0].textContent ?? "").trim().match(/^(\d+)\s*\/\s*(\d+)$/);
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
})()"##;
	raw.replace(
		"${JSON.stringify(PROPERTY_GUIDANCE_ABSENT_PROMPT)}",
		&json_string(PROPERTY_GUIDANCE_ABSENT_PROMPT),
	)
	.replace(
		"${JSON.stringify(PROPERTY_GUIDANCE_COPY_FAILURE)}",
		&json_string(PROPERTY_GUIDANCE_COPY_FAILURE),
	)
	.replace(
		"${JSON.stringify(PROPERTY_GUIDANCE_CSS)}",
		&json_string(PROPERTY_GUIDANCE_CSS),
	)
	.replace(
		"${JSON.stringify(PROPERTY_GUIDANCE_ITEMS)}",
		&json_string(&PROPERTY_GUIDANCE_ITEMS),
	)
}

const CLEANUP_SOURCE: &str = r##"(() => {
  const stateKey = "__opencodePropertyWatcherListenerV3";
  const state = globalThis[stateKey];
  if (state === undefined) return;
  document.removeEventListener("click", state.listener, true);
  state.panel?.remove();
  state.style?.remove();
  delete globalThis[stateKey];
})()"##;

static LISTENER_SOURCE: LazyLock<String> = LazyLock::new(substitute_listener);

/// 런타임 최초 접근 시 한 번 치환된다(TS 모듈 상수와 동일).
pub fn listener_source() -> &'static str {
	&LISTENER_SOURCE
}

pub fn cleanup_source() -> &'static str {
	CLEANUP_SOURCE
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn listener_source_substitutes_guidance_and_wires_binding() {
		let source = listener_source();
		// 치환이 끝나면 템플릿 자리표시자가 남지 않는다.
		assert!(!source.contains("${JSON.stringify("));
		assert!(source.contains(PROPERTY_GUIDANCE_ABSENT_PROMPT));
		assert!(source.contains(PROPERTY_GUIDANCE_COPY_FAILURE));
		assert!(source.contains("#opencode-property-guidance-panel {"));
		assert!(source.contains("부동산 종합증명서 조회결과 건물 시유재산 확인"));
		assert!(source.contains("globalThis.__opencodePropertyWatcherRowV3(payload)"));
		assert!(source.contains("globalThis[stateKey] = state"));
		// JS 정규식은 치환 후에도 백slashes를 유지한다.
		assert!(source.contains(r"^(\d+)\s*\/\s*(\d+)$"));
	}

	#[test]
	fn cleanup_source_removes_listener_and_panel() {
		let cleanup = cleanup_source();
		assert!(cleanup.contains("document.removeEventListener(\"click\", state.listener, true)"));
		assert!(cleanup.contains("delete globalThis[stateKey]"));
	}
}
