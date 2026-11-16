// TS scripts/property-watcher-control.ts — 워처 정지용 네이밍 파이프 서버·클라이언트.
// Windows 네이밍 파이프(std)로 옮기되 TS의 프로토콜 규칙을 그대로 재현한다:
//   요청은 "stop\n"(64바이트 이하·접두사), 응답은 "stopped\n" | "failed\n".

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Pipes::{
	ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW,
};

/// PIPE_ACCESS_DUPLEX — 양방향 바이트 파이프.
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
/// PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT (모두 0)
const PIPE_BYTE_WAIT: u32 = 0;

/// UTF-8 → NUL 종료 UTF-16 (CreateNamedPipeW용).
fn to_wide(value: &str) -> Vec<u16> {
	value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// TS createServer(pipeName) 대응 — 파이프 인스턴스를 하나 만든다.
fn create_pipe_instance(pipe_name: &str) -> std::io::Result<File> {
	let wide = to_wide(pipe_name);
	let handle = unsafe {
		CreateNamedPipeW(
			wide.as_ptr(),
			PIPE_ACCESS_DUPLEX,
			PIPE_BYTE_WAIT,
			1,
			4096,
			4096,
			0,
			std::ptr::null(),
		)
	};
	if handle == INVALID_HANDLE_VALUE {
		return Err(std::io::Error::last_os_error());
	}
	Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

/// TS server.listen(...) 대응 — 클라이언트가 붙을 때까지 기다린다.
///
/// Windows는 이전 클라이언트가 이미 끊긴 상태에서 ConnectNamedPipe를 부르면
/// ERROR_NO_DATA/ERROR_PIPE_NOT_CONNECTED를 돌려준다. TS의 리스너는 이 경우에도
/// 계속 대기하므로, 같은 인스턴스를 DisconnectNamedPipe로 되돌린 뒤 재시도한다.
fn connect_pipe_instance(instance: &File) -> std::io::Result<()> {
	const ERROR_PIPE_CONNECTED: u32 = 535;
	const ERROR_PIPE_LISTENING: u32 = 536;
	const ERROR_NO_DATA: u32 = 232;
	const ERROR_PIPE_NOT_CONNECTED: u32 = 233;
	let handle: HANDLE = instance.as_raw_handle() as HANDLE;
	for attempt in 0..500 {
		let ok = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) };
		if ok != 0 {
			return Ok(());
		}
		match unsafe { GetLastError() } {
			// 클라이언트가 이미 붙어 있는 경우도 성공이다(TS listen 콜백과 동일)
			ERROR_PIPE_CONNECTED => return Ok(()),
			// 수신 대기 상태 — 잠시 뒤 다시 호출하면 블로킹한다
			ERROR_PIPE_LISTENING => {
				std::thread::sleep(std::time::Duration::from_millis(5));
				continue;
			}
			// 이전 클라이언트가 끊긴 상태 — 인스턴스를 되돌린 뒤 재시도
			ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED => unsafe {
				DisconnectNamedPipe(handle);
			},
			other => return Err(std::io::Error::from_raw_os_error(other as i32)),
		}
		if attempt % 50 == 0 {
			std::thread::sleep(std::time::Duration::from_millis(10));
		}
	}
	Err(std::io::Error::new(
		std::io::ErrorKind::TimedOut,
		"named pipe connect retry exhausted",
	))
}

/// 파이프 인스턴스가 열릴 때까지 기다린다(연결하지 않는다).
/// 서버가 재수신 대기로 되돌아오는 순간을 기다릴 때 테스트가 쓴다.
pub fn wait_named_pipe_available(pipe_name: &str, timeout_ms: u32) -> bool {
	wait_named_pipe_available_code(pipe_name, timeout_ms) == 0
}

/// 진단용 — 성공이면 0, 실패면 GetLastError 값을 돌려준다.
pub fn wait_named_pipe_available_code(pipe_name: &str, timeout_ms: u32) -> u32 {
	use windows_sys::Win32::Foundation::SetLastError;
	unsafe { SetLastError(0) };
	let wide = to_wide(pipe_name);
	let ok = unsafe { WaitNamedPipeW(wide.as_ptr(), timeout_ms) != 0 };
	if ok {
		return 0;
	}
	unsafe { GetLastError() }
}

/// TS createConnection(pipeName) 대응 — 파이프 클라이언트를 연다.
fn open_pipe_client(pipe_name: &str) -> std::io::Result<File> {
	OpenOptions::new().read(true).write(true).open(pipe_name)
}
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// TS MAX_STOP_REQUEST_BYTES
pub const MAX_STOP_REQUEST_BYTES: usize = 64;

/// TS PropertyWatcherControlErrorCode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherControlErrorCode {
	PipeUnresponsive,
}

impl PropertyWatcherControlErrorCode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::PipeUnresponsive => "PIPE_UNRESPONSIVE",
		}
	}
}

/// TS PropertyWatcherControlError — 메시지가 코드 문자열 그 자체다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherControlError {
	pub code: PropertyWatcherControlErrorCode,
	/// TS의 `cause` (원인 메시지 보존용).
	pub cause: Option<String>,
}

impl PropertyWatcherControlError {
	pub fn new(code: PropertyWatcherControlErrorCode) -> Self {
		Self { code, cause: None }
	}

	pub fn with_cause(code: PropertyWatcherControlErrorCode, cause: impl std::fmt::Display) -> Self {
		Self { code, cause: Some(cause.to_string()) }
	}
}

impl std::fmt::Display for PropertyWatcherControlError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.code.as_str())?;
		if let Some(cause) = &self.cause {
			write!(f, ": {cause}")?;
		}
		Ok(())
	}
}

impl std::error::Error for PropertyWatcherControlError {}

fn unresponsive() -> PropertyWatcherControlError {
	PropertyWatcherControlError::new(PropertyWatcherControlErrorCode::PipeUnresponsive)
}

/// 길이 제한(64바이트 초과 시 무효)을 지키며 개행까지 읽는다.
/// 반환: (버퍼, 무효 여부, EOF 여부)
fn read_until_newline<R: Read>(reader: &mut R) -> std::io::Result<(Vec<u8>, bool, bool)> {
	let mut buffer = Vec::new();
	let mut byte = [0u8; 1];
	loop {
		let read = reader.read(&mut byte)?;
		if read == 0 {
			return Ok((buffer, false, true));
		}
		buffer.push(byte[0]);
		if buffer.len() > MAX_STOP_REQUEST_BYTES {
			return Ok((buffer, true, false));
		}
		if byte[0] == b'\n' {
			return Ok((buffer, false, false));
		}
	}
}

/// TS의 `"stop\n".startsWith(request)` — 접두사 검사.
fn is_stop_prefix(bytes: &[u8]) -> bool {
	b"stop\n".starts_with(bytes)
}

/// TS 응답 판정: `stopped\n` 성공 / `failed\n` 실패 / 둘 다 아닌 앞쪽이면 무효 / 더 필요하면 None.
enum ResponseState {
	Success,
	Failed,
	Invalid,
	NeedMore,
}

fn classify_response(buffer: &[u8]) -> ResponseState {
	if buffer == b"stopped\n" {
		return ResponseState::Success;
	}
	if buffer == b"failed\n" {
		return ResponseState::Failed;
	}
	if b"stopped\n".starts_with(buffer) || b"failed\n".starts_with(buffer) {
		return ResponseState::NeedMore;
	}
	ResponseState::Invalid
}

/// TS sendWatcherStop — "stop\n"을 보내고 응답을 기다린다(5초 제한).
pub fn send_watcher_stop(pipe_name: &str) -> Result<(), Box<dyn std::error::Error>> {
	let mut client = open_pipe_client(pipe_name)
		.map_err(|e| PropertyWatcherControlError::with_cause(
			PropertyWatcherControlErrorCode::PipeUnresponsive,
			e,
		))?;
	client
		.write_all(b"stop\n")
		.map_err(|e| PropertyWatcherControlError::with_cause(
			PropertyWatcherControlErrorCode::PipeUnresponsive,
			e,
		))?;
	let _ = client.flush();

	let (sender, receiver) = std::sync::mpsc::channel::<Result<(), PropertyWatcherControlError>>();
	// 읽기는 스레드에서 수행한다 — 블로킹 읽기에는 std가 타임아웃을 제공하지 않는다.
	std::thread::spawn(move || {
		let mut client = client;
		let mut response: Vec<u8> = Vec::new();
		let mut chunk = [0u8; 64];
		loop {
			let read = match client.read(&mut chunk) {
				Ok(0) => 0,
				Ok(n) => n,
				Err(_) => {
					let _ = sender.send(Err(unresponsive()));
					return;
				}
			};
			if read == 0 {
				// EOF: 결정되지 않았다면 무응답이다
				let _ = sender.send(Err(unresponsive()));
				return;
			}
			response.extend_from_slice(&chunk[..read]);
			if response.len() > MAX_STOP_REQUEST_BYTES {
				let _ = sender.send(Err(unresponsive()));
				return;
			}
			match classify_response(&response) {
				ResponseState::Success => {
					let _ = sender.send(Ok(()));
					return;
				}
				ResponseState::Failed => {
					let _ = sender.send(Err(unresponsive()));
					return;
				}
				ResponseState::Invalid => {
					let _ = sender.send(Err(unresponsive()));
					return;
				}
				ResponseState::NeedMore => continue,
			}
		}
	});

	match receiver.recv_timeout(Duration::from_secs(5)) {
		Ok(result) => result.map_err(|e| Box::from(e) as Box<dyn std::error::Error>),
		// 5초 제한(TS socket.setTimeout(5000)) — 읽기 스레드는 버려진다.
		Err(_) => Err(Box::from(unresponsive())),
	}
}

#[derive(Debug, Clone)]
enum StopOutcome {
	Success,
	Failed(String),
}

struct ServerState {
	/// Some이면 처리가 끝났다(Condvar로 알린다).
	outcome: Mutex<Option<Result<StopOutcome, PropertyWatcherControlError>>>,
	condvar: Condvar,
	accepted: AtomicBool,
	shutdown: AtomicBool,
	pipe_name: String,
	join: Mutex<Option<std::thread::JoinHandle<()>>>,
}

fn wait_for_outcome(
	state: &ServerState,
) -> Result<StopOutcome, PropertyWatcherControlError> {
	let mut guard = state.outcome.lock().unwrap_or_else(|e| e.into_inner());
	while guard.is_none() {
		guard = state
			.condvar
			.wait(guard)
			.unwrap_or_else(|e| e.into_inner());
	}
	guard.clone().unwrap_or(Err(unresponsive()))
}

fn finish(state: &ServerState, outcome: Result<StopOutcome, PropertyWatcherControlError>) {
	let mut guard = state.outcome.lock().unwrap_or_else(|e| e.into_inner());
	if guard.is_none() {
		*guard = Some(outcome);
	}
	state.condvar.notify_all();
}

/// TS WatcherStopServer — `stopped`와 `close` 두 가지 진행을 노출한다.
#[derive(Clone)]
pub struct WatcherStopServer {
	state: Arc<ServerState>,
}

impl WatcherStopServer {
	/// TS `stopped` promise — 처리가 끝날 때까지 기다린다.
	pub fn stopped(&self) -> Result<(), Box<dyn std::error::Error>> {
		match wait_for_outcome(&self.state) {
			Ok(StopOutcome::Success) => Ok(()),
			Ok(StopOutcome::Failed(message)) => Err(Box::from(message)),
			Err(error) => Err(Box::from(error)),
		}
	}

	/// TS `close()` — 수신 대기만 멈추고 정리한다(결과에 상관없이 닫기는 성공).
	pub fn close(&self) -> Result<(), Box<dyn std::error::Error>> {
		let was_accepted = self.state.accepted.load(Ordering::SeqCst);
		self.state.shutdown.store(true, Ordering::SeqCst);
		if !was_accepted {
			// 블로킹 accept를 깨우기 위해 자기 자신에 연결한다.
			if let Ok(mut client) = open_pipe_client(&self.state.pipe_name) {
				let _ = client.write_all(b"");
			}
			// 아무것도 안 보낸 연결도 수신부를 깨우면 충분하다
			let mut guard = self.state.outcome.lock().unwrap_or_else(|e| e.into_inner());
			if guard.is_none() {
				*guard = Some(Ok(StopOutcome::Success));
			}
			drop(guard);
			self.state.condvar.notify_all();
		}
		if let Some(handle) = self
			.state
			.join
			.lock()
			.unwrap_or_else(|e| e.into_inner())
			.take()
		{
			let _ = handle.join();
		}
		Ok(())
	}

	/// TS accepted 플래그 (테스트·진단용).
	pub fn accepted(&self) -> bool {
		self.state.accepted.load(Ordering::SeqCst)
	}
}

impl std::fmt::Debug for WatcherStopServer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("WatcherStopServer")
			.field("pipe_name", &self.state.pipe_name)
			.field("accepted", &self.accepted())
			.finish_non_exhaustive()
	}
}

impl Drop for WatcherStopServer {
	fn drop(&mut self) {
		// 사용자가 명시적으로 닫지 않았더라도 리스너를 남기지 않는다
		if self.state.join.lock().map(|g| g.is_some()).unwrap_or(false) {
			let _ = self.close();
		}
	}
}

impl std::fmt::Debug for ServerState {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ServerState")
			.field("pipe_name", &self.pipe_name)
			.finish_non_exhaustive()
	}
}

/// TS openWatcherStopServer — 파이프에서 정지 요청을 기다린다.
pub fn open_watcher_stop_server<F>(
	pipe_name: &str,
	on_stop: F,
) -> Result<WatcherStopServer, Box<dyn std::error::Error>>
where
	F: FnOnce() -> Result<(), String> + Send + 'static,
{
	let server = create_pipe_instance(pipe_name).map_err(|e| {
		PropertyWatcherControlError::with_cause(
			PropertyWatcherControlErrorCode::PipeUnresponsive,
			e,
		)
	})?;
	let state = Arc::new(ServerState {
		outcome: Mutex::new(None),
		condvar: Condvar::new(),
		accepted: AtomicBool::new(false),
		shutdown: AtomicBool::new(false),
		pipe_name: pipe_name.to_string(),
		join: Mutex::new(None),
	});
	let thread_state = state.clone();
	let handle = std::thread::spawn(move || {
		// 잘못된 요청은 끊고 같은 인스턴스로 다시 받는다(TS의 destroy + 재수신).
		// 인스턴스를 닫고 새로 만들면 그 틈에 들어오는 클라이언트가 파이프를 찾지 못한다.
		let mut server = server;
		let mut on_stop = Some(on_stop);
		loop {
			if thread_state.shutdown.load(Ordering::SeqCst) {
				finish(&thread_state, Ok(StopOutcome::Success));
				return;
			}
			// 블로킹 수신 대기 — close()의 자기 연결이 여기를 깨운다
			if connect_pipe_instance(&server).is_err() {
				finish(&thread_state, Err(unresponsive()));
				return;
			}
			if thread_state.shutdown.load(Ordering::SeqCst)
				&& !thread_state.accepted.load(Ordering::SeqCst)
			{
				finish(&thread_state, Ok(StopOutcome::Success));
				return;
			}

			let (bytes, invalid, eof) = match read_until_newline(&mut server) {
				Ok(value) => value,
				// Windows은 끊긴 파이프를 EOF(0) 대신 오류(BROKEN_PIPE 등)로 돌려준다.
				// 요청 수신 단계의 끊김은 무효 요청으로 간주하고 계속 받는다(TS와 동일).
				Err(_) => (Vec::new(), false, true),
			};

			// TS: 이미 수락된 뒤의 연결은 즉시 끊는다
			let duplicate = thread_state.accepted.load(Ordering::SeqCst);
			// TS: 64바이트 초과 또는 stop 요청이 아니면 끊고 다시 받는다
			let rejected = invalid || eof || !is_stop_prefix(&bytes) || bytes != b"stop\n";
			if duplicate || rejected {
				// 같은 인스턴스를 수신 대기 상태로 되돌린다
				unsafe { DisconnectNamedPipe(server.as_raw_handle() as HANDLE) };
				continue;
			}

			thread_state.accepted.store(true, Ordering::SeqCst);
			let Some(callback) = on_stop.take() else {
				finish(&thread_state, Err(unresponsive()));
				return;
			};
			match callback() {
				Ok(()) => {
					// TS: 5초 안에 클라이언트가 끝을 받아야 성공
					let _ = server.write_all(b"stopped\n");
					let _ = server.flush();
					let mut eof_seen = false;
					let mut buf = [0u8; 1];
					let deadline = std::time::Instant::now() + Duration::from_secs(5);
					while std::time::Instant::now() < deadline {
						match server.read(&mut buf) {
							Ok(0) => {
								eof_seen = true;
								break;
							}
							Ok(_) => continue,
							Err(_) => break,
						}
					}
					if eof_seen {
						finish(&thread_state, Ok(StopOutcome::Success));
					} else {
						finish(&thread_state, Err(unresponsive()));
					}
					return;
				}
				Err(message) => {
					let _ = server.write_all(b"failed\n");
					let _ = server.flush();
					finish(&thread_state, Ok(StopOutcome::Failed(message)));
					return;
				}
			}
		}
	});
	state
		.join
		.lock()
		.unwrap_or_else(|e| e.into_inner())
		.replace(handle);
	Ok(WatcherStopServer { state })
}

#[cfg(test)]
mod tests {
	use super::*;

	fn unique_pipe(tag: &str) -> String {
		format!(
			"\\\\.\\pipe\\opencode-control-test-{}-{tag}",
			std::process::id()
		)
	}

	#[test]
	fn error_display_is_the_code() {
		let err = PropertyWatcherControlError::new(PropertyWatcherControlErrorCode::PipeUnresponsive);
		assert_eq!(err.to_string(), "PIPE_UNRESPONSIVE");
		assert_eq!(err.code.as_str(), "PIPE_UNRESPONSIVE");
		assert_eq!(MAX_STOP_REQUEST_BYTES, 64);
	}

	#[test]
	fn stop_prefix_and_response_classification() {
		assert!(is_stop_prefix(b"stop\n"));
		assert!(is_stop_prefix(b"stop"));
		assert!(is_stop_prefix(b"s"));
		assert!(is_stop_prefix(b""));
		assert!(!is_stop_prefix(b"stoq"));
		assert!(!is_stop_prefix(b"stop\n!"));

		assert!(matches!(classify_response(b"stopped\n"), ResponseState::Success));
		assert!(matches!(classify_response(b"failed\n"), ResponseState::Failed));
		assert!(matches!(classify_response(b"stop"), ResponseState::NeedMore));
		assert!(matches!(classify_response(b"st"), ResponseState::NeedMore));
		assert!(matches!(classify_response(b"stopped"), ResponseState::NeedMore));
		assert!(matches!(classify_response(b"stopx"), ResponseState::Invalid));
		assert!(matches!(classify_response(b"stopp"), ResponseState::NeedMore), "stopp은 stopped\\n의 접두사");
		assert!(matches!(classify_response(b"other\n"), ResponseState::Invalid));
	}

	#[test]
	fn full_stop_handshake_over_real_named_pipe() {
		let pipe = unique_pipe("handshake");
		let stops = Arc::new(Mutex::new(0u32));
		let stops_for_server = stops.clone();

		let handle = open_watcher_stop_server(&pipe, move || {
			*stops_for_server.lock().unwrap() += 1;
			Ok(())
		})
		.expect("서버 열기");

		// 클라이언트가 별도 스레드에서 정지를 보낸다
		let client_pipe = pipe.clone();
		let client = std::thread::spawn(move || send_watcher_stop(&client_pipe).map_err(|e| e.to_string()));
		let client_result = client.join().expect("클라이언트 스레드");
		assert_eq!(client_result, Ok(()), "클라이언트 성공");

		handle.stopped().expect("서버도 성공");
		assert_eq!(*stops.lock().unwrap(), 1, "onStop은 정확히 한 번");
		handle.close().expect("닫기");
	}

	#[test]
	fn on_stop_failure_is_reported_to_both_sides() {
		let pipe = unique_pipe("failure");
		let handle = open_watcher_stop_server(&pipe, || Err("정지 처리 실패".to_string()))
			.expect("서버 열기");

		let client_pipe = pipe.clone();
		let client = std::thread::spawn(move || send_watcher_stop(&client_pipe).map_err(|e| e.to_string()));
		let client_result = client.join().expect("클라이언트 스레드");
		// TS: 서버는 "failed\n"를 보내고 클라이언트는 PIPE_UNRESPONSIVE로 실패한다
		assert!(client_result.is_err(), "클라이언트는 실패로 받아야 한다");
		assert!(
			client_result.unwrap_err().contains("PIPE_UNRESPONSIVE"),
			"코드는 PIPE_UNRESPONSIVE"
		);

		let stopped = handle.stopped().map_err(|e| e.to_string());
		assert_eq!(stopped, Err("정지 처리 실패".to_string()), "서버는 원인을 보존한다");
		handle.close().expect("닫기");
	}

	#[test]
	fn close_without_stop_resolves_as_success_and_skips_callback() {
		let pipe = unique_pipe("close-only");
		let stops = Arc::new(Mutex::new(0u32));
		let stops_for_server = stops.clone();
		let handle = open_watcher_stop_server(&pipe, move || {
			*stops_for_server.lock().unwrap() += 1;
			Ok(())
		})
		.expect("서버 열기");

		assert!(!handle.accepted());
		handle.close().expect("닫기");
		// TS close(): 수락 전이면 outcome을 success로 바꾼다
		handle.stopped().expect("닫기만 했으므로 성공");
		assert_eq!(*stops.lock().unwrap(), 0, "onStop은 호출되지 않는다");
		assert!(!handle.accepted());
	}

	#[test]
	fn client_to_missing_pipe_reports_unresponsive() {
		let result = send_watcher_stop(&unique_pipe("missing"));
		let message = result.expect_err("없는 파이프").to_string();
		assert!(
			message.contains("PIPE_UNRESPONSIVE"),
			"PIPE_UNRESPONSIVE 기대: {message}"
		);
	}

	#[test]
	fn oversized_request_is_ignored_then_valid_stop_works() {
		let pipe = unique_pipe("oversized");
		let stops = Arc::new(Mutex::new(0u32));
		let stops_for_server = stops.clone();
		let handle = open_watcher_stop_server(&pipe, move || {
			*stops_for_server.lock().unwrap() += 1;
			Ok(())
		})
		.expect("서버 열기");

		// 64바이트를 넘기는 잘못된 요청 먼저 보낸다
		{
			let mut bad = open_pipe_client(&pipe).expect("연결");
			let payload = format!("{}A", "s".repeat(MAX_STOP_REQUEST_BYTES));
			bad.write_all(payload.as_bytes()).expect("전송");
			let _ = bad.flush();
			drop(bad);
		}
		// 이어서 정상 요청 — 서버가 재수신 대기로 돌아올 때까지 기다린다
		assert!(
			wait_named_pipe_available(&pipe, 3_000),
			"서버가 다시 파이프를 열 때까지 기다린다 (code={}, pipe={})",
			wait_named_pipe_available_code(&pipe, 300),
			pipe
		);
		let client_pipe = pipe.clone();
		let client = std::thread::spawn(move || send_watcher_stop(&client_pipe).map_err(|e| e.to_string()));
		let client_result = client.join().expect("클라이언트");
		assert_eq!(client_result, Ok(()), "무효 요청 후에도 정상 수신");
		handle.stopped().expect("서버 성공");
		assert_eq!(*stops.lock().unwrap(), 1);
		handle.close().expect("닫기");
	}

	#[test]
	fn partial_prefix_then_valid_stop_is_accepted() {
		let pipe = unique_pipe("prefix");
		let handle = open_watcher_stop_server(&pipe, || Ok(())).expect("서버 열기");

		// 개행 없이 끊긴 요청 — TS는 무효로 보고 다시 받는다
		{
			let mut partial = open_pipe_client(&pipe).expect("연결");
			partial.write_all(b"sto").expect("전송");
			let _ = partial.flush();
			drop(partial);
		}
		assert!(
			wait_named_pipe_available(&pipe, 3_000),
			"서버가 다시 파이프를 열 때까지 기다린다 (code={}, pipe={})",
			wait_named_pipe_available_code(&pipe, 300),
			pipe
		);
		let client_pipe = pipe.clone();
		let client = std::thread::spawn(move || send_watcher_stop(&client_pipe).map_err(|e| e.to_string()));
		let result = client.join().expect("클라이언트");
		assert_eq!(result, Ok(()));
		handle.stopped().expect("서버 성공");
		handle.close().expect("닫기");
	}
}
