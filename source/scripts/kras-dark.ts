export const KRAS_DARK_MODE_CSS = `:root {
  --kras-page: #202124;
  --kras-panel: #292b2f;
  --kras-raised: #33363b;
  --kras-control: #303338;
  --kras-text: #e8eaed;
  --kras-muted: #b6bbc2;
  --kras-border: #555b63;
  --kras-link: #9fc5e8;
  --kras-focus: #a8c7fa;
}

html,
body {
  background-color: var(--kras-page) !important;
  color: var(--kras-text) !important;
}

main,
section,
article,
header,
footer,
nav,
form,
fieldset,
table,
tbody,
tr {
  background-color: var(--kras-panel) !important;
  color: var(--kras-text) !important;
  border-color: var(--kras-border) !important;
}

th,
[role="rowheader"],
[role="columnheader"] {
  background-color: var(--kras-raised) !important;
  color: var(--kras-text) !important;
  border-color: var(--kras-border) !important;
}

td {
  background-color: var(--kras-panel) !important;
  color: var(--kras-text) !important;
  border-color: var(--kras-border) !important;
}

caption,
.radio__name,
.con_tit {
  color: var(--kras-text) !important;
}

.surely {
  color: var(--kras-muted) !important;
}

input,
select,
textarea,
button {
  background-color: var(--kras-control) !important;
  color: var(--kras-text) !important;
  border-color: var(--kras-border) !important;
  accent-color: var(--kras-focus) !important;
}

a,
[role="link"] {
  color: var(--kras-link) !important;
}

input::placeholder,
textarea::placeholder {
  color: var(--kras-muted) !important;
  opacity: 1 !important;
}

input:disabled,
select:disabled,
textarea:disabled,
button:disabled,
option:disabled,
[aria-disabled="true"] {
  background-color: var(--kras-raised) !important;
  color: var(--kras-muted) !important;
  border-color: var(--kras-border) !important;
  opacity: 1 !important;
}

.disabled,
.txt_gray,
.gray,
.guide,
.desc {
  color: var(--kras-muted) !important;
  opacity: 1 !important;
}

:focus-visible {
  outline: 2px solid var(--kras-focus) !important;
  outline-offset: 2px !important;
}`

export function darkModeEvaluation(css: string): void {
  const existing = document.getElementById("opencode-dark-mode")
  const style = existing ?? document.createElement("style")
  style.id = "opencode-dark-mode"
  style.textContent = css
  if (existing === null) document.head.append(style)
}
