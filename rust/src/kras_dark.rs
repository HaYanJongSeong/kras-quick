// TS scripts/kras-dark.ts — KRAS 다크 모드 테마 CSS와 주입 평가.
// 다크 모드는 선택 기능이며 기본 요청은 darkMode:false다(TS 동일).

/// TS KRAS_DARK_MODE_CSS — 색·포커스 선언만 바꾸고 KRAS 지오메트리는 건드리지 않는다.
pub const KRAS_DARK_MODE_CSS: &str = r##":root {
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
}"##;

/// TS darkModeEvaluation — style 태그를 한 번만 주입한다(멱등).
const DARK_MODE_EVALUATION_JS: &str = r#"(css) => {
	const existing = document.getElementById("opencode-dark-mode");
	const style = existing ?? document.createElement("style");
	style.id = "opencode-dark-mode";
	style.textContent = css;
	if (existing === null) document.head.append(style);
}"#;

/// TS darkModeEvaluation — CDP Runtime.evaluate로 다크 모드 CSS를 적용한다.
/// (인자 css를 평가식에 넘기기 위해 함수 꼴을 문자열로 감싼다.)
pub fn apply_kras_dark_mode(
	surface: &crate::cdp_surface::CdpSurface,
) -> Result<String, Box<dyn std::error::Error>> {
	let expression = format!(
		"({})({})",
		DARK_MODE_EVALUATION_JS,
		serde_json::to_string(KRAS_DARK_MODE_CSS)?
	);
	let params = serde_json::json!({ "expression": expression, "returnByValue": true });
	surface.send_cdp("Runtime.evaluate", params)?;
	// TS runKrasDarkMode 반환 문자열
	Ok("다크 모드 적용.".to_string())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn css_is_complete_soft_neutral_theme() {
		// TS kras-dark.test: 전체 테마 토큰이 빠짐없이 있는지
		for token in [
			"--kras-page", "--kras-panel", "--kras-raised", "--kras-control",
			"--kras-text", "--kras-muted", "--kras-border", "--kras-link",
			"--kras-focus",
		] {
			assert!(KRAS_DARK_MODE_CSS.contains(token), "{token} 토큰 누락");
		}
		assert!(KRAS_DARK_MODE_CSS.contains(":focus-visible"));
		// 지오메트리(크기·간격) 선언은 건드리지 않는다
		assert!(!KRAS_DARK_MODE_CSS.contains("width:"));
		assert!(!KRAS_DARK_MODE_CSS.contains("height:"));
	}

	#[test]
	fn css_only_declares_colors_and_focus() {
		// TS tests/kras-dark.test "allows only color and focus declarations"
		// 선언 속성 추출: (?:^|[;{])\s*([-\w]+)\s*:
		let allowed = [
			"accent-color",
			"background-color",
			"border-color",
			"color",
			"opacity",
			"outline",
			"outline-offset",
		];
		let property = regex::Regex::new(r"(?:^|[;{])\s*([-\w]+)\s*:").expect("정규식");
		let mut rejected: Vec<String> = Vec::new();
		for capture in property.captures_iter(KRAS_DARK_MODE_CSS) {
			let name = capture.get(1).map(|m| m.as_str()).unwrap_or("");
			if !name.starts_with("--kras-") && !allowed.contains(&name) {
				rejected.push(name.to_string());
			}
		}
		assert_eq!(rejected, Vec::<String>::new(), "허용되지 않은 선언");

		// opacity 값은 "1 !important" 하나뿐이다(TS는 Set으로 비교해 중복을 허용)
		let opacity = regex::Regex::new(r"opacity\s*:\s*([^;]+);").expect("정규식");
		let mut values: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
		for capture in opacity.captures_iter(KRAS_DARK_MODE_CSS) {
			if let Some(value) = capture.get(1) {
				values.insert(value.as_str().trim().to_string());
			}
		}
		let mut expected = std::collections::BTreeSet::new();
		expected.insert("1 !important".to_string());
		assert_eq!(values, expected);
	}

	#[test]
	fn documents_one_distinct_raw_color_for_every_runtime_theme_token() {
		// TS 테스트 — 테마 토큰마다 원시 색상 하나씩, 순서·중복 없이
		let expected = [
			"#202124", "#292b2f", "#33363b", "#303338", "#e8eaed", "#b6bbc2",
			"#555b63", "#9fc5e8", "#a8c7fa",
		];
		let color = regex::Regex::new(r"(?i)#[\da-f]{6}").expect("정규식");
		let found: Vec<String> = color
			.find_iter(KRAS_DARK_MODE_CSS)
			.map(|m| m.as_str().to_lowercase())
			.collect();
		assert_eq!(found, expected);
		let unique: std::collections::BTreeSet<&String> = found.iter().collect();
		assert_eq!(unique.len(), expected.len(), "색상이 중복되면 안 된다");
	}

	#[test]
	fn dark_mode_injection_is_idempotent_in_source() {
		// 같은 id를 재사용하고 새로 만들 때만 append 한다(JS 원문에 그 로직이 있는지)
		assert!(DARK_MODE_EVALUATION_JS.contains("opencode-dark-mode"));
		assert!(DARK_MODE_EVALUATION_JS.contains("if (existing === null) document.head.append(style);"));
		// 문자열 직렬화가 안전해야 한다(역슬래시 이스케이프)
		let serialized = serde_json::to_string(KRAS_DARK_MODE_CSS).expect("직렬화");
		assert!(serialized.starts_with('"') && serialized.ends_with('"'));
	}
}
