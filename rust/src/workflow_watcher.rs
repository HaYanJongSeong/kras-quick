// TS scripts/kras-workflow-watcher.ts — runKrasWorkflowWatcher 정식 루프.
// 조회 재시도 / 건물 선택 / 층-호 로딩 재시도 / 층-호 다중 선택 차단 / 주소 복귀를
// 훅(hook)으로 분기한다. 훅이 없으면 TS와 동일하게 absent로 취급한다.

use crate::auto_batch::AutoStage1Result;
use crate::auto_batch::AutoBuilding;

/// KRAS 대상 URL(증명서 열람 페이지).
pub const KRAS_TARGET_URL: &str = "https://www.kras.go.kr/kras/cert/certView.do";

/// TS isKrasTargetUrl — 단일 출처인 property_watcher_page의 판별에 위임한다.
pub fn is_kras_target_url(value: &str) -> bool {
	crate::property_watcher_page::is_kras_watcher_target_url(value)
}

/// TS KrasDialogKind — 대화상자 분류 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KrasDialogKind {
	Confirm,
	NeedLookup,
	LoginExpired,
	Error,
}

impl KrasDialogKind {
	/// TS discriminated 값 문자열.
	pub fn as_str(self) -> &'static str {
		match self {
			KrasDialogKind::Confirm => "confirm",
			KrasDialogKind::NeedLookup => "need_lookup",
			KrasDialogKind::LoginExpired => "login_expired",
			KrasDialogKind::Error => "error",
		}
	}
}

/// TS classifyKrasDialogText — 같은 순서의 판정을 그대로 따른다.
/// (캡차 제출 JS의 classify와 동일한 규칙이며, 여기는 호출부용 순수 함수로 porting했다.)
pub fn classify_kras_dialog_text(text: &str) -> Option<KrasDialogKind> {
	if text.contains("도면정보") || text.contains("도호가 불일치") || text.contains("출력축척") {
		return Some(KrasDialogKind::Confirm);
	}
	if text.contains("신청물건을 먼저 조회") {
		return Some(KrasDialogKind::NeedLookup);
	}
	if text.contains("로그인이 만료되었습니다") {
		return Some(KrasDialogKind::LoginExpired);
	}
	// TS /보안문자|일치하지 않|오류|실패/ — 리터럴 대안이라 contains와 동일
	if text.contains("보안문자") || text.contains("일치하지 않") || text.contains("오류")
		|| text.contains("실패")
	{
		return Some(KrasDialogKind::Error);
	}
	None
}

/// TS shouldWaitForFloorRoomOptions — 집합건물이거나 옵션이 있으면 층-호를 기다린다.
pub fn should_wait_for_floor_room_options(
	building_label: &str,
	options_len: usize,
) -> bool {
	building_label.contains("(집합)") || options_len > 0
}

/// TS isKrasLoginUrl — pathname에 /login 이 포함되면 로그인 페이지.
pub fn is_kras_login_url(value: &str) -> bool {
	let Some(rest) = value.split_once("//").map(|(_, r)| r) else {
		return false;
	};
	let Some((_host, path)) = rest.split_once('/') else {
		return false;
	};
	let path = path.split(['?', '#']).next().unwrap_or(path);
	path.to_ascii_lowercase().contains("login")
}

/// TS isKrasLoginPage — pathname에 login 문자열이 있으면 로그인 페이지.
pub fn is_kras_login_page(url: &str) -> bool {
	let Some(rest) = url.split_once("//").map(|(_, r)| r) else {
		return false;
	};
	let Some((_host, path)) = rest.split_once('/') else {
		return false;
	};
	let path = path.split(['?', '#']).next().unwrap_or(path);
	path.to_ascii_lowercase().contains("login")
}

/// TS KrasEvaluationResult — 조회 평가 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupResult {
	Lookup {
		has_building: bool,
		building_options: Vec<AutoBuilding>,
		no_parcel: bool,
	},
	EvaluationError {
		code: String,
	},
}

/// TS KrasWorkflowPage — 루프가 쓰는 페이지 연산.
pub trait WorkflowPage {
	fn url(&self) -> String;
	/// TS page.lookup — lookupEvaluation(mode: lookup).
	fn lookup(&self, address: &str) -> Result<LookupResult, Box<dyn std::error::Error>>;
	/// TS page.readBuildingOptions — 조회 없이 현재 #bldgSeCd 옵션 재읽기.
	fn read_building_options(&self) -> Vec<AutoBuilding>;
	/// TS page.selectBuilding — 건물 선택 후 층-호 옵션 반환(타이밍이면 Err).
	fn select_building(&self, value: &str) -> Result<Vec<AutoBuilding>, Box<dyn std::error::Error>>;
	/// TS page.selectFloorRoom — #flrSeCd 선택.
	fn select_floor_room(&self, value: &str) -> Result<(), Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorStatus {
	Empty,
	Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorAction {
	Retry,
	Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupAction {
	Retry,
	Manual,
}

/// TS chooseBuilding 반환값. `Absent`는 훅이 정의되지 않은 경우.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildingChoice {
	Index(u64),
	Value(String),
	Address,
	Reject,
	Absent,
}

/// TS chooseFloorRoom 반환값. `Absent`는 훅이 정의되지 않은 경우.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloorChoice {
	Index(u64),
	None,
	Reject,
	Absent,
}

/// TS KrasWorkflowHooks — 기본 구현은 "훅 없음"을 뜻한다.
pub trait WorkflowHooks {
	fn on_stage1(&mut self, _stage1: &AutoStage1Result) {}

	fn choose_building(&mut self, _options: &[AutoBuilding]) -> BuildingChoice {
		BuildingChoice::Absent
	}

	fn choose_floor_room(
		&mut self,
		_options: &[AutoBuilding],
		_building: &AutoBuilding,
	) -> FloorChoice {
		FloorChoice::Absent
	}

	/// `None`이면 훅 미정의 — 층-호 재시도 불가.
	fn handle_floor_room_options(
		&mut self,
		_building: &AutoBuilding,
		_status: FloorStatus,
	) -> Option<FloorAction> {
		None
	}

	/// `None`이면 훅 미정의 — 조회 실패 시 throw(TS 동일).
	fn handle_lookup_error(&mut self) -> Option<LookupAction> {
		None
	}
}

/// 훅이 없는 기본 구현.
pub struct NoHooks;
impl WorkflowHooks for NoHooks {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowStage2 {
	pub status: String,
	pub address: String,
	pub building: AutoBuilding,
	pub floor_room: Option<AutoBuilding>,
	pub reason: Option<String>,
	pub floor_room_options: Vec<AutoBuilding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowResult {
	pub stage1: AutoStage1Result,
	pub stage2: Option<WorkflowStage2>,
	pub awaiting_user: bool,
	pub return_to_address: bool,
}

fn blocked_stage1(_address: &str, reason: &str) -> AutoStage1Result {
	AutoStage1Result {
		status: "blocked".to_string(),
		building_options: Vec::new(),
		reason: Some(reason.to_string()),
	}
}

fn blocked_stage2(
	address: &str,
	building: &AutoBuilding,
	options: &[AutoBuilding],
) -> WorkflowStage2 {
	WorkflowStage2 {
		status: "blocked".to_string(),
		address: address.to_string(),
		building: building.clone(),
		floor_room: None,
		reason: Some("floor_room_selection_required".to_string()),
		floor_room_options: options.to_vec(),
	}
}

/// 층-호 로딩 타이미인지(TS isFloorRoomLoadTimeoutError).
fn is_floor_room_load_timeout(error: &str) -> bool {
	error.contains("층-호명칭 목록 로딩 시간이 초과되었습니다")
}

/// TS runKrasWorkflowWatcher — 조회 → stage1 → 건물 선택 → 층-호 선택 → stage2 기록.
pub fn run_kras_workflow_watcher<P: WorkflowPage, H: WorkflowHooks>(
	page: &P,
	raw_address: &str,
	hooks: &mut H,
	reuse_loaded_options: bool,
	stage1_out: &mut AutoStage1Result,
	stage2_out: &mut Option<WorkflowStage2>,
) -> Result<WorkflowResult, Box<dyn std::error::Error>> {
	let address = raw_address.trim().to_string();
	if !is_kras_target_url(&page.url()) {
		return Err("KRAS 대상 페이지가 아닙니다.".into());
	}

	// TS: reuseLoadedOptions === true 면 조회를 건너뛰고 현재 옵션을 재사용
	let mut lookup = if reuse_loaded_options {
		let loaded = page.read_building_options();
		if !loaded.is_empty() {
			LookupResult::Lookup {
				has_building: true,
				building_options: loaded,
				no_parcel: false,
			}
		} else {
			page.lookup(&address)?
		}
	} else {
		page.lookup(&address)?
	};

	// TS: while (lookup.kind === "evaluation_error") → 훅에 따라 재시도/수동
	while let LookupResult::EvaluationError { code } = &lookup {
		match hooks.handle_lookup_error() {
			None => {
				return Err(format!("Stage 1 조회 실패: {code}").into());
			}
			Some(LookupAction::Manual) => {
				let stage1 = blocked_stage1(&address, "no_building");
				*stage1_out = stage1.clone();
				hooks.on_stage1(&stage1);
				return Ok(WorkflowResult {
					stage1,
					stage2: None,
					awaiting_user: false,
					return_to_address: false,
				});
			}
			Some(LookupAction::Retry) => {
				// "지금 다시 확인" — 조회 버튼을 다시 누르지 않고 옵션을 재읽는다
				let re_read = page.read_building_options();
				lookup = LookupResult::Lookup {
					has_building: !re_read.is_empty(),
					building_options: re_read,
					no_parcel: false,
				};
			}
		}
	}

	let (has_building, options, no_parcel) = match lookup {
		LookupResult::Lookup { has_building, building_options, no_parcel } => {
			(has_building, building_options, no_parcel)
		}
		LookupResult::EvaluationError { code } => {
			return Err(format!("Stage 1 조회 결과가 아닙니다: {code}").into());
		}
	};

	let stage1 = if no_parcel {
		blocked_stage1(&address, "no_parcel")
	} else if !has_building || options.is_empty() {
		blocked_stage1(&address, "no_building")
	} else {
		AutoStage1Result {
			status: "completed".to_string(),
			building_options: options,
			reason: None,
		}
	};
	hooks.on_stage1(&stage1);
	*stage1_out = stage1.clone();

	if stage1.status != "completed" && stage1.reason.as_deref() != Some("no_building") {
		return Ok(WorkflowResult { stage1, stage2: None, awaiting_user: false, return_to_address: false });
	}

	// TS: chooseBuilding이 undefined 훅이면 awaitingUser
	let choice = hooks.choose_building(&stage1.building_options);
	if stage1.reason.as_deref() == Some("no_building") && choice != BuildingChoice::Reject {
		return Ok(WorkflowResult { stage1, stage2: None, awaiting_user: false, return_to_address: false });
	}
	if let BuildingChoice::Absent = choice {
		return Ok(WorkflowResult { stage1, stage2: None, awaiting_user: true, return_to_address: false });
	}
	if let BuildingChoice::Address = choice {
		return Ok(WorkflowResult {
			stage1,
			stage2: None,
			awaiting_user: false,
			return_to_address: true,
		});
	}

	let building = match choice {
		BuildingChoice::Value(value) => stage1
			.building_options
			.iter()
			.find(|b| b.value == value)
			.cloned()
			.ok_or("선택한 건물 식별자가 현재 조회 목록에 없습니다.")?,
		BuildingChoice::Index(idx) => stage1
			.building_options
			.iter()
			.find(|b| b.index == idx)
			.cloned()
			.ok_or("선택한 건물 번호가 유효하지 않습니다.")?,
		BuildingChoice::Reject => {
			// TS: 건물 자동 선택 거부 → 빈 건물명의 수동 stage2
			let stage2 = WorkflowStage2 {
				status: "completed".to_string(),
				address: stage1_addr(&stage1, &address),
				building: AutoBuilding { index: 0, value: String::new(), label: String::new() },
				floor_room: None,
				reason: None,
				floor_room_options: Vec::new(),
			};
			*stage2_out = Some(stage2.clone());
			return Ok(WorkflowResult { stage1, stage2: Some(stage2), awaiting_user: false, return_to_address: false });
		}
		BuildingChoice::Absent | BuildingChoice::Address => unreachable!("앞에서 이미 반환함"),
	};

	// TS: 층-호 옵션 읽기 — empty/timeout 은 훅이 "retry"면 재시도
	let mut floor_room_options: Vec<AutoBuilding> = loop {
		match page.select_building(&building.value) {
			Ok(list) => {
				if list.is_empty() {
					match hooks.handle_floor_room_options(&building, FloorStatus::Empty) {
						Some(FloorAction::Retry) => continue,
						_ => break Vec::new(),
					}
				}
				break list;
			}
			Err(e) => {
				let message = e.to_string();
				if !is_floor_room_load_timeout(&message) {
					return Err(e);
				}
				match hooks.handle_floor_room_options(&building, FloorStatus::Timeout) {
					Some(FloorAction::Retry) => continue,
					_ => break Vec::new(),
				}
			}
		}
	};

	if floor_room_options.is_empty()
		&& should_wait_for_floor_room_options(&building.label, floor_room_options.len())
		&& hooks.handle_floor_room_options(&building, FloorStatus::Empty).is_none()
	{
		return Err("집합건물 층-호명칭 목록을 불러오지 못했습니다. 세션 상태를 확인한 뒤 다시 시도하세요.".into());
	}

	let mut floor_room: Option<AutoBuilding> = None;
	if floor_room_options.len() == 1 {
		floor_room = floor_room_options.first().cloned();
		if let Some(selected) = &floor_room {
			page.select_floor_room(&selected.value)?;
		}
	} else if floor_room_options.len() > 1 {
		let choice = hooks.choose_floor_room(&floor_room_options, &building);
		let index = match choice {
			FloorChoice::Absent | FloorChoice::Reject => {
				let blocked = blocked_stage2(&stage1_addr(&stage1, &address), &building, &floor_room_options);
				*stage2_out = Some(blocked.clone());
				return Ok(WorkflowResult { stage1, stage2: Some(blocked), awaiting_user: true, return_to_address: false });
			}
			FloorChoice::None => None,
			FloorChoice::Index(idx) => Some(idx),
		};
		if let Some(idx) = index {
			floor_room = floor_room_options.iter().find(|f| f.index == idx).cloned();
			let selected = floor_room
				.as_ref()
				.ok_or("선택한 층-호명칭 번호가 유효하지 않습니다.")?;
			page.select_floor_room(&selected.value)?;
		}
	}
	// 빈 옵션이어도 진행 — TS는 floorRoom 없이 stage2를 완료로 기록한다.

	let stage2 = WorkflowStage2 {
		status: "completed".to_string(),
		address: stage1_addr(&stage1, &address),
		building: building.clone(),
		floor_room,
		reason: None,
		floor_room_options,
	};
	*stage2_out = Some(stage2.clone());
	Ok(WorkflowResult { stage1, stage2: Some(stage2), awaiting_user: false, return_to_address: false })
}

fn stage1_addr(_stage1: &AutoStage1Result, fallback: &str) -> String {
	// AutoStage1Result에는 address가 없어 호출자 주소를 그대로 쓴다(TS는 normalizeAddress).
	fallback.to_string()
}


// ——— 실제 CDP 페이지 구현 (TS createPlaywrightPage 대응) ———

/// TS readBuildingOptionsEvaluation — 조회 없이 현재 #bldgSeCd 옵션 재읽기.
const READ_BUILDING_OPTIONS_JS: &str = r#"() => {
	const building = document.querySelector('#bldgSeCd');
	if (building === null) return [];
	const normalize = (v) => (v ?? '').replace(/\s+/g, ' ').trim();
	return Array.from(building.options)
		.map((option, index) => ({ index, value: option.value, label: normalize(option.textContent) }))
		.filter(({ label, value }) => value !== '' && label !== '' && label !== '선택' && label !== '선택사항없음');
}"#;

/// TS selectFloorRoomEvaluation — #flrSeCd 선택 후 값 검증까지 수행.
const SELECT_FLOOR_ROOM_JS: &str = r#"(value) => {
	const select = document.querySelector('#flrSeCd');
	if (select === null) throw new Error('층-호명칭 필드를 찾지 못했습니다.');
	const option = Array.from(select.options).find((c) => c.value === value);
	if (option === undefined) throw new Error('선택한 층-호명칭을 찾지 못했습니다.');
	select.value = value;
	select.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	select.dispatchEvent(new Event('input', { bubbles: true }));
	select.dispatchEvent(new Event('change', { bubbles: true }));
	if (select.value !== value) throw new Error('층-호명칭 선택값 검증에 실패했습니다.');
	return true;
}"#;

fn eval_value(
	surface: &crate::cdp_surface::CdpSurface,
	expression: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
	let params = serde_json::json!({
		"expression": expression,
		"returnByValue": true,
		"awaitPromise": true
	});
	let result = surface.send_cdp("Runtime.evaluate", params)?;
	if let Some(details) = result.get("exceptionDetails") {
		let desc = details
			.get("exception")
			.and_then(|e| e.get("description").or(e.get("text")))
			.and_then(|v| v.as_str())
			.unwrap_or("evaluate failed");
		return Err(desc.to_string().into());
	}
	Ok(result.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

impl WorkflowPage for crate::cdp_surface::CdpSurface {
	fn url(&self) -> String {
		crate::kras_auto_stages::KrasOzViewerSurface::url(self)
	}

	/// TS page.lookup — 조회 평가 원본을 그대로 해석한다(재시도용으로 오류도 값으로 받는다).
	fn lookup(&self, address: &str) -> Result<LookupResult, Box<dyn std::error::Error>> {
		let payload = crate::stage12::lookup_evaluation_payload(self, address)?;
		match payload.get("kind").and_then(|v| v.as_str()) {
			Some("lookup") => {
				let options = crate::auto_batch::AutoStage1Result::from_value(&payload)
					.building_options;
				Ok(LookupResult::Lookup {
					has_building: payload
						.get("hasBuilding")
						.and_then(|v| v.as_bool())
						.unwrap_or(false),
					building_options: options,
					no_parcel: payload
						.get("noParcel")
						.and_then(|v| v.as_bool())
						.unwrap_or(false),
				})
			}
			Some("evaluation_error") => Ok(LookupResult::EvaluationError {
				code: payload
					.get("code")
					.and_then(|v| v.as_str())
					.unwrap_or("")
					.to_string(),
			}),
			_ => Err(format!("KRAS Stage 1 예상치 못한 결과: {payload}").into()),
		}
	}

	fn read_building_options(&self) -> Vec<AutoBuilding> {
		let expression = format!("({})()", READ_BUILDING_OPTIONS_JS);
		match eval_value(self, &expression) {
			Ok(v) => crate::auto_batch::AutoStage1Result::from_value(&serde_json::json!({
				"buildingOptions": v.get("value").cloned().unwrap_or(serde_json::Value::Null)
			}))
			.building_options,
			Err(_) => Vec::new(),
		}
	}

	fn select_building(&self, value: &str) -> Result<Vec<AutoBuilding>, Box<dyn std::error::Error>> {
		let list = crate::stage12::select_building(self, value)?;
		Ok(list
			.iter()
			.filter_map(|o| {
				Some(AutoBuilding {
					index: o.get("index")?.as_u64()?,
					value: o.get("value")?.as_str()?.to_string(),
					label: o.get("label")?.as_str()?.to_string(),
				})
			})
			.collect())
	}

	fn select_floor_room(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> {
		let expression = format!("({})({})", SELECT_FLOOR_ROOM_JS, serde_json::json!(value));
		eval_value(self, &expression)?;
		Ok(())
	}
}

#[cfg(test)]
mod dialog_classification_tests {
	use super::*;

	// TS tests/kras-workflow-watcher.test.ts

	#[test]
	fn classifies_expired_login_dialog() {
		assert_eq!(
			classify_kras_dialog_text("로그인이 만료되었습니다. 로그인 후 다시 이용해주세요. OK"),
			Some(KrasDialogKind::LoginExpired)
		);
		assert_eq!(
			classify_kras_dialog_text("로그인이 만료되었습니다. 로그인 후 다시 이용해주세요. OK")
				.map(|kind| kind.as_str()),
			Some("login_expired")
		);
	}

	#[test]
	fn confirms_the_output_scale_warning_dialog() {
		let text = "열람하고자 하는 지번은 도호가 불일치하여 출력축척에 맞추어 도면을 재설정합니다.";
		assert_eq!(classify_kras_dialog_text(text), Some(KrasDialogKind::Confirm));
	}

	#[test]
	fn detects_need_lookup_and_error_dialogs() {
		assert_eq!(
			classify_kras_dialog_text("신청물건을 먼저 조회하세요."),
			Some(KrasDialogKind::NeedLookup)
		);
		assert_eq!(
			classify_kras_dialog_text("보안문자가 일치하지 않습니다."),
			Some(KrasDialogKind::Error)
		);
		assert_eq!(classify_kras_dialog_text("알 수 없는 안내 문구"), None);
	}

	#[test]
	fn waits_for_floor_room_only_for_aggregate_buildings_or_existing_options() {
		assert!(should_wait_for_floor_room_options("A동 (집합)", 0));
		assert!(should_wait_for_floor_room_options("일반건물", 3));
		assert!(!should_wait_for_floor_room_options("일반건물", 0));
	}
}

#[cfg(test)]
mod target_url_consistency_tests {
	use super::*;
	use crate::property_watcher_page::is_kras_watcher_target_url;

	#[test]
	fn workflow_and_watcher_page_agree_on_kras_target() {
		for candidate in [
			"https://www.kras.go.kr/kras/cert/certView.do",
			"https://www.kras.go.kr/kras/cert/certView.do?uri=/x",
			"https://www.kras.go.kr/mainView.do",
			"https://www.kras.go.kr/login/loginView.do?uri=/kras/cert/certView.do",
			"https://kras.go.kr/kras/cert/certView.do",
			"nonsense",
		] {
			assert_eq!(
				is_kras_target_url(candidate),
				is_kras_watcher_target_url(candidate),
				"불일치: {candidate}"
			);
		}
		// 로그인 페이지는 대상이 아니다(워처 가드가 로그인을 거부함)
		assert!(!is_kras_target_url(
			"https://www.kras.go.kr/login/loginView.do?uri=/kras/cert/certView.do"
		));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;

	fn building(index: u64, label: &str) -> AutoBuilding {
		AutoBuilding { index, value: format!("v{index}"), label: label.to_string() }
	}

	#[derive(Default)]
	struct FakePage {
		url: String,
		lookup_result: RefCell<Option<LookupResult>>,
		loaded_options: Vec<AutoBuilding>,
		floor_options: Vec<AutoBuilding>,
		select_building_error: RefCell<Option<String>>,
		floor_room_selected: RefCell<Vec<String>>,
		lookup_calls: RefCell<u32>,
		select_building_calls: RefCell<u32>,
	}

	impl WorkflowPage for FakePage {
		fn url(&self) -> String {
			self.url.clone()
		}
		fn lookup(&self, _address: &str) -> Result<LookupResult, Box<dyn std::error::Error>> {
			*self.lookup_calls.borrow_mut() += 1;
			Ok(self
				.lookup_result
				.borrow()
				.clone()
				.unwrap_or(LookupResult::Lookup { has_building: false, building_options: vec![], no_parcel: false }))
		}
		fn read_building_options(&self) -> Vec<AutoBuilding> {
			self.loaded_options.clone()
		}
		fn select_building(&self, _value: &str) -> Result<Vec<AutoBuilding>, Box<dyn std::error::Error>> {
			*self.select_building_calls.borrow_mut() += 1;
			if let Some(msg) = self.select_building_error.borrow().clone() {
				return Err(msg.into());
			}
			Ok(self.floor_options.clone())
		}
		fn select_floor_room(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> {
			self.floor_room_selected.borrow_mut().push(value.to_string());
			Ok(())
		}
	}

	fn page_on_target() -> FakePage {
		FakePage { url: KRAS_TARGET_URL.to_string(), ..Default::default() }
	}

	#[test]
	fn login_and_target_url_detection() {
		assert!(is_kras_target_url("https://www.kras.go.kr/kras/cert/certView.do"));
		assert!(!is_kras_target_url("https://www.kras.go.kr/mainView.do"));
		assert!(is_kras_login_url("https://www.kras.go.kr/login/loginView.do?uri=/x"));
		assert!(!is_kras_login_url("https://www.kras.go.kr/kras/cert/certView.do"));
		assert!(is_kras_login_page("https://www.kras.go.kr/login/loginView.do"));
	}

	#[test]
	fn rejects_non_target_page() {
		let page = FakePage { url: "https://example.com/".into(), ..Default::default() };
		let err = run_kras_workflow_watcher(&page, "주소", &mut NoHooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect_err("비대상 페이지는 거부");
		assert!(err.to_string().contains("KRAS 대상 페이지"));
	}

	#[test]
	fn lookup_error_without_hook_throws_like_ts() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::EvaluationError { code: "KRAS_EVALUATION_ERROR".into() });
		let err = run_kras_workflow_watcher(&page, "주소", &mut NoHooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect_err("훅이 없으면 throw");
		assert!(err.to_string().contains("Stage 1 조회 실패"));
		assert_eq!(*page.lookup_calls.borrow(), 1);
	}

	#[test]
	fn lookup_error_retry_rereads_options_without_new_lookup() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::EvaluationError { code: "E".into() });
		page.loaded_options = vec![building(1, "(일반)건물")];
		let mut hooks = RetryOnce::default();
		let mut stage1 = AutoStage1Result::default();
		let result = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut stage1, &mut None)
			.expect("재시도 후 성공");
		// TS: 재시도는 조회를 다시 하지 않고 readBuildingOptions만 사용
		assert_eq!(*page.lookup_calls.borrow(), 1);
		assert_eq!(stage1.status, "completed");
		assert_eq!(result.stage1.building_options.len(), 1);
	}

	#[derive(Default)]
	struct RetryOnce {
		seen: bool,
	}
	impl WorkflowHooks for RetryOnce {
		fn handle_lookup_error(&mut self) -> Option<LookupAction> {
			if self.seen {
				return None;
			}
			self.seen = true;
			Some(LookupAction::Retry)
		}
		fn choose_building(&mut self, _options: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Index(1)
		}
	}

	#[test]
	fn missing_choose_building_hook_returns_awaiting_user() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(1, "(일반)건물")],
			no_parcel: false,
		});
		let mut stage1 = AutoStage1Result::default();
		let r = run_kras_workflow_watcher(&page, "주소", &mut NoHooks, false, &mut stage1, &mut None)
			.expect("awaitingUser");
		assert!(r.awaiting_user);
		assert!(r.stage2.is_none());
	}

	#[test]
	fn choose_building_reject_writes_manual_stage2() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(1, "(일반)건물")],
			no_parcel: false,
		});
		let mut hooks = RejectBuilding::default();
		let mut stage1 = AutoStage1Result::default();
		let mut stage2 = None;
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut stage1, &mut stage2)
			.expect("manual stage2");
		let s2 = r.stage2.expect("stage2");
		assert_eq!(s2.status, "completed");
		assert_eq!(s2.building.label, "", "빈 건물명의 수동 stage2");
		assert!(s2.building.value.is_empty());
		assert!(page.floor_room_selected.borrow().is_empty(), "층-호를 선택하지 않는다");
	}

	#[derive(Default)]
	struct RejectBuilding;

	#[test]
	fn land_reaches_stage2_when_parcel_has_no_building() {
		let page = page_on_target();
		let result = run_kras_workflow_watcher(
			&page, "서울특별시 종로구 행촌동 210-730", &mut RejectBuilding,
			false, &mut AutoStage1Result::default(), &mut None,
		).expect("토지 stage2");
		let stage2 = result.stage2.expect("건물 없는 토지도 출력 대상으로 전달");
		assert_eq!(stage2.status, "completed");
		assert!(stage2.building.value.is_empty());
		assert_eq!(*page.select_building_calls.borrow(), 0);
	}

	struct SelectValue;
	impl WorkflowHooks for SelectValue {
		fn choose_building(&mut self, _options: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Value("v1".into())
		}
	}

	#[test]
	fn building_identity_survives_reordered_indices() {
		let page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![
				AutoBuilding { index: 1, value: "v2".into(), label: "다른 건물".into() },
				AutoBuilding { index: 2, value: "v1".into(), label: "요청 건물".into() },
			],
			no_parcel: false,
		});
		let result = run_kras_workflow_watcher(
			&page, "주소", &mut SelectValue, false,
			&mut AutoStage1Result::default(), &mut None,
		).expect("value 선택");
		let selected = result.stage2.expect("stage2").building;
		assert_eq!(selected.value, "v1");
		assert_eq!(selected.index, 2);
	}

	#[test]
	fn missing_building_value_does_not_select_another_building() {
		let page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true, building_options: vec![building(2, "다른 건물")], no_parcel: false,
		});
		let result = run_kras_workflow_watcher(
			&page, "주소", &mut SelectValue, false,
			&mut AutoStage1Result::default(), &mut None,
		);
		assert!(result.is_err());
		assert_eq!(*page.select_building_calls.borrow(), 0);
	}
	impl WorkflowHooks for RejectBuilding {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Reject
		}
	}

	#[test]
	fn single_floor_room_is_selected_automatically() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(2, "A동 (집합)")],
			no_parcel: false,
		});
		page.floor_options = vec![building(10, "3층 301호")];
		let mut hooks = IndexTwo::default();
		let mut stage1 = AutoStage1Result::default();
		let mut stage2 = None;
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut stage1, &mut stage2)
			.expect("ok");
		assert_eq!(r.stage2.as_ref().unwrap().floor_room.as_ref().unwrap().index, 10);
		assert_eq!(*page.floor_room_selected.borrow(), vec!["v10".to_string()]);
	}

	#[derive(Default)]
	struct IndexTwo;
	impl WorkflowHooks for IndexTwo {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Index(2)
		}
	}

	#[test]
	fn multiple_floors_without_hook_become_blocked_awaiting_user() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(2, "A동 (집합)")],
			no_parcel: false,
		});
		page.floor_options = vec![building(10, "301호"), building(11, "401호")];
		let mut hooks = IndexTwo::default();
		let mut stage1 = AutoStage1Result::default();
		let mut stage2 = None;
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut stage1, &mut stage2)
			.expect("blocked");
		assert!(r.awaiting_user);
		let s2 = r.stage2.expect("blocked stage2");
		assert_eq!(s2.status, "blocked");
		assert_eq!(s2.reason.as_deref(), Some("floor_room_selection_required"));
		assert_eq!(s2.floor_room_options.len(), 2);
		assert!(s2.floor_room.is_none());
	}

	#[test]
	fn floor_timeout_without_hook_gives_up_without_retry() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(2, "A동 (집합)")],
			no_parcel: false,
		});
		*page.select_building_error.borrow_mut() =
			Some("층-호명칭 목록 로딩 시간이 초과되었습니다.".to_string());
		let mut hooks = IndexTwo::default();
		let err = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect_err("훅이 없으면 재시도 없이 집합건물은 실패한다");
		// TS: 재시도 훅이 없고 집합건물인데 층-호가 비면 throw
		assert!(err.to_string().contains("집합건물 층-호명칭 목록"));
		assert_eq!(*page.select_building_calls.borrow(), 1, "재시도하지 않는다");
	}

	#[test]
	fn floor_timeout_without_hook_but_ordinary_building_continues() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(1, "(일반)건물")],
			no_parcel: false,
		});
		*page.select_building_error.borrow_mut() =
			Some("층-호명칭 목록 로딩 시간이 초과되었습니다.".to_string());
		let mut hooks = IndexOne::default();
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect("일반건물은 층-호가 없어도 진행");
		assert_eq!(*page.select_building_calls.borrow(), 1);
		assert!(r.stage2.expect("stage2").floor_room.is_none());
	}

	#[derive(Default)]
	struct IndexOne;
	impl WorkflowHooks for IndexOne {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Index(1)
		}
	}

	#[test]
	fn floor_timeout_with_retry_hook_retries() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(2, "A동 (집합)")],
			no_parcel: false,
		});
		*page.select_building_error.borrow_mut() =
			Some("층-호명칭 목록 로딩 시간이 초과되었습니다.".to_string());
		let mut hooks = RetryFloorOnce::default();
		let _ = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None);
		assert_eq!(*page.select_building_calls.borrow(), 2, "retry 한 번 후 재시도");
	}

	#[derive(Default)]
	struct RetryFloorOnce {
		seen: bool,
	}
	impl WorkflowHooks for RetryFloorOnce {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Index(2)
		}
		fn handle_floor_room_options(&mut self, _b: &AutoBuilding, _s: FloorStatus) -> Option<FloorAction> {
			if self.seen {
				return Some(FloorAction::Continue);
			}
			self.seen = true;
			Some(FloorAction::Retry)
		}
	}

	#[test]
	fn non_timeout_error_from_select_building_is_propagated() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(2, "A동 (집합)")],
			no_parcel: false,
		});
		*page.select_building_error.borrow_mut() = Some("건물구분 필드를 찾지 못했습니다.".into());
		let mut hooks = IndexTwo::default();
		let err = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect_err("타이미가 아닌 오류는 전파");
		assert!(err.to_string().contains("건물구분"));
		assert_eq!(*page.select_building_calls.borrow(), 1);
	}

	#[test]
	fn reuse_loaded_options_skips_lookup_entirely() {
		let mut page = page_on_target();
		page.loaded_options = vec![building(3, "(일반)건물")];
		let mut hooks = IndexThree::default();
		let mut stage1 = AutoStage1Result::default();
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, true, &mut stage1, &mut None)
			.expect("reuse");
		assert_eq!(*page.lookup_calls.borrow(), 0, "조회를 건너뛴다");
		assert_eq!(r.stage1.building_options.len(), 1);
	}

	#[derive(Default)]
	struct IndexThree;
	impl WorkflowHooks for IndexThree {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Index(3)
		}
	}

	#[test]
	fn choose_building_address_returns_return_to_address() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::Lookup {
			has_building: true,
			building_options: vec![building(1, "(일반)건물")],
			no_parcel: false,
		});
		let mut hooks = AddressChoice::default();
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect("address");
		assert!(r.return_to_address);
		assert!(r.stage2.is_none());
	}

	#[derive(Default)]
	struct AddressChoice;
	impl WorkflowHooks for AddressChoice {
		fn choose_building(&mut self, _o: &[AutoBuilding]) -> BuildingChoice {
			BuildingChoice::Address
		}
	}

	#[test]
	fn no_building_and_no_parcel_become_blocked_stage1() {
		for (lookup, expected) in [
			(
				LookupResult::Lookup { has_building: false, building_options: vec![], no_parcel: false },
				"no_building",
			),
			(
				LookupResult::Lookup { has_building: false, building_options: vec![], no_parcel: true },
				"no_parcel",
			),
		] {
			let mut page = page_on_target();
			*page.lookup_result.borrow_mut() = Some(lookup);
			let mut stage1 = AutoStage1Result::default();
			let r = run_kras_workflow_watcher(&page, "주소", &mut NoHooks, false, &mut stage1, &mut None)
				.expect("blocked stage1");
			assert_eq!(r.stage1.status, "blocked");
			assert_eq!(r.stage1.reason.as_deref(), Some(expected));
			assert!(!r.awaiting_user);
		}
	}

	#[test]
	fn manual_lookup_action_writes_blocked_stage1_without_throwing() {
		let mut page = page_on_target();
		*page.lookup_result.borrow_mut() = Some(LookupResult::EvaluationError { code: "E".into() });
		let mut hooks = ManualLookup::default();
		let r = run_kras_workflow_watcher(&page, "주소", &mut hooks, false, &mut AutoStage1Result::default(), &mut None)
			.expect("manual");
		assert_eq!(r.stage1.status, "blocked");
		assert_eq!(r.stage1.reason.as_deref(), Some("no_building"));
		assert!(!r.awaiting_user);
	}

	#[derive(Default)]
	struct ManualLookup;
	impl WorkflowHooks for ManualLookup {
		fn handle_lookup_error(&mut self) -> Option<LookupAction> {
			Some(LookupAction::Manual)
		}
	}
}
