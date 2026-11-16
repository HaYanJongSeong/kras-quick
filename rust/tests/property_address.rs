use kras_quick::{parse_property_row, PropertyAddressErrorCode};

const NON_ADDRESS_CELL: &str = "<not-address>";

fn row_with_address(address: &str) -> Vec<Option<&str>> {
	vec![
		Some("1"),
		Some("공유재산"),
		Some("토지"),
		Some("강서구"),
		Some("개화동"),
		Some(NON_ADDRESS_CELL),
		Some(address),
		Some("미처리"),
	]
}

#[test]
fn returns_normalized_zero_based_cell_index_6_from_an_exact_eight_string_row() {
	// Given
	let row = row_with_address("  서울특별시\t강서구  개화동  255-1  ");

	// When
	let address = parse_property_row(&row).expect("정상 주소");

	// Then
	assert_eq!(address.as_str(), "서울특별시 강서구 개화동 255-1");
}

#[test]
fn accepts_ordinary_seoul_mountain_lot_forms() {
	// Given
	let rows = [
		row_with_address("서울특별시 종로구 청운동 산 1"),
		row_with_address("서울특별시 강서구 개화동 산255-1"),
	];

	// When / Then
	assert_eq!(
		parse_property_row(&rows[0])
			.expect("산 1 주소")
			.as_str(),
		"서울특별시 종로구 청운동 산 1"
	);
	assert_eq!(
		parse_property_row(&rows[1])
			.expect("산255-1 주소")
			.as_str(),
		"서울특별시 강서구 개화동 산 255-1"
	);
}

#[test]
fn accepts_nationwide_parcel_addresses() {
	// Given
	let inputs = [
		"경기도 군포시 금정동 849",
		"경기도 성남시 분당구 정자동 178-1",
		"부산광역시 해운대구 우동 1400",
		"경상북도 경주시 안강읍 옥산리 100-2",
	];

	// When / Then
	for input in inputs {
		let address = parse_property_row(&row_with_address(input))
			.expect("전국 지번 주소")
			.as_str()
			.to_string();
		assert_eq!(address, input);
	}
}

#[test]
fn rejects_rows_that_are_not_exact_eight_string_tuples() {
	// Given
	let base = row_with_address("서울특별시 강서구 개화동 255-1");
	let short = base[..7].to_vec();
	let long = {
		let mut row = base.clone();
		row.push(Some("extra"));
		row
	};
	let non_string_cell = {
		let mut row = base.clone();
		// TS의 숫자 셀 255(zod 문자열 튜플 실패 → INVALID_ROW)를 None으로 표현.
		row[6] = None;
		row
	};
	// TS의 { address: "..." } 객체 행은 Rust 타입(&[Option<&str>])으로 표현 불가
	// — 길이 검증이 같은 INVALID_ROW 경로를 커버한다.

	// When / Then
	for row in [short, long, non_string_cell] {
		let error = parse_property_row(&row).expect_err("잘못된 행");
		assert_eq!(error.code, PropertyAddressErrorCode::InvalidRow);
	}
}

#[test]
fn rejects_non_parcel_and_malformed_address_values() {
	// Given
	let long_address = format!("서울특별시 강서구 {}동 1", "가".repeat(150));
	let invalid_addresses = [
		"서울특별시 강서구 하늘길 38",
		"강서구 개화동 255-1",
		"서울특별시 강서구 개화동",
		"서울특별시 강서구 개화동 0",
		"서울특별시 강서구 개화동 255-0",
		"서울특별시 강서구 개화동 255-1 메모",
		long_address.as_str(),
	];

	// When / Then
	for address in invalid_addresses {
		let error = parse_property_row(&row_with_address(address))
			.expect_err("잘못된 주소");
		assert_eq!(error.code, PropertyAddressErrorCode::InvalidAddress);
		assert!(!error.to_string().contains(address));
	}
}
