// TS scripts/kras-bridge.ts — 조회/다크 모드를 별도 자식 프로세스로 실행하는 다리.
// TS는 `node kras-runner.ts ...`를 띄우고, Rust는 자기 자신(`kras-quick lookup|dark_mode`)을 띄운다.
// 실행기(executor)를 주입받아 자식 비용 없이 결과 계약만 검증할 수 있다(TS 테스트와 동일 구조).

use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// TS CHILD_OPTIONS = { encoding: "utf8", timeout: 30000, windowsHide: true }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildCallOptions {
	pub timeout_ms: u64,
	pub windows_hide: bool,
}

impl Default for ChildCallOptions {
	fn default() -> Self {
		Self { timeout_ms: 30_000, windows_hide: true }
	}
}

/// TS ChildResult
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildResult {
	Ok { stdout: String },
	Failed,
}

impl ChildResult {
	pub fn is_ok(&self) -> bool {
		matches!(self, ChildResult::Ok { .. })
	}
}

/// TS KrasChildCommand
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KrasChildCommand {
	Lookup { address: String, dark_mode: bool },
	DarkMode,
}

/// TS KrasChildError — name이 실패 JSON 줄에 실린다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasChildError(String);

impl KrasChildError {
	pub fn new(message: impl Into<String>) -> Self {
		Self(message.into())
	}

	/// TS error.name
	pub fn name(&self) -> &'static str {
		"KrasChildError"
	}
}

impl std::fmt::Display for KrasChildError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(&self.0)
	}
}

impl std::error::Error for KrasChildError {}

/// TS childArgs — TS는 [RUNNER_PATH, mode, ...]를 node에 넘기고,
/// Rust는 실행 파일 자체가 러너이므로 mode부터 넘긴다.
pub fn child_kras_args(command: &KrasChildCommand) -> Vec<String> {
	match command {
		KrasChildCommand::Lookup { address, dark_mode } => vec![
			"lookup".to_string(),
			address.clone(),
			if *dark_mode { "true" } else { "false" }.to_string(),
		],
		KrasChildCommand::DarkMode => vec!["dark_mode".to_string()],
	}
}

/// TS ChildExecutor — (실행 파일, 인자, 옵션)을 받아 성공 여부와 stdout를 돌려준다.
pub type ChildExecutor =
	Box<dyn FnMut(&str, &[String], &ChildCallOptions) -> ChildResult>;

/// TS runKrasChild — 실패·빈 출력을 오류로 바꾸고, 성공이면 stdout를 정리해 돌려준다.
pub fn run_kras_child(
	execute: &mut ChildExecutor,
	command: &KrasChildCommand,
) -> Result<String, KrasChildError> {
	let args = child_kras_args(command);
	let result = execute("kras-quick", &args, &ChildCallOptions::default());
	if !result.is_ok() {
		return Err(KrasChildError::new("KRAS 실행에 실패했습니다."));
	}
	let ChildResult::Ok { stdout } = result else {
		return Err(KrasChildError::new("KRAS 실행에 실패했습니다."));
	};
	let output = stdout.trim().to_string();
	if output.is_empty() {
		return Err(KrasChildError::new("KRAS 실행 결과가 비어 있습니다."));
	}
	Ok(output)
}

/// 기본 실행기 — 현재 실행 파일의 같은 모드를 자식으로 띄운다(TS executeNode 대응).
/// 창이 뜨지 않게 windowsHide를 지키고, 제한 시간을 넘기면 종료한다.
pub fn default_child_executor() -> ChildExecutor {
	Box::new(|executable, args, options| {
		let exe = match std::env::current_exe() {
			Ok(path) => path,
			Err(_) => return ChildResult::Failed,
		};
		let _ = executable;
		let mut command = std::process::Command::new(exe);
		command.args(args);
		command.stdout(std::process::Stdio::piped());
		command.stderr(std::process::Stdio::null());
		if options.windows_hide {
			// CREATE_NO_WINDOW (0x08000000) — TS windowsHide: true
			command.creation_flags(0x08000000);
		}
		let mut child = match command.spawn() {
			Ok(child) => child,
			Err(_) => return ChildResult::Failed,
		};
		// stdout를 별도 스레드에서 읽는다(버퍼 고착 방지)
		let mut stdout_pipe = child.stdout.take();
		let reader = std::thread::spawn(move || {
			let mut buffer = Vec::new();
			if let Some(pipe) = stdout_pipe.as_mut() {
				use std::io::Read;
				let _ = pipe.read_to_end(&mut buffer);
			}
			String::from_utf8_lossy(&buffer).into_owned()
		});
		let deadline = Instant::now() + Duration::from_millis(options.timeout_ms);
		loop {
			match child.try_wait() {
				Ok(Some(status)) => {
					let stdout = reader.join().unwrap_or_default();
					return if status.success() {
						ChildResult::Ok { stdout }
					} else {
						ChildResult::Failed
					};
				}
				Ok(None) => {
					if Instant::now() >= deadline {
						let _ = child.kill();
						let _ = child.wait();
						let _ = reader.join();
						return ChildResult::Failed;
					}
					std::thread::sleep(Duration::from_millis(25));
				}
				Err(_) => {
					let _ = child.kill();
					let _ = child.wait();
					let _ = reader.join();
					return ChildResult::Failed;
				}
			}
		}
	})
}

/// TS lookupKrasViaNode — 기본 실행기로 조회 자식 하나를 돌린다.
pub fn lookup_kras_via_runner(
	address: &str,
	dark_mode: bool,
) -> Result<String, KrasChildError> {
	let mut execute = default_child_executor();
	run_kras_child(
		&mut execute,
		&KrasChildCommand::Lookup { address: address.to_string(), dark_mode },
	)
}

/// TS applyKrasDarkModeViaNode — 기본 실행기로 다크 모드 자식 하나를 돌린다.
pub fn apply_kras_dark_mode_via_runner() -> Result<String, KrasChildError> {
	let mut execute = default_child_executor();
	run_kras_child(&mut execute, &KrasChildCommand::DarkMode)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn fake_executor(
		response: ChildResult,
		seen: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>,
	) -> ChildExecutor {
		Box::new(move |executable, args, _options| {
			seen.lock()
				.unwrap()
				.push((executable.to_string(), args.to_vec()));
			response.clone()
		})
	}

	// TS tests/kras-bridge.test.ts

	#[test]
	fn run_kras_child_returns_trimmed_stdout_on_success() {
		let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
		let mut execute = fake_executor(
			ChildResult::Ok { stdout: "  부동산 종합증명서 조회결과 건물 등장\n".to_string() },
			seen.clone(),
		);
		let output = run_kras_child(
			&mut execute,
			&KrasChildCommand::Lookup { address: "주소".to_string(), dark_mode: false },
		)
		.expect("성공");
		assert_eq!(output, "부동산 종합증명서 조회결과 건물 등장");
		let calls = seen.lock().unwrap();
		assert_eq!(calls[0].1, vec!["lookup", "주소", "false"]);
	}

	#[test]
	fn run_kras_child_maps_failed_execution_to_error() {
		let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
		let mut execute = fake_executor(ChildResult::Failed, seen);
		let error = run_kras_child(&mut execute, &KrasChildCommand::DarkMode)
			.expect_err("실패는 오류여야 한다");
		assert_eq!(error.to_string(), "KRAS 실행에 실패했습니다.");
		assert_eq!(error.name(), "KrasChildError");
	}

	#[test]
	fn run_kras_child_rejects_empty_output() {
		let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
		let mut execute = fake_executor(ChildResult::Ok { stdout: "   \n".to_string() }, seen);
		let error = run_kras_child(&mut execute, &KrasChildCommand::DarkMode)
			.expect_err("빈 출력은 오류여야 한다");
		assert_eq!(error.to_string(), "KRAS 실행 결과가 비어 있습니다.");
	}

	#[test]
	fn child_args_match_runner_contract() {
		assert_eq!(
			child_kras_args(&KrasChildCommand::Lookup {
				address: "서울특별시 노원구 월계동 392-19".to_string(),
				dark_mode: true,
			}),
			vec!["lookup", "서울특별시 노원구 월계동 392-19", "true"]
		);
		assert_eq!(child_kras_args(&KrasChildCommand::DarkMode), vec!["dark_mode"]);
		// TS CHILD_OPTIONS 기본값 유지
		let options = ChildCallOptions::default();
		assert_eq!(options.timeout_ms, 30_000);
		assert!(options.windows_hide);
	}

	#[test]
	fn dark_mode_child_uses_dark_mode_argument() {
		let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
		let mut execute = fake_executor(
			ChildResult::Ok { stdout: "다크 모드 적용.".to_string() },
			seen.clone(),
		);
		let output =
			run_kras_child(&mut execute, &KrasChildCommand::DarkMode).expect("성공");
		assert_eq!(output, "다크 모드 적용.");
		assert_eq!(seen.lock().unwrap()[0].1, vec!["dark_mode"]);
	}
}
