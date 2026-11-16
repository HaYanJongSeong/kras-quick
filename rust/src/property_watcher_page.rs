// TS scripts/property-watcher-page.ts — SCP/KRAS 타깃 URL 판별과 대상 발견.
// URL은 URL.canParse + origin/pathname 정확 일치로 판별한다(TS matchesTarget 동일).

/// TS SCP_ORIGIN / SCP_PATHNAME
pub const SCP_ORIGIN: &str = "http://scpweb.softgraphy.biz";
pub const SCP_PATHNAME: &str = "/board";

/// TS KRAS_ORIGIN / KRAS_PATHNAME
pub const KRAS_ORIGIN: &str = "https://www.kras.go.kr";
pub const KRAS_PATHNAME: &str = "/kras/cert/certView.do";

/// TS PropertyWatcherErrorCode — 현재는 TARGET_COUNT_INVALID 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherErrorCode {
	TargetCountInvalid,
}

impl PropertyWatcherErrorCode {
	pub fn as_str(self) -> &'static str {
		match self {
			PropertyWatcherErrorCode::TargetCountInvalid => "TARGET_COUNT_INVALID",
		}
	}
}

/// TS PropertyWatcherError — 코드별 기본 메시지를 갖는 오류.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherError {
	pub code: PropertyWatcherErrorCode,
	pub message: String,
}

impl PropertyWatcherError {
	pub fn new(code: PropertyWatcherErrorCode) -> Self {
		let message = match code {
			PropertyWatcherErrorCode::TargetCountInvalid => {
				"SCP와 KRAS 대상 페이지는 각각 정확히 하나여야 합니다."
			}
		};
		Self { code, message: message.to_string() }
	}

	pub fn with_message(code: PropertyWatcherErrorCode, message: impl Into<String>) -> Self {
		Self { code, message: message.into() }
	}
}

impl std::fmt::Display for PropertyWatcherError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}: {}", self.code.as_str(), self.message)
	}
}

impl std::error::Error for PropertyWatcherError {}

/// 스킴+호스트(origin)와 경로(pathname, 쿼리·프래그먼트 제외)를 분리한다.
/// URL 파서 대신 직접 분리한다(의존성 최소화, TS `new URL()`과 동일 결과).
fn origin_and_pathname(value: &str) -> Option<(String, String)> {
	let scheme_end = value.find("://")?;
	let scheme = &value[..scheme_end];
	if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+') {
		return None;
	}
	let after = &value[scheme_end + 3..];
	let host_end = after.find(['/', '?', '#']).unwrap_or(after.len());
	let host = &after[..host_end];
	if host.is_empty() {
		return None;
	}
	let rest = if host_end >= after.len() { "" } else { &after[host_end..] };
	let path = rest.split(['?', '#']).next().unwrap_or("");
	// URL 규약상 경로가 비면 "/" 로 취급한다
	let path = if path.is_empty() { "/" } else { path };
	Some((format!("{scheme}://{host}"), path.to_string()))
}

/// TS matchesTarget — origin과 pathname이 정확히 일치해야 한다.
pub fn matches_target(value: &str, origin: &str, pathname: &str) -> bool {
	match origin_and_pathname(value) {
		Some((o, p)) => o == origin && p == pathname,
		None => false,
	}
}

/// URL의 origin(스킴+호스트)만 돌려준다 — 상태 기계가 컨텍스트 origin과 비교할 때 쓴다.
pub fn url_origin(value: &str) -> Option<String> {
	origin_and_pathname(value).map(|(origin, _)| origin)
}

/// TS isScpTargetUrl
pub fn is_scp_target_url(value: &str) -> bool {
	matches_target(value, SCP_ORIGIN, SCP_PATHNAME)
}

/// TS isKrasWatcherTargetUrl — KRAS 워처 대상 페이지 판별.
pub fn is_kras_watcher_target_url(value: &str) -> bool {
	matches_target(value, KRAS_ORIGIN, KRAS_PATHNAME)
}

/// TS discoverPropertyWatcherTargets — SCP·KRAS 타깃이 각각 정확히 하나씩 있어야 한다.
/// 반환값은 `(kras_index, scp_index)`다.
pub fn discover_property_watcher_targets<T: AsRef<str>>(
	pages: &[T],
) -> Result<(usize, usize), PropertyWatcherError> {
	let scp: Vec<usize> = pages
		.iter()
		.enumerate()
		.filter(|(_, p)| is_scp_target_url(p.as_ref()))
		.map(|(i, _)| i)
		.collect();
	let kras: Vec<usize> = pages
		.iter()
		.enumerate()
		.filter(|(_, p)| is_kras_watcher_target_url(p.as_ref()))
		.map(|(i, _)| i)
		.collect();
	if scp.len() != 1 || kras.len() != 1 {
		return Err(PropertyWatcherError::new(PropertyWatcherErrorCode::TargetCountInvalid));
	}
	Ok((kras[0], scp[0]))
}

/// TS discoverKrasTarget — KRAS 타깃이 최소 하나 있어야 한다.
pub fn discover_kras_target<T: AsRef<str>>(pages: &[T]) -> Result<usize, PropertyWatcherError> {
	pages
		.iter()
		.enumerate()
		.find(|(_, p)| is_kras_watcher_target_url(p.as_ref()))
		.map(|(i, _)| i)
		.ok_or_else(|| {
			PropertyWatcherError::with_message(
				PropertyWatcherErrorCode::TargetCountInvalid,
				"KRAS 대상 페이지를 찾지 못했습니다.",
			)
		})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn kras_target_url_matches_exactly() {
		let exact = "https://www.kras.go.kr/kras/cert/certView.do";
		assert!(is_kras_watcher_target_url(exact));
		// 쿼리·프래그먼트는 pathname에 영향 없음(TS new URL과 동일)
		assert!(is_kras_watcher_target_url(&format!("{exact}?uri=/x")));
		assert!(is_kras_watcher_target_url(&format!("{exact}#top")));
		// 호스트/스킴/경로 불일치
		assert!(!is_kras_watcher_target_url("https://kras.go.kr/kras/cert/certView.do"));
		assert!(!is_kras_watcher_target_url("http://www.kras.go.kr/kras/cert/certView.do"));
		assert!(!is_kras_watcher_target_url("https://www.kras.go.kr/mainView.do"));
		assert!(!is_kras_watcher_target_url("https://www.kras.go.kr/kras/cert/certView.doExtra"));
		assert!(!is_kras_watcher_target_url("https://www.kras.go.kr/login/loginView.do?uri=/kras/cert/certView.do"));
		assert!(!is_kras_watcher_target_url("not a url"));
		assert!(!is_kras_watcher_target_url(""));
		assert!(!is_kras_watcher_target_url("https:///kras/cert/certView.do"));
	}

	#[test]
	fn scp_target_url_requires_http_origin_and_board_path() {
		assert!(is_scp_target_url("http://scpweb.softgraphy.biz/board"));
		assert!(is_scp_target_url("http://scpweb.softgraphy.biz/board?page=2"));
		assert!(!is_scp_target_url("https://scpweb.softgraphy.biz/board"));
		assert!(!is_scp_target_url("http://scpweb.softgraphy.biz/board/list"));
		assert!(!is_scp_target_url("http://scpweb.softgraphy.biz/"));
	}

	#[test]
	fn empty_path_is_root_only() {
		// origin 뒤에 경로가 없으면 "/" — /board와는 일치하지 않아야 한다
		assert!(!matches_target("http://scpweb.softgraphy.biz", SCP_ORIGIN, SCP_PATHNAME));
		assert!(matches_target("https://www.kras.go.kr/", KRAS_ORIGIN, "/"));
	}

	#[test]
	fn discover_targets_requires_exactly_one_of_each() {
		let pages = vec![
			"http://scpweb.softgraphy.biz/board".to_string(),
			"https://www.kras.go.kr/kras/cert/certView.do".to_string(),
			"https://www.kras.go.kr/mainView.do".to_string(),
		];
		assert_eq!(discover_property_watcher_targets(&pages).expect("1:1"), (1, 0));
	}

	#[test]
	fn discover_targets_rejects_duplicate_scp() {
		let pages = vec![
			"http://scpweb.softgraphy.biz/board".to_string(),
			"http://scpweb.softgraphy.biz/board?page=2".to_string(),
			"https://www.kras.go.kr/kras/cert/certView.do".to_string(),
		];
		let err = discover_property_watcher_targets(&pages).expect_err("SCP 둘");
		assert_eq!(err.code, PropertyWatcherErrorCode::TargetCountInvalid);
		assert_eq!(err.code.as_str(), "TARGET_COUNT_INVALID");
		assert_eq!(
			err.message,
			"SCP와 KRAS 대상 페이지는 각각 정확히 하나여야 합니다."
		);
	}

	#[test]
	fn discover_targets_rejects_missing_kras() {
		let pages = vec!["http://scpweb.softgraphy.biz/board".to_string()];
		let err = discover_property_watcher_targets(&pages).expect_err("KRAS 없음");
		assert_eq!(err.code, PropertyWatcherErrorCode::TargetCountInvalid);
	}

	#[test]
	fn discover_kras_target_returns_index_or_named_error() {
		let pages = vec![
			"https://www.kras.go.kr/mainView.do".to_string(),
			"https://www.kras.go.kr/kras/cert/certView.do".to_string(),
		];
		assert_eq!(discover_kras_target(&pages).expect("found"), 1);

		let none = vec!["https://www.kras.go.kr/mainView.do".to_string()];
		let err = discover_kras_target(&none).expect_err("없음");
		assert_eq!(err.message, "KRAS 대상 페이지를 찾지 못했습니다.");
		assert_eq!(err.code.as_str(), "TARGET_COUNT_INVALID");
	}

	#[test]
	fn error_display_contains_code_and_message() {
		let err = PropertyWatcherError::new(PropertyWatcherErrorCode::TargetCountInvalid);
		let text = err.to_string();
		assert!(text.starts_with("TARGET_COUNT_INVALID: "));
		assert!(text.contains("정확히 하나"));
	}
}
