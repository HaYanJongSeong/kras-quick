// TS scripts/kras-lookup.ts — CDP로 KRAS 타깃 페이지를 골라 조회/다크 모드를 수행한다.
// TS는 playwright connectOverCDP를 쓰고 여기는 같은 엔드포인트의 CDP 타깃 목록을 쓴다.

use crate::cdp_surface::CdpSurface;
use crate::kras_dark::KRAS_DARK_MODE_CSS;
use crate::stage12::{lookup_evaluation_payload_with_dark, KrasPage};

/// TS CDP_ENDPOINT (kras-lookup.ts)
pub const KRAS_LOOKUP_CDP_ENDPOINT: &str = "http://127.0.0.1:9222";
/// TS CDP_OPTIONS.timeout = 5000
pub const KRAS_LOOKUP_CDP_TIMEOUT_MS: u64 = 5000;

/// TS BUILDING_PRESENT
pub const BUILDING_PRESENT: &str = "부동산 종합증명서 조회결과 건물 등장";
/// TS BUILDING_UNAVAILABLE
pub const BUILDING_UNAVAILABLE: &str = "부동산 종합증명서 조회결과 건물 조회 불가";

/// TS KrasPageError — name이 CLI 실패 줄에 그대로 실린다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasPageError(String);

impl KrasPageError {
	pub fn new(message: impl Into<String>) -> Self {
		Self(message.into())
	}

	/// TS error.name
	pub fn name(&self) -> &'static str {
		"KrasPageError"
	}
}

impl std::fmt::Display for KrasPageError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.0)
	}
}

impl std::error::Error for KrasPageError {}

/// TS targetPage — KRAS 조회 대상 페이지는 정확히 하나여야 한다.
pub fn target_page_index(pages: &[crate::property_watcher_runtime::CdpTargetPage]) -> Result<usize, KrasPageError> {
	let matches: Vec<usize> = pages
		.iter()
		.enumerate()
		.filter(|(_, page)| crate::property_watcher_page::is_kras_watcher_target_url(&page.url))
		.map(|(index, _)| index)
		.collect();
	match matches.as_slice() {
		[] => Err(KrasPageError::new("KRAS 조회 페이지를 찾지 못했습니다.")),
		[index] => Ok(*index),
		_ => Err(KrasPageError::new("KRAS 조회 페이지는 정확히 하나여야 합니다.")),
	}
}

/// TS runKrasLookup의 평가·판정부 — 페이지가 이미 골라진 상태에서 조회한다.
/// 다크 모드 요청은 lookupEvaluation에 그대로 전달된다(TS payload와 동일).
pub fn lookup_on_page(
	page: &dyn KrasPage,
	address: &str,
	dark_mode: bool,
) -> Result<String, KrasPageError> {
	let payload = lookup_evaluation_payload_with_dark(
		page,
		address,
		dark_mode,
		if dark_mode { KRAS_DARK_MODE_CSS } else { "" },
	)
	.map_err(|error| KrasPageError::new(format!("KRAS 페이지 평가에 실패했습니다: {error}")))?;
	match payload.get("kind").and_then(|value| value.as_str()) {
		Some("lookup") => Ok(if payload
			.get("hasBuilding")
			.and_then(|value| value.as_bool())
			.unwrap_or(false)
		{
			BUILDING_PRESENT.to_string()
		} else {
			BUILDING_UNAVAILABLE.to_string()
		}),
		// TS: evaluation_error → throw KrasPageError("KRAS 페이지 평가에 실패했습니다.")
		Some("evaluation_error") => {
			Err(KrasPageError::new("KRAS 페이지 평가에 실패했습니다."))
		}
		// TS: fill_only → throw KrasPageError("KRAS 조회 평가 결과가 아닙니다.")
		_ => Err(KrasPageError::new("KRAS 조회 평가 결과가 아닙니다.")),
	}
}

/// TS runKrasLookup — 엔드포인트의 KRAS 타깃을 골라 조회 문장을 돌려준다.
pub fn run_kras_lookup(
	endpoint: &str,
	address: &str,
	dark_mode: bool,
) -> Result<String, Box<dyn std::error::Error>> {
	let pages = crate::property_watcher_runtime::list_cdp_pages(endpoint)?;
	let index = target_page_index(&pages).map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
	let surface = CdpSurface::from_page(endpoint, &pages[index]);
	lookup_on_page(&surface, address, dark_mode).map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
}

/// TS lookupKras(address, darkMode = false) — 기본 엔드포인트로 조회한다.
pub fn lookup_kras(address: &str, dark_mode: bool) -> Result<String, Box<dyn std::error::Error>> {
	run_kras_lookup(KRAS_LOOKUP_CDP_ENDPOINT, address, dark_mode)
}

/// TS runKrasDarkMode — 다크 모드 CSS를 한 번 주입하고 동일한 문장을 돌려준다.
pub fn run_kras_dark_mode(endpoint: &str) -> Result<String, Box<dyn std::error::Error>> {
	let pages = crate::property_watcher_runtime::list_cdp_pages(endpoint)?;
	let index = target_page_index(&pages).map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
	let surface = CdpSurface::from_page(endpoint, &pages[index]);
	crate::kras_dark::apply_kras_dark_mode(&surface)
}

/// TS applyKrasDarkMode() — 기본 엔드포인트로 다크 모드를 적용한다.
pub fn apply_kras_dark_mode() -> Result<String, Box<dyn std::error::Error>> {
	run_kras_dark_mode(KRAS_LOOKUP_CDP_ENDPOINT)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;

	/// 평가 요청을 기록하고 미리 정해 둔 페이로드를 돌려주는 가짜 페이지(TS fake page).
	struct FakePage {
		url: String,
		payload: Result<Value, String>,
		requests: RefCell<Vec<Value>>,
	}

	impl KrasPage for FakePage {
		fn url(&self) -> String {
			self.url.clone()
		}
		fn evaluate(&self, request: &Value) -> Result<Value, Box<dyn std::error::Error>> {
			self.requests.borrow_mut().push(request.clone());
			match &self.payload {
				Ok(value) => Ok(value.clone()),
				Err(message) => Err(message.clone().into()),
			}
		}
	}

	use serde_json::Value;

	fn target_url() -> String {
		"https://www.kras.go.kr/kras/cert/certView.do".to_string()
	}

	fn fake(payload: Value) -> FakePage {
		FakePage {
			url: target_url(),
			payload: Ok(payload),
			requests: RefCell::new(Vec::new()),
		}
	}

	#[test]
	fn maps_lookup_has_building_to_present_line() {
		let page = fake(serde_json::json!({
			"kind": "lookup", "hasBuilding": true, "buildingOptions": [], "noParcel": false
		}));
		assert_eq!(lookup_on_page(&page, "서울특별시 노원구 월계동 392-19", false).unwrap(), BUILDING_PRESENT);
	}

	#[test]
	fn maps_lookup_without_building_to_unavailable_line() {
		let page = fake(serde_json::json!({
			"kind": "lookup", "hasBuilding": false, "buildingOptions": [], "noParcel": false
		}));
		assert_eq!(
			lookup_on_page(&page, "서울특별시 노원구 월계동 392-19", false).unwrap(),
			BUILDING_UNAVAILABLE
		);
	}

	#[test]
	fn evaluation_error_becomes_kras_page_error() {
		let page = fake(serde_json::json!({ "kind": "evaluation_error", "code": "KRAS_EVALUATION_ERROR" }));
		let error = lookup_on_page(&page, "주소", false).expect_err("오류여야 한다");
		assert_eq!(error.to_string(), "KRAS 페이지 평가에 실패했습니다.");
		assert_eq!(error.name(), "KrasPageError");
	}

	#[test]
	fn fill_only_result_is_fatal_lookup_result_error() {
		let page = fake(serde_json::json!({ "kind": "fill_only" }));
		let error = lookup_on_page(&page, "주소", false).expect_err("오류여야 한다");
		assert_eq!(error.to_string(), "KRAS 조회 평가 결과가 아닙니다.");
	}

	#[test]
	fn normalizes_seoul_district_first_input_before_evaluation() {
		let page = fake(serde_json::json!({
			"kind": "lookup", "hasBuilding": false, "buildingOptions": [], "noParcel": false
		}));
		lookup_on_page(&page, "  강서구\t개화동   255-1  ", false).expect("조회");
		let request = page.requests.borrow()[0].clone();
		assert_eq!(
			request.get("address").and_then(|v| v.as_str()),
			Some("서울특별시 강서구 개화동 255-1")
		);
		// 기본 요청은 다크 모드가 꺼져 있다(TS 기본값)
		assert_eq!(request.get("darkMode").and_then(|v| v.as_bool()), Some(false));
	}

	#[test]
	fn dark_mode_request_passes_css_payload() {
		let page = fake(serde_json::json!({
			"kind": "lookup", "hasBuilding": false, "buildingOptions": [], "noParcel": false
		}));
		lookup_on_page(&page, "주소", true).expect("조회");
		let request = page.requests.borrow()[0].clone();
		assert_eq!(request.get("darkMode").and_then(|v| v.as_bool()), Some(true));
		assert_eq!(
			request.get("darkModeCss").and_then(|v| v.as_str()),
			Some(KRAS_DARK_MODE_CSS)
		);
	}

	#[test]
	fn target_page_requires_exactly_one_kras_page() {
		let make = |url: &str| crate::property_watcher_runtime::CdpTargetPage {
			id: "target-1".to_string(),
			url: url.to_string(),
			ws_url: "ws://example/devtools/page/1".to_string(),
		};
		let none = vec![make("https://example.com/")];
		assert_eq!(
			target_page_index(&none).expect_err("0개면 거부"),
			KrasPageError::new("KRAS 조회 페이지를 찾지 못했습니다.")
		);
		let two = vec![make(&target_url()), make(&target_url())];
		assert_eq!(
			target_page_index(&two).expect_err("2개면 거부"),
			KrasPageError::new("KRAS 조회 페이지는 정확히 하나여야 합니다.")
		);
		let one = vec![make("https://example.com/"), make(&target_url())];
		assert_eq!(target_page_index(&one).expect("1개면 통과"), 1);
	}

	#[test]
	fn lookup_defaults_are_non_empty_address_and_false_dark_mode() {
		assert!(!BUILDING_PRESENT.is_empty());
		assert!(!BUILDING_UNAVAILABLE.is_empty());
		assert!(!KRAS_LOOKUP_CDP_ENDPOINT.is_empty());
	}
}
