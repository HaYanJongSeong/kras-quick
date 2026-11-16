// TS scripts/kras-domain.ts 중 주소 라우팅 부분 — whitespace 정규화와 서울 구 단위 접두어 추론.
// (URL 판별 KRAS_TARGET_ORIGIN/PATHNAME은 property_watcher_page::matches_target가 이미 porting되어 있다.)

/// TS normalizeWhitespace — 모든 연속 공백을 한 칸으로 모은 뒤 양끝을 뗀다.
pub fn normalize_whitespace(address: &str) -> String {
	address.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// TS normalizeAddress — 정규화한 뒤 첫 토큰이 "구"로 끝나면 서울특별시를 붙인다.
/// 이미 "서울특별시"로 시작하는 완전한 주소는 그대로다(멱등).
pub fn normalize_address(address: &str) -> String {
	let normalized = normalize_whitespace(address);
	let first_token = normalized.split(' ').next().unwrap_or("");
	if first_token.ends_with('구') {
		format!("서울특별시 {normalized}")
	} else {
		normalized
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// TS tests/kras.test.ts "KRAS address routing"

	#[test]
	fn normalizes_whitespace_heavy_seoul_district_first_input() {
		assert_eq!(
			normalize_address("  강서구\t개화동   255-1  "),
			"서울특별시 강서구 개화동 255-1"
		);
	}

	#[test]
	fn keeps_complete_seoul_addresses_geographically_unchanged() {
		assert_eq!(
			normalize_address("서울특별시  강서구 개화동 255-1"),
			"서울특별시 강서구 개화동 255-1"
		);
	}

	#[test]
	fn keeps_fully_qualified_non_seoul_addresses_unchanged() {
		assert_eq!(
			normalize_address("충청북도  제천시 백운면 평동리 123-4"),
			"충청북도 제천시 백운면 평동리 123-4"
		);
	}

	#[test]
	fn keeps_non_seoul_legal_ri_addresses_unchanged() {
		assert_eq!(
			normalize_address("전라남도  담양군 무정면 봉안리 12-3"),
			"전라남도 담양군 무정면 봉안리 12-3"
		);
	}

	#[test]
	fn does_not_infer_seoul_when_first_token_does_not_end_in_gu() {
		assert_eq!(normalize_address("강서 동 255-1"), "강서 동 255-1");
	}

	#[test]
	fn is_idempotent() {
		let normalized = normalize_address("  강서구\t개화동 255-1 ");
		assert_eq!(normalize_address(&normalized), normalized);
	}
}
