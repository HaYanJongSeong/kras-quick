// TS scripts/property-progress.ts — 현장조사 진행 파일(마커·지번·페이지) 갱신 계약.
// 준비(순수) / 잠금 커밋(Windows PowerShell)을 나눠 재현한다.

use std::path::{Path, PathBuf};

use crate::property_address::parse_parcel_address;
use crate::property_progress_windows::{compare_and_write_locked, LockedWriteResult};
use crate::property_watcher_payload::parse_property_pager;

/// TS PROPERTY_PROGRESS_FILE
pub const PROPERTY_PROGRESS_FILE: &str =
	"C:\\Users\\admin\\Desktop\\시유재산\\현장조사_분류_활용방안.txt";
/// TS PROPERTY_PROGRESS_MARKER
pub const PROPERTY_PROGRESS_MARKER: &str = "현재 어디까지?";

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// TS ProgressFileErrorCode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressFileErrorCode {
	InvalidEncoding,
	MarkerNotUnique,
	AddressLineMissing,
	InvalidAddressLine,
	PagerLineMissing,
	InvalidPagerLine,
	SourceChanged,
	IoFailure,
}

impl ProgressFileErrorCode {
	pub fn as_str(self) -> &'static str {
		match self {
			ProgressFileErrorCode::InvalidEncoding => "INVALID_ENCODING",
			ProgressFileErrorCode::MarkerNotUnique => "MARKER_NOT_UNIQUE",
			ProgressFileErrorCode::AddressLineMissing => "ADDRESS_LINE_MISSING",
			ProgressFileErrorCode::InvalidAddressLine => "INVALID_ADDRESS_LINE",
			ProgressFileErrorCode::PagerLineMissing => "PAGER_LINE_MISSING",
			ProgressFileErrorCode::InvalidPagerLine => "INVALID_PAGER_LINE",
			ProgressFileErrorCode::SourceChanged => "SOURCE_CHANGED",
			ProgressFileErrorCode::IoFailure => "IO_FAILURE",
		}
	}

	fn message(self) -> &'static str {
		match self {
			ProgressFileErrorCode::InvalidEncoding => "진행 파일은 올바른 UTF-8이어야 합니다.",
			ProgressFileErrorCode::MarkerNotUnique => "진행 파일의 현재 위치 표시는 정확히 하나여야 합니다.",
			ProgressFileErrorCode::AddressLineMissing => "진행 파일에 갱신할 지번 주소가 없습니다.",
			ProgressFileErrorCode::InvalidAddressLine => "진행 파일의 현재 지번 주소가 올바르지 않습니다.",
			ProgressFileErrorCode::PagerLineMissing => "진행 파일에 갱신할 페이지 진행 표시가 없습니다.",
			ProgressFileErrorCode::InvalidPagerLine => "진행 파일의 페이지 진행 표시가 올바르지 않습니다.",
			ProgressFileErrorCode::SourceChanged => "준비 후 진행 파일이 변경되어 갱신하지 않았습니다.",
			ProgressFileErrorCode::IoFailure => "진행 파일을 안전하게 갱신할 수 없습니다.",
		}
	}
}

/// TS ProgressFileError — 코드별 고정 메시지를 가진 오류.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressFileError {
	pub code: ProgressFileErrorCode,
}

impl ProgressFileError {
	pub fn new(code: ProgressFileErrorCode) -> Self {
		Self { code }
	}
}

impl std::fmt::Display for ProgressFileError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(self.code.message())
	}
}

impl std::error::Error for ProgressFileError {}

impl From<crate::property_address::PropertyAddressError> for ProgressFileError {
	fn from(_: crate::property_address::PropertyAddressError) -> Self {
		ProgressFileError::new(ProgressFileErrorCode::InvalidAddressLine)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineSpan {
	start: usize,
	end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProgressSpans {
	address: LineSpan,
	pager_value: LineSpan,
}

/// TS PreparedProgressUpdate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedProgressUpdate {
	pub path: PathBuf,
	pub source_bytes: Vec<u8>,
	pub updated_bytes: Vec<u8>,
}

/// TS decodeUtf8 — fatal 디코딩. 실패 시 INVALID_ENCODING.
fn decode_utf8(bytes: &[u8]) -> Result<&str, ProgressFileError> {
	std::str::from_utf8(bytes).map_err(|_| ProgressFileError::new(ProgressFileErrorCode::InvalidEncoding))
}

/// TS findLines — 개행 기준으로 (start, end) 스팬을 만든다. CRLF의 CR은 제외한다.
fn find_lines(bytes: &[u8], start: usize) -> Vec<LineSpan> {
	let mut lines = Vec::new();
	let mut line_start = start;
	for (index, byte) in bytes.iter().enumerate().skip(start) {
		if *byte != 0x0A {
			continue;
		}
		let end = if index > line_start && bytes[index - 1] == 0x0D { index - 1 } else { index };
		lines.push(LineSpan { start: line_start, end });
		line_start = index + 1;
	}
	lines.push(LineSpan { start: line_start, end: bytes.len() });
	lines
}

fn line_text<'a>(bytes: &'a [u8], span: LineSpan) -> &'a str {
	decode_utf8(&bytes[span.start..span.end]).unwrap_or("")
}

/// TS progressSpans — 마커 유일성 → 마지막 지번 줄 → 그 앞 페이지 줄을 찾는다.
fn progress_spans(source_bytes: &[u8]) -> Result<ProgressSpans, ProgressFileError> {
	let content_start = if source_bytes.starts_with(&UTF8_BOM) { UTF8_BOM.len() } else { 0 };
	decode_utf8(&source_bytes[content_start..])?;
	let lines = find_lines(source_bytes, content_start);

	let marker_indexes: Vec<usize> = lines
		.iter()
		.enumerate()
		.filter(|(_, span)| line_text(source_bytes, **span).trim() == PROPERTY_PROGRESS_MARKER)
		.map(|(index, _)| index)
		.collect();
	if marker_indexes.len() != 1 {
		return Err(ProgressFileError::new(ProgressFileErrorCode::MarkerNotUnique));
	}
	let marker_index = marker_indexes[0];

	// 마커 뒤에서 "비어 있지 않은 마지막 줄"이 지번 줄이다
	let mut address_index: Option<usize> = None;
	for index in (marker_index + 1)..lines.len() {
		if !line_text(source_bytes, lines[index]).trim().is_empty() {
			address_index = Some(index);
		}
	}
	let address_index = address_index
		.ok_or_else(|| ProgressFileError::new(ProgressFileErrorCode::AddressLineMissing))?;
	let address_line = lines[address_index];

	let pager_index = address_index.wrapping_sub(1);
	if address_index == 0 || pager_index <= marker_index {
		return Err(ProgressFileError::new(ProgressFileErrorCode::PagerLineMissing));
	}
	let pager_line = lines[pager_index];
	let pager_text = line_text(source_bytes, pager_line);
	if pager_text.trim().is_empty() {
		return Err(ProgressFileError::new(ProgressFileErrorCode::PagerLineMissing));
	}
	// TS /^(\d+\/\d+)/ — 줄 앞쪽에 오는 첫 "숫자/숫자"만 쓴다
	let existing_pager = extract_pager_prefix(pager_text)
		.filter(|candidate| parse_property_pager(candidate).is_some())
		.ok_or_else(|| ProgressFileError::new(ProgressFileErrorCode::InvalidPagerLine))?;

	let existing_address = line_text(source_bytes, address_line);
	parse_parcel_address(existing_address)
		.map_err(|_| ProgressFileError::new(ProgressFileErrorCode::InvalidAddressLine))?;

	Ok(ProgressSpans {
		address: address_line,
		pager_value: LineSpan {
			start: pager_line.start,
			end: pager_line.start + existing_pager.len(),
		},
	})
}

/// TS 정규식 `^(\d+\/\d+)` — 줄 처음부터 이어지는 숫자/숫자만 추출한다.
fn extract_pager_prefix(text: &str) -> Option<&str> {
	let trimmed_start = 0;
	let bytes = text.as_bytes();
	let mut index = trimmed_start;
	let first_start = index;
	while index < bytes.len() && bytes[index].is_ascii_digit() {
		index += 1;
	}
	if index == first_start {
		return None;
	}
	if index >= bytes.len() || bytes[index] != b'/' {
		return None;
	}
	index += 1;
	let second_start = index;
	while index < bytes.len() && bytes[index].is_ascii_digit() {
		index += 1;
	}
	if index == second_start {
		return None;
	}
	text.get(..index)
}

/// TS prepareProgressUpdate — 읽기 → 스캔 → 지번·페이지 위치를 치환한 새 바이트를 만든다.
pub fn prepare_progress_update(
	path: &Path,
	address: &str,
	pager: &str,
) -> Result<PreparedProgressUpdate, ProgressFileError> {
	let source_bytes =
		std::fs::read(path).map_err(|_| ProgressFileError::new(ProgressFileErrorCode::IoFailure))?;
	let spans = progress_spans(&source_bytes)?;

	let mut updated = Vec::with_capacity(source_bytes.len() + address.len() + pager.len());
	updated.extend_from_slice(&source_bytes[..spans.pager_value.start]);
	updated.extend_from_slice(pager.as_bytes());
	updated.extend_from_slice(&source_bytes[spans.pager_value.end..spans.address.start]);
	updated.extend_from_slice(address.as_bytes());
	updated.extend_from_slice(&source_bytes[spans.address.end..]);

	Ok(PreparedProgressUpdate {
		path: path.to_path_buf(),
		source_bytes,
		updated_bytes: updated,
	})
}

/// TS stagingPath — `.{basename}.{kind}.{pid}.{uuid}.tmp`
fn staging_path(target_path: &Path, kind: &str) -> PathBuf {
	let dir = target_path.parent().unwrap_or_else(|| Path::new("."));
	let base = target_path
		.file_name()
		.map(|n| n.to_string_lossy().to_string())
		.unwrap_or_default();
	dir.join(format!(".{base}.{kind}.{}.{}.tmp", std::process::id(), unique_token()))
}

/// 임시 파일명용 고유 토큰(.uuid 형식). 정확한 UUID 값은 아니며 유일성만 요구된다.
fn unique_token() -> String {
	use std::sync::atomic::{AtomicU64, Ordering};
	static COUNTER: AtomicU64 = AtomicU64::new(0);
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default();
	let nanos = now.subsec_nanos() as u64 ^ (now.as_secs() << 17);
	let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
	let mixed = nanos ^ (std::process::id() as u64).rotate_left(13) ^ seq.rotate_left(7);
	format!("{:08x}-{:04x}-{:04x}-{:04x}-{:012x}", mixed >> 32, (mixed >> 16) & 0xFFFF, (mixed >> 4) & 0xFFFF, seq & 0xFFFF, mixed & 0xFFFF_FFFF_FFFF)
}

/// TS stageFile — O_EXCL로 만들고 쓴 뒤 sync 한다.
fn stage_file(path: &Path, bytes: &[u8]) -> Result<(), ProgressFileError> {
	use std::io::Write;
	let mut file = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)
		.map_err(|_| ProgressFileError::new(ProgressFileErrorCode::IoFailure))?;
	file.write_all(bytes)
		.map_err(|_| ProgressFileError::new(ProgressFileErrorCode::IoFailure))?;
	file.sync_all()
		.map_err(|_| ProgressFileError::new(ProgressFileErrorCode::IoFailure))
}

fn remove_quiet(path: &Path) -> Result<(), ProgressFileError> {
	match std::fs::remove_file(path) {
		Ok(()) => Ok(()),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
		Err(_) => Err(ProgressFileError::new(ProgressFileErrorCode::IoFailure)),
	}
}

/// TS commitProgressUpdate — 스냅샷·교체본을 만들고 잠금 비교 후 원자 교체.
pub fn commit_progress_update(update: &PreparedProgressUpdate) -> Result<(), ProgressFileError> {
	let expected_path = staging_path(&update.path, "expected");
	let replacement_path = staging_path(&update.path, "replacement");

	let outcome: Result<LockedWriteResult, ProgressFileError> = (|| {
		stage_file(&expected_path, &update.source_bytes)?;
		stage_file(&replacement_path, &update.updated_bytes)?;
		compare_and_write_locked(&update.path, &expected_path, &replacement_path)
			.map_err(|_| ProgressFileError::new(ProgressFileErrorCode::IoFailure))
	})();

	// 정리 실패는 반드시 IO_FAILURE로 보고한다(TS Promise.allSetled 대응)
	let cleanup_failed =
		remove_quiet(&expected_path).is_err() || remove_quiet(&replacement_path).is_err();
	if cleanup_failed {
		return Err(ProgressFileError::new(ProgressFileErrorCode::IoFailure));
	}

	match outcome {
		Ok(LockedWriteResult::Written) => Ok(()),
		Ok(LockedWriteResult::SourceChanged) => {
			Err(ProgressFileError::new(ProgressFileErrorCode::SourceChanged))
		}
		Err(e) => Err(e),
	}
}

/// TS updateProgressFile — 준비 후 커밋.
pub fn update_progress_file(path: &Path, address: &str, pager: &str) -> Result<(), ProgressFileError> {
	let update = prepare_progress_update(path, address, pager)?;
	commit_progress_update(&update)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn valid_file() -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend_from_slice("다음 주소 목록\n".as_bytes());
		bytes.extend_from_slice(PROPERTY_PROGRESS_MARKER.as_bytes());
		bytes.extend_from_slice(b"\n7/30\n");
		bytes.extend_from_slice("서울특별시 노원구 월계동 392-19".as_bytes());
		bytes.push(b'\n');
		bytes
	}

	fn temp_dir(tag: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("kras-progress-{tag}"));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).expect("mkdir");
		dir
	}

	#[test]
	fn error_codes_and_messages_match_ts() {
		for (code, text) in [
			(ProgressFileErrorCode::InvalidEncoding, "진행 파일은 올바른 UTF-8이어야 합니다."),
			(ProgressFileErrorCode::MarkerNotUnique, "진행 파일의 현재 위치 표시는 정확히 하나여야 합니다."),
			(ProgressFileErrorCode::AddressLineMissing, "진행 파일에 갱신할 지번 주소가 없습니다."),
			(ProgressFileErrorCode::InvalidAddressLine, "진행 파일의 현재 지번 주소가 올바르지 않습니다."),
			(ProgressFileErrorCode::PagerLineMissing, "진행 파일에 갱신할 페이지 진행 표시가 없습니다."),
			(ProgressFileErrorCode::InvalidPagerLine, "진행 파일의 페이지 진행 표시가 올바르지 않습니다."),
			(ProgressFileErrorCode::SourceChanged, "준비 후 진행 파일이 변경되어 갱신하지 않았습니다."),
			(ProgressFileErrorCode::IoFailure, "진행 파일을 안전하게 갱신할 수 없습니다."),
		] {
			assert_eq!(ProgressFileError::new(code).to_string(), text, "{:?}", code);
			assert_eq!(ProgressFileError::new(code).code.as_str().len() > 0, true);
		}
		assert_eq!(PROPERTY_PROGRESS_MARKER, "현재 어디까지?");
		assert!(PROPERTY_PROGRESS_FILE.ends_with("현장조사_분류_활용방안.txt"));
	}

	#[test]
	fn marker_must_be_unique() {
		let dir = temp_dir("marker");
		// 마커 없음
		let no_marker = dir.join("a.txt");
		std::fs::write(&no_marker, "주소\n서울특별시 노원구 월계동 392-19\n").unwrap();
		let err = prepare_progress_update(&no_marker, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("마커 없음");
		assert_eq!(err.code, ProgressFileErrorCode::MarkerNotUnique);

		// 마커 2개
		let twice = dir.join("b.txt");
		let mut bytes = valid_file();
		bytes.extend_from_slice(format!("\n{}\n", PROPERTY_PROGRESS_MARKER).as_bytes());
		std::fs::write(&twice, bytes).unwrap();
		let err = prepare_progress_update(&twice, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("마커 2개");
		assert_eq!(err.code, ProgressFileErrorCode::MarkerNotUnique);
	}

	#[test]
	fn address_line_missing_when_nothing_after_marker() {
		let dir = temp_dir("addr-missing");
		let path = dir.join("a.txt");
		std::fs::write(&path, format!("{}\n\n", PROPERTY_PROGRESS_MARKER)).unwrap();
		let err = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("지번 줄 없음");
		assert_eq!(err.code, ProgressFileErrorCode::AddressLineMissing);
	}

	#[test]
	fn pager_line_missing_when_address_is_directly_after_marker() {
		let dir = temp_dir("pager-missing");
		let path = dir.join("a.txt");
		std::fs::write(
			&path,
			format!("{}\n{}\n", PROPERTY_PROGRESS_MARKER, "서울특별시 노원구 월계동 392-19"),
		)
		.unwrap();
		let err = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("페이지 줄 없음");
		assert_eq!(err.code, ProgressFileErrorCode::PagerLineMissing);
	}

	#[test]
	fn invalid_pager_line_is_rejected() {
		let dir = temp_dir("pager-bad");
		let path = dir.join("a.txt");
		std::fs::write(
			&path,
			format!(
				"{}\n{}InProgress\n{}\n",
				PROPERTY_PROGRESS_MARKER,
				"NOW:",
				"서울특별시 노원구 월계동 392-19"
			),
		)
		.unwrap();
		let err = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("페이지 표시가 없음");
		assert_eq!(err.code, ProgressFileErrorCode::InvalidPagerLine);
	}

	#[test]
	fn invalid_address_line_is_rejected() {
		let dir = temp_dir("addr-bad");
		let path = dir.join("a.txt");
		std::fs::write(
			&path,
			format!("{}\n7/30\n{}\n", PROPERTY_PROGRESS_MARKER, "이상한주소"),
		)
		.unwrap();
		let err = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("지번이 올바르지 않음");
		assert_eq!(err.code, ProgressFileErrorCode::InvalidAddressLine);
	}

	#[test]
	fn invalid_utf8_is_rejected() {
		let dir = temp_dir("utf8");
		let path = dir.join("a.txt");
		std::fs::write(&path, [0xFF, 0xFE, 0x41]).unwrap();
		let err = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("깨진 인코딩");
		assert_eq!(err.code, ProgressFileErrorCode::InvalidEncoding);
	}

	#[test]
	fn missing_file_reports_io_failure() {
		let dir = temp_dir("missing");
		let err = prepare_progress_update(&dir.join("absent.txt"), "서울특별시 노원구 월계동 392-20", "8/30")
			.expect_err("파일 없음");
		assert_eq!(err.code, ProgressFileErrorCode::IoFailure);
	}

	#[test]
	fn prepare_splices_pager_and_address_bytes() {
		let dir = temp_dir("splice");
		let path = dir.join("progress.txt");
		let original = valid_file();
		std::fs::write(&path, &original).unwrap();

		let update = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect("prepare");
		assert_eq!(update.source_bytes, original, "원본은 보존된다");

		let text = String::from_utf8(update.updated_bytes.clone()).expect("utf8");
		assert!(text.contains("8/30"), "페이지가 갱신된다: {text}");
		assert!(!text.contains("7/30"), "기존 페이지는 치환된다");
		assert!(text.contains("서울특별시 노원구 월계동 392-20"), "지번이 갱신된다: {text}");
		assert!(!text.contains("392-19"), "기존 지번은 치환된다");
		// 마커와 앞머리는 보존
		assert!(text.starts_with("다음 주소 목록\n현재 어디까지?"));
	}

	#[test]
	fn prepare_accepts_bom_and_crlf() {
		let dir = temp_dir("bom-crlf");
		let path = dir.join("progress.txt");
		let mut bytes = Vec::new();
		bytes.extend_from_slice(&UTF8_BOM);
		bytes.extend_from_slice("헤더\r\n".as_bytes());
		bytes.extend_from_slice(PROPERTY_PROGRESS_MARKER.as_bytes());
		bytes.extend_from_slice(b"\r\n7/30\r\n");
		bytes.extend_from_slice("서울특별시 노원구 월계동 392-19\r\n".as_bytes());
		std::fs::write(&path, &bytes).unwrap();

		let update = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect("BOM+CRLF 허용");
		let text = String::from_utf8(update.updated_bytes).expect("utf8");
		assert!(text.contains("8/30"));
		assert!(text.contains("392-20"));
		// CRLF 앞머리 보존
		assert!(text.starts_with("\u{feff}헤더\r\n현재 어디까지?"));
	}

	#[test]
	fn pager_prefix_takes_first_numeric_slash_numeric() {
		assert_eq!(extract_pager_prefix("7/30 남음"), Some("7/30"));
		assert_eq!(extract_pager_prefix("12/120"), Some("12/120"));
		assert_eq!(extract_pager_prefix(" NOW 7/30"), None, "줄 앞에 다른 텍스트면 매칭 안 됨");
		assert_eq!(extract_pager_prefix("0/5"), Some("0/5"), "형식은 맞지만 유효성 검사는 별도");
		assert_eq!(extract_pager_prefix("7/"), None);
		assert_eq!(extract_pager_prefix("/30"), None);
		assert_eq!(extract_pager_prefix(""), None);
		// 형식은 맞으나 pager 규약 위반(현재>전체)이면 prepare가 거부한다
		assert!(parse_property_pager("0/5").is_none());
		assert!(parse_property_pager("6/5").is_none());
	}

	/// Windows + PowerShell이 필요하다 — 실제 잠금 쓰기와 소스 변경 감지를 검증한다.
	#[cfg(windows)]
	#[test]
	fn commit_replaces_file_and_detects_source_change() {
		let dir = temp_dir("commit");
		let path = dir.join("progress.txt");
		let original = valid_file();
		std::fs::write(&path, &original).unwrap();

		let update = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-20", "8/30")
			.expect("prepare");
		commit_progress_update(&update).expect("commit");

		let after = std::fs::read(&path).expect("read back");
		let text = String::from_utf8(after.clone()).expect("utf8");
		assert!(text.contains("8/30"));
		assert!(text.contains("392-20"));
		// 스테이징 파일이 남지 않았다
		let leftovers: Vec<String> = std::fs::read_dir(&dir)
			.unwrap()
			.filter_map(|e| e.ok())
			.map(|e| e.file_name().to_string_lossy().to_string())
			.filter(|name| name.contains(".tmp"))
			.collect();
		assert!(leftovers.is_empty(), "스테이징 파일 잔존: {leftovers:?}");

		// 준비 이후 파일이 바뀌었으면 SOURCE_CHANGED
		let stale = prepare_progress_update(&path, "서울특별시 노원구 월계동 392-21", "9/30")
			.expect("prepare2");
		let mut tampered = std::fs::read(&path).expect("read");
		tampered.extend_from_slice(b"\n");
		std::fs::write(&path, tampered).expect("tamper");
		let err = commit_progress_update(&stale).expect_err("소스 변경 감지");
		assert_eq!(err.code, ProgressFileErrorCode::SourceChanged);
	}

	#[cfg(windows)]
	#[test]
	fn update_progress_file_round_trip() {
		let dir = temp_dir("update");
		let path = dir.join("progress.txt");
		std::fs::write(&path, valid_file()).unwrap();
		update_progress_file(&path, "서울특별시 노원구 월계동 392-18", "15/30").expect("update");
		let text = String::from_utf8(std::fs::read(&path).expect("read")).expect("utf8");
		assert!(text.contains("15/30"));
		assert!(text.contains("392-18"));
		assert!(text.contains(PROPERTY_PROGRESS_MARKER), "마커는 보존된다");
	}
}
