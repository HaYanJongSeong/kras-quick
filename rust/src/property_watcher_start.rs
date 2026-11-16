// TS scripts/property-watcher-start.ts — SCP 워처를 데티시로 띄우고 PID 기록을 남긴다.

use std::path::{Path, PathBuf};

use crate::property_watcher_process::{
	ensure_watcher_artifact_directory, inspect_watcher_process, is_uuid,
	matches_watcher_process_identity, parse_watcher_pid_record, reserve_watcher_pid_file,
	same_watcher_pid_record,
	watcher_pipe_name, PidReservationOps, PropertyWatcherProcessError,
	PropertyWatcherProcessErrorCode, WatcherPidRecord, WatcherPidState, WatcherProcessSnapshot,
	PROPERTY_WATCHER_ARTIFACT_DIRECTORY, PROPERTY_WATCHER_LOG_FILE, PROPERTY_WATCHER_PID_FILE,
	PROPERTY_WATCHER_TOKEN_ENV,
};
use crate::property_watcher_state::read_watcher_pid_record;

/// TS DetachedSpawnRequest — 자식 프로세스 생성 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedSpawnRequest {
	pub executable: String,
	pub arguments: Vec<String>,
	pub env: Vec<(String, String)>,
	/// 표준 출력·에러를 이어 붙일 로그 경로(TS의 log fd 대응).
	pub log_path: Option<PathBuf>,
}

/// TS spawn 반환값의 pid 부분.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnedProcess {
	pub pid: u64,
}

fn process_error(code: PropertyWatcherProcessErrorCode) -> Box<dyn std::error::Error> {
	Box::from(PropertyWatcherProcessError::new(code))
}

fn is_process_error(
	error: &(dyn std::error::Error + 'static),
	code: PropertyWatcherProcessErrorCode,
) -> bool {
	error
		.downcast_ref::<PropertyWatcherProcessError>()
		.map(|e| e.code == code)
		.unwrap_or(false)
}

/// 임시 파일명용 고유 토큰(.uuid 형식) — TS randomUUID 대응.
pub fn unique_token() -> String {
	use std::sync::atomic::{AtomicU64, Ordering};
	static COUNTER: AtomicU64 = AtomicU64::new(0);
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default();
	let nanos = now.subsec_nanos() as u64 ^ (now.as_secs() << 17);
	let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
	let mixed = nanos ^ (std::process::id() as u64).rotate_left(13) ^ seq.rotate_left(7);
	format!(
		"{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
		mixed >> 32,
		(mixed >> 16) & 0xFFFF,
		(mixed >> 4) & 0xFFFF,
		seq & 0xFFFF,
		mixed & 0xFFFF_FFFF_FFFF
	)
}

/// 실제 데티시 실행 — 로그를 stdout/stderr로 연결하고 부모와 무관한 프로세스로 띄운다.
pub fn spawn_detached(
	request: &DetachedSpawnRequest,
) -> Result<SpawnedProcess, Box<dyn std::error::Error>> {
	use std::process::{Command, Stdio};

	let mut command = Command::new(&request.executable);
	command.args(&request.arguments);
	for (key, value) in &request.env {
		command.env(key, value);
	}
	command.stdin(Stdio::null());
	if let Some(log_path) = &request.log_path {
		let file = std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(log_path)
			.map_err(|e| format!("로그 파일을 열 수 없습니다: {e}"))?;
		command.stdout(Stdio::from(file.try_clone()?));
		command.stderr(Stdio::from(file));
	}
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// TS detached:true + windowsHide:true
		const DETACHED_PROCESS: u32 = 0x0000_0008;
		const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
		unsafe {
			command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
		}
	}
	let child = command.spawn().map_err(|e| {
		let _ = e;
		PropertyWatcherProcessError::new(PropertyWatcherProcessErrorCode::SpawnFailed)
	})?;
	Ok(SpawnedProcess { pid: u64::from(child.id()) })
}

/// TS DetachedStartDependencies
pub struct DetachedStartDependencies {
	pub ensure_artifact_directory:
		Box<dyn FnMut() -> Result<(), Box<dyn std::error::Error>>>,
	pub inspect_process:
		Box<dyn FnMut(u64) -> Result<Option<WatcherProcessSnapshot>, Box<dyn std::error::Error>>>,
	pub open_append_log: Box<dyn FnMut() -> Result<PathBuf, Box<dyn std::error::Error>>>,
	pub random_uuid: Box<dyn FnMut() -> String>,
	pub read_record: Box<dyn FnMut() -> Result<WatcherPidRecord, Box<dyn std::error::Error>>>,
	pub remove_reservation: Box<dyn FnMut() -> Result<(), Box<dyn std::error::Error>>>,
	pub reserve_pid:
		Box<dyn FnMut() -> Result<Box<dyn PidReservationOps>, Box<dyn std::error::Error>>>,
	pub spawn:
		Box<dyn FnMut(&DetachedSpawnRequest) -> Result<SpawnedProcess, Box<dyn std::error::Error>>>,
	/// TS는 process.execPath — Rust는 실행 파일 경로를 주입한다.
	pub runtime_executable: Box<dyn Fn() -> String>,
}

impl Default for DetachedStartDependencies {
	fn default() -> Self {
		Self {
			ensure_artifact_directory: Box::new(|| {
				ensure_watcher_artifact_directory(Path::new(PROPERTY_WATCHER_ARTIFACT_DIRECTORY))
			}),
			inspect_process: Box::new(|pid| inspect_watcher_process(pid)),
			open_append_log: Box::new(|| {
				std::fs::OpenOptions::new()
					.create(true)
					.append(true)
					.open(PROPERTY_WATCHER_LOG_FILE)?;
				Ok(PathBuf::from(PROPERTY_WATCHER_LOG_FILE))
			}),
			random_uuid: Box::new(unique_token),
			read_record: Box::new(|| read_watcher_pid_record(Path::new(PROPERTY_WATCHER_PID_FILE))),
			remove_reservation: Box::new(|| {
				match std::fs::remove_file(PROPERTY_WATCHER_PID_FILE) {
					Ok(()) => Ok(()),
					Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
					Err(e) => Err(Box::from(e)),
				}
			}),
			reserve_pid: Box::new(|| {
				reserve_watcher_pid_file(Path::new(PROPERTY_WATCHER_PID_FILE))
					.map(|r| Box::new(r) as Box<dyn PidReservationOps>)
			}),
			spawn: Box::new(|request| spawn_detached(request)),
			runtime_executable: Box::new(|| {
				std::env::current_exe()
					.map(|p| p.to_string_lossy().to_string())
					.unwrap_or_default()
			}),
		}
	}
}

/// TS reserveForStart — 이미 예약됐으면 오래된 기록을 확인해 정리 후 재예약한다.
fn reserve_for_start(
	dependencies: &mut DetachedStartDependencies,
) -> Result<Box<dyn PidReservationOps>, Box<dyn std::error::Error>> {
	let already_reserved = match (dependencies.reserve_pid)() {
		Ok(reservation) => return Ok(reservation),
		Err(error) => {
			if !is_process_error(
				error.as_ref(),
				PropertyWatcherProcessErrorCode::PidAlreadyReserved,
			) {
				return Err(error);
			}
			true
		}
	};
	debug_assert!(already_reserved);

	// 살아 있는 이전 워처라면 그대로 실패를 돌려준다
	let stale = (dependencies.read_record)();
	let snapshot = (dependencies.inspect_process)(stale.as_ref().map(|r| r.pid).unwrap_or(0))?;
	if let (Ok(stale), Some(snapshot)) = (stale.as_ref(), snapshot.as_ref()) {
		if matches_watcher_process_identity(stale, snapshot) {
			return Err(process_error(PropertyWatcherProcessErrorCode::PidAlreadyReserved));
		}
	}
	let stale = stale?;
	// 두 번 읽어 사이에 바뀌지 않았는지 확인한 뒤 예약을 비우고 다시 예약한다
	let rechecked = (dependencies.read_record)()?;
	if !same_watcher_pid_record(&stale, &rechecked) {
		return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch));
	}
	(dependencies.remove_reservation)()?;
	(dependencies.reserve_pid)()
}

/// TS startDetachedPropertyWatcher — 러너를 분리 실행하고 starting 기록을 남긴다.
pub fn start_detached_property_watcher(
	runner_path: &Path,
	dependencies: &mut DetachedStartDependencies,
) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
	if !runner_path.is_absolute() {
		return Err(process_error(PropertyWatcherProcessErrorCode::SpawnFailed));
	}
	if let Err(error) = (dependencies.ensure_artifact_directory)() {
		// TS: ENOENT → ARTIFACT_DIRECTORY_MISSING
		if is_process_error(
			error.as_ref(),
			PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing,
		) || error.to_string().contains("ENOENT")
		{
			return Err(process_error(
				PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing,
			));
		}
		return Err(error);
	}

	let mut reservation = reserve_for_start(dependencies)?;

	let outcome = (|| -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
		let token = (dependencies.random_uuid)();
		if !is_uuid(&token) {
			return Err(process_error(PropertyWatcherProcessErrorCode::SpawnFailed));
		}
		let log_path = (dependencies.open_append_log)()?;
		let executable = (dependencies.runtime_executable)();
		if executable.is_empty() {
			return Err(process_error(PropertyWatcherProcessErrorCode::SpawnFailed));
		}
		let runner = runner_path.to_string_lossy().to_string();
		let request = DetachedSpawnRequest {
			executable,
			arguments: vec![
				format!("--title=opencode-property-watcher:{token}"),
				runner.clone(),
				"watch".to_string(),
			],
			env: vec![(PROPERTY_WATCHER_TOKEN_ENV.to_string(), token.clone())],
			log_path: Some(log_path),
		};
		let spawned = (dependencies.spawn)(&request)?;

		let snapshot = match (dependencies.inspect_process)(spawned.pid)? {
			Some(snapshot) => snapshot,
			None => return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch)),
		};
		let record = WatcherPidRecord {
			version: 1,
			state: WatcherPidState::Starting,
			pid: spawned.pid,
			executable_path: snapshot.executable_path.clone(),
			runner_path: runner,
			instance_token: token,
			creation_identity: snapshot.creation_identity.clone(),
			pipe_name: watcher_pipe_name(&request_token(&request)),
		};
		if !matches_watcher_process_identity(&record, &snapshot) {
			return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch));
		}
		reservation.write(&serde_json::to_string(&record)?)?;
		reservation.sync()?;
		reservation.close()?;
		Ok(record)
	})();

	if outcome.is_err() {
		// TS: allSettled([close, removeReservation]) 후 원본 오류를 재던진다
		let _ = reservation.close();
		let _ = (dependencies.remove_reservation)();
	}
	outcome
}

/// 요청에 담긴 인스턴스 토큰(env)을 돌려준다.
fn request_token(request: &DetachedSpawnRequest) -> String {
	request
		.env
		.iter()
		.find(|(key, _)| key == PROPERTY_WATCHER_TOKEN_ENV)
		.map(|(_, value)| value.clone())
		.unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	fn token() -> &'static str {
		"123e4567-e89b-42d3-a456-426614174000"
	}

	fn runner() -> &'static str {
		"C:\\app\\property-watcher-runner.ts"
	}

	/// 예약이 쓴 내용과 닫힘 횟수, 제거 횟수, 요청을 함께 기록한다.
	#[derive(Clone)]
	struct Ctx {
		written: Rc<RefCell<Option<String>>>,
		closed: Rc<RefCell<u32>>,
		removed: Rc<RefCell<u32>>,
		reserve_calls: Rc<RefCell<u32>>,
		record_reads: Rc<RefCell<u32>>,
		inspect_calls: Rc<RefCell<u32>>,
		requests: Rc<RefCell<Vec<DetachedSpawnRequest>>>,
	}

	impl Default for Ctx {
		fn default() -> Self {
			Self {
				written: Rc::new(RefCell::new(None)),
				closed: Rc::new(RefCell::new(0)),
				removed: Rc::new(RefCell::new(0)),
				reserve_calls: Rc::new(RefCell::new(0)),
				record_reads: Rc::new(RefCell::new(0)),
				inspect_calls: Rc::new(RefCell::new(0)),
				requests: Rc::new(RefCell::new(Vec::new())),
			}
		}
	}

	struct FakeReservation {
		written: Rc<RefCell<Option<String>>>,
		closed: Rc<RefCell<u32>>,
	}

	impl PidReservationOps for FakeReservation {
		fn write(&mut self, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
			*self.written.borrow_mut() = Some(contents.to_string());
			Ok(())
		}
		fn sync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
			Ok(())
		}
		fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
			*self.closed.borrow_mut() += 1;
			Ok(())
		}
	}

	impl std::fmt::Debug for FakeReservation {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.debug_struct("FakeReservation").finish_non_exhaustive()
		}
	}

	fn snapshot(pid: u64) -> WatcherProcessSnapshot {
		WatcherProcessSnapshot {
			pid,
			executable_path: "C:\\node.exe".into(),
			command_line: format!(
				"C:\\node.exe --title=opencode-property-watcher:{} {} watch",
				token(),
				runner()
			),
			creation_identity: "id-1".into(),
		}
	}

	#[derive(Clone, Copy, PartialEq, Eq)]
	enum ReserveMode {
		FirstTime,
		AlreadyReservedAlive,
		AlreadyReservedDead,
		AlreadyReservedChanged,
		OtherFailure,
	}

	#[derive(Clone, Copy)]
	enum Inspect {
		Matches,
		Null,
		Mismatch,
	}

	struct Opts {
		ensure_error: bool,
		reserve: ReserveMode,
		inspect: Inspect,
		spawn_fails: bool,
		bad_uuid: bool,
	}

	impl Default for Opts {
		fn default() -> Self {
			Self {
				ensure_error: false,
				reserve: ReserveMode::FirstTime,
				inspect: Inspect::Matches,
				spawn_fails: false,
				bad_uuid: false,
			}
		}
	}

	fn deps(ctx: &Ctx, opts: Opts) -> DetachedStartDependencies {
		let ctx = ctx.clone();
		let ctx_for_record = ctx.clone();
		let ctx_for_reserve = ctx.clone();
		let ctx_for_remove = ctx.clone();
		let ctx_for_spawn = ctx.clone();
		let ctx_for_inspect = ctx.clone();
		let ctx_for_reserve_call = ctx.clone();

		DetachedStartDependencies {
			ensure_artifact_directory: Box::new(move || {
				if opts.ensure_error {
					return Err(process_error(
						PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing,
					));
				}
				Ok(())
			}),
			inspect_process: Box::new(move |pid| {
				let n = *ctx_for_inspect.inspect_calls.borrow();
				*ctx_for_inspect.inspect_calls.borrow_mut() = n + 1;
				// 오래된 프로세스 확인(1회)과 새 자식 확인(2회)을 구분한다
				let stale_phase = n == 0
					&& matches!(
						opts.reserve,
						ReserveMode::AlreadyReservedDead | ReserveMode::AlreadyReservedChanged
					);
				Ok(if stale_phase {
					match opts.reserve {
						ReserveMode::AlreadyReservedDead => None,
						_ => {
							let mut snap = snapshot(pid);
							snap.command_line = "C:\\node.exe".into();
							Some(snap)
						}
					}
				} else {
					match opts.inspect {
						Inspect::Null => None,
						Inspect::Matches => Some(snapshot(pid)),
						Inspect::Mismatch => {
							let mut snap = snapshot(pid);
							snap.command_line = "C:\\node.exe".into();
							Some(snap)
						}
					}
				})
			}),
			open_append_log: Box::new(|| Ok(PathBuf::from("C:\\temp\\watcher.log"))),
			random_uuid: Box::new(move || {
				if opts.bad_uuid {
					"not-a-uuid".to_string()
				} else {
					token().to_string()
				}
			}),
			read_record: Box::new(move || {
				let n = *ctx_for_record.record_reads.borrow();
				*ctx_for_record.record_reads.borrow_mut() = n + 1;
				// 두 번째 읽기에서 기록이 바뀐 상황을 재현한다
				if opts.reserve == ReserveMode::AlreadyReservedChanged && n >= 1 {
					let mut record = stale_record();
					record.pid = 9;
					return Ok(record);
				}
				Ok(stale_record())
			}),
			remove_reservation: Box::new(move || {
				*ctx_for_remove.removed.borrow_mut() += 1;
				Ok(())
			}),
			reserve_pid: Box::new(move || {
				let n = *ctx_for_reserve_call.reserve_calls.borrow();
				*ctx_for_reserve_call.reserve_calls.borrow_mut() = n + 1;
				match opts.reserve {
					ReserveMode::FirstTime => Ok(Box::new(FakeReservation {
						written: ctx.written.clone(),
						closed: ctx.closed.clone(),
					}) as Box<dyn PidReservationOps>),
					// 첫 예약은 항상 "이미 예약됨" — 정리 후 재예약은 성공한다
					ReserveMode::AlreadyReservedAlive => Err(Box::from(
						PropertyWatcherProcessError::new(
							PropertyWatcherProcessErrorCode::PidAlreadyReserved,
						),
					)),
					ReserveMode::AlreadyReservedDead | ReserveMode::AlreadyReservedChanged => {
						if n == 0 {
							Err(Box::from(PropertyWatcherProcessError::new(
								PropertyWatcherProcessErrorCode::PidAlreadyReserved,
							)))
						} else {
							Ok(Box::new(FakeReservation {
								written: ctx_for_reserve.written.clone(),
								closed: ctx_for_reserve.closed.clone(),
							}) as Box<dyn PidReservationOps>)
						}
					}
					ReserveMode::OtherFailure => Err(Box::from(
						PropertyWatcherProcessError::new(PropertyWatcherProcessErrorCode::SpawnFailed),
					)),
				}
			}),
			spawn: Box::new(move |request| {
				ctx_for_spawn.requests.borrow_mut().push(request.clone());
				if opts.spawn_fails {
					return Err(Box::from(PropertyWatcherProcessError::new(
						PropertyWatcherProcessErrorCode::SpawnFailed,
					)));
				}
				Ok(SpawnedProcess { pid: 4242 })
			}),
			runtime_executable: Box::new(|| "C:\\node.exe".to_string()),
		}
	}

	fn stale_record() -> WatcherPidRecord {
		WatcherPidRecord {
			version: 1,
			state: WatcherPidState::Starting,
			pid: 4242,
			executable_path: "C:\\node.exe".into(),
			runner_path: runner().to_string(),
			instance_token: token().to_string(),
			creation_identity: "id-1".into(),
			pipe_name: watcher_pipe_name(token()),
		}
	}

	fn run(opts: Opts) -> (Result<WatcherPidRecord, String>, Ctx) {
		let ctx = Ctx::default();
		let mut deps = deps(&ctx, opts);
		let result =
			start_detached_property_watcher(Path::new(runner()), &mut deps).map_err(|e| e.to_string());
		(result, ctx)
	}

	#[test]
	fn absolute_runner_path_is_required() {
		let ctx = Ctx::default();
		let mut dependencies = deps(&ctx, Opts::default());
		let relative = start_detached_property_watcher(
			Path::new("relative\\runner.ts"),
			&mut dependencies,
		)
		.expect_err("상대 경로 거부");
		assert_eq!(relative.to_string(), "SPAWN_FAILED");
		// 어떤 준비 동작도 하지 않는다
		assert_eq!(*ctx.reserve_calls.borrow(), 0);
		assert_eq!(*ctx.removed.borrow(), 0);
		assert!(ctx.requests.borrow().is_empty());
	}

	#[test]
	fn missing_artifact_directory_becomes_coded_error() {
		let (result, _ctx) = run(Opts { ensure_error: true, ..Default::default() });
		assert_eq!(result.expect_err("디렉터리 오류"), "ARTIFACT_DIRECTORY_MISSING");
	}

	#[test]
	fn invalid_uuid_fails_spawn_and_removes_reservation() {
		let (result, ctx) = run(Opts { bad_uuid: true, ..Default::default() });
		assert_eq!(result.expect_err("잘못된 토큰"), "SPAWN_FAILED");
		assert_eq!(*ctx.closed.borrow(), 1, "예약을 닫는다");
		assert_eq!(*ctx.removed.borrow(), 1, "예약을 제거한다");
		assert!(ctx.written.borrow().is_none(), "기록을 남기지 않는다");
	}

	#[test]
	fn spawn_failure_propagates_and_cleans_reservation() {
		let (result, ctx) = run(Opts { spawn_fails: true, ..Default::default() });
		assert_eq!(result.expect_err("spawn 실패"), "SPAWN_FAILED");
		assert_eq!(*ctx.removed.borrow(), 1);
		assert!(ctx.written.borrow().is_none());
	}

	#[test]
	fn null_snapshot_becomes_identity_mismatch_with_cleanup() {
		let (result, ctx) = run(Opts { inspect: Inspect::Null, ..Default::default() });
		assert_eq!(result.expect_err("스냅샷 없음"), "IDENTITY_MISMATCH");
		assert_eq!(*ctx.removed.borrow(), 1);
	}

	#[test]
	fn mismatched_identity_becomes_identity_mismatch_with_cleanup() {
		let (result, ctx) = run(Opts { inspect: Inspect::Mismatch, ..Default::default() });
		assert_eq!(result.expect_err("명령행 불일치"), "IDENTITY_MISMATCH");
		assert_eq!(*ctx.removed.borrow(), 1);
		assert!(ctx.written.borrow().is_none(), "기록하지 않는다");
	}

	#[test]
	fn success_writes_record_and_spawns_expected_process() {
		let (result, ctx) = run(Opts::default());
		let record = result.expect("성공");
		assert_eq!(record.version, 1);
		assert_eq!(record.state, WatcherPidState::Starting);
		assert_eq!(record.pid, 4242);
		assert_eq!(record.instance_token, token());
		assert_eq!(record.pipe_name, watcher_pipe_name(token()));
		assert_eq!(record.runner_path, runner());

		// 쓴 내용이 실제로 파싱 가능한 기록인지 검증
		let written = ctx.written.borrow().clone().expect("기록 쓰기");
		let parsed = parse_watcher_pid_record(&serde_json::from_str::<serde_json::Value>(&written).unwrap())
			.expect("기록 파싱");
		assert_eq!(parsed, record, "쓰인 기록과 반환값이 같다");
		assert_eq!(*ctx.closed.borrow(), 1, "예약을 닫는다");
		assert_eq!(*ctx.removed.borrow(), 0, "성공 시 예약을 지우지 않는다");

		// spawn 요청 형식: 제목 인자 · 러너 · watch · 토큰 환경변수 · 로그 경로
		let requests = ctx.requests.borrow();
		assert_eq!(requests.len(), 1);
		let request = &requests[0];
		assert_eq!(request.executable, "C:\\node.exe");
		assert_eq!(
			request.arguments,
			vec![
				format!("--title=opencode-property-watcher:{}", token()),
				runner().to_string(),
				"watch".to_string()
			]
		);
		assert_eq!(request.env, vec![(PROPERTY_WATCHER_TOKEN_ENV.to_string(), token().to_string())]);
		assert_eq!(request.log_path, Some(PathBuf::from("C:\\temp\\watcher.log")));
	}

	#[test]
	fn already_reserved_with_live_process_rethrows_original_error() {
		let (result, ctx) = run(Opts {
			reserve: ReserveMode::AlreadyReservedAlive,
			..Default::default()
		});
		assert_eq!(result.expect_err("살아 있는 워처"), "PID_ALREADY_RESERVED");
		assert_eq!(*ctx.removed.borrow(), 0, "살아 있으면 제거하지 않는다");
	}

	#[test]
	fn already_reserved_with_dead_process_removes_and_reserves_again() {
		let (result, ctx) = run(Opts {
			reserve: ReserveMode::AlreadyReservedDead,
			..Default::default()
		});
		// 오래된 기록의 프로세스가 죽어 있으면 → 정리 → 재예약 → 정상 기록까지 간다
		let record = result.expect("정리 후 재예약 성공");
		assert_eq!(record.pid, 4242);
		assert_eq!(*ctx.removed.borrow(), 1, "한 번만 정리한다");
		assert_eq!(*ctx.reserve_calls.borrow(), 2, "한 번 실패한 뒤 재예약");
		assert!(ctx.written.borrow().is_some(), "새 기록을 남긴다");
	}

	#[test]
	fn already_reserved_with_changed_record_is_mismatch() {
		let (result, _ctx) = run(Opts {
			reserve: ReserveMode::AlreadyReservedChanged,
			..Default::default()
		});
		assert_eq!(result.expect_err("기록 변경"), "IDENTITY_MISMATCH");
	}

	#[test]
	fn non_reserved_failure_is_propagated_unchanged() {
		let (result, _ctx) = run(Opts {
			reserve: ReserveMode::OtherFailure,
			..Default::default()
		});
		assert_eq!(result.expect_err("예약 실패가 아닌 오류"), "SPAWN_FAILED");
	}
}
