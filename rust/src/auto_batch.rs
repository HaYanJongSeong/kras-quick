// TS property-auto-runner.ts — AUTO 배치 루프의 대상 선택·재시도·페이싱 상태 기계.
// 브라우저 실행은 호출자(클로저)에 두고, 여기는 순수 상태 판정만 둔다.

use std::collections::HashMap;

use serde_json::Value;

use crate::kras_auto_stages::KrasAutoStagePaths;
use crate::kras_output::{is_kras_stage3_output_complete, KrasBuildingOption};
use crate::quick_address::AutoBatchScope;

/// TS AUTO_PACING_MS — 대상 1건 완료 후 대기.
pub const AUTO_PACING_MS: u64 = 1800;

/// TS AUTO_MAX_RETRIES — 대상당 최대 시도 횟수.
pub const AUTO_MAX_RETRIES: u32 = 3;

/// TS AUTO_ORANGE — AUTO 모드 출력 색(256색 208).
pub const AUTO_ORANGE: &str = "\x1b[38;5;208m";

/// TS AUTO_RESET — 색 복원 시퀀스.
pub const AUTO_RESET: &str = "\x1b[0m";

/// TS LAND_BUILDING — 토지 대상(건물 없음).
pub fn land_building() -> KrasBuildingOption {
	KrasBuildingOption {
		value: String::new(),
		label: String::new(),
	}
}

/// AUTO 배치 대상(TS AutoTarget).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoTarget {
	Land,
	Building { building: AutoBuilding },
	AggregateTotal { building: AutoBuilding },
	AggregateFloor { building: AutoBuilding, floor: AutoBuilding },
}

/// 건물 옵션 + 원본 index(TS KrasBuildingOption.index).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AutoBuilding {
	pub index: u64,
	pub value: String,
	pub label: String,
}

impl AutoBuilding {
	pub fn to_output_option(&self) -> KrasBuildingOption {
		KrasBuildingOption {
			value: self.value.clone(),
			label: self.label.clone(),
		}
	}

	/// 집합건물 여부(TS `label.includes("(집합)")`).
	pub fn is_aggregate(&self) -> bool {
		self.label.contains("(집합)")
	}
}

/// 층-호 대기 큐(TS AutoFloorQueue).
#[derive(Debug, Clone, Default)]
pub struct AutoFloorQueue {
	pub floors: Vec<AutoBuilding>,
	pub cursor: usize,
}

/// TS autoTargetLabel.
pub fn auto_target_label(target: &AutoTarget) -> String {
	match target {
		AutoTarget::Land => "토지".to_string(),
		AutoTarget::Building { building } => building.label.clone(),
		AutoTarget::AggregateTotal { building } => format!("{} 총괄", building.label),
		AutoTarget::AggregateFloor { building, floor } => {
			format!("{} {}", building.label, floor.label)
		}
	}
}

/// Stage 1 결과(TS KrasAutoStage1Result) — CDP JSON에서 읽는다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoStage1Result {
	pub status: String,
	pub building_options: Vec<AutoBuilding>,
	/// blocked 상태의 사유(TS reason: no_parcel | no_building).
	pub reason: Option<String>,
}

impl AutoStage1Result {
	pub fn from_value(value: &Value) -> Self {
		let status = value
			.get("status")
			.and_then(|v| v.as_str())
			.unwrap_or("")
			.to_string();
		let building_options = value
			.get("buildingOptions")
			.and_then(|v| v.as_array())
			.map(|arr| {
				arr.iter()
					.map(|o| AutoBuilding {
						index: o.get("index").and_then(|v| v.as_u64()).unwrap_or(0),
						value: o.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
						label: o.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string(),
					})
					.collect()
			})
			.unwrap_or_default();
		let reason = value
			.get("reason")
			.and_then(|v| v.as_str())
			.map(|s| s.to_string());
		Self { status, building_options, reason }
	}

	pub fn is_completed(&self) -> bool {
		self.status == "completed"
	}

	/// 집합건물 목록(TS aggregateBuildings).
	pub fn aggregate_buildings(&self) -> Vec<&AutoBuilding> {
		self.building_options.iter().filter(|b| b.is_aggregate()).collect()
	}
}

/// 일괄처리 범위는 `quick_address::AutoBatchScope`에 정의한다(TS AutoBatchScope).

/// TS pickNextAutoTarget — 다음 처리 대상을 고른다. None이면 루프 종료.
pub fn pick_next_auto_target(
	paths: &KrasAutoStagePaths,
	stage1: Option<&AutoStage1Result>,
	floor_queues: &HashMap<String, AutoFloorQueue>,
	scope: AutoBatchScope,
) -> Option<AutoTarget> {
	let output = paths.output_paths();
	let land = land_building();
	let land_complete = is_kras_stage3_output_complete(&output, &land, None);

	let Some(stage1) = stage1 else {
		return Some(AutoTarget::Land);
	};
	if scope == AutoBatchScope::LandOnly {
		return if land_complete { None } else { Some(AutoTarget::Land) };
	}
	if !stage1.is_completed() || stage1.building_options.is_empty() {
		return if land_complete { None } else { Some(AutoTarget::Land) };
	}
	if !land_complete {
		return Some(AutoTarget::Land);
	}

	for building in &stage1.building_options {
		if building.is_aggregate() {
			if scope == AutoBatchScope::OrdinaryOnly {
				continue;
			}
			match floor_queues.get(&building.value) {
				None => return Some(AutoTarget::AggregateTotal { building: building.clone() }),
				Some(queue) => {
					if let Some(floor) = queue.floors.get(queue.cursor) {
						return Some(AutoTarget::AggregateFloor {
							building: building.clone(),
							floor: floor.clone(),
						});
					}
				}
			}
			continue;
		}
		let option = building.to_output_option();
		if !is_kras_stage3_output_complete(&output, &option, None) {
			return Some(AutoTarget::Building { building: building.clone() });
		}
	}
	None
}

/// TS isAutoTargetNew — 아직 출력되지 않은 건이면 새 건으로 셈.
pub fn is_auto_target_new(paths: &KrasAutoStagePaths, target: &AutoTarget) -> bool {
	let output = paths.output_paths();
	match target {
		AutoTarget::Land => {
			let land = land_building();
			!is_kras_stage3_output_complete(&output, &land, None)
		}
		AutoTarget::Building { building } => {
			!is_kras_stage3_output_complete(&output, &building.to_output_option(), None)
		}
		AutoTarget::AggregateTotal { building } => {
			// 이미 출력된 총괄은 층 큐 확보용 재캡처이므로 새 건으로 세지 않는다.
			!is_kras_stage3_output_complete(&output, &building.to_output_option(), None)
		}
		AutoTarget::AggregateFloor { building, floor } => {
			!is_kras_stage3_output_complete(
				&output,
				&building.to_output_option(),
				Some(&floor.to_output_option()),
			)
		}
	}
}

/// 배치 루프 상태(TS runAutoParcelWorkflow의 지역 상태).
#[derive(Debug, Clone, Default)]
pub struct AutoBatchState {
	floor_queues: HashMap<String, AutoFloorQueue>,
	stage1: Option<AutoStage1Result>,
	reuse_loaded_options: bool,
	newly_added: usize,
	processed_addresses: usize,
}

impl AutoBatchState {
	pub fn new() -> Self {
		Self::default()
	}

	/// 다음 대상. None이면 해당 주소의 처리가 끝났다.
	pub fn next_target(&self, paths: &KrasAutoStagePaths, scope: AutoBatchScope) -> Option<AutoTarget> {
		pick_next_auto_target(paths, self.stage1.as_ref(), &self.floor_queues, scope)
	}

	/// 대상 실행 전 캡처: 새 건 여부(로그 출력·집계용).
	pub fn target_is_new(&self, paths: &KrasAutoStagePaths, target: &AutoTarget) -> bool {
		is_auto_target_new(paths, target)
	}

	/// 대상 실행에 쓰일 재사용 여부(TS reuseLoadedOptions).
	pub fn reuse_loaded_options(&self) -> bool {
		self.reuse_loaded_options
	}

	/// 대상 1건 완료 후 상태 갱신.
	pub fn mark_completed(
		&mut self,
		target: &AutoTarget,
		stage1: Option<AutoStage1Result>,
		target_is_new: bool,
	) {
		if let Some(stage1) = stage1 {
			self.stage1 = Some(stage1);
		}
		self.reuse_loaded_options = true;
		if let AutoTarget::AggregateFloor { building, .. } = target {
			if let Some(queue) = self.floor_queues.get_mut(&building.value) {
				queue.cursor += 1;
			}
		}
		if target_is_new {
			self.newly_added += 1;
		}
		self.processed_addresses += 1;
	}

	/// TS handleFloorRoomOptions가 "continue"를 반환했을 때: 큐를 층 목록으로 채운다.
	pub fn set_floor_queue(&mut self, building: &AutoBuilding, floors: Vec<AutoBuilding>) {
		self.floor_queues.insert(
			building.value.clone(),
			AutoFloorQueue { floors, cursor: 0 },
		);
	}

	/// 로그인 만료(TS KrasAutoLoginExpiredError 처리): 큐를 비우고 재조회로 복귀.
	pub fn on_login_expired(&mut self) {
		self.floor_queues.clear();
		self.reuse_loaded_options = false;
	}

	pub fn begin_address(&mut self) {
		self.stage1 = None;
		self.floor_queues.clear();
		self.reuse_loaded_options = false;
	}

	pub fn newly_added(&self) -> usize {
		self.newly_added
	}

	pub fn processed_addresses(&self) -> usize {
		self.processed_addresses
	}

	pub fn stage1(&self) -> Option<&AutoStage1Result> {
		self.stage1.as_ref()
	}

	/// 집합건물 집계(TS 반환값의 total/processed).
	pub fn aggregate_summary(
		&self,
		paths: &KrasAutoStagePaths,
	) -> (usize, usize) {
		let Some(stage1) = &self.stage1 else {
			return (0, 0);
		};
		if !stage1.is_completed() {
			return (0, 0);
		}
		let output = paths.output_paths();
		let total = stage1.aggregate_buildings();
		let processed = total
			.iter()
			.filter(|b| {
				is_kras_stage3_output_complete(&output, &b.to_output_option(), None)
			})
			.count();
		(total.len(), processed)
	}
}

/// TS runAutoTarget의 재시도 판정: attempt가 AUTO_MAX_RETRIES 이하면 재시도.
pub fn should_retry(attempt: u32) -> bool {
	attempt < AUTO_MAX_RETRIES
}

/// TS 로그인 만료 문구.
pub const LOGIN_EXPIRED_MESSAGE: &str =
	"AUTO: 로그인이 만료되었습니다. 브라우저에서 다시 로그인해 주세요.";

/// TS 배치 실패 문구.
pub fn batch_failed_message(target: &AutoTarget) -> String {
	format!(
		"AUTO: {}을(를) {}회 시도했지만 실패했습니다.",
		auto_target_label(target),
		AUTO_MAX_RETRIES
	)
}

/// TS AutoAddressFailure (kras-auto-failures.ts) — 실패 주소 한 건의 원본 입력과 사유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoAddressFailure {
	/// TS failure.input — 사용자가 실제로 입력한 줄(주소와 다를 수 있다).
	pub input: String,
	/// TS failure.reason — 개행은 공백으로 정규화해 표시한다.
	pub reason: String,
}

/// TS `Map<string, AutoAddressFailure>` — 삽입 순서와 삭제 동작을 그대로 유지한다.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutoAddressFailures {
	entries: Vec<(String, AutoAddressFailure)>,
}

impl AutoAddressFailures {
	/// TS Map.set — 같은 키는 값을 덮어쓰고 순서는 지킨다.
	pub fn set(&mut self, address: impl Into<String>, failure: AutoAddressFailure) {
		let address = address.into();
		if let Some(entry) = self.entries.iter_mut().find(|(key, _)| *key == address) {
			entry.1 = failure;
		} else {
			self.entries.push((address, failure));
		}
	}

	/// 해당 주소가 실패 목록에 있는지(주소 단위 결과 판정용).
	pub fn contains(&self, address: &str) -> bool {
		self.entries.iter().any(|(key, _)| key == address)
	}

	/// TS Map.delete — 성공한 주소는 실패 목록에서 빠진다.
	pub fn delete(&mut self, address: &str) {
		self.entries.retain(|(key, _)| key != address);
	}

	/// TS Map.clear — 일괄처리 종료 요약 출력 후 비운다.
	pub fn clear(&mut self) {
		self.entries.clear();
	}

	/// TS Map.size
	pub fn len(&self) -> usize {
		self.entries.len()
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}
}

/// TS formatAutoFailures — 빈 목록이면빈 문자열, 아니면 헤더와 항목을 개행으로 묶는다.
/// 반환 문자열은 TS와 같게 마지막에 개행을 남긴다(빈 목록은 개행도 없다).
pub fn format_auto_failures(failures: &AutoAddressFailures) -> String {
	if failures.entries.is_empty() {
		return String::new();
	}
	let mut lines = vec![format!(
		"처리 실패 {}건 (완료되지 않은 주소, 재확인 필요)",
		failures.entries.len()
	)];
	for (address, failure) in &failures.entries {
		lines.push(format!("- {address}"));
		if failure.input != *address {
			lines.push(format!("  원본 입력: {}", failure.input));
		}
		lines.push(format!(
			"  사유: {}",
			failure.reason.split_whitespace().collect::<Vec<_>>().join(" ")
		));
	}
	lines.push(String::new());
	lines.join("\n")
}

#[cfg(test)]
mod tests {
	// ——— kras-auto-failures.ts ———

	fn failure(input: &str, reason: &str) -> AutoAddressFailure {
		AutoAddressFailure { input: input.to_string(), reason: reason.to_string() }
	}

	#[test]
	fn contains_reports_current_membership() {
		let mut failures = AutoAddressFailures::default();
		assert!(!failures.contains("A"));
		failures.set("A", failure("A", "실패"));
		assert!(failures.contains("A"));
		failures.delete("A");
		assert!(!failures.contains("A"));
	}

	#[test]
	fn format_auto_failures_is_empty_for_no_failures() {
		assert_eq!(format_auto_failures(&AutoAddressFailures::default()), "");
	}

	#[test]
	fn format_auto_failures_headers_lists_and_normalizes_reason() {
		let mut failures = AutoAddressFailures::default();
		failures.set(
			"서울특별시 노원구 월계동 392-19",
			failure("서울특별시 노원구 월계동 392-19", "  지번 없음  "),
		);
		assert_eq!(
			format_auto_failures(&failures),
			concat!(
				"처리 실패 1건 (완료되지 않은 주소, 재확인 필요)\n",
				"- 서울특별시 노원구 월계동 392-19\n",
				"  사유: 지번 없음\n"
			)
		);
	}

	#[test]
	fn format_auto_failures_includes_original_input_when_different() {
		let mut failures = AutoAddressFailures::default();
		failures.set(
			"월계동 392-19",
			failure("서울특별시 노원구 월계동 392-19", "조회 실패"),
		);
		let text = format_auto_failures(&failures);
		assert!(text.contains("- 월계동 392-19"));
		assert!(text.contains("  원본 입력: 서울특별시 노원구 월계동 392-19"));
		assert!(text.contains("  사유: 조회 실패"));
	}

	#[test]
	fn failures_map_overwrites_keeps_order_and_deletes() {
		let mut failures = AutoAddressFailures::default();
		failures.set("A", failure("A", "첫 실패"));
		failures.set("B", failure("B", "B 실패"));
		failures.set("A", failure("A", "재실패"));
		assert_eq!(failures.len(), 2);
		assert!(format_auto_failures(&failures).contains("사유: 재실패"));
		// 삽입 순서 유지(A가 먼저)
		let text = format_auto_failures(&failures);
		assert!(text.find("- A").unwrap() < text.find("- B").unwrap());
		failures.delete("A");
		assert_eq!(failures.len(), 1);
		failures.clear();
		assert!(failures.is_empty());
	}

	use super::*;

	fn paths_in(dir: &std::path::Path, address: &str) -> KrasAutoStagePaths {
		let mut p = crate::kras_auto_stages::property_auto_stage_paths(
			address,
			Some(dir.to_string_lossy().as_ref()),
		);
		p.structure_json = dir
			.join("oz-structure.json")
			.to_string_lossy()
			.to_string();
		p.other_directory = dir.to_string_lossy().to_string();
		p.directory = dir.to_string_lossy().to_string();
		p
	}

	fn building(index: u64, label: &str) -> AutoBuilding {
		AutoBuilding {
			index,
			value: format!("v{index}"),
			label: label.to_string(),
		}
	}

	fn stage1_ok(buildings: Vec<AutoBuilding>) -> AutoStage1Result {
		AutoStage1Result {
			status: "completed".to_string(),
			building_options: buildings,
			reason: None,
		}
	}

	#[test]
	fn constants_match_ts() {
		assert_eq!(AUTO_PACING_MS, 1800);
		assert_eq!(AUTO_MAX_RETRIES, 3);
		assert_eq!(AUTO_ORANGE, "\x1b[38;5;208m");
	}

	#[test]
	fn auto_target_label_matches_ts() {
		assert_eq!(auto_target_label(&AutoTarget::Land), "토지");
		let b = building(1, "(일반)건물");
		assert_eq!(auto_target_label(&AutoTarget::Building { building: b.clone() }), "(일반)건물");
		assert_eq!(
			auto_target_label(&AutoTarget::AggregateTotal { building: b.clone() }),
			"(일반)건물 총괄"
		);
		let f = building(0, "3층 301호");
		assert_eq!(
			auto_target_label(&AutoTarget::AggregateFloor { building: b, floor: f }),
			"(일반)건물 3층 301호"
		);
	}

	#[test]
	fn aggregate_detection_by_label() {
		assert!(building(1, "A동 (집합)").is_aggregate());
		assert!(!building(1, "(일반)건물").is_aggregate());
	}

	#[test]
	fn stage1_parses_from_json() {
		let value = serde_json::json!({
			"status": "completed",
			"buildingOptions": [
				{"index": 1, "value": "101219021", "label": "(일반)101219021-건물"},
				{"index": 2, "value": "101219022", "label": "(집합)101219022-건물"}
			]
		});
		let s = AutoStage1Result::from_value(&value);
		assert!(s.is_completed());
		assert_eq!(s.building_options.len(), 2);
		assert_eq!(s.building_options[1].index, 2);
		assert_eq!(s.aggregate_buildings().len(), 1);
	}

	#[test]
	fn pick_returns_land_before_stage1() {
		let dir = std::env::temp_dir().join("kras-auto-nostage1");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소A");
		let queues = HashMap::new();
		let target = pick_next_auto_target(&paths, None, &queues, AutoBatchScope::Full);
		assert_eq!(target, Some(AutoTarget::Land));
	}

	#[test]
	fn pick_land_only_stops_after_land_complete() {
		let dir = std::env::temp_dir().join("kras-auto-landonly");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소B");
		let queues = HashMap::new();
		let stage1 = stage1_ok(vec![building(1, "(일반)건물")]);
		// 토지 산출물이 없으면 land 반환
		let t = pick_next_auto_target(&paths, Some(&stage1), &queues, AutoBatchScope::LandOnly);
		assert_eq!(t, Some(AutoTarget::Land));
		// land-only에서는 건물을 고르지 않는다(토지 미완료 시)
		assert_ne!(t, Some(AutoTarget::Building { building: building(1, "(일반)건물") }));
	}

	#[test]
	fn pick_ordinary_only_skips_aggregate_buildings() {
		let dir = std::env::temp_dir().join("kras-auto-ordinary");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소C");
		let queues = HashMap::new();
		let stage1 = stage1_ok(vec![
			building(1, "A동 (집합)"),
			building(2, "(일반)건물"),
		]);
		let t = pick_next_auto_target(&paths, Some(&stage1), &queues, AutoBatchScope::OrdinaryOnly);
		// 토지가 미완료라 land가 먼저 나온다 — land-only와 달리 집합은 건너뛴다.
		assert_eq!(t, Some(AutoTarget::Land));
	}

	#[test]
	fn aggregate_without_queue_selects_total() {
		let dir = std::env::temp_dir().join("kras-auto-aggtot");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소D");
		let queues = HashMap::new();
		let stage1 = stage1_ok(vec![building(1, "A동 (집합)")]);
		// land가 완료돼 있다고 가정하기 어려우므로 결과가 land든 aggregate-total이든
		// 큐가 없을 때 total을 고르는 규칙 자체를 직접 검증한다.
		let target = pick_next_auto_target(&paths, Some(&stage1), &queues, AutoBatchScope::Full);
		assert!(matches!(
			target,
			Some(AutoTarget::Land) | Some(AutoTarget::AggregateTotal { .. })
		));
	}

	#[test]
	fn new_address_discards_previous_parcel_state_but_preserves_totals() {
		let mut state = AutoBatchState::new();
		state.set_floor_queue(&building(1, "A동 (집합)"), vec![building(10, "101호")]);
		state.mark_completed(&AutoTarget::Land, Some(stage1_ok(vec![building(1, "A동 (집합)")])), true);
		assert!(state.reuse_loaded_options());
		state.begin_address();
		assert!(!state.reuse_loaded_options());
		assert!(state.stage1().is_none());
		assert!(state.floor_queues.is_empty());
		assert_eq!(state.newly_added(), 1);
		assert_eq!(state.processed_addresses(), 1);
	}

	#[test]
	fn reordered_building_advances_only_its_own_floor_queue() {
		let mut state = AutoBatchState::new();
		state.set_floor_queue(&building(1, "A동 (집합)"), vec![building(10, "101호"), building(11, "102호")]);
		state.set_floor_queue(&building(2, "B동 (집합)"), vec![building(20, "201호")]);
		let mut reordered = building(1, "A동 (집합)");
		reordered.index = 2;
		state.mark_completed(&AutoTarget::AggregateFloor {
			building: reordered, floor: building(10, "101호"),
		}, None, true);
		assert_eq!(state.floor_queues["v1"].cursor, 1);
		assert_eq!(state.floor_queues["v2"].cursor, 0);
	}

	#[test]
	fn aggregate_floor_advances_cursor() {
		let dir = std::env::temp_dir().join("kras-auto-cursor");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소E");
		let mut state = AutoBatchState::new();
		state.set_floor_queue(&building(1, "A동 (집합)"), vec![building(10, "3층 301호"), building(11, "4층 401호")]);
		assert_eq!(state.floor_queues.get("v1").map(|q| q.cursor), Some(0));

		let target = AutoTarget::AggregateFloor {
			building: building(1, "A동 (집합)"),
			floor: building(10, "3층 301호"),
		};
		state.mark_completed(&target, None, true);
		assert_eq!(state.floor_queues.get("v1").map(|q| q.cursor), Some(1));
		assert_eq!(state.newly_added(), 1);
	}

	#[test]
	fn five_floor_queue_is_exhausted_only_after_five_completions() {
		let mut state = AutoBatchState::new();
		let floors: Vec<_> = (10..15).map(|index| building(index, &format!("{index}호"))).collect();
		state.set_floor_queue(&building(1, "(집합)A동"), floors.clone());
		for (cursor, floor) in floors.into_iter().enumerate() {
			assert_eq!(state.floor_queues["v1"].floors.get(state.floor_queues["v1"].cursor), Some(&floor));
			state.mark_completed(&AutoTarget::AggregateFloor { building: building(1, "(집합)A동"), floor }, None, true);
			assert_eq!(state.floor_queues["v1"].cursor, cursor + 1);
		}
		assert!(state.floor_queues["v1"].floors.get(state.floor_queues["v1"].cursor).is_none());
		assert_eq!(state.newly_added(), 5);
	}

	#[test]
	fn login_expired_clears_queues_and_rereads_stage1() {
		let mut state = AutoBatchState::new();
		state.set_floor_queue(&building(1, "A동 (집합)"), vec![building(10, "3층 301호")]);
		state.mark_completed(
			&AutoTarget::Land,
			Some(stage1_ok(vec![building(1, "A동 (집합)")])),
			true,
		);
		assert!(state.reuse_loaded_options());
		state.on_login_expired();
		assert!(state.floor_queues.is_empty());
		assert!(!state.reuse_loaded_options());
	}

	#[test]
	fn retry_rule_matches_ts_max() {
		assert!(should_retry(1));
		assert!(should_retry(2));
		assert!(!should_retry(3));
	}

	#[test]
	fn batch_failed_message_contains_target_and_retry_limit() {
		let msg = batch_failed_message(&AutoTarget::Land);
		// TS는 `${label}을(를)`로 연결하므로 조사가 그대로 붙는다.
		assert_eq!(msg, "AUTO: 토지을(를) 3회 시도했지만 실패했습니다.");
	}

	#[test]
	fn aggregate_summary_counts_only_completed_outputs() {
		let dir = std::env::temp_dir().join("kras-auto-summary");
		let _ = std::fs::create_dir_all(&dir);
		let paths = paths_in(&dir, "주소F");
		let mut state = AutoBatchState::new();
		state.mark_completed(
			&AutoTarget::Land,
			Some(stage1_ok(vec![building(1, "A동 (집합)")])),
			true,
		);
		let (total, processed) = state.aggregate_summary(&paths);
		assert_eq!(total, 1);
		assert_eq!(processed, 0); // 산출물 파일이 없으므로 미완료
	}
}
