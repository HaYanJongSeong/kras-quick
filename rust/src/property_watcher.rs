// TS scripts/property-watcher.ts — SCP 행 수신 후 KRAS 조회·진행 기록을 수행하는 감시자.
// TS는 프로미스 큐로 직렬화한다. Rust는 콜백을 동기 호출해 같은 순서·같은 분기를 보장한다.

use crate::property_address::{parse_property_row, ParcelAddress, PropertyAddressError};
use crate::property_progress::ProgressFileErrorCode;
use crate::property_watcher_payload::{
	PropertyBuildingStatusUpdateResult, PropertyBuildingTerminalStatus,
	PropertyWatcherSelectionHandle,
};

/// TS PROPERTY_WATCHER_EVENTS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherEvent {
	CommitFailed,
	DuplicateSkipped,
	InvalidRow,
	LookupFailed,
	PrepareFailed,
}

impl PropertyWatcherEvent {
	pub fn as_str(self) -> &'static str {
		match self {
			PropertyWatcherEvent::CommitFailed => "property_watcher.commit_failed",
			PropertyWatcherEvent::DuplicateSkipped => "property_watcher.duplicate_skipped",
			PropertyWatcherEvent::InvalidRow => "property_watcher.invalid_row",
			PropertyWatcherEvent::LookupFailed => "property_watcher.lookup_failed",
			PropertyWatcherEvent::PrepareFailed => "property_watcher.prepare_failed",
		}
	}
}

/// TS PropertyWatcherOperation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherOperation {
	Prepare,
	Lookup,
	Commit,
}

impl PropertyWatcherOperation {
	pub fn as_str(self) -> &'static str {
		match self {
			PropertyWatcherOperation::Prepare => "prepare",
			PropertyWatcherOperation::Lookup => "lookup",
			PropertyWatcherOperation::Commit => "commit",
		}
	}
}

/// TS가 `instanceof`로 구분하는 오류를 값으로 재현한다.
/// runtime의 `isRecoverableRuntimeError`가 같은 분기를 쓰도록 타입으로 보존한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatcherFailure {
	/// ProgressFileError — prepare/commit에서 복구 가능
	ProgressFile(ProgressFileErrorCode),
	/// KrasLookupError / KrasLookupTargetError — lookup에서 복구 가능
	KrasLookup,
	KrasLookupTarget,
	/// PropertyAddressError — 레이아웃 오류는 즉시 중단 대상
	Address(crate::property_address::PropertyAddressErrorCode),
	/// 그 외(로그 실패·해제 실패 등)
	Other(String),
}

impl WatcherFailure {
	/// TS runtime isRecoverableRuntimeError의 분기 그대로.
	pub fn is_recoverable(operation: PropertyWatcherOperation, failure: &WatcherFailure) -> bool {
		use PropertyWatcherOperation::*;
		match operation {
			Prepare | Commit => matches!(failure, WatcherFailure::ProgressFile(_)),
			Lookup => matches!(failure, WatcherFailure::KrasLookup | WatcherFailure::KrasLookupTarget),
		}
	}
}

impl std::fmt::Display for WatcherFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			WatcherFailure::ProgressFile(code) => {
				write!(f, "{}", crate::property_progress::ProgressFileError::new(*code))
			}
			WatcherFailure::KrasLookup => write!(f, "KRAS 건물 조회 평가에 실패했습니다."),
			WatcherFailure::KrasLookupTarget => write!(f, "KRAS 건물 조회 대상 페이지가 아닙니다."),
			WatcherFailure::Address(_) => write!(f, "주소 행을 해석할 수 없습니다."),
			WatcherFailure::Other(message) => f.write_str(message),
		}
	}
}

impl std::error::Error for WatcherFailure {}

impl From<PropertyAddressError> for WatcherFailure {
	fn from(e: PropertyAddressError) -> Self {
		WatcherFailure::Address(e.code)
	}
}

impl From<crate::property_progress::ProgressFileError> for WatcherFailure {
	fn from(e: crate::property_progress::ProgressFileError) -> Self {
		WatcherFailure::ProgressFile(e.code)
	}
}

/// TS InstalledPropertyWatcherTarget의 상태 갱신 핸들.
pub struct PropertyWatcherTarget<T> {
	pub target: T,
	/// TS updateBuildingStatus — 실패 시 오류가 호출자로 전파된다.
	pub update_building_status: Box<
		dyn FnMut(&PropertyWatcherSelectionHandle, PropertyBuildingTerminalStatus)
			-> Result<PropertyBuildingStatusUpdateResult, WatcherFailure>
			+ Send,
	>,
	/// TS dispose — stop()에서 호출된다.
	pub dispose: Box<dyn FnOnce() -> Result<(), WatcherFailure> + Send>,
}

/// TS PropertyWatcherDependencies의 동작부(비동기 → 동기 클로저).
pub struct PropertyWatcherCallbacks<T, U> {
	/// TS prepareProgress(address, pager)
	pub prepare_progress:
		Box<dyn FnMut(&ParcelAddress, &str) -> Result<U, WatcherFailure> + Send>,
	/// TS lookupKras(target, address)
	pub lookup_kras: Box<
		dyn FnMut(&T, &ParcelAddress) -> Result<PropertyBuildingTerminalStatus, WatcherFailure>
			+ Send,
	>,
	/// TS commitProgress(update)
	pub commit_progress: Box<dyn FnMut(&U) -> Result<(), WatcherFailure> + Send>,
	/// TS isRecoverableError(operation, error) — 기본값은 WatcherFailure::is_recoverable
	pub is_recoverable: Option<Box<dyn Fn(PropertyWatcherOperation, &WatcherFailure) -> bool + Send>>,
	/// TS logger.log(event, count)
	pub log: Box<dyn FnMut(PropertyWatcherEvent, u32) -> Result<(), WatcherFailure> + Send>,
}

struct LastSuccessful {
	address: ParcelAddress,
	status: PropertyBuildingTerminalStatus,
}

/// TS PropertyWatcher — 상태를 가진 감시자. enqueue로 행을 밀어 넣는다.
pub struct PropertyWatcher<T, U> {
	target: PropertyWatcherTarget<T>,
	callbacks: PropertyWatcherCallbacks<T, U>,
	accepting: bool,
	fatal: Option<WatcherFailure>,
	last_successful: Option<LastSuccessful>,
	/// TS stopped promise — 한 번 끝나면 Some(정상 종료는 Some(Ok))로 남는다.
	stopped: Option<Result<(), String>>,
}

impl<T, U> PropertyWatcher<T, U> {
	pub fn new(target: PropertyWatcherTarget<T>, callbacks: PropertyWatcherCallbacks<T, U>) -> Self {
		Self {
			target,
			callbacks,
			accepting: true,
			fatal: None,
			last_successful: None,
			stopped: None,
		}
	}

	pub fn is_accepting(&self) -> bool {
		self.accepting
	}

	pub fn fatal(&self) -> Option<&WatcherFailure> {
		self.fatal.as_ref()
	}

	pub fn target(&self) -> &T {
		&self.target.target
	}

	fn mark_fatal(&mut self, failure: WatcherFailure) {
		// TS: 첫 fatal만 기록하고 곧바로 중단에 들어간다
		if self.fatal.is_none() {
			self.fatal = Some(failure);
			self.accepting = false;
		}
	}

	fn recoverable(&self, operation: PropertyWatcherOperation, failure: &WatcherFailure) -> bool {
		match &self.callbacks.is_recoverable {
			Some(check) => check(operation, failure),
			None => WatcherFailure::is_recoverable(operation, failure),
		}
	}

	fn log(&mut self, event: PropertyWatcherEvent) {
		match (self.callbacks.log)(event, 1) {
			Ok(()) => {}
			Err(failure) => self.mark_fatal(failure),
		}
	}

	/// TS processAddress — 준비 → 조회 → 상태 기록 → 커밋.
	fn process_address(
		&mut self,
		address: ParcelAddress,
		selection: PropertyWatcherSelectionHandle,
		pager: &str,
	) -> Result<(), WatcherFailure> {
		// 중복: 직전 성공 주소와 같으면 상태만 갱신
		if let Some(last) = &self.last_successful {
			if last.address.as_str() == address.as_str() {
				let status = last.status;
				(self.target.update_building_status)(&selection, status)?;
				self.log(PropertyWatcherEvent::DuplicateSkipped);
				return Ok(());
			}
		}
		self.last_successful = None;

		// prepare
		let update = match (self.callbacks.prepare_progress)(&address, pager) {
			Ok(update) => update,
			Err(failure) => {
				if self.recoverable(PropertyWatcherOperation::Prepare, &failure) {
					(self.target.update_building_status)(&selection, PropertyBuildingTerminalStatus::Failed)?;
					self.log(PropertyWatcherEvent::PrepareFailed);
					return Ok(());
				}
				return Err(failure);
			}
		};

		// lookup
		let status = match (self.callbacks.lookup_kras)(&self.target.target, &address) {
			Ok(status) => status,
			Err(failure) => {
				if self.recoverable(PropertyWatcherOperation::Lookup, &failure) {
					(self.target.update_building_status)(&selection, PropertyBuildingTerminalStatus::Failed)?;
					self.log(PropertyWatcherEvent::LookupFailed);
					return Ok(());
				}
				return Err(failure);
			}
		};

		// 상태 기록
		(self.target.update_building_status)(&selection, status)?;
		match status {
			PropertyBuildingTerminalStatus::Failed => {
				self.log(PropertyWatcherEvent::LookupFailed);
				return Ok(());
			}
			PropertyBuildingTerminalStatus::Absent | PropertyBuildingTerminalStatus::Present => {}
		}

		// commit
		if let Err(failure) = (self.callbacks.commit_progress)(&update) {
			if self.recoverable(PropertyWatcherOperation::Commit, &failure) {
				self.log(PropertyWatcherEvent::CommitFailed);
				return Ok(());
			}
			return Err(failure);
		}
		self.last_successful = Some(LastSuccessful { address, status });
		Ok(())
	}

	/// TS enqueue — 행 파싱 후 (fatal이 없으면) 처리를 실행한다.
	pub fn enqueue(
		&mut self,
		row: &[String; 8],
		selection: PropertyWatcherSelectionHandle,
		pager: String,
	) {
		if !self.accepting {
			return;
		}
		let cells: Vec<Option<&str>> = row.iter().map(|cell| Some(cell.as_str())).collect();
		let address = match parse_property_row(&cells) {
			Ok(address) => address,
			// TS: error instanceof PropertyAddressError → invalidRow 로그 후 무시
			Err(_error) => {
				self.log(PropertyWatcherEvent::InvalidRow);
				return;
			}
		};
		if self.fatal.is_some() {
			return;
		}
		if let Err(failure) = self.process_address(address, selection, &pager) {
			self.mark_fatal(failure);
		}
	}

	/// TS stop() — 해제 후 fatal과 해제 실패를 합쳐 보고한다(AggregateError 대응).
	/// TS stop() — 소유권을 넘겨 받아 한 번만 정리한다.
	pub fn stop(mut self) -> Result<(), WatcherFailure> {
		self.finish_stop()
	}

	/// 정리 본체 — 소유권 없이도 호출할 수 있게 분리(request_stop에서 재사용).
	fn finish_stop(&mut self) -> Result<(), WatcherFailure> {
		self.accepting = false;
		// dispose는 FnOnce라 한 번만 호출할 수 있으니 교체해 둔다
		let dispose = std::mem::replace(&mut self.target.dispose, Box::new(|| Ok(())));
		let cleanup = dispose();
		match (self.fatal.take(), cleanup) {
			(Some(fatal), Ok(())) => Err(fatal),
			(None, Err(cleanup)) => Err(cleanup),
			(Some(fatal), Err(cleanup)) => Err(WatcherFailure::Other(format!(
				"Property watcher failed and disposal also failed. fatal={fatal}; dispose={cleanup}"
			))),
			(None, Ok(())) => Ok(()),
		}
	}

	/// TS watcher.stop() — 라이프사이클이 소유권 없이 호출할 때 쓴다. 결과를 남긴다.
	pub fn request_stop(&mut self) -> Result<(), String> {
		if let Some(previous) = &self.stopped {
			return previous.clone();
		}
		let outcome = self.finish_stop().map_err(|e| e.to_string());
		self.stopped = Some(outcome.clone());
		outcome
	}

	/// TS watcher.stopped — 이미 끝났다면 Some(한 번 꺼내면 비워진다).
	pub fn take_stop_outcome(&mut self) -> Option<Result<(), String>> {
		self.stopped.take()
	}

	/// 아직 정리되지 않았는지(TS watcherState === "active")
	pub fn is_running(&self) -> bool {
		self.stopped.is_none()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{Arc, Mutex};

	fn row(cell6: &str) -> [String; 8] {
		[
			"1".into(), "2".into(), "3".into(), "4".into(),
			"5".into(), "6".into(), cell6.into(), "8".into(),
		]
	}

	fn handle(selection_id: u64) -> PropertyWatcherSelectionHandle {
		PropertyWatcherSelectionHandle {
			context_id: 1,
			context_unique_id: "ctx".to_string(),
			generation: 1,
			selection_id,
		}
	}

	/// 테스트용 표준 감시자 — 호출 순서와 로그를 기록한다.
	fn build(probe: ProbeInner) -> (
		PropertyWatcher<String, String>,
		Arc<Mutex<Vec<String>>>,
		Arc<Mutex<Vec<(&'static str, u64)>>>,
	) {
		let events = probe.events.clone();
		let statuses = probe.statuses.clone();
		let events_for_log = events.clone();
		let statuses_for_update = statuses.clone();
		let prepare_fail = probe.prepare_fail.clone();
		let lookup_fail = probe.lookup_fail.clone();
		let commit_fail = probe.commit_fail.clone();
		let lookup_status = probe.lookup_status;
		let dispose_fail = probe.dispose_fail;

		let target = PropertyWatcherTarget {
			target: "target".to_string(),
			update_building_status: Box::new(move |_sel, status| {
				statuses_for_update.lock().unwrap().push((status.as_str(), 0));
				Ok(PropertyBuildingStatusUpdateResult::Applied)
			}),
			dispose: Box::new(move || {
				if dispose_fail {
					Err(WatcherFailure::Other("해제 실패".to_string()))
				} else {
					Ok(())
				}
			}),
		};

		let callbacks = PropertyWatcherCallbacks {
			prepare_progress: Box::new(move |_addr, _pager| match &prepare_fail {
				Some(failure) => Err(failure.clone()),
				None => Ok("update".to_string()),
			}),
			lookup_kras: Box::new(move |_target, _addr| match &lookup_fail {
				Some(failure) => Err(failure.clone()),
				None => Ok(lookup_status),
			}),
			commit_progress: Box::new(move |_update| match &commit_fail {
				Some(failure) => Err(failure.clone()),
				None => Ok(()),
			}),
			is_recoverable: None,
			log: Box::new(move |event, count| {
				events_for_log.lock().unwrap().push(format!("{}:{}", event.as_str(), count));
				Ok(())
			}),
		};
		let watcher = PropertyWatcher::new(target, callbacks);
		(watcher, events, statuses)
	}

	#[derive(Clone)]
	struct ProbeInner {
		events: Arc<Mutex<Vec<String>>>,
		statuses: Arc<Mutex<Vec<(&'static str, u64)>>>,
		prepare_fail: Option<WatcherFailure>,
		lookup_fail: Option<WatcherFailure>,
		commit_fail: Option<WatcherFailure>,
		lookup_status: PropertyBuildingTerminalStatus,
		dispose_fail: bool,
	}

	fn probe() -> ProbeInner {
		ProbeInner {
			events: Arc::new(Mutex::new(Vec::new())),
			statuses: Arc::new(Mutex::new(Vec::new())),
			prepare_fail: None,
			lookup_fail: None,
			commit_fail: None,
			lookup_status: PropertyBuildingTerminalStatus::Present,
			dispose_fail: false,
		}
	}

	#[test]
	fn event_names_match_ts_constants() {
		assert_eq!(PropertyWatcherEvent::CommitFailed.as_str(), "property_watcher.commit_failed");
		assert_eq!(PropertyWatcherEvent::DuplicateSkipped.as_str(), "property_watcher.duplicate_skipped");
		assert_eq!(PropertyWatcherEvent::InvalidRow.as_str(), "property_watcher.invalid_row");
		assert_eq!(PropertyWatcherEvent::LookupFailed.as_str(), "property_watcher.lookup_failed");
		assert_eq!(PropertyWatcherEvent::PrepareFailed.as_str(), "property_watcher.prepare_failed");
		assert_eq!(PropertyWatcherOperation::Prepare.as_str(), "prepare");
		assert_eq!(PropertyWatcherOperation::Lookup.as_str(), "lookup");
		assert_eq!(PropertyWatcherOperation::Commit.as_str(), "commit");
	}

	#[test]
	fn recoverability_matches_runtime_rules() {
		let file = WatcherFailure::ProgressFile(ProgressFileErrorCode::IoFailure);
		assert!(WatcherFailure::is_recoverable(PropertyWatcherOperation::Prepare, &file));
		assert!(WatcherFailure::is_recoverable(PropertyWatcherOperation::Commit, &file));
		assert!(!WatcherFailure::is_recoverable(PropertyWatcherOperation::Lookup, &file));

		assert!(WatcherFailure::is_recoverable(PropertyWatcherOperation::Lookup, &WatcherFailure::KrasLookup));
		assert!(WatcherFailure::is_recoverable(PropertyWatcherOperation::Lookup, &WatcherFailure::KrasLookupTarget));
		assert!(!WatcherFailure::is_recoverable(PropertyWatcherOperation::Prepare, &WatcherFailure::KrasLookup));
		assert!(!WatcherFailure::is_recoverable(PropertyWatcherOperation::Commit, &WatcherFailure::KrasLookup));
		assert!(!WatcherFailure::is_recoverable(PropertyWatcherOperation::Lookup, &WatcherFailure::Other("x".into())));
	}

	#[test]
	fn success_commits_and_records_status() {
		let p = probe();
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert!(watcher.fatal().is_none());
		assert!(events.lock().unwrap().is_empty(), "성공 시 로그가 없다: {:?}", events.lock().unwrap());
		assert_eq!(*statuses.lock().unwrap(), vec![("present", 0u64)]);
	}

	#[test]
	fn duplicate_address_skips_lookup_and_commit() {
		let p = probe();
		let (mut watcher, events, statuses) = build(p);
		let address = "서울특별시 노원구 월계동 392-19";
		watcher.enqueue(&row(address), handle(1), "1/20".into());
		let count_after_first = statuses.lock().unwrap().len();
		watcher.enqueue(&row(address), handle(2), "2/20".into());
		assert_eq!(
			*events.lock().unwrap(),
			vec!["property_watcher.duplicate_skipped:1".to_string()]
		);
		// 중복은 상태 기록만 추가한다 (prepare/lookup/commit 없음)
		assert_eq!(statuses.lock().unwrap().len(), count_after_first + 1);
		assert!(watcher.fatal().is_none());
	}

	#[test]
	fn invalid_row_logs_and_skips_processing() {
		let p = probe();
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("이상한주소"), handle(1), "1/20".into());
		assert_eq!(*events.lock().unwrap(), vec!["property_watcher.invalid_row:1".to_string()]);
		assert!(statuses.lock().unwrap().is_empty(), "처리하지 않는다");
		assert!(watcher.fatal().is_none(), "레이아웃 오류는 fatal이 아니다");
	}

	#[test]
	fn recoverable_prepare_failure_marks_failed_and_stops_flow() {
		let p = ProbeInner {
			prepare_fail: Some(WatcherFailure::ProgressFile(ProgressFileErrorCode::SourceChanged)),
			..probe()
		};
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert_eq!(*events.lock().unwrap(), vec!["property_watcher.prepare_failed:1".to_string()]);
		assert_eq!(*statuses.lock().unwrap(), vec![("failed", 0u64)]);
		assert!(watcher.fatal().is_none(), "복구 가능 오류는 fatal이 아니다");
	}

	#[test]
	fn unrecoverable_prepare_failure_becomes_fatal_and_blocks_next_rows() {
		let p = ProbeInner {
			prepare_fail: Some(WatcherFailure::Other("치명적 준비 오류".into())),
			..probe()
		};
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert!(watcher.fatal().is_some());
		assert!(!watcher.is_accepting(), "fatal 후 수신 중단");
		assert!(events.lock().unwrap().is_empty());
		assert!(statuses.lock().unwrap().is_empty());

		// 그 다음 행은 무시된다
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-20"), handle(2), "2/20".into());
		assert!(statuses.lock().unwrap().is_empty(), "fatal 이후 처리하지 않는다");
	}

	#[test]
	fn recoverable_lookup_failure_marks_failed() {
		let p = ProbeInner {
			lookup_fail: Some(WatcherFailure::KrasLookup),
			..probe()
		};
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert_eq!(*events.lock().unwrap(), vec!["property_watcher.lookup_failed:1".to_string()]);
		assert_eq!(*statuses.lock().unwrap(), vec![("failed", 0u64)]);
		assert!(watcher.fatal().is_none());
	}

	#[test]
	fn lookup_status_failed_logs_lookup_failed_without_commit() {
		let p = ProbeInner {
			lookup_status: PropertyBuildingTerminalStatus::Failed,
			..probe()
		};
		let (mut watcher, events, _statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert_eq!(*events.lock().unwrap(), vec!["property_watcher.lookup_failed:1".to_string()]);
		// commit은 실패하지 않았으므로 fatal 없음
		assert!(watcher.fatal().is_none());
	}

	#[test]
	fn lookup_status_absent_is_treated_as_success_path() {
		let p = ProbeInner {
			lookup_status: PropertyBuildingTerminalStatus::Absent,
			..probe()
		};
		let (mut watcher, events, statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert!(events.lock().unwrap().is_empty(), "absent는 실패 로그가 아니다: {:?}", events.lock().unwrap());
		assert_eq!(*statuses.lock().unwrap(), vec![("absent", 0u64)]);
		assert!(watcher.fatal().is_none());
	}

	#[test]
	fn recoverable_commit_failure_logs_and_does_not_record_success() {
		let p = ProbeInner {
			commit_fail: Some(WatcherFailure::ProgressFile(ProgressFileErrorCode::IoFailure)),
			..probe()
		};
		let (mut watcher, events, _statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		assert_eq!(*events.lock().unwrap(), vec!["property_watcher.commit_failed:1".to_string()]);
		assert!(watcher.fatal().is_none());

		// 직전 성공이 없으므로 중복으로 분류되지 않는다
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(2), "2/20".into());
		let dedup_count = events
			.lock().unwrap()
			.iter()
			.filter(|e| e.starts_with("property_watcher.duplicate_skipped"))
			.count();
		assert_eq!(dedup_count, 0, "커밋 실패는 성공으로 기록되지 않는다");
	}

	#[test]
	fn unrecoverable_lookup_failure_becomes_fatal() {
		let p = ProbeInner {
			lookup_fail: Some(WatcherFailure::Other("알 수 없는 조회 오류".into())),
			..probe()
		};
		let (mut watcher, _events, _statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		let fatal = watcher.fatal().expect("fatal");
		assert_eq!(fatal.to_string(), "알 수 없는 조회 오류");
	}

	#[test]
	fn stop_returns_fatal_error_only() {
		let p = ProbeInner {
			prepare_fail: Some(WatcherFailure::Other("치명적 준비 오류".into())),
			..probe()
		};
		let (watcher, _events, _statuses) = build(p);
		let mut watcher = watcher;
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		let err = watcher.stop().expect_err("fatal 전파");
		assert_eq!(err.to_string(), "치명적 준비 오류");
	}

	#[test]
	fn stop_returns_cleanup_error_only() {
		let p = ProbeInner { dispose_fail: true, ..probe() };
		let (watcher, _events, _statuses) = build(p);
		let err = watcher.stop().expect_err("해제 실패 전파");
		assert_eq!(err.to_string(), "해제 실패");
	}

	#[test]
	fn stop_aggregates_fatal_and_cleanup_failures() {
		let p = ProbeInner {
			prepare_fail: Some(WatcherFailure::Other("치명적 준비 오류".into())),
			dispose_fail: true,
			..probe()
		};
		let (mut watcher, _events, _statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		let err = watcher.stop().expect_err("둘 다 실패");
		let message = err.to_string();
		assert!(message.contains("치명적 준비 오류"), "{message}");
		assert!(message.contains("해제 실패"), "{message}");
		assert!(message.contains("disposal also failed"), "TS AggregateError 문구: {message}");
	}

	#[test]
	fn clean_stop_is_ok() {
		let p = probe();
		let (mut watcher, _events, _statuses) = build(p);
		watcher.enqueue(&row("서울특별시 노원구 월계동 392-19"), handle(1), "1/20".into());
		watcher.stop().expect("깔끔한 종료");
	}
}

/// 라이프사이클이 워처를 다루도록 어댑터한다(TS watcher.stopped/stop 대응).
impl<T: Send, U: Send> crate::property_watcher_lifecycle::LifecycleWatcher
	for PropertyWatcher<T, U>
{
	fn take_stop_outcome(&mut self) -> Option<Result<(), String>> {
		PropertyWatcher::take_stop_outcome(self)
	}

	fn request_stop(&mut self) -> Result<(), String> {
		PropertyWatcher::request_stop(self)
	}

	fn is_active(&self) -> bool {
		self.is_running()
	}
}
