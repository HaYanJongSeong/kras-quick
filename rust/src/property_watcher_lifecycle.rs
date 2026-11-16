// TS scripts/property-watcher-lifecycle.ts — SCP 워처의 전경/분리 라이프사이클.
// TS의 Promise.race는 여기서 "제어 서버 종료 스레드 + 워처 실패 폴링"으로 재현한다.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::property_watcher_control::WatcherStopServer;
use crate::property_watcher_process::{WatcherPidRecord, PROPERTY_WATCHER_PID_FILE};

/// TS LifecycleFailure — 최초 실패 한 건만 유지한다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LifecycleFailure {
	#[default]
	None,
	Failed(String),
}

/// TS retainFirstFailure — 두 번째 오류는 덮어쓰지 않는다.
pub fn retain_first_failure(current: LifecycleFailure, error: impl ToString) -> LifecycleFailure {
	match current {
		LifecycleFailure::None => LifecycleFailure::Failed(error.to_string()),
		LifecycleFailure::Failed(existing) => LifecycleFailure::Failed(existing),
	}
}

impl LifecycleFailure {
	pub fn into_result(self) -> Result<(), String> {
		match self {
			LifecycleFailure::None => Ok(()),
			LifecycleFailure::Failed(message) => Err(message),
		}
	}
}

/// TS ObservedOutcome — observeOutcome/observeFailure의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedOutcome {
	Succeeded,
	Failed(String),
}

/// TS PropertyWatcher의 stopped/stop에 대응하는 최소 계약.
pub trait LifecycleWatcher: Send {
	/// TS watcher.stopped — 이미 끝났다면 Some(정상 종료는 Some(Ok)). 한 번 꺼내면 비어 있다.
	fn take_stop_outcome(&mut self) -> Option<Result<(), String>>;
	/// TS watcher.stop()
	fn request_stop(&mut self) -> Result<(), String>;
	/// 아직 살아 있는지(TS watcherState === "active")
	fn is_active(&self) -> bool;
}

/// TS WatcherStopServer의 비동기 진행을 차단 호출로 노출하는 계약.
pub trait StopServer: Send + Sync {
	/// TS stopped promise — 성공이면 Ok, 실패면 오류 메시지.
	fn stopped(&self) -> Result<(), String>;
	/// TS close()
	fn close(&self) -> Result<(), String>;
}

impl StopServer for WatcherStopServer {
	fn stopped(&self) -> Result<(), String> {
		WatcherStopServer::stopped(self).map_err(|e| e.to_string())
	}

	fn close(&self) -> Result<(), String> {
		WatcherStopServer::close(self).map_err(|e| e.to_string())
	}
}

/// TS WatcherLifecycleRuntimeDependencies
pub struct WatcherLifecycleRuntimeDependencies {
	pub wait_for_record: Box<
		dyn FnMut(&str) -> Result<WatcherPidRecord, Box<dyn std::error::Error>>,
	>,
	pub start_watcher:
		Box<dyn FnMut(&str) -> Result<Box<dyn LifecycleWatcher>, Box<dyn std::error::Error>>>,
	pub open_control: Box<
		dyn FnMut(
			&str,
			Box<dyn FnOnce() -> Result<(), String> + Send>,
		) -> Result<Box<dyn StopServer>, Box<dyn std::error::Error>>,
	>,
	pub mark_ready: Box<
		dyn FnMut(&WatcherPidRecord) -> Result<WatcherPidRecord, Box<dyn std::error::Error>>,
	>,
	pub remove_pid: Box<
		dyn FnMut(&Path, &WatcherPidRecord) -> Result<(), Box<dyn std::error::Error>>,
	>,
}

/// TS ForegroundWatcherDependencies
pub struct ForegroundWatcherDependencies {
	pub start_watcher:
		Box<dyn FnMut(&str) -> Result<Box<dyn LifecycleWatcher>, Box<dyn std::error::Error>>>,
	/// TS waitForStop(signal) — 정지 신호를 기다린다.
	pub wait_for_stop: Box<dyn FnMut() -> Result<(), String> + Send>,
}

/// 제어 서버의 stopped를 백그라운드에서 기다리는 이벤트.
enum RaceEvent {
	Control(Result<(), String>),
}

/// 제어 서버 종료를 스레드에서 기다리는 동안 워처의 치명 실패를 폴링한다(TS Promise.race).
fn race_control_and_watcher(
	control: Arc<dyn StopServer>,
	watcher: Arc<Mutex<Option<Box<dyn LifecycleWatcher>>>>,
) -> ObservedOutcome {
	let (sender, receiver) = mpsc::channel::<RaceEvent>();
	let for_thread = control.clone();
	std::thread::spawn(move || {
		let result = for_thread.stopped();
		let _ = sender.send(RaceEvent::Control(result));
	});
	loop {
		// 1) 워처 쪽 치명 실패가 먼저면 그 오류를 돌려준다(TS observeFailure)
		if let Ok(mut guard) = watcher.lock() {
			if let Some(slot) = guard.as_mut() {
				if let Some(Err(message)) = slot.take_stop_outcome() {
					return ObservedOutcome::Failed(message);
				}
			}
		}
		// 2) 제어 서버가 끝나면 그 결과를 그대로 쓴다
		match receiver.recv_timeout(Duration::from_millis(20)) {
			Ok(RaceEvent::Control(Ok(()))) => return ObservedOutcome::Succeeded,
			Ok(RaceEvent::Control(Err(message))) => return ObservedOutcome::Failed(message),
			Err(mpsc::RecvTimeoutError::Timeout) => continue,
			Err(mpsc::RecvTimeoutError::Disconnected) => {
				return ObservedOutcome::Failed("제어 서버 스레드가 종료되었습니다.".to_string());
			}
		}
	}
}

/// 정리 경로(TS finally) — 제어 닫기 → 워처 정지 → PID 기록 제거.
/// 정리 단계가 다루는 상태(TS try/finally의 지역 변수).
struct DetachedState {
	starting: Option<WatcherPidRecord>,
	ready: Option<WatcherPidRecord>,
	control: Option<Arc<dyn StopServer>>,
	watcher_slot: Arc<Mutex<Option<Box<dyn LifecycleWatcher>>>>,
	active_flag: Arc<AtomicBool>,
	watcher_state_active: bool,
	failure: LifecycleFailure,
}

impl DetachedState {
	fn new() -> Self {
		Self {
			starting: None,
			ready: None,
			control: None,
			watcher_slot: Arc::new(Mutex::new(None)),
			active_flag: Arc::new(AtomicBool::new(false)),
			watcher_state_active: false,
			failure: LifecycleFailure::None,
		}
	}

	/// TS try 블록 — 각 단계의 실패는 최초 오류로 남긴다.
	fn attempt(
		&mut self,
		instance_token: &str,
		dependencies: &mut WatcherLifecycleRuntimeDependencies,
	) -> Result<(), Box<dyn std::error::Error>> {
		let started = match (dependencies.wait_for_record)(instance_token) {
			Ok(record) => record,
			Err(error) => {
				self.failure = retain_first_failure(self.failure.clone(), error.to_string());
				return Err(error);
			}
		};
		self.starting = Some(started);
		let started = self.starting.clone().expect("starting 확보");

		let instance = match (dependencies.start_watcher)(instance_token) {
			Ok(instance) => instance,
			Err(error) => {
				self.failure = retain_first_failure(self.failure.clone(), error.to_string());
				return Err(error);
			}
		};
		{
			let mut guard = self.watcher_slot.lock().unwrap_or_else(|e| e.into_inner());
			*guard = Some(instance);
		}
		self.watcher_state_active = true;
		self.active_flag.store(true, Ordering::SeqCst);

		// TS openControl의 onStop — 워처 정지와 상태 전환
		let slot_for_stop = self.watcher_slot.clone();
		let active_for_stop = self.active_flag.clone();
		let on_stop: Box<dyn FnOnce() -> Result<(), String> + Send> = Box::new(move || {
			let result = match slot_for_stop.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
				Some(instance) => instance.request_stop(),
				None => Ok(()),
			};
			active_for_stop.store(false, Ordering::SeqCst);
			result
		});
		let opened = match (dependencies.open_control)(&started.pipe_name, on_stop) {
			Ok(server) => server,
			Err(error) => {
				self.failure = retain_first_failure(self.failure.clone(), error.to_string());
				return Err(error);
			}
		};
		self.control = Some(Arc::from(opened));

		self.ready = match (dependencies.mark_ready)(&started) {
			Ok(record) => Some(record),
			Err(error) => {
				self.failure = retain_first_failure(self.failure.clone(), error.to_string());
				return Err(error);
			}
		};

		// TS Promise.race([controlOutcome, watcherFailure])
		let outcome = match self.control.as_ref() {
			Some(server) => race_control_and_watcher(server.clone(), self.watcher_slot.clone()),
			None => ObservedOutcome::Failed("제어 서버 없음".to_string()),
		};
		match outcome {
			ObservedOutcome::Succeeded => Ok(()),
			ObservedOutcome::Failed(message) => Err(Box::from(message)),
		}
	}

	/// TS finally — 제어 닫기 → 워처 정지 → PID 기록 제거.
	fn cleanup(&mut self, dependencies: &mut WatcherLifecycleRuntimeDependencies) {
		if let Some(server) = self.control.take() {
			if let Err(message) = server.close() {
				self.failure = retain_first_failure(self.failure.clone(), message);
			}
		}
		if self.watcher_state_active {
			let mut guard = self.watcher_slot.lock().unwrap_or_else(|e| e.into_inner());
			if let Some(instance) = guard.as_mut() {
				if let Err(message) = instance.request_stop() {
					self.failure = retain_first_failure(self.failure.clone(), message);
				}
			}
			*guard = None;
		}
		if let Some(record) = self.ready.as_ref().or(self.starting.as_ref()) {
			if let Err(error) =
				(dependencies.remove_pid)(Path::new(PROPERTY_WATCHER_PID_FILE), record)
			{
				self.failure = retain_first_failure(self.failure.clone(), error.to_string());
			}
		}
	}
}

/// TS runDetachedPropertyWatcherLifecycle — 기록 대기 → 시작 → 제어 → ready → 경합 → 정리.
pub fn run_detached_property_watcher_lifecycle(
	instance_token: &str,
	dependencies: &mut WatcherLifecycleRuntimeDependencies,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut state = DetachedState::new();
	let attempt = state.attempt(instance_token, dependencies);
	state.cleanup(dependencies);
	// TS: if (failure.kind === "failed") throw failure.error;
	match state.failure {
		LifecycleFailure::Failed(message) => Err(Box::from(message)),
		LifecycleFailure::None => attempt,
	}
}

pub fn run_foreground_property_watcher_lifecycle(
	instance_token: &str,
	dependencies: &mut ForegroundWatcherDependencies,
) -> Result<(), Box<dyn std::error::Error>> {
	let mut failure = LifecycleFailure::None;
	let mut watcher: Option<Box<dyn LifecycleWatcher>> = None;

	let attempt: Result<(), Box<dyn std::error::Error>> = (|| {
		let instance = (dependencies.start_watcher)(instance_token)?;
		watcher = Some(instance);

		// 정지 신호 대기를 스레드로 돌리고 워처 종료와 경합시킨다(TS Promise.race)
		// 스레드로 넘기려면 소유권이 필요하므로 박스를 꺼낸다.
		let wait_box = std::mem::replace(
			&mut dependencies.wait_for_stop,
			Box::new(|| Ok(())),
		);
		let wait_handle = Arc::new(Mutex::new(wait_box));
		let (sender, receiver) = mpsc::channel::<Result<(), String>>();
		let waiter = wait_handle.clone();
		std::thread::spawn(move || {
			let result = {
				let mut guard = waiter.lock().unwrap_or_else(|e| e.into_inner());
				guard.as_mut()()
			};
			let _ = sender.send(result);
		});

		loop {
			let watcher_outcome = watcher
				.as_mut()
				.and_then(|instance| instance.take_stop_outcome());
			match watcher_outcome {
				// 정상 종료/실패 모두 경합이 끝난다(TS watcher.stopped)
				Some(Ok(())) => return Ok(()),
				Some(Err(message)) => return Err(Box::from(message)),
				None => {}
			}
			match receiver.recv_timeout(Duration::from_millis(20)) {
				Ok(Ok(())) => return Ok(()),
				Ok(Err(message)) => return Err(Box::from(message)),
				Err(mpsc::RecvTimeoutError::Timeout) => continue,
				Err(mpsc::RecvTimeoutError::Disconnected) => {
					return Err(Box::from("정지 대기 스레드가 종료되었습니다."));
				}
			}
		}
	})();

	// TS finally: stopWaiting.abort() → watcher.stop()
	if let Some(instance) = watcher.as_mut() {
		if instance.is_active() {
			if let Err(message) = instance.request_stop() {
				failure = retain_first_failure(failure, message);
			}
		}
	}

	// TS: 종료 오류가 있으면 먼저 그쪽을 보고한다
	match attempt {
		Err(error) => {
			failure = retain_first_failure(failure, error.to_string());
		}
		Ok(()) => {}
	}
	failure.into_result().map_err(|message| Box::from(message) as Box<dyn std::error::Error>)
}

/// TS runLiveDetachedPropertyWatcher — 실제 PID 파일·제어 파이프·라이브 러스타임으로 분리 실행을 완성한다.
pub fn run_live_detached_property_watcher(
	instance_token: &str,
	mut logger: Box<
		dyn FnMut(
			crate::property_watcher::PropertyWatcherEvent,
			u32,
		) -> Result<(), crate::property_watcher::WatcherFailure>
			+ Send,
	>,
) -> Result<(), Box<dyn std::error::Error>> {
	let pid = std::process::id() as u64;
	let mut logger_slot = Some(logger);
	let mut dependencies = WatcherLifecycleRuntimeDependencies {
		// TS waitForStartingWatcherRecord(token) — 기본 pid=process.pid, 타임아웃 5초.
		wait_for_record: Box::new(move |token| {
			crate::property_watcher_state::wait_for_starting_watcher_record(token, pid, 5000)
		}),
		// TS startLivePropertyWatcher(token, logger) — 로거는 한 번만 꺼내 쓴다.
		start_watcher: Box::new(move |_token| {
			let logger = logger_slot.take().ok_or_else(|| {
				Box::<dyn std::error::Error>::from("워처 로거가 이미 사용되었습니다.")
			})?;
			crate::property_watcher_runtime::start_live_property_watcher(logger)
		}),
		// TS openWatcherStopServer(pipeName, onStop)
		open_control: Box::new(|pipe_name, on_stop| {
			let server = crate::property_watcher_control::open_watcher_stop_server(
				pipe_name,
				on_stop,
			)?;
			Ok(Box::new(server) as Box<dyn StopServer>)
		}),
		// TS markWatcherPidReady(record, PID_FILE, watcherReadyStateOperations)
		mark_ready: Box::new(|record| {
			crate::property_watcher_state::mark_watcher_pid_ready(
				record,
				Path::new(PROPERTY_WATCHER_PID_FILE),
				&crate::property_watcher_state::FsWatcherReadyStateOperations,
			)
		}),
		// TS removeMatchingWatcherPidRecord
		remove_pid: Box::new(|path, record| {
			crate::property_watcher_state::remove_matching_watcher_pid_record(path, record)
		}),
	};
	run_detached_property_watcher_lifecycle(instance_token, &mut dependencies)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::AtomicU32;

	/// 제어 서버 페이크 — 미리 정해둔 결과를 돌려주고 닫힘 횟수를 센다.
	#[derive(Clone)]
	struct FakeStopServer {
		outcome: Result<(), String>,
		closed: Arc<AtomicU32>,
		fail_close: bool,
	}

	impl StopServer for FakeStopServer {
		fn stopped(&self) -> Result<(), String> {
			self.outcome.clone()
		}
		fn close(&self) -> Result<(), String> {
			self.closed.fetch_add(1, Ordering::SeqCst);
			if self.fail_close {
				return Err("제어 닫기 실패".to_string());
			}
			Ok(())
		}
	}

	impl std::fmt::Debug for FakeStopServer {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.debug_struct("FakeStopServer").finish_non_exhaustive()
		}
	}

	/// 워처 페이크 — 미리 정해둔 stop 결과와 정지 시도 횟수를 기록한다.
	#[derive(Clone)]
	struct FakeWatcher {
		stop_outcome: Option<Result<(), String>>,
		fail_stop: bool,
		stopped: bool,
		stop_calls: Arc<AtomicU32>,
	}

	impl LifecycleWatcher for FakeWatcher {
		fn take_stop_outcome(&mut self) -> Option<Result<(), String>> {
			self.stop_outcome.take()
		}
		fn request_stop(&mut self) -> Result<(), String> {
			self.stop_calls.fetch_add(1, Ordering::SeqCst);
			self.stopped = true;
			if self.fail_stop {
				return Err("워처 정지 실패".to_string());
			}
			Ok(())
		}
		fn is_active(&self) -> bool {
			!self.stopped
		}
	}

	impl std::fmt::Debug for FakeWatcher {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.debug_struct("FakeWatcher").finish_non_exhaustive()
		}
	}

	fn record() -> WatcherPidRecord {
		WatcherPidRecord {
			version: 1,
			state: crate::property_watcher_process::WatcherPidState::Starting,
			pid: 4242,
			executable_path: "C:\\node.exe".into(),
			runner_path: "C:\\app\\runner.ts".into(),
			instance_token: "123e4567-e89b-42d3-a456-426614174000".to_string(),
			creation_identity: "id-1".into(),
			pipe_name: crate::property_watcher_process::watcher_pipe_name(
				"123e4567-e89b-42d3-a456-426614174000",
			),
		}
	}

	#[derive(Default)]
	struct Options {
		wait_fails: bool,
		start_fails: bool,
		open_control_fails: bool,
		mark_ready_fails: bool,
		control_outcome: Option<Result<(), String>>,
		control_fail_close: bool,
		watcher_stop_outcome: Option<Result<(), String>>,
		watcher_fail_stop: bool,
	}

	#[derive(Clone)]
	struct Ctx {
		removed: Arc<Mutex<Vec<String>>>,
		closed: Arc<AtomicU32>,
		stops: Arc<AtomicU32>,
	}

	impl Default for Ctx {
		fn default() -> Self {
			Self {
				removed: Arc::new(Mutex::new(Vec::new())),
				closed: Arc::new(AtomicU32::new(0)),
				stops: Arc::new(AtomicU32::new(0)),
			}
		}
	}

	fn deps(options: Options, ctx: &Ctx) -> WatcherLifecycleRuntimeDependencies {
		let ctx = ctx.clone();
		let ctx_control = ctx.clone();
		let ctx_remove = ctx.clone();
		WatcherLifecycleRuntimeDependencies {
			wait_for_record: Box::new(move |token| {
				if options.wait_fails {
					return Err(Box::from("기록 대기 실패") as Box<dyn std::error::Error>);
				}
				let _ = token;
				Ok(record())
			}),
			start_watcher: Box::new(move |_token| {
				if options.start_fails {
					return Err(Box::from("워처 시작 실패") as Box<dyn std::error::Error>);
				}
				Ok(Box::new(FakeWatcher {
					stop_outcome: options.watcher_stop_outcome.clone(),
					fail_stop: options.watcher_fail_stop,
					stopped: false,
					stop_calls: ctx.stops.clone(),
				}) as Box<dyn LifecycleWatcher>)
			}),
			open_control: Box::new(move |_pipe, _on_stop| {
				if options.open_control_fails {
					return Err(Box::from("제어 열기 실패") as Box<dyn std::error::Error>);
				}
				Ok(Box::new(FakeStopServer {
					outcome: options
						.control_outcome
						.clone()
						.unwrap_or(Ok(())),
					closed: ctx_control.closed.clone(),
					fail_close: options.control_fail_close,
				}) as Box<dyn StopServer>)
			}),
			mark_ready: Box::new(move |starting| {
				if options.mark_ready_fails {
					return Err(Box::from("ready 표시 실패") as Box<dyn std::error::Error>);
				}
				let mut ready = starting.clone();
				ready.state = crate::property_watcher_process::WatcherPidState::Ready;
				Ok(ready)
			}),
			remove_pid: Box::new(move |_path, rec| {
				ctx_remove
					.removed
					.lock()
					.unwrap_or_else(|e| e.into_inner())
					.push(rec.instance_token.clone());
				Ok(())
			}),
		}
	}

	fn run(options: Options) -> (Result<(), String>, Ctx) {
		let ctx = Ctx::default();
		let mut dependencies = deps(options, &ctx);
		let result = run_detached_property_watcher_lifecycle(
			"123e4567-e89b-42d3-a456-426614174000",
			&mut dependencies,
		)
		.map_err(|e| e.to_string());
		(result, ctx)
	}

	#[test]
	fn retain_first_failure_keeps_first_error() {
		let mut failure = LifecycleFailure::None;
		failure = retain_first_failure(failure, "첫 오류");
		failure = retain_first_failure(failure, "둘째 오류");
		assert_eq!(failure, LifecycleFailure::Failed("첫 오류".to_string()));
		assert_eq!(failure.clone().into_result(), Err("첫 오류".to_string()));
		assert!(LifecycleFailure::None.into_result().is_ok());
	}

	#[test]
	fn detached_happy_path_closes_control_and_removes_record() {
		let (result, ctx) = run(Options::default());
		assert!(result.is_ok(), "성공 경로: {result:?}");
		assert_eq!(ctx.closed.load(Ordering::SeqCst), 1, "제어 서버를 닫는다");
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1, "PID 기록을 제거한다");
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 1, "워처 정지 요청");
	}

	#[test]
	fn detached_wait_failure_skips_record_removal() {
		let (result, ctx) = run(Options { wait_fails: true, ..Default::default() });
		assert_eq!(result, Err("기록 대기 실패".to_string()));
		assert!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).is_empty(), "기록이 없으면 제거하지 않는다");
		assert_eq!(ctx.closed.load(Ordering::SeqCst), 0, "제어가 아직 없다");
	}

	#[test]
	fn detached_start_failure_still_removes_record_and_stops_nothing() {
		let (result, ctx) = run(Options { start_fails: true, ..Default::default() });
		assert_eq!(result, Err("워처 시작 실패".to_string()));
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1, "starting 기록은 제거한다");
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 0, "워처가 없으니 정지 요청 없음");
	}

	#[test]
	fn detached_open_control_failure_stops_watcher_and_removes_record() {
		let (result, ctx) = run(Options { open_control_fails: true, ..Default::default() });
		assert_eq!(result, Err("제어 열기 실패".to_string()));
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 1, "살아 있는 워처는 정지한다");
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
	}

	#[test]
	fn detached_mark_ready_failure_closes_control_and_removes_starting_record() {
		let (result, ctx) = run(Options { mark_ready_fails: true, ..Default::default() });
		assert_eq!(result, Err("ready 표시 실패".to_string()));
		assert_eq!(ctx.closed.load(Ordering::SeqCst), 1, "열린 제어 서버를 닫는다");
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 1);
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1, "starting 기록을 제거한다");
	}

	#[test]
	fn detached_control_failure_is_reported() {
		let (result, ctx) = run(Options {
			control_outcome: Some(Err("제어 서버 무응답".to_string())),
			..Default::default()
		});
		assert_eq!(result, Err("제어 서버 무응답".to_string()));
		assert_eq!(ctx.closed.load(Ordering::SeqCst), 1);
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
	}

	#[test]
	fn detached_close_failure_becomes_the_reported_error() {
		let (result, _ctx) = run(Options { control_fail_close: true, ..Default::default() });
		assert_eq!(result, Err("제어 닫기 실패".to_string()));
	}

	#[test]
	fn detached_watcher_failure_during_race_is_reported() {
		// 제어는 무한히 대기하는 대신 즉시 실패로 두고, 워처 실패가 먼저 잡히는지 확인한다
		let (result, ctx) = run(Options {
			watcher_stop_outcome: Some(Err("워처 내부 실패".to_string())),
			control_outcome: Some(Ok(())),
			..Default::default()
		});
		assert_eq!(result, Err("워처 내부 실패".to_string()));
		assert_eq!(ctx.closed.load(Ordering::SeqCst), 1, "제어 서버는 닫는다");
		assert_eq!(ctx.removed.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);
	}

	#[test]
	fn detached_cleanup_keeps_first_failure_in_finally_order() {
		// TS finally 순서는 control.close → watcher.stop 이므로 둘 다 실패하면
		// 제어 쪽 오류가 최초 실패로 남는다.
		let (result, _ctx) = run(Options {
			watcher_fail_stop: true,
			control_fail_close: true,
			..Default::default()
		});
		assert_eq!(result, Err("제어 닫기 실패".to_string()));

		// 워처 정지만 실패하면 그 오류가 남는다
		let (result, _ctx) = run(Options {
			watcher_fail_stop: true,
			..Default::default()
		});
		assert_eq!(result, Err("워처 정지 실패".to_string()));
	}

	#[test]
	fn foreground_normal_stop_is_success() {
		let ctx = Ctx::default();
		let stops = ctx.stops.clone();
		let mut dependencies = ForegroundWatcherDependencies {
			start_watcher: Box::new(move |_token| {
				Ok(Box::new(FakeWatcher {
					stop_outcome: None,
					fail_stop: false,
					stopped: false,
					stop_calls: stops.clone(),
				}) as Box<dyn LifecycleWatcher>)
			}),
			wait_for_stop: Box::new(|| Ok(())),
		};
		run_foreground_property_watcher_lifecycle("tok", &mut dependencies)
			.expect("정상 종료");
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 1, "정리에서 워처를 정지한다");
	}

	#[test]
	fn foreground_start_failure_is_reported_without_stop() {
		let ctx = Ctx::default();
		let mut dependencies = ForegroundWatcherDependencies {
			start_watcher: Box::new(|_token| {
				Err(Box::from("전경 시작 실패") as Box<dyn std::error::Error>)
			}),
			wait_for_stop: Box::new(|| Ok(())),
		};
		let error = run_foreground_property_watcher_lifecycle("tok", &mut dependencies)
			.expect_err("시작 실패");
		assert_eq!(error.to_string(), "전경 시작 실패");
		assert_eq!(ctx.stops.load(Ordering::SeqCst), 0);
	}

	#[test]
	fn foreground_watcher_failure_beats_stop_signal() {
		let ctx = Ctx::default();
		let stops = ctx.stops.clone();
		let mut dependencies = ForegroundWatcherDependencies {
			start_watcher: Box::new(move |_token| {
				Ok(Box::new(FakeWatcher {
					// 경합 루프의 첫 폴링에서 잡히도록 미리 넣어 둔 실패
					stop_outcome: Some(Err("전경 워처 실패".to_string())),
					fail_stop: false,
					stopped: false,
					stop_calls: stops.clone(),
				}) as Box<dyn LifecycleWatcher>)
			}),
			// 신호 대기는 끝나지 않아야 한다 — 경합에서 워처 실패가 이긴다
			wait_for_stop: Box::new(|| {
				std::thread::sleep(Duration::from_secs(5));
				Ok(())
			}),
		};
		let error = run_foreground_property_watcher_lifecycle("tok", &mut dependencies)
			.expect_err("워처 실패가 이긴다");
		assert_eq!(error.to_string(), "전경 워처 실패");
	}
}
