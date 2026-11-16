// TS scripts/property-watcher-process.ts — SCP 워처 프로세스 PID 기록·신원 검증·예약.

use std::path::Path;

/// TS PROPERTY_WATCHER_ARTIFACT_DIRECTORY
pub const PROPERTY_WATCHER_ARTIFACT_DIRECTORY: &str =
	"C:\\Users\\admin\\AppData\\Local\\Temp\\opencode";
/// TS PROPERTY_WATCHER_PID_FILE
pub const PROPERTY_WATCHER_PID_FILE: &str =
	"C:\\Users\\admin\\AppData\\Local\\Temp\\opencode\\property-watcher.pid.json";
/// TS PROPERTY_WATCHER_LOG_FILE
pub const PROPERTY_WATCHER_LOG_FILE: &str =
	"C:\\Users\\admin\\AppData\\Local\\Temp\\opencode\\property-watcher.log";
/// TS PROPERTY_WATCHER_TOKEN_ENV
pub const PROPERTY_WATCHER_TOKEN_ENV: &str = "PROPERTY_WATCHER_INSTANCE_TOKEN";

/// TS PropertyWatcherProcessErrorCode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherProcessErrorCode {
	ArtifactDirectoryMissing,
	IdentityMismatch,
	PidAlreadyReserved,
	PidRecordInvalid,
	PidRecordMissing,
	SpawnFailed,
}

impl PropertyWatcherProcessErrorCode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::ArtifactDirectoryMissing => "ARTIFACT_DIRECTORY_MISSING",
			Self::IdentityMismatch => "IDENTITY_MISMATCH",
			Self::PidAlreadyReserved => "PID_ALREADY_RESERVED",
			Self::PidRecordInvalid => "PID_RECORD_INVALID",
			Self::PidRecordMissing => "PID_RECORD_MISSING",
			Self::SpawnFailed => "SPAWN_FAILED",
		}
	}
}

/// TS PropertyWatcherProcessError — 메시지가 코드 문자열 그 자체다(`super(code)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherProcessError {
	pub code: PropertyWatcherProcessErrorCode,
}

impl PropertyWatcherProcessError {
	pub fn new(code: PropertyWatcherProcessErrorCode) -> Self {
		Self { code }
	}
}

impl std::fmt::Display for PropertyWatcherProcessError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(self.code.as_str())
	}
}

impl std::error::Error for PropertyWatcherProcessError {}

/// TS pidRecordSchema의 `state` 리터널.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WatcherPidState {
	Starting,
	Ready,
}

impl WatcherPidState {
	pub fn as_str(self) -> &'static str {
		match self {
			WatcherPidState::Starting => "starting",
			WatcherPidState::Ready => "ready",
		}
	}
}

/// TS WatcherPidRecord (strict 스키마).
/// 필드명은 TS 스키마와 같아야 한다(executablePath/runnerPath/instanceToken/...).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherPidRecord {
	pub version: u8,
	pub state: WatcherPidState,
	pub pid: u64,
	pub executable_path: String,
	pub runner_path: String,
	pub instance_token: String,
	pub creation_identity: String,
	pub pipe_name: String,
}

/// TS WatcherProcessSnapshot (strict, nullable 응답의 non-null).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatcherProcessSnapshot {
	pub pid: u64,
	pub executable_path: String,
	pub command_line: String,
	pub creation_identity: String,
}

/// TS watcherPipeName
pub fn watcher_pipe_name(instance_token: &str) -> String {
	format!("\\\\.\\pipe\\opencode-property-watcher-{instance_token}")
}

/// zod `z.uuid()` — 8-4-4-4-12, 버전 1~8, RFC 변이 8/9/a/b.
pub fn is_uuid(value: &str) -> bool {
	let bytes = value.as_bytes();
	if value.len() != 36
		|| bytes[8] != b'-'
		|| bytes[13] != b'-'
		|| bytes[18] != b'-'
		|| bytes[23] != b'-'
	{
		return false;
	}
	let hex = |range: std::ops::Range<usize>| -> bool {
		bytes[range].iter().all(|b| b.is_ascii_hexdigit())
	};
	if !(hex(0..8) && hex(9..13) && hex(14..18) && hex(19..23) && hex(24..36)) {
		return false;
	}
	matches!(bytes[14], b'1'..=b'8')
		&& matches!(bytes[19] | 0x20, b'8' | b'9' | b'a' | b'b')
}

const PID_RECORD_KEYS: [&str; 8] = [
	"version",
	"state",
	"pid",
	"executablePath",
	"runnerPath",
	"instanceToken",
	"creationIdentity",
	"pipeName",
];

fn invalid() -> PropertyWatcherProcessError {
	PropertyWatcherProcessError::new(PropertyWatcherProcessErrorCode::PidRecordInvalid)
}

fn str_field<'a>(object: &'a serde_json::Map<String, serde_json::Value>, key: &str) -> &'a str {
	object.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// TS parseWatcherPidRecord — strict 스키마 + pipeName 재계산 일치 검사.
pub fn parse_watcher_pid_record(
	value: &serde_json::Value,
) -> Result<WatcherPidRecord, PropertyWatcherProcessError> {
	let Some(object) = value.as_object() else {
		return Err(invalid());
	};
	// strict: 허용 키 8개와 정확히 일치해야 한다
	if object.len() != PID_RECORD_KEYS.len()
		|| !PID_RECORD_KEYS.iter().all(|key| object.contains_key(*key))
	{
		return Err(invalid());
	}
	// version: z.number().int().literal(1)
	if object.get("version").and_then(|v| v.as_u64()) != Some(1) {
		return Err(invalid());
	}
	let state = match object.get("state").and_then(|v| v.as_str()) {
		Some("starting") => WatcherPidState::Starting,
		Some("ready") => WatcherPidState::Ready,
		_ => return Err(invalid()),
	};
	// pid: int positive
	let pid = match object.get("pid").and_then(|v| v.as_u64()) {
		Some(pid) if pid > 0 => pid,
		_ => return Err(invalid()),
	};
	let executable_path = str_field(object, "executablePath");
	let runner_path = str_field(object, "runnerPath");
	let instance_token = str_field(object, "instanceToken");
	let creation_identity = str_field(object, "creationIdentity");
	let pipe_name = str_field(object, "pipeName");
	// z.string().min(1)
	if executable_path.is_empty()
		|| runner_path.is_empty()
		|| instance_token.is_empty()
		|| creation_identity.is_empty()
		|| pipe_name.is_empty()
	{
		return Err(invalid());
	}
	// z.uuid()
	if !is_uuid(instance_token) {
		return Err(invalid());
	}
	// 파이프 이름은 토큰에서 재계산해야 한다
	if pipe_name != watcher_pipe_name(instance_token) {
		return Err(invalid());
	}
	Ok(WatcherPidRecord {
		version: 1,
		state,
		pid,
		executable_path: executable_path.to_string(),
		runner_path: runner_path.to_string(),
		instance_token: instance_token.to_string(),
		creation_identity: creation_identity.to_string(),
		pipe_name: pipe_name.to_string(),
	})
}

/// TS normalizedPath — `/`→`\` + 소문자(en-US).
pub fn normalized_path(value: &str) -> String {
	value.replace('/', "\\").to_lowercase()
}

/// TS matchesWatcherProcessIdentity — pid·실행파일·식별자·명령행 포함 조건을 모두 검사한다.
pub fn matches_watcher_process_identity(
	record: &WatcherPidRecord,
	snapshot: &WatcherProcessSnapshot,
) -> bool {
	let command = normalized_path(&snapshot.command_line);
	record.pid == snapshot.pid
		&& normalized_path(&record.executable_path) == normalized_path(&snapshot.executable_path)
		&& record.creation_identity == snapshot.creation_identity
		&& command.contains(&normalized_path(&record.runner_path))
		&& command.contains(" watch")
		&& command.contains(&format!(
			"--title=opencode-property-watcher:{}",
			record.instance_token
		))
}

/// TS sameWatcherInstance
pub fn same_watcher_instance(left: &WatcherPidRecord, right: &WatcherPidRecord) -> bool {
	left.pid == right.pid
		&& left.instance_token == right.instance_token
		&& left.creation_identity == right.creation_identity
}

/// TS sameWatcherPidRecord
pub fn same_watcher_pid_record(left: &WatcherPidRecord, right: &WatcherPidRecord) -> bool {
	same_watcher_instance(left, right)
		&& left.version == right.version
		&& left.state == right.state
		&& left.executable_path == right.executable_path
		&& left.runner_path == right.runner_path
		&& left.pipe_name == right.pipe_name
}

/// TS INSPECT_PROCESS_SCRIPT (원문 유지 — PS -Command 인자로 그대로 넘긴다).
pub const INSPECT_PROCESS_SCRIPT: &str = "& { param([int]$targetPid);[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$process=Get-CimInstance Win32_Process -Filter ('ProcessId = ' + $targetPid);if($null -eq $process){'null'}else{[pscustomobject]@{pid=[int]$process.ProcessId;executablePath=[string]$process.ExecutablePath;commandLine=[string]$process.CommandLine;creationIdentity=[string]$process.CreationDate}|ConvertTo-Json -Compress} }";

fn identity_mismatch() -> PropertyWatcherProcessError {
	PropertyWatcherProcessError::new(PropertyWatcherProcessErrorCode::IdentityMismatch)
}

/// TS processSnapshotSchema 파싱 — `null`이면 None, 형식이 깨지면 IDENTITY_MISMATCH.
pub fn parse_process_snapshot(
	value: Option<&serde_json::Value>,
) -> Result<Option<WatcherProcessSnapshot>, PropertyWatcherProcessError> {
	let Some(value) = value else {
		return Ok(None);
	};
	// TS 스키마가 nullable이라 JSON `null`은 성공이며 null을 돌려준다
	if value.is_null() {
		return Ok(None);
	}
	let Some(object) = value.as_object() else {
		return Err(identity_mismatch());
	};
	const KEYS: [&str; 4] = ["pid", "executablePath", "commandLine", "creationIdentity"];
	if object.len() != KEYS.len() || !KEYS.iter().all(|k| object.contains_key(*k)) {
		return Err(identity_mismatch());
	}
	let pid = match object.get("pid").and_then(|v| v.as_u64()) {
		Some(pid) if pid > 0 => pid,
		_ => return Err(identity_mismatch()),
	};
	let executable_path = str_field(object, "executablePath");
	let command_line = str_field(object, "commandLine");
	let creation_identity = str_field(object, "creationIdentity");
	if executable_path.is_empty() || command_line.is_empty() || creation_identity.is_empty() {
		return Err(identity_mismatch());
	}
	Ok(Some(WatcherProcessSnapshot {
		pid,
		executable_path: executable_path.to_string(),
		command_line: command_line.to_string(),
		creation_identity: creation_identity.to_string(),
	}))
}

/// TS inspectWatcherProcess — PowerShell로 프로세스 신원을 조회한다.
pub fn inspect_watcher_process(
	pid: u64,
) -> Result<Option<WatcherProcessSnapshot>, Box<dyn std::error::Error>> {
	let output = std::process::Command::new("powershell.exe")
		.args(["-NoProfile", "-NonInteractive", "-Command", INSPECT_PROCESS_SCRIPT, &pid.to_string()])
		.output()
		.map_err(|e| -> Box<dyn std::error::Error> {
			let _ = e;
			Box::from(PropertyWatcherProcessError::new(
				PropertyWatcherProcessErrorCode::SpawnFailed,
			))
		})?;
	if !output.status.success() {
		return Err(Box::from(identity_mismatch()));
	}
	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
		Ok(v) => v,
		Err(_) => return Err(Box::from(identity_mismatch())),
	};
	parse_process_snapshot(Some(&parsed)).map_err(|e| Box::from(e) as Box<dyn std::error::Error>)
}

/// TS WatcherPidReservation의 핸들 조작 — 예약 파일을 다루는 타입이 공통 계약을 갖도록 한다.
pub trait PidReservationOps {
	fn write(&mut self, contents: &str) -> Result<(), Box<dyn std::error::Error>>;
	fn sync(&mut self) -> Result<(), Box<dyn std::error::Error>>;
	fn close(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// TS WatcherPidReservation — O_EXCL로 예약한 PID 파일 핸들.
pub struct WatcherPidReservation {
	file: Option<std::fs::File>,
}

impl std::fmt::Debug for WatcherPidReservation {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WatcherPidReservation").finish_non_exhaustive()
	}
}

impl PidReservationOps for WatcherPidReservation {
	fn write(&mut self, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
		use std::io::Write;
		let file = self.file.as_mut().ok_or("이미 닫힌 예약 파일")?;
		file.write_all(contents.as_bytes())?;
		Ok(())
	}

	fn sync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		if let Some(file) = self.file.as_mut() {
			file.sync_all()?;
		}
		Ok(())
	}

	fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		// Option 교체로 핸들을 닫는다
		if let Some(file) = self.file.take() {
			drop(file);
		}
		Ok(())
	}
}

/// TS reserveWatcherPidFile — 이미 있으면 PID_ALREADY_RESERVED.
pub fn reserve_watcher_pid_file(
	path: &Path,
) -> Result<WatcherPidReservation, Box<dyn std::error::Error>> {
	match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
		Ok(file) => Ok(WatcherPidReservation { file: Some(file) }),
		Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
			Err(Box::from(PropertyWatcherProcessError::new(
				PropertyWatcherProcessErrorCode::PidAlreadyReserved,
			)))
		}
		Err(e) => Err(Box::from(e)),
	}
}

/// TS ensureWatcherArtifactDirectory — ENOENT·디렉터리 아님 모두 ARTIFACT_DIRECTORY_MISSING.
pub fn ensure_watcher_artifact_directory(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
	ensure_watcher_artifact_directory_with(path, |p| std::fs::metadata(p).map(|m| m.is_dir()))
}

/// 주입 가능한 stat 버전(테스트용) — TS의 readStat 매개변수 대응.
pub fn ensure_watcher_artifact_directory_with<F>(
	path: &Path,
	read_stat: F,
) -> Result<(), Box<dyn std::error::Error>>
where
	F: FnOnce(&Path) -> Result<bool, std::io::Error>,
{
	match read_stat(path) {
		Ok(true) => Ok(()),
		Ok(false) => Err(Box::from(PropertyWatcherProcessError::new(
			PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing,
		))),
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(Box::from(
			PropertyWatcherProcessError::new(PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing),
		)),
		Err(e) => Err(Box::from(e)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn valid_record_json() -> serde_json::Value {
		let token = "123e4567-e89b-42d3-a456-426614174000";
		serde_json::json!({
			"version": 1,
			"state": "ready",
			"pid": 4242,
			"executablePath": "C:\\node.exe",
			"runnerPath": "C:\\app\\property-watcher-runner.ts",
			"instanceToken": token,
			"creationIdentity": "2026-11-14T00:00:00",
			"pipeName": watcher_pipe_name(token),
		})
	}

	fn record() -> WatcherPidRecord {
		parse_watcher_pid_record(&valid_record_json()).expect("valid record")
	}

	fn snapshot(pid: u64, executable: &str, command: &str, identity: &str) -> WatcherProcessSnapshot {
		WatcherProcessSnapshot {
			pid,
			executable_path: executable.to_string(),
			command_line: command.to_string(),
			creation_identity: identity.to_string(),
		}
	}

	#[test]
	fn constants_match_ts() {
		assert_eq!(
			PROPERTY_WATCHER_ARTIFACT_DIRECTORY,
			"C:\\Users\\admin\\AppData\\Local\\Temp\\opencode"
		);
		assert!(PROPERTY_WATCHER_PID_FILE.ends_with("\\property-watcher.pid.json"));
		assert!(PROPERTY_WATCHER_LOG_FILE.ends_with("\\property-watcher.log"));
		assert_eq!(PROPERTY_WATCHER_TOKEN_ENV, "PROPERTY_WATCHER_INSTANCE_TOKEN");
	}

	#[test]
	fn error_display_is_the_code_itself() {
		for code in [
			PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing,
			PropertyWatcherProcessErrorCode::IdentityMismatch,
			PropertyWatcherProcessErrorCode::PidAlreadyReserved,
			PropertyWatcherProcessErrorCode::PidRecordInvalid,
			PropertyWatcherProcessErrorCode::PidRecordMissing,
			PropertyWatcherProcessErrorCode::SpawnFailed,
		] {
			assert_eq!(PropertyWatcherProcessError::new(code).to_string(), code.as_str());
		}
	}

	#[test]
	fn pipe_name_follows_instance_token() {
		let token = "123e4567-e89b-42d3-a456-426614174000";
		assert_eq!(
			watcher_pipe_name(token),
			format!("\\\\.\\pipe\\opencode-property-watcher-{token}")
		);
	}

	#[test]
	fn parse_accepts_both_states_and_rebuilds_record() {
		for state in ["starting", "ready"] {
			let mut json = valid_record_json();
			json["state"] = serde_json::json!(state);
			let parsed = parse_watcher_pid_record(&json).expect("state 허용");
			assert_eq!(parsed.version, 1);
			assert_eq!(parsed.state.as_str(), state);
			assert_eq!(parsed.pid, 4242);
			assert_eq!(parsed.version, 1);
		}
	}

	#[test]
	fn parse_rejects_shape_violations() {
		let mut case = valid_record_json();
		case["version"] = serde_json::json!(2);
		assert_eq!(
			parse_watcher_pid_record(&case).unwrap_err().code,
			PropertyWatcherProcessErrorCode::PidRecordInvalid
		);

		let mut case = valid_record_json();
		case["state"] = serde_json::json!("running");
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		let mut case = valid_record_json();
		case["pid"] = serde_json::json!(0);
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		let mut case = valid_record_json();
		case["pid"] = serde_json::json!(1.5);
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		let mut case = valid_record_json();
		case["executablePath"] = serde_json::json!("");
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		let mut case = valid_record_json();
		case["instanceToken"] = serde_json::json!("not-a-uuid");
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		// pipeName이 토큰과 불일치
		let mut case = valid_record_json();
		case["pipeName"] = serde_json::json!("\\\\.\\pipe\\other");
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		// strict — 키 추가
		let mut case = valid_record_json();
		case["extra"] = serde_json::json!(1);
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		// strict — 키 누락
		let mut case = valid_record_json();
		case.as_object_mut().unwrap().remove("pipeName");
		assert_eq!(parse_watcher_pid_record(&case).unwrap_err().code, PropertyWatcherProcessErrorCode::PidRecordInvalid);

		// 객체가 아님
		assert_eq!(
			parse_watcher_pid_record(&serde_json::json!([1, 2])).unwrap_err().code,
			PropertyWatcherProcessErrorCode::PidRecordInvalid
		);
	}

	#[test]
	fn uuid_validation_follows_rfc_shape() {
		assert!(is_uuid("123e4567-e89b-42d3-a456-426614174000"));
		assert!(is_uuid("123E4567-E89B-12D3-A456-426614174000"));
		assert!(!is_uuid("123e4567e89b42d3a456426614174000"), "구분자 없음");
		assert!(!is_uuid("123e4567-e89b-42d3-a456-42661417400"), "길이 35");
		assert!(!is_uuid("123e4567-e89b-92d3-a456-426614174000"), "버전 9 허용 안 함");
		assert!(!is_uuid("123e4567-e89b-42d3-c456-426614174000"), "변이 c 허용 안 함");
		assert!(!is_uuid("123g4567-e89b-42d3-a456-426614174000"), "비 hex");
	}

	#[test]
	fn normalized_path_folds_case_and_slashes() {
		assert_eq!(normalized_path("C:/App/Runner.TS"), "c:\\app\\runner.ts");
		assert_eq!(normalized_path("C:\\App\\Runner.TS"), "c:\\app\\runner.ts");
	}

	fn ready_record_with_runner() -> WatcherPidRecord {
		WatcherPidRecord {
			version: 1,
			state: WatcherPidState::Ready,
			pid: 777,
			executable_path: "C:\\node.exe".into(),
			runner_path: "C:\\app\\property-watcher-runner.ts".into(),
			instance_token: "123e4567-e89b-42d3-a456-426614174000".into(),
			creation_identity: "2026-11-14T00:00:00".into(),
			pipe_name: watcher_pipe_name("123e4567-e89b-42d3-a456-426614174000"),
		}
	}

	#[test]
	fn identity_match_requires_all_conditions() {
		let record = ready_record_with_runner();
		let good = snapshot(
			777,
			"c:\\node.exe",
			"C:\\node.exe C:\\APP\\PROPERTY-WATCHER-RUNNER.TS watch --title=opencode-property-watcher:123e4567-e89b-42d3-a456-426614174000",
			"2026-11-14T00:00:00",
		);
		assert!(matches_watcher_process_identity(&record, &good), "대소문자·슬래시 무시 일치");

		// pid 불일치
		let mut case = good.clone();
		case.pid = 778;
		assert!(!matches_watcher_process_identity(&record, &case));
		// 실행파일 불일치
		let mut case = good.clone();
		case.executable_path = "c:\\other.exe".into();
		assert!(!matches_watcher_process_identity(&record, &case));
		// 생성 식별자 불일치
		let mut case = good.clone();
		case.creation_identity = "other".into();
		assert!(!matches_watcher_process_identity(&record, &case));
		// runner 경로 미포함
		let mut case = good.clone();
		case.command_line = "C:\\node.exe other.ts watch --title=opencode-property-watcher:123e4567-e89b-42d3-a456-426614174000".into();
		assert!(!matches_watcher_process_identity(&record, &case));
		// " watch" 미포함
		let mut case = good.clone();
		case.command_line = "C:\\node.exe C:\\app\\property-watcher-runner.ts --title=opencode-property-watcher:123e4567-e89b-42d3-a456-426614174000".into();
		assert!(!matches_watcher_process_identity(&record, &case));
		// 타이틀 미포함
		let mut case = good.clone();
		case.command_line = "C:\\node.exe C:\\app\\property-watcher-runner.ts watch".into();
		assert!(!matches_watcher_process_identity(&record, &case));
	}

	#[test]
	fn same_instance_and_pid_record_equality() {
		let left = ready_record_with_runner();
		let mut right = left.clone();
		assert!(same_watcher_instance(&left, &right));
		assert!(same_watcher_pid_record(&left, &right));

		right.pid = 778;
		assert!(!same_watcher_instance(&left, &right));
		assert!(!same_watcher_pid_record(&left, &right));

		let mut right = left.clone();
		right.state = WatcherPidState::Starting;
		assert!(same_watcher_instance(&left, &right), "상태는 인스턴스 동일성에 영향 없음");
		assert!(!same_watcher_pid_record(&left, &right), "상태는 레코드 동일성에 영향 있음");

		let mut right = left.clone();
		right.instance_token = "123e4567-e89b-42d3-a456-426614174999".into();
		right.pipe_name = watcher_pipe_name("123e4567-e89b-42d3-a456-426614174999");
		assert!(!same_watcher_instance(&left, &right));
	}

	#[test]
	fn snapshot_parsing_handles_null_and_invalid() {
		assert_eq!(parse_process_snapshot(None).expect("null"), None);
		let valid = serde_json::json!({
			"pid": 9, "executablePath": "C:\\x.exe", "commandLine": "cmd", "creationIdentity": "id"
		});
		let parsed = parse_process_snapshot(Some(&valid)).expect("valid").expect("some");
		assert_eq!(parsed.pid, 9);

		// strict — 키 추가/누락/형식 오류
		let mut extra = valid.clone();
		extra["more"] = serde_json::json!(1);
		assert_eq!(
			parse_process_snapshot(Some(&extra)).unwrap_err().code,
			PropertyWatcherProcessErrorCode::IdentityMismatch
		);
		let mut missing = valid.clone();
		missing.as_object_mut().unwrap().remove("commandLine");
		assert_eq!(
			parse_process_snapshot(Some(&missing)).unwrap_err().code,
			PropertyWatcherProcessErrorCode::IdentityMismatch
		);
		let zero = serde_json::json!({
			"pid": 0, "executablePath": "C:\\x.exe", "commandLine": "cmd", "creationIdentity": "id"
		});
		assert_eq!(
			parse_process_snapshot(Some(&zero)).unwrap_err().code,
			PropertyWatcherProcessErrorCode::IdentityMismatch
		);
		assert_eq!(
			parse_process_snapshot(Some(&serde_json::json!("null"))).unwrap_err().code,
			PropertyWatcherProcessErrorCode::IdentityMismatch
		);

		// PowerShell이 출력하는 JSON null은 성공(스키마 nullable)
		assert_eq!(
			parse_process_snapshot(Some(&serde_json::Value::Null)).expect("json null"),
			None,
			"JSON null은 스냅샷 없음으로 해석된다"
		);
	}

	/// 실제 PowerShell 조회 — 없는 PID는 null을 돌려주어야 한다.
	#[cfg(windows)]
	#[test]
	fn inspect_watcher_process_returns_null_for_missing_pid() {
		let result = inspect_watcher_process(2_147_483_647).expect("PowerShell 실행");
		assert_eq!(result, None, "없는 PID는 null 스냅샷");
	}

	/// 실제 PowerShell 조회 — 존재하는 PID는 스냅샷을 돌려준다.
	#[cfg(windows)]
	#[test]
	fn inspect_watcher_process_reads_current_process() {
		let pid = std::process::id() as u64;
		let snapshot = inspect_watcher_process(pid)
			.expect("PowerShell 실행")
			.expect("현재 프로세스 존재");
		assert_eq!(snapshot.pid, pid);
		assert!(!snapshot.command_line.is_empty());
		assert!(!snapshot.creation_identity.is_empty());
	}

	#[test]
	fn reserve_pid_file_rejects_existing_file() {
		let dir = std::env::temp_dir().join("kras-pid-reserve");
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		let path = dir.join("property-watcher.pid.json");

		let mut reservation = reserve_watcher_pid_file(&path).expect("첫 예약 성공");
		reservation.write("{\"version\":1}").expect("쓰기");
		reservation.sync().expect("동기화");
		reservation.close().expect("닫기");
		assert!(path.exists());

		let err = reserve_watcher_pid_file(&path).expect_err("이미 존재");
		assert_eq!(
			err.to_string(),
			PropertyWatcherProcessErrorCode::PidAlreadyReserved.as_str()
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn ensure_artifact_directory_classifies_missing_and_non_directory() {
		let missing = std::path::Path::new("Z:\\definitely-not-here\\x");
		let err = ensure_watcher_artifact_directory_with(missing, |_| {
			Err(std::io::Error::new(std::io::ErrorKind::NotFound, "enoent"))
		})
		.expect_err("없음");
		assert_eq!(
			err.to_string(),
			PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing.as_str()
		);

		let not_dir = ensure_watcher_artifact_directory_with(missing, |_| Ok(false))
			.expect_err("디렉터리 아님");
		assert_eq!(
			not_dir.to_string(),
			PropertyWatcherProcessErrorCode::ArtifactDirectoryMissing.as_str()
		);

		ensure_watcher_artifact_directory_with(missing, |_| Ok(true)).expect("디렉터리면 통과");

		// 그 외 IO 오류는 그대로 전파한다
		let other = ensure_watcher_artifact_directory_with(missing, |_| {
			Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"))
		})
		.expect_err("권한 오류");
		assert!(other.to_string().contains("denied"), "{other}");
	}
}
