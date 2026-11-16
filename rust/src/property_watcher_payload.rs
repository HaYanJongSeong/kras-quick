// TS scripts/property-watcher-payload.ts — SCP 행 페이로드 계약(zod 스키마)과 파서.
// zod 스키마의 조건(길이·형식·strict·브랜드)을 그대로 재현한다.

use serde_json::Value;

/// 건물 상태 결과/말단 상태는 CDP 계층에 이미 정의되어 있다 — 단일 출처로 재-export.
pub use crate::property_watcher_cdp::{
	PropertyBuildingStatusUpdateResult, PropertyBuildingTerminalStatus,
};

/// TS PropertyBuildingStatus = "pending" | PropertyBuildingTerminalStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyBuildingStatus {
	Pending,
	Absent,
	Failed,
	Present,
}

impl PropertyBuildingStatus {
	pub fn as_str(self) -> &'static str {
		match self {
			PropertyBuildingStatus::Pending => "pending",
			PropertyBuildingStatus::Absent => "absent",
			PropertyBuildingStatus::Failed => "failed",
			PropertyBuildingStatus::Present => "present",
		}
	}

	pub fn from_terminal(t: PropertyBuildingTerminalStatus) -> Self {
		match t {
			PropertyBuildingTerminalStatus::Absent => PropertyBuildingStatus::Absent,
			PropertyBuildingTerminalStatus::Failed => PropertyBuildingStatus::Failed,
			PropertyBuildingTerminalStatus::Present => PropertyBuildingStatus::Present,
		}
	}
}

/// JS Number.MAX_SAFE_INTEGER — zod의 `.max(Number.MAX_SAFE_INTEGER)` 조건에 쓰인다.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// TS PropertyRow — 정확히 8개의 문자열, 각 4096자 이하.
pub type PropertyRow = [String; 8];

const ROW_MAX_CHARS: usize = 4096;
const PAYLOAD_MAX_BYTES: usize = 65_536;

/// TS PropertyPager — `현재/전체` 형식이며 1부터 시작하고 현재 ≤ 전체.
pub type PropertyPager = String;

fn all_ascii_digits_and_leading_nonzero(text: &str) -> bool {
	!text.is_empty()
		&& text.bytes().all(|b| b.is_ascii_digit())
		&& text.as_bytes()[0] != b'0'
		&& text.len() <= 19
}

fn parse_pager_part(text: &str) -> Option<u64> {
	if !all_ascii_digits_and_leading_nonzero(text) {
		return None;
	}
	let value: u64 = text.parse().ok()?;
	if value > MAX_SAFE_INTEGER {
		return None;
	}
	Some(value)
}

/// TS parsePropertyPager — 유효하면 브랜드 문자열, 아니면 undefined.
pub fn parse_property_pager(value: &str) -> Option<String> {
	let (current_text, total_text) = value.split_once('/')?;
	let current = parse_pager_part(current_text)?;
	let total = parse_pager_part(total_text)?;
	if current > total {
		return None;
	}
	Some(value.to_string())
}

/// TS PropertyWatcherSelectionHandle — 컨텍스트·세대·선택 식별자.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherSelectionHandle {
	pub context_id: i64,
	pub context_unique_id: String,
	pub generation: u64,
	pub selection_id: u64,
}

/// TS PropertyWatcherSelection — strict 스키마(version=3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherSelection {
	pub version: u8,
	pub selection_id: u64,
	pub cells: PropertyRow,
	pub pager: String,
}

fn parse_row(value: &Value) -> Option<PropertyRow> {
	let array = value.as_array()?;
	if array.len() != 8 {
		return None;
	}
	let mut cells = Vec::with_capacity(8);
	for cell in array {
		let text = cell.as_str()?;
		if text.chars().count() > ROW_MAX_CHARS {
			return None;
		}
		cells.push(text.to_string());
	}
	let cells: Vec<String> = cells;
	match <[String; 8]>::try_from(cells) {
		Ok(row) => Some(row),
		Err(_) => None,
	}
}

/// TS parsePropertyWatcherPayload — 64KB 제한 → JSON → strict 스키마 검증.
/// JSON이 깨졌으면(원본의 SyntaxError 분기) undefined, 그 외 형식 오류도 undefined.
pub fn parse_property_watcher_payload(payload: &str) -> Option<PropertyWatcherSelection> {
	if payload.as_bytes().len() > PAYLOAD_MAX_BYTES {
		return None;
	}
	let decoded: Value = serde_json::from_str(payload).ok()?;
	let object = decoded.as_object()?;

	// strict: 허용된 키만 존재해야 한다
	const ALLOWED: [&str; 4] = ["version", "selectionId", "cells", "pager"];
	if object.len() != ALLOWED.len() || !ALLOWED.iter().all(|key| object.contains_key(*key)) {
		return None;
	}

	// version: z.number().literal(3) → 정수 3
	let version = object.get("version")?;
	if version.as_u64() != Some(3) {
		return None;
	}

	// selectionId: int positive max(MAX_SAFE_INTEGER)
	let selection_id = object.get("selectionId")?.as_u64()?;
	if selection_id == 0 || selection_id > MAX_SAFE_INTEGER {
		return None;
	}

	let cells = parse_row(object.get("cells")?)?;
	let pager = parse_property_pager(object.get("pager")?.as_str()?)?;

	Some(PropertyWatcherSelection {
		version: 3,
		selection_id,
		cells,
		pager,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn valid_payload() -> String {
		r#"{"version":3,"selectionId":7,"cells":["a","b","c","d","e","f","g","h"],"pager":"3/20"}"#
			.to_string()
	}

	#[test]
	fn parses_valid_payload() {
		let parsed = parse_property_watcher_payload(&valid_payload()).expect("valid");
		assert_eq!(parsed.version, 3);
		assert_eq!(parsed.selection_id, 7);
		assert_eq!(parsed.pager, "3/20");
		assert_eq!(parsed.cells[0], "a");
		assert_eq!(parsed.cells[7], "h");
	}

	#[test]
	fn rejects_payload_over_64kb() {
		let filler = "x".repeat(70_000);
		let payload = format!(
			r#"{{"version":3,"selectionId":1,"cells":["{filler}","b","c","d","e","f","g","h"],"pager":"1/2"}}"#
		);
		assert!(parse_property_watcher_payload(&payload).is_none());
	}

	#[test]
	fn rejects_invalid_json() {
		assert!(parse_property_watcher_payload("{not json").is_none());
		assert!(parse_property_watcher_payload("").is_none());
		assert!(parse_property_watcher_payload("[]").is_none());
	}

	#[test]
	fn strict_rejects_extra_or_missing_keys() {
		let extra = r#"{"version":3,"selectionId":1,"cells":["a","b","c","d","e","f","g","h"],"pager":"1/2","extra":1}"#;
		assert!(parse_property_watcher_payload(extra).is_none());
		let missing = r#"{"version":3,"selectionId":1,"cells":["a","b","c","d","e","f","g","h"]}"#;
		assert!(parse_property_watcher_payload(missing).is_none());
	}

	#[test]
	fn version_must_be_literal_three() {
		for version in ["2", "4", "0", "\"3\"", "3.5", "null"] {
			let payload = format!(
				r#"{{"version":{version},"selectionId":1,"cells":["a","b","c","d","e","f","g","h"],"pager":"1/2"}}"#
			);
			assert!(
				parse_property_watcher_payload(&payload).is_none(),
				"version={version} 이 통과하면 안 된다"
			);
		}
	}

	#[test]
	fn selection_id_must_be_positive_within_safe_integer() {
		for id in ["0", "-1", "1.5", "9007199254740993", "null", "\"1\""] {
			let payload = format!(
				r#"{{"version":3,"selectionId":{id},"cells":["a","b","c","d","e","f","g","h"],"pager":"1/2"}}"#
			);
			assert!(
				parse_property_watcher_payload(&payload).is_none(),
				"selectionId={id} 이 통과하면 안 된다"
			);
		}
		// 최댓값은 통과
		let ok = format!(
			r#"{{"version":3,"selectionId":{},"cells":["a","b","c","d","e","f","g","h"],"pager":"1/2"}}"#,
			MAX_SAFE_INTEGER
		);
		assert!(parse_property_watcher_payload(&ok).is_some());
	}

	#[test]
	fn cells_must_be_exactly_eight_short_strings() {
		let mut cells = r#"["a","b","c","d","e","f","g","h"]"#.to_string();
		// 8개 초과/미만
		let seven = r#"["a","b","c","d","e","f","g"]"#;
		let nine = r#"["a","b","c","d","e","f","g","h","i"]"#;
		for c in [seven, nine] {
			let payload = format!(r#"{{"version":3,"selectionId":1,"cells":{c},"pager":"1/2"}}"#);
			assert!(parse_property_watcher_payload(&payload).is_none(), "cells={c}");
		}
		// 8개가 아니지만 배열이 아닌 값
		let not_array = r#""abcdefgh""#;
		let payload = format!(r#"{{"version":3,"selectionId":1,"cells":{not_array},"pager":"1/2"}}"#);
		assert!(parse_property_watcher_payload(&payload).is_none());

		// 각 셀 4096자 이하(4097자 거부)
		let long = "x".repeat(4097);
		cells = format!(r#"["{long}","b","c","d","e","f","g","h"]"#);
		let payload = format!(r#"{{"version":3,"selectionId":1,"cells":{cells},"pager":"1/2"}}"#);
		assert!(parse_property_watcher_payload(&payload).is_none());
		// 4096자면 통과
		let exact = "x".repeat(4096);
		cells = format!(r#"["{exact}","b","c","d","e","f","g","h"]"#);
		let payload = format!(r#"{{"version":3,"selectionId":1,"cells":{cells},"pager":"1/2"}}"#);
		assert!(parse_property_watcher_payload(&payload).is_some());
	}

	#[test]
	fn pager_accepts_only_current_over_total_from_one() {
		for ok in ["1/1", "1/20", "12/120", "9/9", "1/9007199254740991"] {
			assert_eq!(parse_property_pager(ok).as_deref(), Some(ok), "{ok}");
		}
		for bad in [
			"0/5",     // 현재 0 (^[1-9])
			"01/5",    // 선행 0
			"1/0",     // 전체 0
			"6/5",     // 현재 > 전체
			"-1/5",    // 음수
			"1/5/9",   // 슬래시 2개
			"1",       // 슬래시 없음
			"",        // 빈 값
			"a/5",     // 비숫자
			"1/5 ",    // 공백
			"9007199254740993/9007199254740993", // safe integer 초과
			"1/9007199254740992", // 전체 safe 초과
		] {
			assert!(parse_property_pager(bad).is_none(), "`{bad}` 가 통과하면 안 된다");
		}
	}

	#[test]
	fn pager_invalid_in_payload_rejects_whole_payload() {
		let payload = r#"{"version":3,"selectionId":1,"cells":["a","b","c","d","e","f","g","h"],"pager":"6/5"}"#;
		assert!(parse_property_watcher_payload(payload).is_none());
	}

	#[test]
	fn building_status_strings_match_ts() {
		assert_eq!(PropertyBuildingStatus::Pending.as_str(), "pending");
		assert_eq!(PropertyBuildingStatus::Absent.as_str(), "absent");
		assert_eq!(PropertyBuildingStatus::Failed.as_str(), "failed");
		assert_eq!(PropertyBuildingStatus::Present.as_str(), "present");
		assert_eq!(
			PropertyBuildingStatus::from_terminal(PropertyBuildingTerminalStatus::Present),
			PropertyBuildingStatus::Present
		);
		assert_eq!(PropertyBuildingStatusUpdateResult::Applied.as_str(), "applied");
		assert_eq!(PropertyBuildingStatusUpdateResult::Stale.as_str(), "stale");
	}

	#[test]
	fn row_and_selection_types_match_ts_shapes() {
		// 타입 레벨 계약: 8칸 행과 핸들 필드가 TS와 같은 이름/개수를 갖는다
		let handle = PropertyWatcherSelectionHandle {
			context_id: 3,
			context_unique_id: "ctx".to_string(),
			generation: 1,
			selection_id: 9,
		};
		assert_eq!(handle.selection_id, 9);
		let cells: PropertyRow = [
			"a".into(), "b".into(), "c".into(), "d".into(),
			"e".into(), "f".into(), "g".into(), "h".into(),
		];
		assert_eq!(cells.len(), 8);
	}
}
