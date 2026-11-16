// TS property-auto-runner.ts — 주소 메뉴(QUICK_ADDRESS_PROMPT) 입력 파싱·배치 분리·프롬프트.
// 대화형 지연 I/O(클립보드·터미널)는 호출자에 두고, 여기는 순수 판정 로직만 둔다.

use crate::kras_pnu::is_pnu_input;

/// TS QUICK_ADDRESS_PROMPT 원문 그대로.
pub const QUICK_ADDRESS_PROMPT: &str = "\n주소 입력 또는 작업 선택\n-: 기본 주소(서울특별시 노원구 월계동 392-19)\n주소: 새 주소 조회\n.: 이 주소의 건물 목록으로 돌아가기\n..: 이 건물의 층-호수로 돌아가기 (집합건물일 때만)\n*: 주소 입력 화면으로 돌아오기\n0: OZ 뷰어 수동 열람 (직전 주소 저장)\nEnter: 종료\n+: 클립보드 붙여넣기 (입력 후 Enter)\n입력 > ";

/// `-` 입력이 대체하는 기본 주소.
pub const DEFAULT_ADDRESS: &str = "서울특별시 노원구 월계동 392-19";

/// QA 프롬프트(주황). TS qaAddressPrompt 동일.
pub const QA_ADDRESS_PROMPT: &str = "\x1b[38;2;255;165;0m주소 입력 > \x1b[0m";

/// AUTO 프롬프트(주황). TS autoAddressPrompt 동일.
pub const AUTO_ADDRESS_PROMPT: &str = "\x1b[38;2;255;165;0mAUTO 주소 입력 > \x1b[0m";

/// 로그인 확인 프롬프트.
pub const LOGIN_PROMPT: &str = "선택한 브라우저의 KRAS 페이지에서 로그인해주세요. 로그인이 끝났나요?\nEnter: 예 (로그인 완료)\n0: 아니오 (아직 로그인 안 됨)\n입력 > ";

/// 일괄처리 범위 선택 프롬프트.
pub const AUTO_SCOPE_PROMPT: &str =
	"일괄처리 범위를 선택하세요. (1: 토지만 / 2: 토지+일반건물 / 3: 토지+일반건물+집합건물 / Enter: 3)\n입력 > ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickAddressInput {
	Address { address: String },
	Qa,
	Auto,
	Batch,
	Start,
	Pause,
	Resume,
	Pnu { pnu: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoBatchScope {
	LandOnly,
	OrdinaryOnly,
	Full,
}

impl AutoBatchScope {
	pub fn as_str(self) -> &'static str {
		match self {
			AutoBatchScope::LandOnly => "land-only",
			AutoBatchScope::OrdinaryOnly => "ordinary-only",
			AutoBatchScope::Full => "full",
		}
	}
}

/// TS parseQuickAddressInput — 입력 한 줄의 종류 판정.
pub fn parse_quick_address_input(input: &str) -> QuickAddressInput {
	match input {
		"QA" => QuickAddressInput::Qa,
		"AUTO" => QuickAddressInput::Auto,
		"..." => QuickAddressInput::Batch,
		"시작" => QuickAddressInput::Start,
		"일시정지" | "pause" => QuickAddressInput::Pause,
		"재개" | "resume" => QuickAddressInput::Resume,
		_ => {
			if is_pnu_input(input) {
				QuickAddressInput::Pnu { pnu: input.to_string() }
			} else {
				QuickAddressInput::Address { address: input.to_string() }
			}
		}
	}
}

/// TS splitQuickAddressBatch — 줄바꿈으로 나누고 공백 줄을 버린다.
pub fn split_quick_address_batch(input: &str) -> Vec<String> {
	input
		.split(['\r', '\n'])
		.map(|line| line.trim())
		.filter(|line| !line.is_empty())
		.map(|line| line.to_string())
		.collect()
}

/// TS parseAutoBatchScopeAnswer — 1/2/3(또는 Enter=3)만 허용, 그 외 None.
pub fn parse_auto_batch_scope_answer(answer: &str) -> Option<AutoBatchScope> {
	let normalized = answer.trim();
	match normalized {
		"1" => Some(AutoBatchScope::LandOnly),
		"2" => Some(AutoBatchScope::OrdinaryOnly),
		"" | "3" => Some(AutoBatchScope::Full),
		_ => None,
	}
}

/// 주소·PNU 행만 골라 대기 목록에 넣는다(TS appendAutoAddresses).
pub fn append_auto_addresses(queue: &mut Vec<String>, input: &str) -> usize {
	let addresses: Vec<String> = split_quick_address_batch(input)
		.into_iter()
		.filter(|line| matches!(
			parse_quick_address_input(line),
			QuickAddressInput::Address { .. } | QuickAddressInput::Pnu { .. }
		))
		.map(|line| if line.trim() == "-" { DEFAULT_ADDRESS.to_string() } else { line })
		.collect();
	let added = addresses.len();
	queue.extend(addresses);
	added
}

/// 단건·일괄처리에서 같은 PNU 변환 규칙을 사용한다.
pub fn resolve_work_address(input: &str) -> Result<String, String> {
	let input = input.trim();
	if input == "-" { return Ok(DEFAULT_ADDRESS.to_string()); }
	if crate::kras_pnu::is_pnu_input(input) {
		crate::kras_pnu::pnu_to_address(input)
	} else {
		Ok(crate::kras_domain::normalize_address(input))
	}
}

#[cfg(test)]
mod batch_pnu_regression_tests {
	use super::{append_auto_addresses, resolve_work_address};

	#[test]
	fn batch_default_parcel_matches_interactive_address() {
		let mut queue = Vec::new();
		assert_eq!(append_auto_addresses(&mut queue, "  -  "), 1);
		assert_eq!(queue, vec![super::DEFAULT_ADDRESS.to_string()]);
		assert_eq!(resolve_work_address(" - ").unwrap(), super::DEFAULT_ADDRESS);
		assert_eq!(resolve_work_address(&queue[0]).unwrap(), super::DEFAULT_ADDRESS);
	}

	#[test]
	fn supplied_batch_pnus_are_resolved_before_lookup() {
		let input = "1111018100102100730\n1111018200101200016\n1111018200102440011\n1111018300100630001\n1111018300100930017";
		let mut queue = Vec::new();
		assert_eq!(append_auto_addresses(&mut queue, input), 5);
		for pnu in &queue {
			let address = resolve_work_address(pnu).expect("테스트 PNU 변환");
			assert!(address.starts_with("서울특별시 종로구 "), "{pnu}: {address}");
			assert_ne!(&address, pnu);
			assert!(address.chars().any(|c| c == '-'));
		}
		assert!(resolve_work_address(&queue[0]).unwrap().ends_with("210-730"));
	}
}

/// 주소 행(PNU 포함)이 여러 개면 AUTO 목록 전환 대상(TS initialBatch.length > 1).
pub fn batch_address_count(input: &str) -> usize {
	split_quick_address_batch(input)
		.iter()
		.filter(|line| matches!(
			parse_quick_address_input(line),
			QuickAddressInput::Address { .. } | QuickAddressInput::Pnu { .. }
		))
		.count()
}

/// 붙여넣기 본문에서 주소 줄만 추출한다(TS `+` 처리).
pub fn split_pasted_addresses(pasted: &str) -> Vec<String> {
	split_quick_address_batch(pasted)
		.into_iter()
		.filter(|line| line != "+")
		.collect()
}

/// 현재 모드에 맞는 프롬프트 선택(TS activeAddressPrompt).
pub fn active_address_prompt(auto_mode: bool, qa_mode: bool) -> &'static str {
	if auto_mode {
		AUTO_ADDRESS_PROMPT
	} else if qa_mode {
		QA_ADDRESS_PROMPT
	} else {
		QUICK_ADDRESS_PROMPT
	}
}

/// 종료 확인 프롬프트(TS readQuickAddress의 빈 입력 확인).
pub const EXIT_CONFIRM_PROMPT: &str =
	"종료하시겠습니까? Enter: 예(종료) / 0: 아니오(입력 창으로)\n";

/// TS createBufferedLineReader — 청크 단위로 읽어 남은 바이트를 버퍼에 두는 줄 리더.
/// `drain_pending`은 새 읽기를 하지 않으므로 절대 차단하지 않는다(TS drainPending 동일).
pub struct BufferedLineReader<R: std::io::Read> {
	inner: R,
	pending: Vec<u8>,
	chunk_size: usize,
	eof: bool,
}

pub fn lock_stdin_reader() -> std::sync::MutexGuard<'static, BufferedLineReader<std::io::Stdin>> {
	static READER: std::sync::OnceLock<std::sync::Mutex<BufferedLineReader<std::io::Stdin>>> = std::sync::OnceLock::new();
	READER.get_or_init(|| std::sync::Mutex::new(BufferedLineReader::new(std::io::stdin(), 8192)))
		.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl<R: std::io::Read> BufferedLineReader<R> {
	/// `chunk_size`는 TS의 256 기본값을 따르되, stdin 버퍼와 겹치지 않게 크게 둘 수 있다.
	pub fn new(inner: R, chunk_size: usize) -> Self {
		Self { inner, pending: Vec::new(), chunk_size: chunk_size.max(1), eof: false }
	}

	/// 입력이 끝났는지 — 빈 줄(Enter)과 구분해 배치 수집기를 무한 루프로 만들지 않는다.
	pub fn is_eof(&self) -> bool {
		self.eof
	}

	/// 다음 줄을 읽는다(개행 제외). EOF면 남은 내용을 돌려준다.
	pub fn read_line(&mut self) -> String {
		self.try_read_line().unwrap_or_default()
	}

	pub fn try_read_line(&mut self) -> std::io::Result<String> {
		loop {
			if let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
				let line: Vec<u8> = self.pending.drain(..=pos).collect();
				let mut line = line;
				line.pop(); // '\n' 제거
				if line.last() == Some(&b'\r') {
					line.pop();
				}
				return Ok(String::from_utf8_lossy(&line).to_string());
			}
			let mut chunk = vec![0u8; self.chunk_size];
			let read = self.inner.read(&mut chunk)?;
			if read == 0 {
				self.eof = true;
				let rest = std::mem::take(&mut self.pending);
				let mut text = String::from_utf8_lossy(&rest).to_string();
				if text.ends_with('\r') {
					text.pop();
				}
				return Ok(text);
			}
			self.pending.extend_from_slice(&chunk[..read]);
		}
	}

	/// 버퍼에 이미 들어온 완결된 줄만 뽑는다 — 읽지 않으므로 비차단.
	pub fn drain_pending(&mut self) -> Vec<String> {
		let mut lines = Vec::new();
		loop {
			let Some(pos) = self.pending.iter().position(|b| *b == b'\n') else {
				return lines;
			};
			let mut line: Vec<u8> = self.pending.drain(..=pos).collect();
			line.pop();
			if line.last() == Some(&b'\r') {
				line.pop();
			}
			lines.push(String::from_utf8_lossy(&line).to_string());
		}
	}
}

/// TS readQuickAddress 마지막 분기 — 입력과 남은 줄을 하나의 배치 문자열로 합친다.
pub fn join_pending_lines(input: &str, pending: &[String]) -> String {
	if pending.is_empty() {
		return input.to_string();
	}
	let mut parts: Vec<&str> = Vec::with_capacity(pending.len() + 1);
	parts.push(input);
	for line in pending {
		parts.push(line.as_str());
	}
	parts
		.iter()
		.map(|s| s.trim())
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>()
		.join("\n")
}

/// 주소 메뉴 한 줄 입력의 처리 방향(TS quick 루프의 분기 순서와 동일).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
	/// Enter — 종료 확인으로 진행.
	Quit,
	/// `-` — 기본 주소 조회.
	DefaultAddress,
	/// `*` — 주소 입력 화면으로 돌아오기.
	Redisplay,
	/// `.` — 직전 주소의 건물 목록으로 돌아가기.
	PreviousAddress,
	/// `..` — 직전 건물의 층-호수로 돌아가기.
	FloorRoomReturn,
	/// `0` — OZ 뷰어 수동 열람.
	ViewerManual,
	/// `+` — 클립보드 붙여넣기.
	ClipboardPaste,
	/// `QA` — QA 모드 토글.
	ToggleQa,
	/// `AUTO` — AUTO 모드 토글.
	ToggleAuto,
	/// `...` — 대규모 처리 모드 진입.
	EnterBatch,
	/// `시작` — 대기 목록 실행.
	StartBatch,
	/// `일시정지` / `pause`.
	Pause,
	/// `재개` / `resume`.
	Resume,
	/// 주소 또는 PNU — 조회 실행.
	Run { input: String },
}

/// TS quick 루프의 분기 순서 그대로: `-` 치환 → 커맨드 파싱 → `*`/`.`/`..`/`0`/`+` → 주소.
pub fn classify_menu_input(input: &str) -> MenuAction {
	if input == "-" {
		return MenuAction::DefaultAddress;
	}
	match parse_quick_address_input(input) {
		QuickAddressInput::Qa => MenuAction::ToggleQa,
		QuickAddressInput::Auto => MenuAction::ToggleAuto,
		QuickAddressInput::Batch => MenuAction::EnterBatch,
		QuickAddressInput::Start => MenuAction::StartBatch,
		QuickAddressInput::Pause => MenuAction::Pause,
		QuickAddressInput::Resume => MenuAction::Resume,
		QuickAddressInput::Pnu { pnu } => MenuAction::Run { input: pnu },
		QuickAddressInput::Address { address } => {
			match address.as_str() {
				"" => MenuAction::Quit,
				"*" => MenuAction::Redisplay,
				"." => MenuAction::PreviousAddress,
				".." => MenuAction::FloorRoomReturn,
				"0" => MenuAction::ViewerManual,
				"+" => MenuAction::ClipboardPaste,
				_ => MenuAction::Run { input: address },
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prompt_matches_ts_constant() {
		assert!(QUICK_ADDRESS_PROMPT.starts_with("\n주소 입력 또는 작업 선택\n"));
		assert!(QUICK_ADDRESS_PROMPT.contains("-: 기본 주소(서울특별시 노원구 월계동 392-19)"));
		assert!(QUICK_ADDRESS_PROMPT.contains("+: 클립보드 붙여넣기 (입력 후 Enter)"));
		assert!(QUICK_ADDRESS_PROMPT.ends_with("입력 > "));
		assert_eq!(DEFAULT_ADDRESS, "서울특별시 노원구 월계동 392-19");
	}

	#[test]
	fn parse_menu_commands() {
		assert_eq!(parse_quick_address_input("QA"), QuickAddressInput::Qa);
		assert_eq!(parse_quick_address_input("AUTO"), QuickAddressInput::Auto);
		assert_eq!(parse_quick_address_input("..."), QuickAddressInput::Batch);
		assert_eq!(parse_quick_address_input("시작"), QuickAddressInput::Start);
		assert_eq!(parse_quick_address_input("일시정지"), QuickAddressInput::Pause);
		assert_eq!(parse_quick_address_input("pause"), QuickAddressInput::Pause);
		assert_eq!(parse_quick_address_input("재개"), QuickAddressInput::Resume);
		assert_eq!(parse_quick_address_input("resume"), QuickAddressInput::Resume);
	}

	#[test]
	fn parse_plain_address_becomes_address() {
		match parse_quick_address_input("서울특별시 노원구 월계동 392-19") {
			QuickAddressInput::Address { address } => {
				assert_eq!(address, "서울특별시 노원구 월계동 392-19")
			}
			other => panic!("unexpected: {other:?}"),
		}
	}

	#[test]
	fn split_batch_handles_crlf_and_blank_lines() {
		let input = "주소A\r\n\r\n주소B\n  \n주소C\n";
		assert_eq!(
			split_quick_address_batch(input),
			vec!["주소A", "주소B", "주소C"]
		);
	}

	#[test]
	fn scope_answer_accepts_only_1_2_3_and_empty() {
		assert_eq!(parse_auto_batch_scope_answer("1"), Some(AutoBatchScope::LandOnly));
		assert_eq!(parse_auto_batch_scope_answer("2"), Some(AutoBatchScope::OrdinaryOnly));
		assert_eq!(parse_auto_batch_scope_answer("3"), Some(AutoBatchScope::Full));
		assert_eq!(parse_auto_batch_scope_answer(""), Some(AutoBatchScope::Full));
		assert_eq!(parse_auto_batch_scope_answer("  2  "), Some(AutoBatchScope::OrdinaryOnly));
		assert_eq!(parse_auto_batch_scope_answer("4"), None);
		assert_eq!(parse_auto_batch_scope_answer("x"), None);
	}

	#[test]
	fn scope_str_matches_ts_values() {
		assert_eq!(AutoBatchScope::LandOnly.as_str(), "land-only");
		assert_eq!(AutoBatchScope::OrdinaryOnly.as_str(), "ordinary-only");
		assert_eq!(AutoBatchScope::Full.as_str(), "full");
	}

	#[test]
	fn append_keeps_only_address_and_pnu_lines() {
		let mut queue = Vec::new();
		let added = append_auto_addresses(&mut queue, "주소A\nAUTO\n주소B\n시작");
		assert_eq!(added, 2);
		assert_eq!(queue, vec!["주소A", "주소B"]);
	}

	#[test]
	fn batch_address_count_excludes_commands() {
		assert_eq!(batch_address_count("주소A"), 1);
		assert_eq!(batch_address_count("주소A\n주소B"), 2);
		assert_eq!(batch_address_count("AUTO\n시작"), 0);
	}

	#[test]
	fn pasted_addresses_drop_plus_marker() {
		assert_eq!(split_pasted_addresses("주소A\n+\n주소B\n"), vec!["주소A", "주소B"]);
	}

	#[test]
	fn active_prompt_follows_mode() {
		assert_eq!(active_address_prompt(false, false), QUICK_ADDRESS_PROMPT);
		assert_eq!(active_address_prompt(false, true), QA_ADDRESS_PROMPT);
		assert_eq!(active_address_prompt(true, false), AUTO_ADDRESS_PROMPT);
		// AUTO가 QA보다 우선한다 (TS: if (autoMode) return autoAddressPrompt)
		assert_eq!(active_address_prompt(true, true), AUTO_ADDRESS_PROMPT);
	}

	#[test]
	fn classify_menu_order_matches_ts() {
		assert_eq!(classify_menu_input(""), MenuAction::Quit);
		assert_eq!(classify_menu_input("-"), MenuAction::DefaultAddress);
		assert_eq!(classify_menu_input("*"), MenuAction::Redisplay);
		assert_eq!(classify_menu_input("."), MenuAction::PreviousAddress);
		assert_eq!(classify_menu_input(".."), MenuAction::FloorRoomReturn);
		assert_eq!(classify_menu_input("0"), MenuAction::ViewerManual);
		assert_eq!(classify_menu_input("+"), MenuAction::ClipboardPaste);
		assert_eq!(classify_menu_input("QA"), MenuAction::ToggleQa);
		assert_eq!(classify_menu_input("AUTO"), MenuAction::ToggleAuto);
		assert_eq!(classify_menu_input("..."), MenuAction::EnterBatch);
		assert_eq!(classify_menu_input("시작"), MenuAction::StartBatch);
		assert_eq!(classify_menu_input("일시정지"), MenuAction::Pause);
		assert_eq!(classify_menu_input("pause"), MenuAction::Pause);
		assert_eq!(classify_menu_input("재개"), MenuAction::Resume);
		assert_eq!(classify_menu_input("resume"), MenuAction::Resume);
	}

	#[test]
	fn classify_runs_plain_address() {
		assert_eq!(
			classify_menu_input("서울특별시 노원구 월계동 392-19"),
			MenuAction::Run { input: "서울특별시 노원구 월계동 392-19".to_string() }
		);
	}

	#[test]
	fn classify_runs_19_digit_pnu() {
		// PNU = 법정동코드10 + 지목1 + 본번4 + 부번4 = 19자리 (TS /^\d{19}$/)
		let pnu = "1135010101000190019";
		assert!(crate::kras_pnu::is_pnu_input(pnu));
		assert_eq!(
			classify_menu_input(pnu),
			MenuAction::Run { input: pnu.to_string() }
		);
		// 15자리 숫자는 PNU가 아니므로 주소로 취급된다 (TS와 동일)
		assert!(!crate::kras_pnu::is_pnu_input("113501013920019"));
		assert_eq!(
			classify_menu_input("113501013920019"),
			MenuAction::Run { input: "113501013920019".to_string() }
		);
	}

	// ——— TS createBufferedLineReader / drainPending parity ———

	/// 청크 1개에 여러 줄이 담긴 상황(붙여넣기)을 재현한다.
	fn reader_with(data: &'static [u8]) -> BufferedLineReader<std::io::Cursor<Vec<u8>>> {
		BufferedLineReader::new(std::io::Cursor::new(data.to_vec()), 256)
	}

	#[test]
	fn read_line_splits_lines_across_one_chunk() {
		let mut r = reader_with(b"addr1\naddr2\naddr3\n");
		assert_eq!(r.read_line(), "addr1");
		// 첫 줄을 읽는 동안 남은 줄은 버퍼에 남는다
		assert_eq!(r.drain_pending(), vec!["addr2", "addr3"]);
		assert!(r.drain_pending().is_empty());
	}

	#[test]
	fn drain_pending_does_not_read_more() {
		let mut r = reader_with(b"one\ntwo\n");
		assert_eq!(r.read_line(), "one");
		// drain은 이미 버퍼에 있는 줄만 — 두 번째 호출에서는 새 읽기가 없다
		assert_eq!(r.drain_pending(), vec!["two"]);
		assert_eq!(r.drain_pending(), Vec::<String>::new());
	}

	#[test]
	fn drain_pending_ignores_incomplete_line() {
		let mut r = BufferedLineReader::new(std::io::Cursor::new(b"complete\npartial".to_vec()), 256);
		assert_eq!(r.read_line(), "complete");
		// 개행 없는 조각은 완결 줄이 아니므로 반환하지 않는다 (TS drainPending과 동일)
		assert!(r.drain_pending().is_empty());
		// EOF 시 read_line이 남은 조각을 돌려준다
		assert_eq!(r.read_line(), "partial");
	}

	#[test]
	fn read_line_handles_crlf_and_eof() {
		let mut r = reader_with(b"abc\r\ndef");
		assert_eq!(r.read_line(), "abc");
		assert_eq!(r.read_line(), "def");
		assert_eq!(r.read_line(), "");
	}

	#[test]
	fn join_pending_matches_ts_filter_and_trim() {
		let pending = vec!["주소B".to_string(), "  ".to_string(), "주소C".to_string()];
		assert_eq!(join_pending_lines("주소A", &pending), "주소A\n주소B\n주소C");
	}

	#[test]
	fn join_pending_returns_input_when_no_pending() {
		assert_eq!(join_pending_lines("주소A", &[]), "주소A");
	}

	#[test]
	fn eof_is_reported_and_empty_enter_is_not() {
		let mut r = reader_with(b"one\n");
		assert!(!r.is_eof());
		assert_eq!(r.read_line(), "one");
		// 개행으로 끝나는 입력은 빈 줄이 아니라 EOF가 아니다
		assert!(!r.is_eof());
		assert_eq!(r.read_line(), "");
		assert!(r.is_eof());
	}

	#[test]
	fn eof_is_reported_for_input_without_trailing_newline() {
		let mut r = reader_with(b"tail");
		assert_eq!(r.read_line(), "tail");
		assert!(r.is_eof());
	}

	#[test]
	fn pasted_batch_becomes_single_multiline_input() {
		// 붙여넣기 한 청크에 3줄 → read_line 1회 + drain → TS처럼 배치 한 줄이 된다
		let mut r = reader_with(b"addr1\r\naddr2\r\naddr3\r\n");
		let input = r.read_line().trim().to_string();
		let pending = r.drain_pending();
		let joined = join_pending_lines(&input, &pending);
		assert_eq!(joined, "addr1\r\naddr2\r\naddr3".replace("\r\n", "\n"));
		// 배치 분리 결과 3건 → AUTO 목록 전환 조건(TS initialBatch.length > 1) 충족
		assert_eq!(batch_address_count(&joined), 3);
		assert_eq!(split_quick_address_batch(&joined).len(), 3);
	}
}
#[cfg(windows)]
pub fn read_clipboard_text() -> Result<String, Box<dyn std::error::Error>> {
	use windows_sys::Win32::System::{DataExchange::*, Memory::*};
	unsafe {
		if OpenClipboard(std::ptr::null_mut()) == 0 {
			return Err(std::io::Error::last_os_error().into());
		}
		struct Clipboard;
		impl Drop for Clipboard {
			fn drop(&mut self) { unsafe { CloseClipboard(); } }
		}
		let _clipboard = Clipboard;
		let handle = GetClipboardData(13);
		if handle.is_null() { return Err("클립보드에 Unicode 텍스트가 없습니다.".into()); }
		let size = GlobalSize(handle);
		if size == 0 || size > 2 * 1024 * 1024 { return Err("클립보드 텍스트 크기 오류".into()); }
		let pointer = GlobalLock(handle) as *const u16;
		if pointer.is_null() { return Err(std::io::Error::last_os_error().into()); }
		let units = std::slice::from_raw_parts(pointer, size / 2);
		let result = match units.iter().position(|unit| *unit == 0) {
			Some(end) => String::from_utf16(&units[..end]).map_err(|error| error.into()),
			None => Err("클립보드 문자열 종료 문자가 없습니다.".into()),
		};
		GlobalUnlock(handle);
		result
	}
}

#[cfg(not(windows))]
pub fn read_clipboard_text() -> Result<String, Box<dyn std::error::Error>> {
	Err("Windows 클립보드만 지원합니다.".into())
}
