// 터미널 환경 스냅샷 — 실제 값은 기본 해석, 테스트는 교체한다.

use std::sync::Mutex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct TermConfig {
	pub stdout_tty: bool,
	pub stdin_tty: bool,
	pub stderr_tty: bool,
	pub term: Option<String>,
	pub columns: usize,
	pub rows: usize,
}

static OVERRIDE: Mutex<Option<TermConfig>> = Mutex::new(None);
static DEFAULT: OnceLock<TermConfig> = OnceLock::new();

fn lock_config<T>(mutex: &Mutex<T>) -> T
where
	T: Clone,
{
	mutex
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner())
		.clone()
}

fn resolve() -> TermConfig {
	use std::io::IsTerminal;
	// 콘솔 폭·높이는 COLUMNS/ROWS 환경변수 우선, 없으면 기본값.
	// (실콘솔 쿼리는 런처 슬라이스에서 winapi로 연결한다.)
	let columns_env = std::env::var("COLUMNS").ok().and_then(|v| v.parse().ok());
	let rows_env = std::env::var("ROWS").ok().and_then(|v| v.parse().ok());
	TermConfig {
		stdout_tty: std::io::stdout().is_terminal(),
		stdin_tty: std::io::stdin().is_terminal(),
		stderr_tty: std::io::stderr().is_terminal(),
		term: std::env::var("TERM").ok(),
		columns: columns_env.unwrap_or(80),
		rows: rows_env.unwrap_or(30),
	}
}

pub fn config() -> TermConfig {
	if let Some(config) = lock_config(&OVERRIDE) {
		return config;
	}
	DEFAULT.get_or_init(resolve).clone()
}

/// 테스트/특수 실행용: Some이면 기본 해석 대신 그 값을 쓴다.
pub fn set_config(config: Option<TermConfig>) {
	*OVERRIDE
		.lock()
		.unwrap_or_else(|poisoned| poisoned.into_inner()) = config;
}
