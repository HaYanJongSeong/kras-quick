use crate::lawdong;
use crate::property_address::parse_parcel_address;

// PNU(19자리) → 지번 주소 변환.
// 구조: 법정동코드(10) + 대지구분(1) + 본번(4) + 부번(4). 대지구분 1=일반, 2=산.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandType {
	Plain,
	Mountain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PnuParts {
	pub legal_dong_code: String,
	pub land_type: LandType,
	pub main_number: u16,
	pub sub_number: u16,
}

pub fn is_pnu_input(value: &str) -> bool {
	// JS /^\d{19}$/ (\d = ASCII). 바이트 19개 전부 ASCII 숫자와 동치.
	value.len() == 19 && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn parse_pnu(pnu: &str) -> Result<PnuParts, String> {
	if !is_pnu_input(pnu) {
		return Err(format!(
			"PNU 형식이 올바르지 않습니다. 19자리 숫자여야 합니다. (입력: {pnu})"
		));
	}
	let legal_dong_code = &pnu[0..10];
	// 4자리 ASCII 숫자의 parseInt는 항상 0..=9999 — TS의 본번/부번 범위 검사는
	// 여기서 도달할 수 없는 코드라 생략했다.
	let main_number: u16 = pnu[11..15].parse().expect("4 ASCII digits");
	let sub_number: u16 = pnu[15..19].parse().expect("4 ASCII digits");
	if main_number == 0 {
		return Err(format!(
			"PNU 본번이 0입니다. 변환할 수 없습니다. (PNU: {pnu})"
		));
	}
	Ok(PnuParts {
		legal_dong_code: legal_dong_code.to_string(),
		land_type: if pnu.as_bytes()[10] == b'2' {
			LandType::Mountain
		} else {
			LandType::Plain
		},
		main_number,
		sub_number,
	})
}

pub fn pnu_to_address(pnu: &str) -> Result<String, String> {
	let parts = parse_pnu(pnu)?;
	let Some(dong_name) = lawdong::lookup(&parts.legal_dong_code) else {
		return Err(format!(
			"알 수 없는 법정동코드입니다. (코드: {}, PNU: {pnu})",
			parts.legal_dong_code
		));
	};
	let parcel = match parts.land_type {
		LandType::Mountain => format!("산 {}", parts.main_number),
		LandType::Plain => parts.main_number.to_string(),
	};
	let address = if parts.sub_number > 0 {
		format!("{dong_name} {parcel}-{}", parts.sub_number)
	} else {
		format!("{dong_name} {parcel}")
	};
	// TS와 같이 변환 결과를 지번 형식으로 재검증한다(실패 시 메시지 감쌈).
	if let Err(error) = parse_parcel_address(&address) {
		return Err(format!(
			"PNU 변환 주소가 지번 형식에 맞지 않습니다. ({address}, PNU: {pnu}) — {error}"
		));
	}
	Ok(address)
}
