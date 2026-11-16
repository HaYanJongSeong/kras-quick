// TS scripts/kras-page.ts 오류 타입 + scripts/kras-navigation.ts 조회 재시도.

/// TS KrasEvaluationResultError — 처리할 수 없는 평가 결과 (기본 메시지 없음, 전달 문구 사용).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasEvaluationResultError {
	pub message: String,
}

impl KrasEvaluationResultError {
	pub fn new(message: impl Into<String>) -> Self {
		Self { message: message.into() }
	}

	/// TS `new KrasEvaluationResultError("KRAS 주소 입력 평가 결과가 아닙니다.")`
	pub fn fill_result() -> Self {
		Self::new("KRAS 주소 입력 평가 결과가 아닙니다.")
	}

	/// TS `new KrasEvaluationResultError("KRAS 건물 조회 평가 결과가 아닙니다.")`
	pub fn lookup_result() -> Self {
		Self::new("KRAS 건물 조회 평가 결과가 아닙니다.")
	}
}

impl std::fmt::Display for KrasEvaluationResultError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for KrasEvaluationResultError {}

/// TS KrasFillError
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KrasFillError;

impl std::fmt::Display for KrasFillError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("KRAS 주소 입력 평가에 실패했습니다.")
	}
}

impl std::error::Error for KrasFillError {}

/// TS KrasFillTargetError
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KrasFillTargetError;

impl std::fmt::Display for KrasFillTargetError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("KRAS 주소 입력 대상 페이지가 아닙니다.")
	}
}

impl std::error::Error for KrasFillTargetError {}

/// TS KrasLookupError
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KrasLookupError;

impl std::fmt::Display for KrasLookupError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("KRAS 건물 조회 평가에 실패했습니다.")
	}
}

impl std::error::Error for KrasLookupError {}

/// TS KrasLookupTargetError
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KrasLookupTargetError;

impl std::fmt::Display for KrasLookupTargetError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("KRAS 건물 조회 대상 페이지가 아닙니다.")
	}
}

impl std::error::Error for KrasLookupTargetError {}

/// TS lookupAfterNavigation의 재시도 조건:
/// `/Execution context was destroyed|Cannot find context with specified id/`
pub fn is_context_destroyed_error(message: &str) -> bool {
	message.contains("Execution context was destroyed")
		|| message.contains("Cannot find context with specified id")
}

/// TS lookupAfterNavigation — 최대 3회, 컨텍스트 파괴 오류만 재시도한다.
///
/// `wait_ready`는 TS의 `waitForLoadState("domcontentloaded")`(15초) 대응,
/// `is_target`은 `isKrasTargetUrl(page.url())` 대응,
/// `not_target_error`는 대상 페이지가 아닐 때 던질 오류(TS의 KrasLookupTargetError),
/// `lookup`은 실제 조회 클로저다.
pub fn lookup_after_navigation<T, E, W, I, F>(
	not_target_error: E,
	mut wait_ready: W,
	is_target: I,
	mut lookup: F,
) -> Result<T, E>
where
	E: Clone + std::fmt::Display,
	W: FnMut() -> Result<(), E>,
	I: Fn() -> bool,
	F: FnMut() -> Result<T, E>,
{
	let mut last_error: Option<E> = None;
	for attempt in 1..=3u32 {
		if let Err(e) = wait_ready() {
			// waitForLoadState 실패는 TS에서 그대로 전파된다
			return Err(e);
		}
		if !is_target() {
			return Err(not_target_error);
		}
		match lookup() {
			Ok(value) => return Ok(value),
			Err(error) => {
				let retryable = is_context_destroyed_error(&error.to_string());
				if !retryable || attempt == 3 {
					return Err(error);
				}
				last_error = Some(error);
			}
		}
	}
	// 루프를 모두 소진한 경우(방어적 도달)
	Err(last_error.unwrap_or(not_target_error))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn error_messages_match_ts_exactly() {
		assert_eq!(KrasFillError.to_string(), "KRAS 주소 입력 평가에 실패했습니다.");
		assert_eq!(KrasFillTargetError.to_string(), "KRAS 주소 입력 대상 페이지가 아닙니다.");
		assert_eq!(KrasLookupError.to_string(), "KRAS 건물 조회 평가에 실패했습니다.");
		assert_eq!(KrasLookupTargetError.to_string(), "KRAS 건물 조회 대상 페이지가 아닙니다.");
		assert_eq!(
			KrasEvaluationResultError::fill_result().to_string(),
			"KRAS 주소 입력 평가 결과가 아닙니다."
		);
		assert_eq!(
			KrasEvaluationResultError::lookup_result().to_string(),
			"KRAS 건물 조회 평가 결과가 아닙니다."
		);
	}

	#[test]
	fn error_names_match_ts_class_names() {
		// TS `override readonly name` — 여기서는 디스플레이 대신 타입 식별자로 대체된다
		assert_eq!(std::any::type_name::<KrasFillError>(), "kras_quick::kras_page::KrasFillError");
		assert_eq!(
			std::any::type_name::<KrasLookupTargetError>(),
			"kras_quick::kras_page::KrasLookupTargetError"
		);
	}

	#[test]
	fn context_destroyed_patterns_match_ts_regex() {
		assert!(is_context_destroyed_error(
			"Execution context was destroyed, most likely because of a navigation."
		));
		assert!(is_context_destroyed_error(
			"Cannot find context with specified id: 123"
		));
		assert!(!is_context_destroyed_error("Element not found"));
		assert!(!is_context_destroyed_error("Execution context destroyed later"));
	}

	#[test]
	fn navigation_retries_context_destroyed_up_to_three_times() {
		let mut waits = 0u32;
		let mut lookups = 0u32;
		let result: Result<u32, String> = lookup_after_navigation(
			"KRAS 건물 조회 대상 페이지가 아닙니다.".to_string(),
			|| { waits += 1; Ok(()) },
			|| true,
			|| {
				lookups += 1;
				if lookups < 3 {
					Err("Execution context was destroyed".to_string())
				} else {
					Ok(42)
				}
			},
		);
		assert_eq!(result.expect("3회째 성공"), 42);
		assert_eq!(lookups, 3, "컨텍스트 파괴는 최대 3회까지 재시도");
		assert_eq!(waits, 3);
	}

	#[test]
	fn navigation_stops_after_third_context_destroyed() {
		let mut lookups = 0u32;
		let result: Result<u32, String> = lookup_after_navigation(
			"KRAS 건물 조회 대상 페이지가 아닙니다.".to_string(),
			|| Ok(()),
			|| true,
			|| { lookups += 1; Err("Execution context was destroyed".to_string()) },
		);
		let err = result.expect_err("3회 실패");
		assert_eq!(err, "Execution context was destroyed");
		assert_eq!(lookups, 3, "3회를 넘게 시도하지 않는다");
	}

	#[test]
	fn navigation_does_not_retry_unrelated_errors() {
		let mut lookups = 0u32;
		let result: Result<u32, String> = lookup_after_navigation(
			"KRAS 건물 조회 대상 페이지가 아닙니다.".to_string(),
			|| Ok(()),
			|| true,
			|| { lookups += 1; Err("요소를 찾지 못했습니다".to_string()) },
		);
		assert_eq!(lookups, 1, "관련 없는 오류는 즉시 전파");
		assert!(result.is_err());
	}

	#[test]
	fn navigation_fails_fast_when_not_target_page() {
		let mut lookups = 0u32;
		let result: Result<u32, KrasLookupTargetError> = lookup_after_navigation(
			KrasLookupTargetError,
			|| Ok(()),
			|| false,
			|| { lookups += 1; Ok(1) },
		);
		let err = result.expect_err("비대상");
		assert_eq!(err, KrasLookupTargetError);
		assert_eq!(err.to_string(), "KRAS 건물 조회 대상 페이지가 아닙니다.");
		assert_eq!(lookups, 0, "대상 페이지가 아니면 조회를 호출하지 않는다");
	}

	#[test]
	fn navigation_propagates_wait_failure() {
		let mut lookups = 0u32;
		let result: Result<u32, String> = lookup_after_navigation(
			"KRAS 건물 조회 대상 페이지가 아닙니다.".to_string(),
			|| Err("waitForLoadState timeout".to_string()),
			|| true,
			|| { lookups += 1; Ok(1) },
		);
		assert_eq!(result.expect_err("대기 실패"), "waitForLoadState timeout");
		assert_eq!(lookups, 0, "대기 실패 시 조회를 시도하지 않는다");
	}
}
