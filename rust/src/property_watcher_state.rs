// TS scripts/property-watcher-state.ts — PID 기록 판독·ready 표시·제거·상태 조회.

use std::path::{Path, PathBuf};

use crate::property_watcher_control::send_watcher_stop;
use crate::property_watcher_process::{
	inspect_watcher_process, matches_watcher_process_identity, parse_watcher_pid_record,
	same_watcher_instance, same_watcher_pid_record, PropertyWatcherProcessError,
	PropertyWatcherProcessErrorCode, WatcherPidRecord, WatcherProcessSnapshot,
	PROPERTY_WATCHER_PID_FILE,
};

/// 오류가 특정 PropertyWatcherProcessError 코드인지 판별한다(TS instanceof 대응).
fn is_process_error(
	error: &(dyn std::error::Error + 'static),
	code: PropertyWatcherProcessErrorCode,
) -> bool {
	error
		.downcast_ref::<PropertyWatcherProcessError>()
		.map(|e| e.code == code)
		.unwrap_or(false)
}

fn process_error(code: PropertyWatcherProcessErrorCode) -> Box<dyn std::error::Error> {
	Box::from(PropertyWatcherProcessError::new(code))
}

/// TS readWatcherPidRecord — 파일 없음/형식 오류를 코드로 보고한다.
pub fn read_watcher_pid_record(
	path: &Path,
) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
	let text = match std::fs::read_to_string(path) {
		Ok(text) => text,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
			return Err(process_error(PropertyWatcherProcessErrorCode::PidRecordMissing));
		}
		Err(e) => return Err(Box::from(e)),
	};
	let value: serde_json::Value = match serde_json::from_str(&text) {
		Ok(value) => value,
		Err(_) => return Err(process_error(PropertyWatcherProcessErrorCode::PidRecordInvalid)),
	};
	parse_watcher_pid_record(&value).map_err(|e| Box::from(e) as Box<dyn std::error::Error>)
}

/// TS WatcherReadyStateOperations — 상태 기록 연산을 주입한다(테스트용).
pub trait WatcherReadyStateOperations {
	fn random_uuid(&self) -> String;
	fn read_record(&self, path: &Path) -> Result<WatcherPidRecord, Box<dyn std::error::Error>>;
	/// TS의 openExclusive + write + sync + close 를 하나로 묶는다(항상 같이 쓰이기 때문).
	fn write_exclusive(&self, path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>>;
	fn rename(&self, from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>>;
	fn unlink(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>>;
}

/// TS watcherReadyStateOperations — 실제 파일 연산.
#[derive(Debug, Clone, Copy, Default)]
pub struct FsWatcherReadyStateOperations;

/// 임시 파일명용 고유 토큰(.uuid 형식).
fn unique_token() -> String {
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

impl WatcherReadyStateOperations for FsWatcherReadyStateOperations {
	fn random_uuid(&self) -> String {
		unique_token()
	}

	fn read_record(&self, path: &Path) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
		read_watcher_pid_record(path)
	}

	fn write_exclusive(&self, path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
		use std::io::Write;
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(path)?;
		file.write_all(contents.as_bytes())?;
		file.sync_all()?;
		// close: drop로 닫힌다
		Ok(())
	}

	fn rename(&self, from: &Path, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
		Ok(std::fs::rename(from, to)?)
	}

	fn unlink(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
		match std::fs::remove_file(path) {
			Ok(()) => Ok(()),
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
			Err(e) => Err(Box::from(e)),
		}
	}
}

/// TS WatcherReadyStateHandle의 임시 파일 경로 계산.
fn temporary_path(path: &Path, token: &str) -> PathBuf {
	let dir = path.parent().unwrap_or_else(|| Path::new("."));
	let base = path
		.file_name()
		.map(|n| n.to_string_lossy().to_string())
		.unwrap_or_default();
	dir.join(format!(".{base}.{token}.tmp"))
}

/// TS markWatcherPidReady — ready 상태로 교체하되 두 번의 identity 검증을 거친다.
pub fn mark_watcher_pid_ready(
	expected: &WatcherPidRecord,
	path: &Path,
	operations: &dyn WatcherReadyStateOperations,
) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
	let current = operations.read_record(path)?;
	if !same_watcher_pid_record(&current, expected) {
		return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch));
	}
	let mut ready = current.clone();
	ready.state = crate::property_watcher_process::WatcherPidState::Ready;

	let temp = temporary_path(path, &operations.random_uuid());
	let mut handle_closed = false;
	let mut handle_opened = false;

	let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
		handle_opened = true;
		operations.write_exclusive(&temp, &serde_json::to_string(&ready)?)?;
		handle_closed = true;
		let rechecked = operations.read_record(path)?;
		if !same_watcher_pid_record(&current, &rechecked) {
			return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch));
		}
		operations.rename(&temp, path)?;
		Ok(())
	})();

	if outcome.is_err() {
		// TS: 핸들 닫기 + 임시 파일 제거는 실패 여부와 무관하게 시도한다
		if handle_opened && !handle_closed {
			// write_exclusive는 내부에서 닫는다 — 여기서는 제거만 남는다
			let _ = operations.unlink(&temp);
		} else {
			let _ = operations.unlink(&temp);
		}
	}
	outcome?;
	Ok(ready)
}

/// TS removeMatchingWatcherPidRecord — 같은 인스턴스의 기록만 재검증 후 제거한다.
pub fn remove_matching_watcher_pid_record(
	path: &Path,
	expected: &WatcherPidRecord,
) -> Result<(), Box<dyn std::error::Error>> {
	let current = match read_watcher_pid_record(path) {
		Ok(record) => record,
		Err(error) => {
			if is_process_error(error.as_ref(), PropertyWatcherProcessErrorCode::PidRecordMissing) {
				return Ok(());
			}
			return Err(error);
		}
	};
	if !same_watcher_instance(&current, expected) {
		return Ok(());
	}
	let rechecked = read_watcher_pid_record(path)?;
	if same_watcher_pid_record(&current, &rechecked) {
		std::fs::remove_file(path)?;
	}
	Ok(())
}

/// TS matchingRecord — 코드 오류(없음·형식 오류)는 없음으로, 그 외 오류는 재던진다.
fn matching_record(
	path: &Path,
	instance_token: &str,
	pid: u64,
) -> Result<Option<WatcherPidRecord>, Box<dyn std::error::Error>> {
	match read_watcher_pid_record(path) {
		Ok(record) => Ok(if record.pid == pid && record.instance_token == instance_token {
			Some(record)
		} else {
			None
		}),
		Err(error) => {
			if error.downcast_ref::<PropertyWatcherProcessError>().is_some() {
				Ok(None)
			} else {
				Err(error)
			}
		}
	}
}

/// TS waitForStartingWatcherRecord — 기록이 생길 때까지 최대 5초 기다린다.
/// (TS는 fs.watch, 여기는 폴링 — 관찰 가능한 동작은 동일하다: 등장 시 반환, 초과 시 PID_RECORD_MISSING)
pub fn wait_for_starting_watcher_record(
	instance_token: &str,
	pid: u64,
	timeout_ms: u64,
) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
	let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
	let path = Path::new(PROPERTY_WATCHER_PID_FILE);
	loop {
		if let Some(record) = matching_record(path, instance_token, pid)? {
			return Ok(record);
		}
		if std::time::Instant::now() >= deadline {
			return Err(process_error(PropertyWatcherProcessErrorCode::PidRecordMissing));
		}
		std::thread::sleep(std::time::Duration::from_millis(100));
	}
}

/// TS WatcherStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatcherStatus {
	Stopped,
	Stale { pid: u64 },
	Starting { pid: u64 },
	Ready { pid: u64 },
}

impl WatcherStatus {
	/// TS discriminated `status` 필드 값.
	pub fn status_str(&self) -> &'static str {
		match self {
			WatcherStatus::Stopped => "stopped",
			WatcherStatus::Stale { .. } => "stale",
			WatcherStatus::Starting { .. } => "starting",
			WatcherStatus::Ready { .. } => "ready",
		}
	}

	pub fn pid(&self) -> Option<u64> {
		match self {
			WatcherStatus::Stopped => None,
			WatcherStatus::Stale { pid }
			| WatcherStatus::Starting { pid }
			| WatcherStatus::Ready { pid } => Some(*pid),
		}
	}
}

/// TS WatcherLifecycleDependencies
pub struct WatcherLifecycleDependencies {
	pub inspect_process: Box<dyn FnMut(u64) -> Result<Option<WatcherProcessSnapshot>, Box<dyn std::error::Error>>>,
	pub read_record: Box<dyn FnMut() -> Result<WatcherPidRecord, Box<dyn std::error::Error>>>,
	pub send_stop: Box<dyn FnMut(&str) -> Result<(), Box<dyn std::error::Error>>>,
}

impl Default for WatcherLifecycleDependencies {
	fn default() -> Self {
		Self {
			inspect_process: Box::new(|pid| inspect_watcher_process(pid)),
			read_record: Box::new(|| read_watcher_pid_record(Path::new(PROPERTY_WATCHER_PID_FILE))),
			send_stop: Box::new(|pipe| send_watcher_stop(pipe)),
		}
	}
}

/// TS getWatcherStatus — 기록 없음 stopped, 신원 불일치 stale, 아니면 실제 상태.
pub fn get_watcher_status(
	dependencies: &mut WatcherLifecycleDependencies,
) -> Result<WatcherStatus, Box<dyn std::error::Error>> {
	let record = match (dependencies.read_record)() {
		Ok(record) => record,
		Err(error) => {
			if is_process_error(error.as_ref(), PropertyWatcherProcessErrorCode::PidRecordMissing) {
				return Ok(WatcherStatus::Stopped);
			}
			return Err(error);
		}
	};
	let snapshot = (dependencies.inspect_process)(record.pid)?;
	if snapshot
		.as_ref()
		.map(|snapshot| !matches_watcher_process_identity(&record, snapshot))
		.unwrap_or(true)
	{
		return Ok(WatcherStatus::Stale { pid: record.pid });
	}
	Ok(match record.state {
		crate::property_watcher_process::WatcherPidState::Starting => {
			WatcherStatus::Starting { pid: record.pid }
		}
		crate::property_watcher_process::WatcherPidState::Ready => {
			WatcherStatus::Ready { pid: record.pid }
		}
	})
}

/// TS requestWatcherStop — 신원이 맞을 때만 정지 요청을 보낸다.
pub fn request_watcher_stop(
	dependencies: &mut WatcherLifecycleDependencies,
) -> Result<(), Box<dyn std::error::Error>> {
	let record = (dependencies.read_record)()?;
	let snapshot = (dependencies.inspect_process)(record.pid)?;
	if snapshot
		.as_ref()
		.map(|snapshot| !matches_watcher_process_identity(&record, snapshot))
		.unwrap_or(true)
	{
		return Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch));
	}
	(dependencies.send_stop)(&record.pipe_name)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::property_watcher_process::{watcher_pipe_name, WatcherPidState};
	use std::cell::RefCell;

	fn token() -> &'static str {
		"123e4567-e89b-42d3-a456-426614174000"
	}

	fn record(state: WatcherPidState) -> WatcherPidRecord {
		WatcherPidRecord {
			version: 1,
			state,
			pid: 4242,
			executable_path: "C:\\node.exe".into(),
			runner_path: "C:\\app\\runner.ts".into(),
			instance_token: token().to_string(),
			creation_identity: "id-1".into(),
			pipe_name: watcher_pipe_name(token()),
		}
	}

	#[test]
	fn read_record_missing_and_invalid_are_coded() {
		let dir = std::env::temp_dir().join("kras-state-missing");
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("pid.json");

		let err = read_watcher_pid_record(&path).expect_err("파일 없음");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::PidRecordMissing));

		std::fs::write(&path, "{not json").unwrap();
		let err = read_watcher_pid_record(&path).expect_err("잘못된 JSON");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::PidRecordInvalid));

		let record = record(WatcherPidState::Starting);
		std::fs::write(&path, serde_json::json!({
			"version": 1, "state": "starting", "pid": 4242,
			"executablePath": record.executable_path, "runnerPath": record.runner_path,
			"instanceToken": record.instance_token, "creationIdentity": record.creation_identity,
			"pipeName": record.pipe_name,
		}).to_string())
		.unwrap();
		let parsed = read_watcher_pid_record(&path).expect("정상");
		assert_eq!(parsed, record);
		let _ = std::fs::remove_dir_all(&dir);
	}

	/// 주입식 연산 — 기록 교체 로직을 파일 없이 검증한다.
	struct FakeOps {
		records: RefCell<String>,
		fail_write: RefCell<bool>,
		fail_rename: RefCell<bool>,
		mutate_on_recheck: RefCell<bool>,
		unlinked: RefCell<u32>,
		written: RefCell<Option<String>>,
	}

	impl FakeOps {
		fn new(record: &WatcherPidRecord) -> Self {
			Self {
				records: RefCell::new(serde_json::json!({
					"version": record.version,
					"state": record.state.as_str(),
					"pid": record.pid,
					"executablePath": record.executable_path,
					"runnerPath": record.runner_path,
					"instanceToken": record.instance_token,
					"creationIdentity": record.creation_identity,
					"pipeName": record.pipe_name,
				})
				.to_string()),
				fail_write: RefCell::new(false),
				fail_rename: RefCell::new(false),
				mutate_on_recheck: RefCell::new(false),
				unlinked: RefCell::new(0),
				written: RefCell::new(None),
			}
		}
	}

	impl WatcherReadyStateOperations for FakeOps {
		fn random_uuid(&self) -> String {
			"00000000-0000-4000-8000-000000000001".to_string()
		}
		fn read_record(&self, _path: &Path) -> Result<WatcherPidRecord, Box<dyn std::error::Error>> {
			if *self.mutate_on_recheck.borrow() && self.written.borrow().is_some() {
				// 두 번째 읽기에서 다른 인스턴스로 바뀐 상황
				let mut changed = parse_watcher_pid_record(
					&serde_json::from_str::<serde_json::Value>(&self.records.borrow())
						.unwrap(),
				)?;
				changed.pid = 9999;
				return Ok(changed);
			}
			let value: serde_json::Value = serde_json::from_str(&self.records.borrow())?;
			parse_watcher_pid_record(&value).map_err(|e| Box::from(e) as _)
		}
		fn write_exclusive(&self, _path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
			if *self.fail_write.borrow() {
				return Err(Box::from("쓰기 실패"));
			}
			*self.written.borrow_mut() = Some(contents.to_string());
			Ok(())
		}
		fn rename(&self, _from: &Path, _to: &Path) -> Result<(), Box<dyn std::error::Error>> {
			if *self.fail_rename.borrow() {
				return Err(Box::from("이름 바꾸기 실패"));
			}
			Ok(())
		}
		fn unlink(&self, _path: &Path) -> Result<(), Box<dyn std::error::Error>> {
			*self.unlinked.borrow_mut() += 1;
			Ok(())
		}
	}

	fn temp_path_for(path: &Path, token: &str) -> PathBuf {
		temporary_path(path, token)
	}

	#[test]
	fn mark_ready_writes_state_and_renames() {
		let expected = record(WatcherPidState::Starting);
		let ops = FakeOps::new(&expected);
		let path = Path::new("pid.json");
		let ready = mark_watcher_pid_ready(&expected, path, &ops).expect("ready 표시");
		assert_eq!(ready.state, WatcherPidState::Ready);
		assert_eq!(ready.pid, expected.pid, "다른 필드는 보존된다");
		let written = ops.written.borrow().clone().expect("기록 쓰기");
		assert!(written.contains("\"state\":\"ready\""), "{written}");
		assert_eq!(*ops.unlinked.borrow(), 0, "성공 시 임시 파일을 지우지 않는다(이름 바꾸기로 소멸)");
	}

	#[test]
	fn mark_ready_rejects_identity_mismatch_before_write() {
		let expected = record(WatcherPidState::Starting);
		let ops = FakeOps::new(&expected);
		// 기대 레코드를 다르게 만들어 mismatch 유발
		let mut other = expected.clone();
		other.pid = 7;
		let err = mark_watcher_pid_ready(&other, Path::new("pid.json"), &ops)
			.expect_err("신원 불일치");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::IdentityMismatch));
		assert!(ops.written.borrow().is_none(), "불일치면 쓰지 않는다");
	}

	#[test]
	fn mark_ready_cleans_temp_when_write_fails() {
		let expected = record(WatcherPidState::Starting);
		let ops = FakeOps::new(&expected);
		*ops.fail_write.borrow_mut() = true;
		let err = mark_watcher_pid_ready(&expected, Path::new("pid.json"), &ops)
			.expect_err("쓰기 실패");
		assert_eq!(err.to_string(), "쓰기 실패");
		assert_eq!(*ops.unlinked.borrow(), 1, "임시 파일을 제거한다");
	}

	#[test]
	fn mark_ready_detects_change_before_rename_and_cleans_up() {
		let expected = record(WatcherPidState::Starting);
		let ops = FakeOps::new(&expected);
		*ops.mutate_on_recheck.borrow_mut() = true;
		let err = mark_watcher_pid_ready(&expected, Path::new("pid.json"), &ops)
			.expect_err("사이 변경 감지");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::IdentityMismatch));
		assert_eq!(*ops.unlinked.borrow(), 1, "임시 파일 제거");
	}

	#[test]
	fn temp_path_uses_uuid_token() {
		let p = temp_path_for(Path::new("C:\\dir\\property-watcher.pid.json"), "abc");
		assert_eq!(
			p.to_string_lossy(),
			"C:\\dir\\.property-watcher.pid.json.abc.tmp"
		);
	}

	#[test]
	fn remove_record_skips_when_missing_or_foreign() {
		let dir = std::env::temp_dir().join("kras-state-remove");
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("pid.json");

		// 없으면 조용히 성공
		remove_matching_watcher_pid_record(&path, &record(WatcherPidState::Ready)).expect("없음");

		// 다른 인스턴스 기록 → 제거하지 않음
		let mut foreign = record(WatcherPidState::Ready);
		foreign.pid = 1111;
		std::fs::write(
			&path,
			serde_json::json!({
				"version":1,"state":"ready","pid":1111,
				"executablePath":foreign.executable_path,"runnerPath":foreign.runner_path,
				"instanceToken":foreign.instance_token,"creationIdentity":foreign.creation_identity,
				"pipeName":foreign.pipe_name,
			}).to_string(),
		)
		.unwrap();
		remove_matching_watcher_pid_record(&path, &record(WatcherPidState::Ready)).expect("무시");
		assert!(path.exists(), "다른 인스턴스 기록은 보존된다");

		// 같은 인스턴스 → 제거
		let mine = record(WatcherPidState::Ready);
		std::fs::write(
			&path,
			serde_json::json!({
				"version":1,"state":"ready","pid":mine.pid,
				"executablePath":mine.executable_path,"runnerPath":mine.runner_path,
				"instanceToken":mine.instance_token,"creationIdentity":mine.creation_identity,
				"pipeName":mine.pipe_name,
			}).to_string(),
		)
		.unwrap();
		remove_matching_watcher_pid_record(&path, &mine).expect("제거");
		assert!(!path.exists(), "같은 인스턴스 기록은 지워진다");
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn status_str_and_pid_follow_ts_shape() {
		assert_eq!(WatcherStatus::Stopped.status_str(), "stopped");
		assert_eq!(WatcherStatus::Stopped.pid(), None);
		assert_eq!(WatcherStatus::Stale { pid: 3 }.status_str(), "stale");
		assert_eq!(WatcherStatus::Starting { pid: 3 }.status_str(), "starting");
		assert_eq!(WatcherStatus::Ready { pid: 4 }.status_str(), "ready");
		assert_eq!(WatcherStatus::Ready { pid: 4 }.pid(), Some(4));
	}

	fn make_deps(
		read: Result<WatcherPidRecord, Box<dyn std::error::Error>>,
		inspect: Result<Option<WatcherProcessSnapshot>, Box<dyn std::error::Error>>,
		stopped: std::rc::Rc<RefCell<Vec<String>>>,
	) -> (WatcherLifecycleDependencies, std::rc::Rc<RefCell<Vec<String>>>) {
		let stopped_for_send = stopped.clone();
		let deps = WatcherLifecycleDependencies {
			read_record: Box::new(move || match &read {
				Ok(record) => Ok(record.clone()),
				Err(_) => Err(process_error(PropertyWatcherProcessErrorCode::PidRecordMissing)),
			}),
			inspect_process: Box::new(move |_| match &inspect {
				Ok(snapshot) => Ok(snapshot.clone()),
				Err(_) => Err(process_error(PropertyWatcherProcessErrorCode::IdentityMismatch)),
			}),
			send_stop: Box::new(move |pipe| {
				stopped_for_send.borrow_mut().push(pipe.to_string());
				Ok(())
			}),
		};
		(deps, stopped)
	}

	fn snapshot(pid: u64) -> WatcherProcessSnapshot {
		WatcherProcessSnapshot {
			pid,
			executable_path: "C:\\node.exe".into(),
			command_line: "C:\\node.exe C:\\app\\runner.ts watch --title=opencode-property-watcher:123e4567-e89b-42d3-a456-426614174000".into(),
			creation_identity: "id-1".into(),
		}
	}

	#[test]
	fn status_is_stopped_when_record_missing() {
		let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
		let (mut deps, _) = make_deps(
			Err(process_error(PropertyWatcherProcessErrorCode::PidRecordMissing)),
			Ok(None),
			stopped,
		);
		assert_eq!(get_watcher_status(&mut deps).expect("stopped"), WatcherStatus::Stopped);
	}

	#[test]
	fn status_is_stale_when_process_missing_or_identity_mismatches() {
		// 스냅샷 없음
		let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
		let (mut deps, _) = make_deps(Ok(record(WatcherPidState::Ready)), Ok(None), stopped);
		assert_eq!(
			get_watcher_status(&mut deps).expect("stale"),
			WatcherStatus::Stale { pid: 4242 }
		);

		// 신원 불일치(명령행이 규칙과 다름)
		let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
		let mut foreign = snapshot(4242);
		foreign.command_line = "C:\\node.exe".into();
		let (mut deps, _) = make_deps(Ok(record(WatcherPidState::Ready)), Ok(Some(foreign)), stopped);
		assert_eq!(
			get_watcher_status(&mut deps).expect("stale"),
			WatcherStatus::Stale { pid: 4242 }
		);
	}

	#[test]
	fn status_reports_record_state_when_identity_matches() {
		for (state, expected) in [
			(WatcherPidState::Starting, WatcherStatus::Starting { pid: 4242 }),
			(WatcherPidState::Ready, WatcherStatus::Ready { pid: 4242 }),
		] {
			let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
			let (mut deps, _) = make_deps(Ok(record(state)), Ok(Some(snapshot(4242))), stopped);
			assert_eq!(get_watcher_status(&mut deps).expect("상태"), expected);
		}
	}

	#[test]
	fn request_stop_requires_matching_identity() {
		// 신원 불일치 → IDENTITY_MISMATCH, 정지 요청 없음
		let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
		let mut foreign = snapshot(1);
		foreign.pid = 1;
		let (mut deps, sends) = make_deps(Ok(record(WatcherPidState::Ready)), Ok(Some(foreign)), stopped);
		let err = request_watcher_stop(&mut deps).expect_err("불일치");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::IdentityMismatch));
		assert!(sends.borrow().is_empty(), "정지 요청을 보내지 않는다");
	}

	#[test]
	fn request_stop_sends_pipe_name_when_identity_matches() {
		let stopped = std::rc::Rc::new(RefCell::new(Vec::new()));
		let (mut deps, sends) = make_deps(Ok(record(WatcherPidState::Ready)), Ok(Some(snapshot(4242))), stopped);
		request_watcher_stop(&mut deps).expect("정지 요청");
		assert_eq!(*sends.borrow(), vec![watcher_pipe_name(token())]);
	}

	#[test]
	fn wait_for_record_times_out_with_missing_code() {
		// 존재하지 않는 토큰 → 어떤 기록과도 매칭되지 않는다
		let started = std::time::Instant::now();
		let err = wait_for_starting_watcher_record("00000000-0000-4000-8000-00000000dead", 1, 300)
			.expect_err("매칭 없음");
		assert!(is_process_error(err.as_ref(), PropertyWatcherProcessErrorCode::PidRecordMissing));
		let elapsed = started.elapsed().as_millis();
		assert!(elapsed >= 250, "제한 시간까지 기다린다: {elapsed}ms");
		assert!(elapsed < 3_000, "제한 시간을 넘기지 않는다: {elapsed}ms");
	}

	#[test]
	fn matching_record_requires_both_pid_and_token() {
		let dir = std::env::temp_dir().join("kras-state-match");
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("pid.json");
		let rec = record(WatcherPidState::Ready);
		std::fs::write(
			&path,
			serde_json::json!({
				"version":1,"state":"ready","pid":rec.pid,
				"executablePath":rec.executable_path,"runnerPath":rec.runner_path,
				"instanceToken":rec.instance_token,"creationIdentity":rec.creation_identity,
				"pipeName":rec.pipe_name,
			}).to_string(),
		)
		.unwrap();

		assert!(matching_record(&path, &rec.instance_token, rec.pid)
			.expect("정상")
			.is_some());
		assert!(matching_record(&path, &rec.instance_token, rec.pid + 1)
			.expect("pid 불일치")
			.is_none());
		assert!(matching_record(&path, "123e4567-e89b-42d3-a456-426614174999", rec.pid)
			.expect("토큰 불일치")
			.is_none());
		// 형식 오류는 없음으로 취급한다(TS: PropertyWatcherProcessError → undefined)
		std::fs::write(&path, "{broken").unwrap();
		assert!(matching_record(&path, &rec.instance_token, rec.pid)
			.expect("형식 오류는 없음")
			.is_none());
		let _ = std::fs::remove_dir_all(&dir);
	}

}
