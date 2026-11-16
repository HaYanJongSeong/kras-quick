// TS scripts/property-watcher-guidance.ts — SCP 페이지 안내 패널의 텍스트·CSS 상수.
// 주입 소스와 함께 listener 모듈에서 관리하므로 여기서는 공개 API로 내보내고
// TS 원문과의 동등성을 검증한다.

pub use crate::property_watcher_listener::{
	PROPERTY_GUIDANCE_ABSENT_PROMPT, PROPERTY_GUIDANCE_COPY_FAILURE, PROPERTY_GUIDANCE_CSS,
	PROPERTY_GUIDANCE_ITEMS,
};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn guidance_items_match_ts_pairs_exactly() {
		assert_eq!(PROPERTY_GUIDANCE_ITEMS.len(), 4);
		assert_eq!(
			PROPERTY_GUIDANCE_ITEMS[0],
			(
				"건물구분에 건물이 안뜨면 - 부동산 종합증명서 조회결과 건물 조회 불가",
				"부동산 종합증명서 조회결과 건물 조회 불가",
			)
		);
		assert_eq!(
			PROPERTY_GUIDANCE_ITEMS[1],
			(
				"건물이 있는데 서울시 소유가 아니면 - 부동산 종합증명서 조회결과 건물 시유재산 아님",
				"부동산 종합증명서 조회결과 건물 시유재산 아님",
			)
		);
		assert_eq!(
			PROPERTY_GUIDANCE_ITEMS[2],
			(
				"소유자가 아예 안 보이면 - 부동산 종합증명서 조회결과 건물 소유 확인 불명",
				"부동산 종합증명서 조회결과 건물 소유 확인 불명",
			)
		);
		assert_eq!(
			PROPERTY_GUIDANCE_ITEMS[3],
			(
				"서울시 소유면 - 부동산 종합증명서 조회결과 건물 시유재산 확인",
				"부동산 종합증명서 조회결과 건물 시유재산 확인",
			)
		);
	}

	#[test]
	fn guidance_prompts_match_ts_exactly() {
		assert_eq!(
			PROPERTY_GUIDANCE_COPY_FAILURE,
			"복사하지 못했습니다. 다시 시도하거나 문장을 직접 선택해 주세요."
		);
		assert_eq!(
			PROPERTY_GUIDANCE_ABSENT_PROMPT,
			"아래 첫 번째 안내를 눌러 ‘부동산 종합증명서 조회결과 건물 조회 불가’를 복사하세요."
		);
		// TS는 리포트 결과 문구를 안내 첫 항목과 동일하게 쓴다
		assert_eq!(
			PROPERTY_GUIDANCE_ABSENT_PROMPT,
			format!(
				"아래 첫 번째 안내를 눌러 ‘{}’를 복사하세요.",
				PROPERTY_GUIDANCE_ITEMS[0].1
			)
		);
	}

	#[test]
	fn guidance_css_keeps_ts_structure() {
		let css = PROPERTY_GUIDANCE_CSS;
		assert!(css.len() > 2_000, "CSS가 잘렸을 가능성: {}바이트", css.len());
		// 패널 식별자와 레이어
		assert!(css.starts_with("#opencode-property-guidance-panel {"));
		assert!(css.contains("z-index: 2147483000;"));
		assert!(css.contains("position: fixed;"));
		// TS와 동일한 토큰·자식 선택자
		for selector in [
			"#opencode-property-guidance-panel *",
			"#opencode-property-guidance-label",
			"#opencode-property-guidance-address",
			"#opencode-property-guidance-building-status",
			"#opencode-property-guidance-building-status.opencode-property-guidance-building-status--present",
			"#opencode-property-guidance-heading",
			"#opencode-property-guidance-feedback:empty",
			"#opencode-property-guidance-feedback:not(:empty)",
		] {
			assert!(css.contains(selector), "누락된 선택자: {selector}");
		}
		// 라이트/다크 변수 체계
		for var in [
			"--opencode-guidance-light-panel: #fff;",
			"--opencode-guidance-dark-panel: #292b2f;",
			"--opencode-guidance-light-present: #087f5b;",
			"--opencode-guidance-dark-present: #7ee2c0;",
		] {
			assert!(css.contains(var), "누락된 변수: {var}");
		}
		// 미디어쿼리·특수 모드·모바일 대응
		assert!(css.contains("@media (prefers-color-scheme: dark)"));
		assert!(css.contains(":root:has(#opencode-dark-mode)"));
		assert!(css.contains("@media (max-width: 767px)"));
		assert!(css.contains("cursor: copy;"));
		assert!(css.contains("word-break: keep-all;"));
		assert!(css.contains("-webkit-user-select: text;"));
	}

	#[test]
	fn guidance_is_really_embedded_in_listener_source() {
		// 상수가 단순 선언에 그치지 않고 주입 소스에 실제로 들어가는지 확인한다.
		let source = crate::property_watcher_listener::listener_source();
		assert!(source.contains(&PROPERTY_GUIDANCE_ITEMS[0].1), "안내 1번이 소스에 없음");
		assert!(source.contains(&PROPERTY_GUIDANCE_ITEMS[3].1), "안내 4번이 없음");
		assert!(source.contains(&PROPERTY_GUIDANCE_COPY_FAILURE), "복사 실패 문구가 없음");
		assert!(source.contains(&PROPERTY_GUIDANCE_ABSENT_PROMPT), "부재 안내가 없음");
		assert!(source.contains("opencode-property-guidance-panel"), "패널 CSS가 없음");
		// JSON.stringify 치환 결과가 남아 있지 않은지 확인
		assert!(!source.contains("JSON.stringify(PROPERTY_GUIDANCE"), "치환 전 토큰이 남아 있다");
	}
}
