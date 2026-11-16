use kras_quick::{is_pnu_input, parse_pnu, pnu_to_address, LandType, PnuParts};

#[test]
fn accepts_exactly_19_digits() {
	assert!(is_pnu_input("1144012700104380000"));
}

#[test]
fn rejects_non_19_digit_or_non_numeric_input() {
	assert!(!is_pnu_input("114401270010438000"));
	assert!(!is_pnu_input("11440127001043800000"));
	assert!(!is_pnu_input("1144012700O4380000"));
	assert!(!is_pnu_input("서울특별시 마포구 상암동 438"));
	assert!(!is_pnu_input(""));
}

#[test]
fn splits_legal_dong_code_land_type_main_and_sub_numbers() {
	assert_eq!(
		parse_pnu("1141011800200110047").expect("유효 PNU"),
		PnuParts {
			legal_dong_code: "1141011800".to_string(),
			land_type: LandType::Mountain,
			main_number: 11,
			sub_number: 47,
		}
	);
}

#[test]
fn treats_land_type_1_as_plain_and_strips_leading_zeros() {
	assert_eq!(
		parse_pnu("1144012700104380000").expect("유효 PNU"),
		PnuParts {
			legal_dong_code: "1144012700".to_string(),
			land_type: LandType::Plain,
			main_number: 438,
			sub_number: 0,
		}
	);
}

#[test]
fn throws_on_a_non_19_digit_pnu() {
	let error = parse_pnu("123").expect_err("잘못된 PNU");
	assert!(error.contains("19자리"), "error: {error}");
}

#[test]
fn throws_when_main_number_is_zero() {
	let error = parse_pnu("1144012700100000000").expect_err("본번 0 PNU");
	assert!(error.contains("본번이 0"), "error: {error}");
}

#[test]
fn converts_a_plain_lot_without_sub_number() {
	assert_eq!(
		pnu_to_address("1144012700104380000").expect("변환"),
		"서울특별시 마포구 상암동 438"
	);
}

#[test]
fn converts_a_plain_lot_with_sub_number() {
	assert_eq!(
		pnu_to_address("1117010200100470013").expect("변환"),
		"서울특별시 용산구 용산동2가 47-13"
	);
}

#[test]
fn converts_a_mountain_lot() {
	assert_eq!(
		pnu_to_address("1141011800200110047").expect("변환"),
		"서울특별시 서대문구 홍은동 산 11-47"
	);
}

#[test]
fn throws_on_an_unknown_legal_dong_code() {
	let error = pnu_to_address("9999999999100010000").expect_err("미등록 코드");
	assert!(error.contains("알 수 없는 법정동코드"), "error: {error}");
}
