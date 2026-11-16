// TS kras-page.ts lookupEvaluation (브라우저 내부 DOM 트랜잭션) Rust 포팅.
// CDP Runtime.evaluate로 JS를 전송하는 방식 — 브라우저 내부 로직은 JS로 유지.

use crate::cdp_surface::CdpSurface;
use crate::numeric_ocr::{NumericMode, recognize_auto_captcha_candidate, recognize_numeric_captcha};
use crate::property_address::parse_parcel_address;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct KrasBuildingOption {
	pub index: usize,
	pub value: String,
	pub label: String,
}

#[derive(Debug, Clone)]
pub enum KrasEvaluationResult {
	FillOnly,
	Lookup {
		has_building: bool,
		building_options: Vec<KrasBuildingOption>,
		no_parcel: bool,
	},
	EvaluationError,
}

/// lookupEvaluation JS 소스 (TS 원본 그대로 — CDP Runtime.evaluate로 전송).
/// 브라우저 내부 DOM 트랜잭션은 JS로 실행된다.
/// TS lookupEvaluation 원본(JS) — 타입 제거본을 파일로 보관하고 include_str!로 센다.
/// 조회 완료 훅(certView.fnCheckLedger/fnEmptyRequestInfo)·안정화 루프·noParcel·대화상자 해제를 포함한다.
const LOOKUP_EVALUATION_JS: &str = include_str!("lookup_evaluation.js");

/// KrasPage trait — 브라우저 페이지 추상화
pub trait KrasPage {
	fn url(&self) -> String;
	fn evaluate(&self, request: &Value) -> Result<Value, Box<dyn std::error::Error>>;
}

impl KrasPage for CdpSurface {
	fn url(&self) -> String {
		crate::kras_auto_stages::KrasOzViewerSurface::url(self)
	}
	fn evaluate(&self, request: &Value) -> Result<Value, Box<dyn std::error::Error>> {
		let expr = format!("lookupEvaluation({})", serde_json::to_string(request).unwrap_or_default());
		// lookupEvaluation은 브라우저에 주입되어 있어야 함 — 여기서는 JS 소스를 직접 전송
		let params = serde_json::json!({ "expression": format!("({})({})", LOOKUP_EVALUATION_JS, serde_json::to_string(request).unwrap_or_default()), "returnByValue": true, "awaitPromise": true });
		let result = self.send_cdp("Runtime.evaluate", params)?;
		Ok(result.get("result").cloned().unwrap_or(Value::Null))
	}
}

/// fillKrasAddress — 주소 입력 (TS 분기·오류 타입 그대로)
pub fn fill_kras_address(page: &dyn KrasPage, raw_address: &str) -> Result<(), Box<dyn std::error::Error>> {
	use crate::kras_page::{KrasEvaluationResultError, KrasFillError, KrasFillTargetError};
	if !crate::property_watcher_page::is_kras_watcher_target_url(&page.url()) {
		return Err(KrasFillTargetError.into());
	}
	// TS normalizeAddress(address) — 평가 요청 직전에 정규화한다.
	let address = crate::kras_domain::normalize_address(raw_address);
	let request = serde_json::json!({
		"mode": "fill_only",
		"address": address,
		"darkMode": false,
		"darkModeCss": "",
		"hasLegalRi": address_has_legal_ri(&address),
		"ledgerLabel": ledger_label_for_address(&address),
	});
	let result = page.evaluate(&request)?;
	let result = result.get("value").cloned().unwrap_or(result);
	let kind = result
		.get("kind")
		.and_then(|v| v.as_str())
		.unwrap_or("undefined")
		.to_string();
	match kind.as_str() {
		"fill_only" => Ok(()),
		"evaluation_error" => Err(KrasFillError.into()),
		"lookup" => Err(KrasEvaluationResultError::fill_result().into()),
		other => Err(KrasEvaluationResultError::new(format!(
			"처리할 수 없는 KRAS 평가 결과: {other}"
		))
		.into()),
	}
}

/// lookupKrasBuilding — 건물 조회 결과를 present/absent로 돌려준다 (TS 동일)
pub fn lookup_kras_building(
	page: &dyn KrasPage,
	raw_address: &str,
) -> Result<crate::property_watcher_payload::PropertyBuildingTerminalStatus, Box<dyn std::error::Error>> {
	use crate::kras_page::{KrasEvaluationResultError, KrasLookupError, KrasLookupTargetError};
	use crate::property_watcher_payload::PropertyBuildingTerminalStatus;
	if !crate::property_watcher_page::is_kras_watcher_target_url(&page.url()) {
		return Err(KrasLookupTargetError.into());
	}
	// TS normalizeAddress(address)
	let address = crate::kras_domain::normalize_address(raw_address);
	let request = serde_json::json!({
		"mode": "lookup",
		"address": address,
		"darkMode": false,
		"darkModeCss": "",
		"hasLegalRi": address_has_legal_ri(&address),
		"ledgerLabel": ledger_label_for_address(&address),
	});
	let result = page.evaluate(&request)?;
	let result = result.get("value").cloned().unwrap_or(result);
	let kind = result
		.get("kind")
		.and_then(|v| v.as_str())
		.unwrap_or("undefined")
		.to_string();
	match kind.as_str() {
		"lookup" => Ok(if result.get("hasBuilding").and_then(|v| v.as_bool()).unwrap_or(false) {
			PropertyBuildingTerminalStatus::Present
		} else {
			PropertyBuildingTerminalStatus::Absent
		}),
		"evaluation_error" => Err(KrasLookupError.into()),
		"fill_only" => Err(KrasEvaluationResultError::lookup_result().into()),
		other => Err(KrasEvaluationResultError::new(format!(
			"처리할 수 없는 KRAS 평가 결과: {other}"
		))
		.into()),
	}
}

// ——— 캡차 처리 루프 (TS handleQuickCaptcha parity) ———

/// 캡차 이미지를 캡처하고 OCR로 인식·제출하는 루프.
/// 최대 5회 시도, 실패 시 수동 입력 폴백.
pub fn handle_captcha_loop(
	surface: &crate::cdp_surface::CdpSurface,
) -> Result<(), Box<dyn std::error::Error>> {
	let fallback = auto_captcha_attempts(
		|attempt| {
			let png = capture_captcha_image(surface)?;
			show_captcha_sixel(&png);
			let mode = if attempt % 2 == 0 {
				crate::numeric_ocr::NumericMode::Crop
			} else {
				crate::numeric_ocr::NumericMode::Original
			};
			match crate::numeric_ocr::recognize_numeric_captcha(
				&png, mode, &crate::numeric_ocr::numeric_ocr_root(), 5000, None,
			) {
				Ok(candidate) => candidate,
				Err(error) => {
					crate::task_screen::task_status(&format!("AUTO 판독 실패 ({attempt}/5): {error}\n"));
					None
				}
			}
		},
		|candidate| fill_captcha_and_submit(surface, candidate),
	)?;
	match fallback {
		None => Ok(()),
		Some(tried) => {
			crate::task_screen::task_status("AUTO 처리를 마치고 수동 입력으로 전환합니다.\n");
			manual_captcha_input(surface, tried)
		}
	}
}

// None은 제출 성공, Some은 수동 입력에 전달할 제출 이력이다.
fn auto_captcha_attempts(
	mut recognize: impl FnMut(u8) -> Option<String>,
	mut submit: impl FnMut(&str) -> String,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
	let mut tried = Vec::new();
	for attempt in 1..=5 {
		let Some(candidate) = recognize(attempt) else { continue };
		if !reserve_captcha_candidate(&mut tried, &candidate) { continue }
		crate::task_screen::task_status(&format!("AUTO 보안문자 제출 ({attempt}/5)\n"));
		match submit(&candidate).as_str() {
			"success" => return Ok(None),
			"access_blocked" => return Err("접속 차단 안내가 감지되어 처리를 중단했습니다.".into()),
			"login_expired" => return Err("로그인이 만료되었습니다.".into()),
			"need_lookup" => return Err("건물 재조회가 필요합니다.".into()),
			_ => {}
		}
	}
	Ok(Some(tried))
}

fn reserve_captcha_candidate(tried: &mut Vec<String>, candidate: &str) -> bool {
	if candidate.len() != 5 || !candidate.bytes().all(|byte| byte.is_ascii_digit())
		|| tried.iter().any(|previous| previous == candidate)
	{
		return false;
	}
	tried.push(candidate.to_string());
	true
}

pub fn manual_captcha_input(surface: &CdpSurface, tried: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
	use std::io::Write;
	manual_captcha_entries(tried, || {
		if let Some(png) = capture_captcha_image(surface) { show_captcha_sixel(&png); }
		crate::task_screen::pause_task_screen();
		print!("보안문자 5자리 입력 (Enter=취소) > ");
		std::io::stdout().flush()?;
		let mut reader = crate::quick_address::lock_stdin_reader();
		let input = reader.try_read_line()?;
		if reader.is_eof() && input.is_empty() { Ok(None) } else { Ok(Some(input)) }
	}, |candidate| fill_captcha_and_submit(surface, candidate))
}

fn manual_captcha_entries(
	mut tried: Vec<String>,
	mut read: impl FnMut() -> Result<Option<String>, Box<dyn std::error::Error>>,
	mut submit: impl FnMut(&str) -> String,
) -> Result<(), Box<dyn std::error::Error>> {
	loop {
		let Some(input) = read()? else {
			return Err("보안문자 입력을 취소했습니다.".into());
		};
		if input.trim().is_empty() {
			return Err("보안문자 입력을 취소했습니다.".into());
		}
		let candidate = input.trim();
		if !reserve_captcha_candidate(&mut tried, candidate) {
			println!("새로운 ASCII 숫자 5자리를 입력해 주세요.");
			continue;
		}
		match submit(candidate).as_str() {
			"success" => return Ok(()),
			"access_blocked" => return Err("접속 차단 안내가 감지되어 처리를 중단했습니다.".into()),
			"login_expired" => return Err("로그인이 만료되었습니다.".into()),
			"need_lookup" => return Err("건물 재조회가 필요합니다.".into()),
			_ => println!("보안문자 제출이 실패했습니다. 화면을 확인해 주세요."),
		}
	}
}

#[cfg(test)]
mod captcha_submission_tests {
	use super::{auto_captcha_attempts, manual_captcha_entries, reserve_captcha_candidate};

	#[test]
	fn access_block_stops_after_one_submission_without_fallback() {
		let mut submissions = 0;
		let result = auto_captcha_attempts(|_| Some("01234".into()), |_| {
			submissions += 1;
			"access_blocked".into()
		});
		assert!(result.unwrap_err().to_string().contains("차단"));
		assert_eq!(submissions, 1);
		let result = manual_captcha_entries(Vec::new(), || Ok(Some("01234".into())), |_| "access_blocked".into());
		assert!(result.unwrap_err().to_string().contains("차단"));
	}

	#[test]
	fn manual_input_rejects_auto_history_and_invalid_text_before_success() {
		let mut inputs = ["00017", "１２３４５", "1234", "00123\r\n"].into_iter();
		let mut submitted = Vec::new();
		manual_captcha_entries(vec!["00017".to_string()],
			|| Ok(inputs.next().map(str::to_string)),
			|candidate| { submitted.push(candidate.to_string()); "success".to_string() },
		).unwrap();
		assert_eq!(submitted, vec!["00123"]);
	}

	#[test]
	fn manual_cancel_and_eof_never_submit_or_report_success() {
		for input in [None, Some(""), Some(" \r\n")] {
			let result = manual_captcha_entries(Vec::new(),
				|| Ok(input.map(str::to_string)),
				|_| panic!("취소 후 제출 금지"),
			);
			assert!(result.unwrap_err().to_string().contains("취소"));
		}
	}

	#[test]
	fn manual_failed_candidate_cannot_be_resubmitted() {
		let mut inputs = ["00017", "00017", "00018"].into_iter();
		let mut submitted = Vec::new();
		manual_captcha_entries(Vec::new(),
			|| Ok(inputs.next().map(str::to_string)),
			|candidate| {
				submitted.push(candidate.to_string());
				if candidate == "00018" { "success" } else { "error" }.to_string()
			},
		).unwrap();
		assert_eq!(submitted, vec!["00017", "00018"]);
	}

	#[test]
	fn manual_read_error_is_propagated_without_submission() {
		let result = manual_captcha_entries(Vec::new(),
			|| Err("stdin read failed".into()),
			|_| panic!("입력 오류 후 제출 금지"),
		);
		assert_eq!(result.unwrap_err().to_string(), "stdin read failed");
	}

	#[test]
	fn five_failed_attempts_transfer_history_to_manual_input() {
		let mut attempts = Vec::new();
		let mut submitted = Vec::new();
		let fallback = auto_captcha_attempts(
			|attempt| { attempts.push(attempt); Some(format!("{attempt:05}")) },
			|candidate| { submitted.push(candidate.to_string()); "error".to_string() },
		).unwrap().expect("수동 전환");
		assert_eq!(attempts, vec![1, 2, 3, 4, 5]);
		assert_eq!(submitted, fallback);
		let mut manual_history = fallback;
		assert!(!reserve_captcha_candidate(&mut manual_history, "00001"));
		assert!(reserve_captcha_candidate(&mut manual_history, "00006"));
	}

	#[test]
	fn duplicates_and_invalid_candidates_consume_attempts_without_submission() {
		let candidates = [Some("00017"), Some("00017"), Some("１２３４５"), None, Some("1234")];
		let mut submitted = Vec::new();
		let fallback = auto_captcha_attempts(
			|attempt| candidates[attempt as usize - 1].map(str::to_string),
			|candidate| { submitted.push(candidate.to_string()); "error".to_string() },
		).unwrap().expect("수동 전환");
		assert_eq!(submitted, vec!["00017"]);
		assert_eq!(fallback, submitted);
	}

	#[test]
	fn successful_submission_stops_before_five_and_skips_manual() {
		let mut count = 0;
		let result = auto_captcha_attempts(
			|attempt| { count += 1; Some(format!("{attempt:05}")) },
			|_| "success".to_string(),
		).unwrap();
		assert_eq!(count, 1);
		assert!(result.is_none());
	}

	#[test]
	fn expired_session_aborts_without_retry_or_manual_fallback() {
		let mut count = 0;
		assert!(auto_captcha_attempts(
			|_| { count += 1; Some("00017".to_string()) },
			|_| "login_expired".to_string(),
		).is_err());
		assert_eq!(count, 1);
	}

	#[test]
	fn preserves_leading_zeroes_and_rejects_repeat_submissions() {
		let mut tried = Vec::new();
		assert!(reserve_captcha_candidate(&mut tried, "00017"));
		assert!(!reserve_captcha_candidate(&mut tried, "00017"));
		assert_eq!(tried, vec!["00017"]);
	}

	#[test]
	fn rejects_non_ascii_and_wrong_lengths() {
		let mut tried = Vec::new();
		for candidate in ["1234", "123456", "１２３４５", "12a45", " 12345"] {
			assert!(!reserve_captcha_candidate(&mut tried, candidate));
		}
		assert!(tried.is_empty());
	}
}

// ——— 주소 파생 헬퍼 (TS kras-domain.ts parity) ———

pub fn address_has_legal_ri(address: &str) -> bool {
	address.split_whitespace().any(|t| {
		t.chars().all(|c| c.is_alphabetic() && ('\u{AC00}'..='\u{D7A3}').contains(&c) || c == '·') && t.ends_with("리") && !t.is_empty()
	})
}

pub fn ledger_label_for_address(address: &str) -> &'static str {
	let mountain = address.contains("임야") || {
		let re_like = address.split_whitespace().any(|t| {
			let t = t.trim();
			t.starts_with("산") && t[3..].chars().all(|c| c.is_ascii_digit())
		});
		// fallback: regex (?:^|\s)산\s*\d
		re_like || address.split_whitespace().any(|t| t == "산") // rough
	};
	let closed = address.contains("폐쇄");
	if mountain {
		if closed { "임야(폐쇄)" } else { "임야" }
	} else {
		if closed { "토지(폐쇄)" } else { "토지" }
	}
}

// ——— Stage 1 실행 (TS runKrasAutoStage1 parity) ———
// lookup mode로 주소 입력 + 조회 + 건물 옵션 반환.
/// 조회 평가 원본 페이로드(kind 검증 전) — 워처가 evaluation_error를 값으로 받도록 분리.
pub fn lookup_evaluation_payload(
	page: &dyn KrasPage,
	raw_address: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
	lookup_evaluation_payload_with_dark(page, raw_address, false, "")
}

/// 다크 모드 요청까지 허용하는 조회 평가(TS lookupKras(address, darkMode=true) 경로).
pub fn lookup_evaluation_payload_with_dark(
	page: &dyn KrasPage,
	raw_address: &str,
	dark_mode: bool,
	dark_mode_css: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
	// TS normalizeAddress(address)
	let address = crate::kras_domain::normalize_address(raw_address);
	let has_legal_ri = address_has_legal_ri(&address);
	let ledger_label = ledger_label_for_address(&address);
	let request = serde_json::json!({
		"mode": "lookup",
		"address": address,
		"darkMode": dark_mode,
		"darkModeCss": dark_mode_css,
		"hasLegalRi": has_legal_ri,
		"ledgerLabel": ledger_label,
	});
	let result = page.evaluate(&request)?;
	// CDP Runtime.evaluate 결과는 {type, value} 래퍼 — value를 꺼낸다.
	Ok(result.get("value").cloned().unwrap_or(result))
}

/// TS runKrasAutoStage1 — kind 검증 후 lookup 페이로드만 돌려준다.
pub fn run_kras_auto_stage1(
	page: &dyn KrasPage,
	address: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
	let payload = lookup_evaluation_payload(page, address)?;
	// TS runKrasAutoStage1의 결과 검증과 동일: evaluation_error는 throw, 그 외 비lookup도 throw.
	match payload.get("kind").and_then(|v| v.as_str()) {
		Some("evaluation_error") => {
			let code = payload.get("code").and_then(|v| v.as_str()).unwrap_or("");
			let detail = payload.get("message").and_then(|v| v.as_str()).unwrap_or("");
			return Err(format!("KRAS Stage 1 lookup failed: {code} {detail}").into());
		}
		Some("lookup") => {}
		_ => {
			return Err(format!("KRAS Stage 1 returned an unexpected result: {payload}").into());
		}
	}
	Ok(payload)
}

// ——— 건물 선택 + 층-호 옵션 읽기 (TS selectBuildingAndReadFloorRoomOptionsEvaluation parity) ———

const SELECT_BUILDING_JS: &str = r##"async (value) => {
	const building = document.querySelector('#bldgSeCd');
	if (building === null) throw new Error("건물구분 필드를 찾지 못했습니다.");
	const buildingOption = Array.from(building.options).find((c) => c.value === value);
	if (buildingOption === undefined) throw new Error("Stage 1 건물 옵션을 찾지 못했습니다.");
	const isAggregate = (buildingOption.textContent ?? "").includes('(집합)');
	if (value !== "" && building.value === value && !isAggregate) return [];
	const normalize = (text) => (text ?? "").replace(/\s+/g, " ").trim();
	const readOptions = () => {
		const floorRoom = document.querySelector('#flrSeCd');
		if (floorRoom === null) return [];
		return Array.from(floorRoom.options).map((option, index) => ({ index, value: option.value, label: normalize(option.textContent) })).filter(({ label, value: v }) => v !== "" && label !== "" && label !== "선택" && label !== "선택사항없음");
	};
	const initialFingerprint = JSON.stringify(readOptions());
	building.value = value;
	building.dispatchEvent(new MouseEvent('click', { bubbles: true }));
	building.dispatchEvent(new Event('input', { bubbles: true }));
	building.dispatchEvent(new Event('change', { bubbles: true }));
	if (value === "") return [];
	const initialOptions = readOptions();
	if (!isAggregate) return [];
	let previousFingerprint = initialFingerprint;
	let stableSince = Date.now();
	const deadline = Date.now() + 4000;
	while (Date.now() < deadline) {
		const options = readOptions();
		const fingerprint = JSON.stringify(options);
		if (fingerprint !== previousFingerprint) { previousFingerprint = fingerprint; stableSince = Date.now(); }
		if (options.length > 0 && Date.now() - stableSince >= 800) return options;
		await new Promise((r) => window.setTimeout(r, 250));
	}
	throw new Error("층-호명칭 목록 로딩 시간이 초과되었습니다.");
}"##;

/// 건물을 선택하고 층-호 옵션 목록을 반환한다. (빌딩이 집합이 아니면 빈 배열)
pub fn select_building(
	surface: &crate::cdp_surface::CdpSurface,
	value: &str,
) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
	let started = std::time::Instant::now();
	let expr = format!("({})({})", SELECT_BUILDING_JS, serde_json::to_string(value).unwrap_or_default());
	let params = serde_json::json!({ "expression": expr, "returnByValue": true, "awaitPromise": true });
	let result = surface.send_cdp("Runtime.evaluate", params);
	crate::task_screen::task_status(&format!("건물 선택 적용 {}ms\n", started.elapsed().as_millis()));
	let result = result?;
	if let Some(err) = result.get("exceptionDetails") {
		return Err(format!("건물 선택 실패: {err}").into());
	}
	let wrapper = result.get("result").cloned().unwrap_or(Value::Null);
	let payload = wrapper.get("value").cloned().unwrap_or(Value::Null);
	match payload {
		Value::Array(arr) => Ok(arr),
		Value::Null => Ok(vec![]),
		other => Err(format!("예상치 못한 층-호 결과: {other}").into()),
	}
}

// ——— 캡차 이미지 캡처·제출 (TS captureCaptchaImage / fillCaptchaAndSubmit parity) ———

const CAPTURE_CAPTCHA_JS: &str = r##"async () => {
	const img = document.querySelector('img[id*="captcha" i], img[src*="captcha" i], img[alt*="보안" i]');
	if (!img) return null;
	if (!img.complete || img.naturalWidth === 0) {
		await new Promise((resolve) => { img.onload = img.onerror = resolve; });
	}
	const canvas = document.createElement('canvas');
	canvas.width = img.naturalWidth * 2;
	canvas.height = img.naturalHeight * 2;
	const ctx = canvas.getContext('2d');
	if (!ctx) return null;
	ctx.imageSmoothingEnabled = false;
	ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
	return canvas.toDataURL('image/png').split(',')[1] || null;
}"##;

pub fn capture_captcha_image(surface: &crate::cdp_surface::CdpSurface) -> Option<String> {
	let params = serde_json::json!({ "expression": format!("({})()", CAPTURE_CAPTCHA_JS), "returnByValue": true, "awaitPromise": true });
	let result = surface.send_cdp("Runtime.evaluate", params).ok()?;
	let wrapper = result.get("result")?.get("value")?;
	wrapper.as_str().map(|s| s.to_string())
}

// ——— 캡차 입력·제출 결과 확인 (TS fillCaptchaAndSubmit parity) ———

const FILL_CAPTCHA_JS: &str = r##"async (captchaText) => {
	const isLoginUrl = () => { try { return /\/login(?:\/|$)/i.test(new URL(location.href).pathname); } catch { return false; } };
	const classify = (text) => {
		if (/(접속|접근|이용).{0,15}(차단|제한)|IP.{0,20}(차단|제한)|비정상.{0,10}접근/i.test(text)) return "access_blocked";
		if (text.includes("도면정보") || text.includes("도호가 불일치") || text.includes("출력축척")) return "confirm";
		if (text.includes("신청물건을 먼저 조회")) return "need_lookup";
		if (text.includes("로그인이 만료되었습니다")) return "login_expired";
		if (/보안문자|일치하지 않|오류|실패/.test(text)) return "error";
		return null;
	};
	if (isLoginUrl()) return "login_expired";
	const existingDialog = document.querySelector('.swal2-container.swal2-backdrop-show');
	if (existingDialog !== null && classify(existingDialog.textContent ?? "") === "access_blocked") return "access_blocked";
	const captcha = document.querySelectorAll("#captcha");
	const viewButton = document.querySelectorAll("button[onclick='certView.fnProcessCert();']");
	if (captcha.length !== 1 || viewButton.length !== 1) return "error";
	captcha[0].value = captchaText;
	captcha[0].dispatchEvent(new Event('input', { bubbles: true }));
	if (captcha[0].value !== captchaText) return "error";
	viewButton[0].click();
	const deadline = Date.now() + 20000;
	while (Date.now() < deadline) {
		if (isLoginUrl()) return "login_expired";
		const el = document.querySelector('.swal2-container.swal2-backdrop-show');
		const dialog = el === null ? null : (el.textContent ?? "").replace(/\s+/g, " ").trim();
		const kind = dialog === null ? null : classify(dialog);
		if (kind === "access_blocked") return "access_blocked";
		if (kind === null) { await new Promise((r) => setTimeout(r, 250)); continue; }
		if (kind === "need_lookup") {
			document.querySelector('.swal2-container.swal2-backdrop-show .swal2-confirm')?.click();
			await new Promise((r) => setTimeout(r, 300));
			return "need_lookup";
		}
		if (kind === "login_expired") {
			document.querySelector('.swal2-container.swal2-backdrop-show .swal2-confirm')?.click();
			await new Promise((r) => setTimeout(r, 300));
			return "login_expired";
		}
		if (kind === "confirm") {
			const confirmBtn = document.querySelector('.swal2-container.swal2-backdrop-show .swal2-confirm');
			if (confirmBtn !== null) {
				confirmBtn.click();
				await new Promise((r) => setTimeout(r, 1500));
				if (document.querySelector('.swal2-container.swal2-backdrop-show') === null) return "success";
				continue;
			}
			await new Promise((r) => setTimeout(r, 250));
			continue;
		}
		if (kind === "error") {
			const cancel = document.querySelector('.swal2-container.swal2-backdrop-show .swal2-cancel, .swal2-container.swal2-backdrop-show .swal2-close');
			cancel?.click();
			return "error";
		}
	}
	return "error";
}"##;

/// 캡차 PNG(base64)를 Sixel로 렌더해 고정줄 화면에 띄운다(열려 있지 않으면 건너뛴다).
fn show_captcha_sixel(png_base64: &str) {
	if !crate::task_screen::task_screen_is_open() {
		return;
	}
	use base64::Engine;
	let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(png_base64) else {
		return;
	};
	let Ok(image) = image::load_from_memory(&bytes) else {
		return;
	};
	let sixel_mode = crate::sixel::captcha_terminal_mode_from_env() == "sixel";
	let (width, height) = captcha_display_dimensions(image.width(), image.height(), sixel_mode);
	let rgba = image.resize_exact(width, height, image::imageops::FilterType::Triangle).to_rgba8();
	let (width, height) = rgba.dimensions();
	let rendered = if sixel_mode {
		crate::sixel::render_captcha_sixel(rgba.as_raw(), width as usize, height as usize)
	} else {
		crate::sixel::render_captcha_terminal_pixels(rgba.as_raw(), width as usize, height as usize)
	};
	crate::task_screen::show_task_captcha(&rendered);
}

fn captcha_display_dimensions(width: u32, height: u32, sixel: bool) -> (u32, u32) {
	let target = if sixel { 320 } else { width.min(40) }.max(1);
	let scaled_height = ((height as f64 * target as f64 / width.max(1) as f64).round() as u32).max(1);
	(target, scaled_height)
}

#[cfg(test)]
mod captcha_display_tests {
	use super::captcha_display_dimensions;

	#[test]
	fn matches_last_typescript_wt_and_block_dimensions() {
		assert_eq!(captcha_display_dimensions(160, 60, true), (320, 120));
		assert_eq!(captcha_display_dimensions(160, 60, false), (40, 15));
		assert_eq!(captcha_display_dimensions(20, 10, false), (20, 10));
		assert_eq!(captcha_display_dimensions(640, 240, true), (320, 120));
	}
}

/// 캡차 텍스트를 입력하고 열람 버튼을 클릭한 뒤 결과를 반환한다.
/// 반환: "success" | "need_lookup" | "login_expired" | "error"
pub fn fill_captcha_and_submit(
	surface: &crate::cdp_surface::CdpSurface,
	captcha_text: &str,
) -> String {
	let params = serde_json::json!({ "expression": format!("({})({})", FILL_CAPTCHA_JS, serde_json::to_string(captcha_text).unwrap_or_default()), "returnByValue": true, "awaitPromise": true });
	match surface.send_cdp("Runtime.evaluate", params) {
		Ok(result) => result
			.get("result")
			.and_then(|v| v.get("value"))
			.and_then(|v| v.as_str())
			.unwrap_or("error")
			.to_string(),
		Err(_) => "error".to_string(),
	}
}
