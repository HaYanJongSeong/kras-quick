// TS scripts/property-watcher-runner.ts — 워처 CLI 명령(watch/dry_run/start/status/stop) 분기와 출력 계약.
// TS의 flushStdout/exitSuccess/exitFailure는 프로세스 종료 경로이므로 main이 맡는다.
// 여기는 명령 파싱·각 동작·실패 JSON 한 줄(`{"status":"error","name":...}`)만 만든다.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::property_watcher::{PropertyWatcherEvent, WatcherFailure};
use crate::property_watcher_lifecycle::{
	run_foreground_property_watcher_lifecycle, run_live_detached_property_watcher,
	ForegroundWatcherDependencies,
};
use crate::property_watcher_process::{
	is_uuid, PropertyWatcherProcessError, PropertyWatcherProcessErrorCode,
	PROPERTY_WATCHER_TOKEN_ENV,
};
use crate::property_watcher_state::{
	get_watcher_status, request_watcher_stop, WatcherLifecycleDependencies, WatcherStatus,
};

/// TS `commandSchema` — 정확히 이 다섯 개만 허용한다.
pub const PROPERTY_WATCHER_COMMANDS: [&str; 5] = ["watch", "dry_run", "start", "status", "stop"];

/// TS PropertyWatcherCommand
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherCommand {
	Watch,
	DryRun,
	Start,
	Status,
	Stop,
}

impl PropertyWatcherCommand {
	/// TS command kind 문자열.
	pub fn kind(self) -> &'static str {
		match self {
			PropertyWatcherCommand::Watch => "watch",
			PropertyWatcherCommand::DryRun => "dry_run",
			PropertyWatcherCommand::Start => "start",
			PropertyWatcherCommand::Status => "status",
			PropertyWatcherCommand::Stop => "stop",
		}
	}
}

/// TS PropertyWatcherUsageError — name이 JSON 실패 줄에 그대로 실린다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherUsageError(&'static str);

impl PropertyWatcherUsageError {
	pub const EXPECTED_ONE_COMMAND: &'static str = "Expected exactly one command";
	pub const UNKNOWN_COMMAND: &'static str = "Unknown command";

	pub fn new(message: &'static str) -> Self {
		Self(message)
	}

	/// TS error.name
	pub fn name(&self) -> &'static str {
		"PropertyWatcherUsageError"
	}
}

impl fmt::Display for PropertyWatcherUsageError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.0)
	}
}

impl std::error::Error for PropertyWatcherUsageError {}

/// TS parsePropertyWatcherCommand — 인자는 정확히 하나여야 하고 다섯 명령 중 하나여야 한다.
pub fn parse_property_watcher_command(
	args: &[String],
) -> Result<PropertyWatcherCommand, PropertyWatcherUsageError> {
	if args.len() != 1 {
		return Err(PropertyWatcherUsageError::new(
			PropertyWatcherUsageError::EXPECTED_ONE_COMMAND,
		));
	}
	match args[0].as_str() {
		"watch" => Ok(PropertyWatcherCommand::Watch),
		"dry_run" => Ok(PropertyWatcherCommand::DryRun),
		"start" => Ok(PropertyWatcherCommand::Start),
		"status" => Ok(PropertyWatcherCommand::Status),
		"stop" => Ok(PropertyWatcherCommand::Stop),
		_ => Err(PropertyWatcherUsageError::new(
			PropertyWatcherUsageError::UNKNOWN_COMMAND,
		)),
	}
}

/// TS 실행 동작들(watch/dryRun/start/status/stop).
pub trait PropertyWatcherRunnerActions {
	fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>>;
	fn dry_run(&mut self) -> Result<(), Box<dyn std::error::Error>>;
	fn start(&mut self) -> Result<(), Box<dyn std::error::Error>>;
	fn status(&mut self) -> Result<(), Box<dyn std::error::Error>>;
	fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// TS executePropertyWatcherCommand — 명령을 정확히 한 동작에 라우팅한다.
pub fn execute_property_watcher_command(
	command: PropertyWatcherCommand,
	actions: &mut dyn PropertyWatcherRunnerActions,
) -> Result<(), Box<dyn std::error::Error>> {
	match command {
		PropertyWatcherCommand::Watch => actions.watch(),
		PropertyWatcherCommand::DryRun => actions.dry_run(),
		PropertyWatcherCommand::Start => actions.start(),
		PropertyWatcherCommand::Status => actions.status(),
		PropertyWatcherCommand::Stop => actions.stop(),
	}
}

/// TS error.name — JSON 실패 줄에 넣을 오류 종류.
/// (TS는 `error instanceof Error ? error.name : "UnknownError"` 이다.)
pub fn error_name(error: &(dyn std::error::Error + 'static)) -> &'static str {
	if let Some(usage) = error.downcast_ref::<PropertyWatcherUsageError>() {
		return usage.name();
	}
	if let Some(process) = error.downcast_ref::<PropertyWatcherProcessError>() {
		return "PropertyWatcherProcessError";
	}
	if let Some(control) = error.downcast_ref::<crate::property_watcher_control::PropertyWatcherControlError>()
	{
		return "PropertyWatcherControlError";
	}
	if let Some(page) = error.downcast_ref::<crate::property_watcher_page::PropertyWatcherError>() {
		return "PropertyWatcherError";
	}
	// TS kras-runner/kras-lookup/kras-bridge 오류 종류
	if let Some(kras_page) = error.downcast_ref::<crate::kras_lookup::KrasPageError>() {
		return kras_page.name();
	}
	if let Some(runner) = error.downcast_ref::<crate::kras_runner::KrasRunnerUsageError>() {
		return runner.name();
	}
	if let Some(child) = error.downcast_ref::<crate::kras_bridge::KrasChildError>() {
		return child.name();
	}
	"UnknownError"
}

/// TS runPropertyWatcherCli의 실패 한 줄 — stderr로 내보낼 JSON 문자열.
pub fn failure_line(error: &(dyn std::error::Error + 'static)) -> String {
	// TS는 삽입 순서를 지킨다(serde_json 맵은 알파벳 정렬이므로 포맷 문자열로 만든다).
	format!(
		r#"{{"status":"error","name":"{}"}}"#,
		error_name(error)
	)
}

/// TS failure completion 전체 — 실패면 Some(JSON 한 줄), 성공이면 None(종료 코드 0).
pub fn run_property_watcher_cli<F>(run_command: F) -> Option<String>
where
	F: FnOnce() -> Result<(), Box<dyn std::error::Error>>,
{
	match run_command() {
		Ok(()) => None,
		Err(error) => Some(failure_line(error.as_ref())),
	}
}

/// TS stdout logger — `{"event": ..., "count": ...}` 한 줄씩 쓴다.
pub type CliLogger = Box<
	dyn FnMut(PropertyWatcherEvent, u32) -> Result<(), WatcherFailure> + Send,
>;

pub fn stdout_logger() -> CliLogger {
	Box::new(|event, count| {
		// TS writeJson({ event, count }) — 순서 유지.
		println!(r#"{{"event":"{}","count":{}}}"#, event.as_str(), count);
		Ok(())
	})
}

/// TS waitForForegroundStop(signal) — Ctrl+C/종료 요청이 오면 멈춘다.
/// 콘솔 핸들러는 플래그만 세우고 대기 루프가 확인한다(TS의 promise resolve와 동일).
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_console_control(_ctrl_type: u32) -> i32 {
	STOP_REQUESTED.store(true, Ordering::SeqCst);
	// TRUE = 이 신호를 우리가 처리했다
	1
}

pub fn wait_for_foreground_stop() -> Result<(), String> {
	unsafe {
		windows_sys::Win32::System::Console::SetConsoleCtrlHandler(
			Some(on_console_control),
			1,
		);
	}
	while !STOP_REQUESTED.load(Ordering::SeqCst) {
		std::thread::sleep(std::time::Duration::from_millis(100));
	}
	Ok(())
}

/// 라이브 워처를 전경 모드로 돌린다(TS runForegroundPropertyWatcherLifecycle 기본 의존).
fn run_foreground_watch() -> Result<(), Box<dyn std::error::Error>> {
	let mut logger_slot = Some(stdout_logger());
	let mut dependencies = ForegroundWatcherDependencies {
		start_watcher: Box::new(move |_instance_token| {
			let logger = logger_slot.take().ok_or_else(|| {
				Box::<dyn std::error::Error>::from("워처 로거가 이미 사용되었습니다.")
			})?;
			crate::property_watcher_runtime::start_live_property_watcher(logger)
		}),
		wait_for_stop: Box::new(wait_for_foreground_stop),
	};
	// TS randomUUID() — 실제 파일 계층이 쓰는 고유 토큰 생성기를 그대로 쓴다.
	let token = crate::property_watcher_state::WatcherReadyStateOperations::random_uuid(
		&crate::property_watcher_state::FsWatcherReadyStateOperations,
	);
	run_foreground_property_watcher_lifecycle(&token, &mut dependencies)
}

/// 실제 파일·파이프·CDP에 붙는 CLI 동작들(TS actions 객체).
pub struct CliActions;

impl PropertyWatcherRunnerActions for CliActions {
	fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		match std::env::var(PROPERTY_WATCHER_TOKEN_ENV) {
			// 토큰 없음 → 전경 라이프사이클
			Err(_) => run_foreground_watch(),
			Ok(token) => {
				// TS instanceTokenSchema.safeParse 실패 → PID_RECORD_INVALID
				if !is_uuid(&token) {
					return Err(Box::new(PropertyWatcherProcessError::new(
						PropertyWatcherProcessErrorCode::PidRecordInvalid,
					)));
				}
				run_live_detached_property_watcher(&token, stdout_logger())
			}
		}
	}

	fn dry_run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		let pages = crate::property_watcher_runtime::connect_property_watcher_browser(
			crate::property_watcher_runtime::CDP_ENDPOINT,
		)?;
		let line = crate::property_watcher_runtime::run_property_watcher_dry_run(&pages)
			.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
		println!("{line}");
		Ok(())
	}

	fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		let runner_path: PathBuf = std::env::current_exe()?;
		let mut dependencies =
			crate::property_watcher_start::DetachedStartDependencies::default();
		let record = crate::property_watcher_start::start_detached_property_watcher(
			&runner_path,
			&mut dependencies,
		)?;
		// TS writeJson({ status: record.state, pid: record.pid }) — 순서 유지.
		println!(
			r#"{{"status":"{}","pid":{}}}"#,
			record.state.as_str(),
			record.pid
		);
		Ok(())
	}

	fn status(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		let mut dependencies = WatcherLifecycleDependencies::default();
		let status = get_watcher_status(&mut dependencies)?;
		println!("{}", status_json(&status));
		Ok(())
	}

	fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		let mut dependencies = WatcherLifecycleDependencies::default();
		request_watcher_stop(&mut dependencies)?;
		println!(r#"{{"status":"stopped"}}"#);
		Ok(())
	}
}

/// TS WatcherStatus → JSON (stopped는 pid를 싣지 않는다).
pub fn status_json(status: &WatcherStatus) -> String {
	match status {
		WatcherStatus::Stopped => r#"{"status":"stopped"}"#.to_string(),
		WatcherStatus::Stale { pid } => {
			format!(r#"{{"status":"stale","pid":{pid}}}"#)
		}
		WatcherStatus::Starting { pid } => {
			format!(r#"{{"status":"starting","pid":{pid}}}"#)
		}
		WatcherStatus::Ready { pid } => {
			format!(r#"{{"status":"ready","pid":{pid}}}"#)
		}
	}
}

/// runner로 분기해야 하는 단일 인자인지(TS와 달리 kras-quick는 주소 인자도같은 자리에 온다).
/// 명령 후보는 공백 없는 ASCII 식별자일 때만 본다 — 주소(한글·공백)와 겹치지 않는다.
pub fn is_runner_command_argument(arg: &str) -> bool {
	!arg.contains(char::is_whitespace) && arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
	use super::*;

	fn command(value: &str) -> PropertyWatcherCommand {
		parse_property_watcher_command(&[value.to_string()]).expect("허용된 명령")
	}

	#[test]
	fn parses_every_exact_command_and_rejects_missing_unknown_or_extra_arguments() {
		assert_eq!(
			PROPERTY_WATCHER_COMMANDS
				.map(command),
			[
				PropertyWatcherCommand::Watch,
				PropertyWatcherCommand::DryRun,
				PropertyWatcherCommand::Start,
				PropertyWatcherCommand::Status,
				PropertyWatcherCommand::Stop,
			]
		);
		for args in [
			vec![],
			vec!["unknown".to_string()],
			vec!["watch".to_string(), "extra".to_string()],
			vec!["stop".to_string(), "extra".to_string()],
		] {
			let error = parse_property_watcher_command(&args).expect_err("거부되어야 한다");
			assert_eq!(error.name(), "PropertyWatcherUsageError");
		}
	}

	#[test]
	fn routes_each_command_exactly_once() {
		struct Recorder(Vec<&'static str>);
		impl PropertyWatcherRunnerActions for Recorder {
			fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				self.0.push("watch");
				Ok(())
			}
			fn dry_run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				self.0.push("dry_run");
				Ok(())
			}
			fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				self.0.push("start");
				Ok(())
			}
			fn status(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				self.0.push("status");
				Ok(())
			}
			fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				self.0.push("stop");
				Ok(())
			}
		}

		let mut recorder = Recorder(Vec::new());
		for name in PROPERTY_WATCHER_COMMANDS {
			let parsed = parse_property_watcher_command(&[name.to_string()]).expect("명령");
			execute_property_watcher_command(parsed, &mut recorder).expect("실행");
		}
		assert_eq!(
			recorder.0,
			["watch", "dry_run", "start", "status", "stop"]
		);
	}

	#[test]
	fn rejected_command_writes_concise_error_line_and_returns_failure() {
		#[derive(Debug)]
		struct SyntheticRunnerError;
		impl fmt::Display for SyntheticRunnerError {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				f.write_str("서울특별시 중구 태평로1가 31")
			}
		}
		impl std::error::Error for SyntheticRunnerError {}

		struct Reject;
		impl PropertyWatcherRunnerActions for Reject {
			fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				Err(Box::new(SyntheticRunnerError))
			}
			fn dry_run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				Err(Box::new(SyntheticRunnerError))
			}
			fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				Err(Box::new(SyntheticRunnerError))
			}
			fn status(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				Err(Box::new(SyntheticRunnerError))
			}
			fn stop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
				Err(Box::new(SyntheticRunnerError))
			}
		}

		for name in PROPERTY_WATCHER_COMMANDS {
			let parsed = parse_property_watcher_command(&[name.to_string()]).expect("명령");
			let mut actions = Reject;
			let line = run_property_watcher_cli(|| {
				execute_property_watcher_command(parsed, &mut actions)
			})
			.expect("실패 줄이 나와야 한다");
			// TS: {"status":"error","name":"<Error.name>"} — 알 수 없는 오류 종류는 UnknownError
			assert_eq!(line, r#"{"status":"error","name":"UnknownError"}"#);
		}
	}

	#[test]
	fn usage_failure_line_uses_usage_error_name() {
		let error = PropertyWatcherUsageError::new(PropertyWatcherUsageError::UNKNOWN_COMMAND);
		let line = failure_line(&error);
		assert_eq!(line, r#"{"status":"error","name":"PropertyWatcherUsageError"}"#);
	}

	#[test]
	fn runner_argument_is_limited_to_bare_ascii_identifiers() {
		assert!(is_runner_command_argument("watch"));
		assert!(is_runner_command_argument("dry_run"));
		assert!(!is_runner_command_argument("서울특별시 노원구 월계동 392-19"));
		assert!(!is_runner_command_argument("월계동"));
		assert!(!is_runner_command_argument("watch extra"));
	}
}
