use std::fmt;
use std::sync::OnceLock;

use regex::Regex;

// TS scripts/property-address.ts와 동일한 계약: 셀·주소 길이는 UTF-16 단위 기준.
const MAX_ROW_CELL_LENGTH: usize = 500;
const MAX_ADDRESS_LENGTH: usize = 120;

// JS의 \d는 ASCII 전용이므로 Rust 쪽은 [0-9]로 치환했다.
const PARCEL_ADDRESS: &str = r"^[가-힣·]+(?:특별시|광역시|특별자치시|특별자치도|도)\s+[가-힣·]+(?:시|군|구)(?:\s+[가-힣·]+구)?\s+(?:[가-힣·]+(?:동|읍|면|리)|[가-힣·]+[1-9]?[0-9]?가|세종로)(?:\s+[가-힣·]+(?:동|읍|면|리))?\s+(?:산\s*)?[1-9][0-9]{0,3}(?:-[1-9][0-9]{0,3})?$";
// JS replace(/ 산\s*(?=\d)/u, " 산 ") — 룩어헤드 미지원이라 자릿수를 캡처로
// 재귀출하는 동치 패턴. /g 없는 치환과 같이 첫 매치만 바꾼다.
const SAN_FIX: &str = r" 산\s*([0-9])";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParcelAddress(String);

impl ParcelAddress {
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyAddressErrorCode {
	InvalidRow,
	InvalidAddress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyAddressError {
	pub code: PropertyAddressErrorCode,
}

impl fmt::Display for PropertyAddressError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		// TS가 모든 오류에 쓰는 고정 메시지 — 입력값을 포함하지 않는다.
		f.write_str("SCP 행의 지번 주소가 올바르지 않습니다.")
	}
}

impl std::error::Error for PropertyAddressError {}

fn row_error() -> PropertyAddressError {
	PropertyAddressError {
		code: PropertyAddressErrorCode::InvalidRow,
	}
}

fn address_error() -> PropertyAddressError {
	PropertyAddressError {
		code: PropertyAddressErrorCode::InvalidAddress,
	}
}

fn utf16_len(value: &str) -> usize {
	value.encode_utf16().count()
}

fn parcel_regex() -> &'static Regex {
	static RE: OnceLock<Regex> = OnceLock::new();
	RE.get_or_init(|| {
		Regex::new(PARCEL_ADDRESS).expect("PARCEL_ADDRESS pattern must compile")
	})
}

fn san_fix_regex() -> &'static Regex {
	static RE: OnceLock<Regex> = OnceLock::new();
	RE.get_or_init(|| Regex::new(SAN_FIX).expect("SAN_FIX pattern must compile"))
}

pub fn parse_parcel_address(value: &str) -> Result<ParcelAddress, PropertyAddressError> {
	if utf16_len(value) > MAX_ADDRESS_LENGTH {
		return Err(address_error());
	}
	// zod: .transform(replace(/\s+/gu, " ").trim())
	let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
	// zod: .transform(replace(/ 산\s*(?=\d)/u, " 산 "))
	let normalized = san_fix_regex().replace(&collapsed, " 산 $1");
	if !parcel_regex().is_match(&normalized) {
		return Err(address_error());
	}
	Ok(ParcelAddress(normalized.into_owned()))
}

pub fn parse_property_row(row: &[Option<&str>]) -> Result<ParcelAddress, PropertyAddressError> {
	if row.len() != 8 {
		return Err(row_error());
	}
	for cell in row {
		match cell {
			Some(text) if utf16_len(text) <= MAX_ROW_CELL_LENGTH => {}
			_ => return Err(row_error()),
		}
	}
	// 위 검증 통과 시 cell 6은 항상 Some.
	let Some(address) = row[6] else {
		return Err(row_error());
	};
	parse_parcel_address(address)
}
