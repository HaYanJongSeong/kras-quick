// TS scripts/kras-runner.ts — 조회/다크 모드 두 모드의 CLI 인자 파싱과 실행.
// TS는 별도 node 스크립트를 자식으로 띄우고, Rust는 같은 실행 파일의 `lookup`/`dark_mode` 모드로 대체한다.

use crate::kras_lookup::{apply_kras_dark_mode, lookup_kras};

/// TS RunnerMode
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KrasRunnerCommand {
	Lookup { address: String, dark_mode: bool },
	DarkMode,
}

/// TS KrasRunnerUsageError — name이 실패 JSON 줄에 실린다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasRunnerUsageError(String);

impl KrasRunnerUsageError {
	pub fn new(message: impl Into<String>) -> Self {
		Self(message.into())
	}

	/// TS error.name
	pub fn name(&self) -> &'static str {
		"KrasRunnerUsageError"
	}
}

impl std::fmt::Display for KrasRunnerUsageError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.0)
	}
}

impl std::error::Error for KrasRunnerUsageError {}

/// TS parseMode — "lookup" | "dark_mode" 둘만 허용한다.
fn parse_mode(value: Option<&str>) -> Result<&'static str, KrasRunnerUsageError> {
	match value {
		Some("lookup") => Ok("lookup"),
		Some("dark_mode") => Ok("dark_mode"),
		_ => Err(KrasRunnerUsageError::new("KRAS 실행 모드가 올바르지 않습니다.")),
	}
}

/// TS parseDarkMode — "true"/"false"만 허용한다.
fn parse_dark_mode(value: Option<&str>) -> Result<bool, KrasRunnerUsageError> {
	match value {
		Some("true") => Ok(true),
		Some("false") => Ok(false),
		_ => Err(KrasRunnerUsageError::new("다크 모드 인수가 올바르지 않습니다.")),
	}
}

/// TS parseCommand — 인자 개수와 공백 주소를 함께 검사한다.
pub fn parse_kras_runner_command(
	args: &[String],
) -> Result<KrasRunnerCommand, KrasRunnerUsageError> {
	match parse_mode(args.first().map(String::as_str))? {
		"lookup" => {
			let address = args.get(1);
			if args.len() != 3 || address.map(|value| value.trim().is_empty()).unwrap_or(true) {
				return Err(KrasRunnerUsageError::new("조회 주소 인수가 올바르지 않습니다."));
			}
			Ok(KrasRunnerCommand::Lookup {
				address: address.cloned().unwrap_or_default(),
				dark_mode: parse_dark_mode(args.get(2).map(String::as_str))?,
			})
		}
		"dark_mode" => {
			if args.len() != 1 {
				return Err(KrasRunnerUsageError::new("다크 모드 인수가 너무 많습니다."));
			}
			Ok(KrasRunnerCommand::DarkMode)
		}
		_ => unreachable!("parse_mode이 두 모드만 허용한다"),
	}
}

/// TS executeCommand — 모드별 KRAS 작업 하나를 수행해 결과 문장을 돌려준다.
pub fn execute_kras_runner_command(
	command: &KrasRunnerCommand,
) -> Result<String, Box<dyn std::error::Error>> {
	match command {
		KrasRunnerCommand::Lookup { address, dark_mode } => lookup_kras(address, *dark_mode),
		KrasRunnerCommand::DarkMode => apply_kras_dark_mode(),
	}
}

/// TS 최상위 실행 — 파싱→실행. 성공 문장은 개행 없이 그대로 표준 출력으로 보낸다.
pub fn run_kras_runner(args: &[String]) -> Result<String, Box<dyn std::error::Error>> {
	let command = parse_kras_runner_command(args)
		.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
	execute_kras_runner_command(&command)
}

/// CLI 출력 계약: TS `process.stdout.write(output)` — 개행 없음.
pub fn print_kras_runner_output(output: &str) {
	print!("{output}");
	let _ = std::io::Write::flush(&mut std::io::stdout());
}

#[cfg(test)]
mod tests {
	use super::*;

	fn args(values: &[&str]) -> Vec<String> {
		values.iter().map(|value| value.to_string()).collect()
	}

	// TS tests/kras-runner 파싱 규칙 (kras-bridge.test와 공유되는 인자 계약)

	#[test]
	fn parses_lookup_with_dark_flag() {
		assert_eq!(
			parse_kras_runner_command(&args(&["lookup", "서울특별시 노원구 월계동 392-19", "true"]))
				.expect("조회 명령"),
			KrasRunnerCommand::Lookup {
				address: "서울특별시 노원구 월계동 392-19".to_string(),
				dark_mode: true,
			}
		);
		assert_eq!(
			parse_kras_runner_command(&args(&["lookup", "주소", "false"])).expect("조회 명령"),
			KrasRunnerCommand::Lookup {
				address: "주소".to_string(),
				dark_mode: false,
			}
		);
	}

	#[test]
	fn parses_dark_mode_without_extra_arguments() {
		assert_eq!(
			parse_kras_runner_command(&args(&["dark_mode"])).expect("다크 모드"),
			KrasRunnerCommand::DarkMode
		);
	}

	#[test]
	fn rejects_wrong_mode_bad_dark_flag_empty_address_and_extra_arguments() {
		let cases: [(&[&str], &str); 6] = [
			(&["other"], "KRAS 실행 모드가 올바르지 않습니다."),
			(&["lookup", "주소", "yes"], "다크 모드 인수가 올바르지 않습니다."),
			(&["lookup", "주소"], "조회 주소 인수가 올바르지 않습니다."),
			(&["lookup", "   ", "true"], "조회 주소 인수가 올바르지 않습니다."),
			(&["lookup", "주소", "true", "extra"], "조회 주소 인수가 올바르지 않습니다."),
			(&["dark_mode", "extra"], "다크 모드 인수가 너무 많습니다."),
		];
		for (values, message) in cases {
			let error = parse_kras_runner_command(&args(values)).expect_err("거부되어야 한다");
			assert_eq!(error.to_string(), message);
			assert_eq!(error.name(), "KrasRunnerUsageError");
		}
	}
}
