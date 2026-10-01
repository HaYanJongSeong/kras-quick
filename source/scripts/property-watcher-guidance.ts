export const PROPERTY_GUIDANCE_ITEMS = [
  [
    "건물구분에 건물이 안뜨면 - 부동산 종합증명서 조회결과 건물 조회 불가",
    "부동산 종합증명서 조회결과 건물 조회 불가",
  ],
  [
    "건물이 있는데 서울시 소유가 아니면 - 부동산 종합증명서 조회결과 건물 시유재산 아님",
    "부동산 종합증명서 조회결과 건물 시유재산 아님",
  ],
  [
    "소유자가 아예 안 보이면 - 부동산 종합증명서 조회결과 건물 소유 확인 불명",
    "부동산 종합증명서 조회결과 건물 소유 확인 불명",
  ],
  [
    "서울시 소유면 - 부동산 종합증명서 조회결과 건물 시유재산 확인",
    "부동산 종합증명서 조회결과 건물 시유재산 확인",
  ],
] as const

export const PROPERTY_GUIDANCE_COPY_FAILURE =
  "복사하지 못했습니다. 다시 시도하거나 문장을 직접 선택해 주세요."

export const PROPERTY_GUIDANCE_ABSENT_PROMPT =
  "아래 첫 번째 안내를 눌러 ‘부동산 종합증명서 조회결과 건물 조회 불가’를 복사하세요."

export const PROPERTY_GUIDANCE_CSS = `#opencode-property-guidance-panel {
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
  --opencode-guidance-present: var(--opencode-guidance-dark-present);
}

@media (max-width: 767px) {
  #opencode-property-guidance-panel {
    inset-block-end: max(8px, env(safe-area-inset-bottom));
    inset-inline-start: max(8px, env(safe-area-inset-left));
    inset-inline-end: max(8px, env(safe-area-inset-right));
    inline-size: auto;
    max-block-size: calc(100dvh - 16px);
  }
}`
