// TS scripts/kras-browser-process.ts — CDP 포트 소유자 검증·브라우저 실행.
// 실행파일·업무용 프로필·루프백 바인딩이 모두 일치할 때만 기존 창을 재사용한다.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::kras_browser_choice::KrasBrowserChoice;

#[derive(Debug, Clone)]
pub struct KrasBrowserLaunchError {
	message: String,
}

impl KrasBrowserLaunchError {
	pub fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
		}
	}
	pub fn name(&self) -> &'static str {
		"KrasBrowserLaunchError"
	}
}

impl std::fmt::Display for KrasBrowserLaunchError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(&self.message)
	}
}

impl std::error::Error for KrasBrowserLaunchError {}

pub type LaunchResult<T> = Result<T, KrasBrowserLaunchError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasCdpOwner {
	pub executable_path: String,
	pub command_line: String,
	pub addresses: Vec<String>,
}

// win32.resolve + 소문자: 경로 비교는 Windows 규칙(구분자·`.`·`..`·대소문자)을 따른다.
fn win32_normalize(path: &str) -> String {
	let replaced = path.replace('/', "\\");
	let base: String = if replaced.starts_with("\\\\") {
		replaced
	} else if replaced.get(1..2) == Some(":") {
		if replaced.get(2..3) == Some("\\") {
			replaced
		} else {
			// 드라이브 상대 경로는 현재 디렉터리 기준(동일 드라이브 우선).
			let cwd = current_dir_string();
			if cwd.len() >= 2
				&& cwd.as_bytes()[1] == b':'
				&& cwd[..1].eq_ignore_ascii_case(&replaced[..1])
			{
				format!("{cwd}\\{}", &replaced[2..])
			} else {
				format!("{}\\{}", &replaced[..2], &replaced[2..])
			}
		}
	} else {
		format!("{}\\{replaced}", current_dir_string())
	};
	// 접두사(드라이브 루트 또는 UNC 공유) 분리 후 `.`/`..` 해석.
	let (mut prefix, tail): (String, &str) = if base.starts_with("\\\\") {
		let rest = &base[2..];
		match rest.find('\\') {
			Some(first) => {
				let after = &rest[first + 1..];
				match after.find('\\') {
					Some(second) => (
						format!("\\\\{}\\{}", &rest[..first], &after[..second]),
						&after[second + 1..],
					),
					None => (format!("\\\\{rest}"), ""),
				}
			}
			None => (format!("\\\\{rest}"), ""),
		}
	} else {
		(base[..3].to_string(), &base[3..])
	};
	let mut stack: Vec<&str> = Vec::new();
	for component in tail.split('\\') {
		match component {
			"" | "." => {}
			".." => {
				stack.pop();
			}
			other => stack.push(other),
		}
	}
	if !stack.is_empty() && !prefix.ends_with('\\') {
		prefix.push('\\');
	}
	let mut result = prefix;
	for (index, component) in stack.iter().enumerate() {
		if index > 0 {
			result.push('\\');
		}
		result.push_str(component);
	}
	result.to_lowercase()
}

fn current_dir_string() -> String {
	std::env::current_dir()
		.map(|path| path.display().to_string().replace('/', "\\"))
		.unwrap_or_else(|_| "C:\\".to_string())
}

/// 명령줄 인용 처리: JS `(?:[^\s"]+|"[^"]*")+` 후 모든 `"` 제거.
fn command_line_arguments(command_line: &str) -> Vec<String> {
	let pattern = regex::Regex::new(r#"(?:[^\s"]+|"[^"]*")+"#).expect("valid regex");
	pattern
		.find_iter(command_line)
		.map(|found| found.as_str().replace('"', ""))
		.collect()
}

pub fn matches_kras_browser_owner(
	choice: &KrasBrowserChoice,
	owner: Option<&KrasCdpOwner>,
) -> bool {
	let Some(owner) = owner else {
		return false;
	};
	let arguments = command_line_arguments(&owner.command_line);
	let profile = arguments
		.iter()
		.find(|argument| argument.starts_with("--user-data-dir="))
		.map(|argument| argument["--user-data-dir=".len()..].to_string())
		.or_else(|| {
			arguments
				.iter()
				.position(|argument| argument == "--user-data-dir")
				.and_then(|index| arguments.get(index + 1).cloned())
		});
	let normalize = win32_normalize;
	normalize(&owner.executable_path) == normalize(&choice.executable_path)
		&& profile.is_some_and(|profile| {
			normalize(&profile) == normalize(&choice.profile_directory)
		})
		&& owner
			.addresses
			.iter()
			.all(|address| address == "127.0.0.1" || address == "::1")
}

fn endpoint_error() -> KrasBrowserLaunchError {
	KrasBrowserLaunchError::new("업무용 브라우저는 로컬 디버깅 주소만 사용할 수 있습니다.")
}

/// 로컬 HTTP 엔드포인트만 허용하고 포트를 돌려준다(TS new URL + 호스트 가드).
fn parse_local_endpoint(endpoint: &str) -> LaunchResult<u16> {
	let Some(rest) = endpoint.get(..7) else {
		return Err(endpoint_error());
	};
	if !rest.eq_ignore_ascii_case("http://") {
		return Err(endpoint_error());
	}
	let remainder = &endpoint[7..];
	let authority = remainder
		.split(|character| matches!(character, '/' | '?' | '#'))
		.next()
		.unwrap_or("");
	let (host, port_text) = if authority.starts_with('[') {
		match authority.find(']') {
			Some(end) => (
				&authority[..=end],
				authority[end + 1..].strip_prefix(':').unwrap_or(""),
			),
			None => return Err(endpoint_error()),
		}
	} else {
		match authority.rfind(':') {
			Some(index) => (&authority[..index], &authority[index + 1..]),
			None => (authority, ""),
		}
	};
	if !matches!(host.to_lowercase().as_str(), "127.0.0.1" | "localhost" | "[::1]") {
		return Err(endpoint_error());
	}
	if port_text.is_empty() {
		return Ok(80);
	}
	if !port_text.bytes().all(|byte| byte.is_ascii_digit()) {
		return Err(endpoint_error());
	}
	port_text
		.parse::<u16>()
		.map_err(|_| endpoint_error())
}

fn port_error() -> KrasBrowserLaunchError {
	KrasBrowserLaunchError::new("브라우저 연결 포트가 유효하지 않습니다.")
}

// 표준 base64(패딩 포함) — PowerShell -EncodedCommand는 UTF-16LE를 요구한다.
fn base64_utf16le(text: &str) -> String {
	const ALPHABET: &[u8; 64] =
		b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let bytes: Vec<u8> = text
		.encode_utf16()
		.flat_map(|unit| unit.to_le_bytes())
		.collect();
	let mut encoded = String::new();
	for chunk in bytes.chunks(3) {
		let first = u32::from(chunk[0]);
		let second = u32::from(*chunk.get(1).unwrap_or(&0));
		let third = u32::from(*chunk.get(2).unwrap_or(&0));
		let packed = (first << 16) | (second << 8) | third;
		encoded.push(ALPHABET[(packed >> 18) as usize & 63] as char);
		encoded.push(ALPHABET[(packed >> 12) as usize & 63] as char);
		encoded.push(if chunk.len() > 1 {
			ALPHABET[(packed >> 6) as usize & 63] as char
		} else {
			'='
		});
		encoded.push(if chunk.len() > 2 {
			ALPHABET[packed as usize & 63] as char
		} else {
			'='
		});
	}
	encoded
}

#[cfg(windows)]
fn query_owner(port: u64) -> LaunchResult<Option<KrasCdpOwner>> {
	use std::os::windows::process::CommandExt;
	use std::time::{Duration, Instant};

	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	let script = format!(
		r#"
    $ErrorActionPreference='Stop'
    [Console]::OutputEncoding=[System.Text.UTF8Encoding]::new()
    $listeners=@(Get-NetTCPConnection -State Listen | Where-Object {{ $_.LocalPort -eq {port} }})
    if($listeners.Count -eq 0) {{ Write-Output 'null'; exit 0 }}
    $owners=@($listeners | Select-Object -ExpandProperty OwningProcess -Unique)
    if($owners.Count -ne 1) {{ throw 'Ambiguous debugging port ownership' }}
    $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$owners[0])
    @{{executablePath=$process.ExecutablePath;commandLine=$process.CommandLine;addresses=@($listeners.LocalAddress)}} | ConvertTo-Json -Compress
  "#
	);
	let encoded = base64_utf16le(&script);
	let mut child = Command::new("powershell.exe")
		.args([
			"-NoLogo",
			"-NoProfile",
			"-NonInteractive",
			"-EncodedCommand",
			&encoded,
		])
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.creation_flags(CREATE_NO_WINDOW)
		.spawn()
		.map_err(|error| KrasBrowserLaunchError::new(format!("포트 소유자 조회 실패: {error}")))?;
	// TS execFile timeout 8000ms — 초과하면 조회를 중단한다.
	let deadline = Instant::now() + Duration::from_millis(8000);
	loop {
		match child.try_wait() {
			Ok(None) => {
				if Instant::now() >= deadline {
					let _ = child.kill();
					let _ = child.wait();
					return Err(KrasBrowserLaunchError::new(
						"브라우저 포트 조회 시간이 초과되었습니다.",
					));
				}
				std::thread::sleep(Duration::from_millis(50));
			}
			Ok(Some(status)) => {
				let output = child
					.wait_with_output()
					.map_err(|error| KrasBrowserLaunchError::new(format!("포트 소유자 조회 실패: {error}")))?;
				if !status.success() {
					let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
					return Err(KrasBrowserLaunchError::new(if detail.is_empty() {
						"브라우저 포트 소유자 조회가 실패했습니다.".to_string()
					} else {
						detail
					}));
				}
				let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
				if text == "null" || text.is_empty() {
					return Ok(None);
				}
				return parse_owner_json(&text);
			}
			Err(error) => {
				return Err(KrasBrowserLaunchError::new(format!(
					"포트 소유자 조회 실패: {error}"
				)));
			}
		}
	}
}

#[cfg(not(windows))]
fn query_owner(_port: u64) -> LaunchResult<Option<KrasCdpOwner>> {
	Err(KrasBrowserLaunchError::new(
		"브라우저 포트 소유자 조회는 Windows에서만 가능합니다.",
	))
}

// zod ownerSchema 동일: executablePath/commandLine 비어있지 않음, addresses 1개 이상.
fn parse_owner_json(text: &str) -> LaunchResult<Option<KrasCdpOwner>> {
	let value: serde_json::Value = serde_json::from_str(text)
		.map_err(|error| KrasBrowserLaunchError::new(format!("포트 소유자 응답 해석 실패: {error}")))?;
	if value.is_null() {
		return Ok(None);
	}
	let executable_path = value
		.get("executablePath")
		.and_then(serde_json::Value::as_str)
		.filter(|text| !text.is_empty());
	let command_line = value
		.get("commandLine")
		.and_then(serde_json::Value::as_str)
		.filter(|text| !text.is_empty());
	let addresses: Option<Vec<String>> = value
		.get("addresses")
		.and_then(serde_json::Value::as_array)
		.map(|items| {
			items
				.iter()
				.filter_map(serde_json::Value::as_str)
				.map(str::to_string)
				.collect()
		})
		.filter(|items: &Vec<String>| !items.is_empty());
	match (executable_path, command_line, addresses) {
		(Some(executable_path), Some(command_line), Some(addresses)) => {
			Ok(Some(KrasCdpOwner {
				executable_path: executable_path.to_string(),
				command_line: command_line.to_string(),
				addresses,
			}))
		}
		_ => Err(KrasBrowserLaunchError::new(
			"브라우저 포트 소유자 응답 형식이 올바르지 않습니다.",
		)),
	}
}

/// 포트 소유자를 조회한다. 포트 검증은 플랫폼 조회보다 먼저 수행한다.
pub fn read_kras_cdp_owner(port: f64) -> LaunchResult<Option<KrasCdpOwner>> {
	if !port.is_finite() || port.fract() != 0.0 || port < 1.0 || port > 65535.0 {
		return Err(port_error());
	}
	query_owner(port as u64)
}

pub fn verify_kras_browser_owner(
	choice: &KrasBrowserChoice,
	endpoint: &str,
) -> LaunchResult<()> {
	let port = parse_local_endpoint(endpoint)?;
	let owner = read_kras_cdp_owner(f64::from(port))?;
	if matches_kras_browser_owner(choice, owner.as_ref()) {
		Ok(())
	} else {
		Err(KrasBrowserLaunchError::new(
			"연결 포트의 실행파일·업무용 프로필이 선택한 브라우저와 다릅니다. 다른 창을 자동 연결하거나 종료하지 않습니다.",
		))
	}
}

pub fn ensure_kras_chromium(
	choice: &KrasBrowserChoice,
	endpoint: &str,
) -> LaunchResult<()> {
	let port = parse_local_endpoint(endpoint)?;
	if read_kras_cdp_owner(f64::from(port))?.is_some() {
		return verify_kras_browser_owner(choice, endpoint);
	}
	// 기존 창이 없으면 업무용 프로필로 새로 띄운다(TS spawn 옵션 동일).
	let mut command = Command::new(&choice.executable_path);
	command
		.arg("--remote-debugging-address=127.0.0.1")
		.arg(format!("--remote-debugging-port={port}"))
		.arg(format!("--user-data-dir={}", choice.profile_directory))
		.arg("--disable-popup-blocking")
		.arg("--no-first-run")
		.arg("https://www.kras.go.kr/kras/cert/certView.do")
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// detached + stdio ignore: DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
		command.creation_flags(0x0000_0008 | 0x0000_0200);
	}
	command.spawn().map_err(|error| {
		KrasBrowserLaunchError::new(format!(
			"{} 시작 실패: {error}",
			choice.label
		))
	})?;
	// TS: 30회 × 500ms 소유자 재확인.
	for _ in 0..30 {
		std::thread::sleep(std::time::Duration::from_millis(500));
		if read_kras_cdp_owner(f64::from(port))?.is_some() {
			return verify_kras_browser_owner(choice, endpoint);
		}
	}
	Err(KrasBrowserLaunchError::new(format!(
		"{} 연결을 시작하지 못했습니다. 브라우저 창과 디버깅 포트 상태를 확인하세요.",
		choice.label
	)))
}

// 프로필 플래그 비교용 — win32 규칙은 위 win32_normalize가 처리한다.
#[allow(dead_code)]
fn _path_is_absolute(path: &str) -> bool {
	Path::new(path).is_absolute()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::kras_browser_choice::KrasBrowserEngine;

	fn choice() -> KrasBrowserChoice {
		KrasBrowserChoice {
			engine: KrasBrowserEngine::Chromium,
			label: "MOCK Chrome".to_string(),
			executable_path: r"C:\Program Files\Google\Chrome\Application\chrome.exe"
				.to_string(),
			profile_directory: r"C:\KRAS Test\업무용 프로필".to_string(),
		}
	}

	fn base_owner() -> KrasCdpOwner {
		let selected = choice();
		KrasCdpOwner {
			executable_path: selected.executable_path.clone(),
			command_line: format!(
				"\"{}\" --user-data-dir=\"{}\" --remote-debugging-port=9222",
				selected.executable_path, selected.profile_directory
			),
			addresses: vec!["127.0.0.1".to_string()],
		}
	}

	#[test]
	fn accepts_owned_listeners() {
		let selected = choice();
		let owner = base_owner();
		let accepted: Vec<(&str, KrasCdpOwner)> = vec![
			("quoted equals-form profile with spaces", owner.clone()),
			(
				"a separate quoted profile argument",
				KrasCdpOwner {
					command_line: format!(
						"\"{}\" --user-data-dir \"{}\"",
						owner.executable_path, selected.profile_directory
					),
					..owner.clone()
				},
			),
			(
				"a quoted whole equals-form argument",
				KrasCdpOwner {
					command_line: format!(
						"\"{}\" \"--user-data-dir={}\"",
						owner.executable_path, selected.profile_directory
					),
					..owner.clone()
				},
			),
			(
				"case-insensitive Windows paths and normalized separators",
				KrasCdpOwner {
					executable_path:
						"c:/PROGRAM FILES/Google/Chrome/Application/CHROME.EXE".to_string(),
					command_line:
						"chrome.exe --user-data-dir=\"c:/kras test/업무용 프로필/./\""
						.to_string(),
					..owner.clone()
				},
			),
			(
				"IPv6 loopback",
				KrasCdpOwner {
					addresses: vec!["::1".to_string()],
					..owner.clone()
				},
			),
			(
				"both loopback families",
				KrasCdpOwner {
					addresses: vec!["127.0.0.1".to_string(), "::1".to_string()],
					..owner.clone()
				},
			),
		];
		for (name, candidate) in accepted {
			assert!(
				matches_kras_browser_owner(&selected, Some(&candidate)),
				"accept {name}"
			);
		}
	}

	#[test]
	fn rejects_unowned_or_foreign_listeners() {
		let selected = choice();
		let owner = base_owner();
		let mut rejected: Vec<(&str, Option<KrasCdpOwner>)> = vec![
			("no listening process", None),
			(
				"another executable",
				Some(KrasCdpOwner {
					executable_path: r"C:\Other\chrome.exe".to_string(),
					..owner.clone()
				}),
			),
			(
				"an executable-name suffix",
				Some(KrasCdpOwner {
					executable_path: format!("{}.other.exe", owner.executable_path),
					..owner.clone()
				}),
			),
			(
				"another profile of the selected executable",
				Some(KrasCdpOwner {
					command_line:
						"chrome.exe --user-data-dir=\"C:\\Users\\MOCK\\personal\""
						.to_string(),
					..owner.clone()
				}),
			),
			(
				"a profile sharing only the expected prefix",
				Some(KrasCdpOwner {
					command_line: format!(
						"chrome.exe --user-data-dir=\"{}-other\"",
						selected.profile_directory
					),
					..owner.clone()
				}),
			),
			(
				"a missing profile flag",
				Some(KrasCdpOwner {
					command_line: "chrome.exe --profile-directory=\"Default\"".to_string(),
					..owner.clone()
				}),
			),
			(
				"a missing separate profile value",
				Some(KrasCdpOwner {
					command_line: "chrome.exe --user-data-dir".to_string(),
					..owner.clone()
				}),
			),
			(
				"a similarly named flag",
				Some(KrasCdpOwner {
					command_line: format!(
						"chrome.exe --other-user-data-dir=\"{}\"",
						selected.profile_directory
					),
					..owner.clone()
				}),
			),
		];
		for address in ["0.0.0.0", "::", "192.0.2.8", "::ffff:127.0.0.1"] {
			rejected.push((
				"a non-approved listener address",
				Some(KrasCdpOwner {
					addresses: vec![address.to_string()],
					..owner.clone()
				}),
			));
		}
		rejected.push((
			"a public binding alongside a loopback binding",
			Some(KrasCdpOwner {
				addresses: vec!["127.0.0.1".to_string(), "0.0.0.0".to_string()],
				..owner.clone()
			}),
		));
		for (name, candidate) in rejected {
			assert!(
				!matches_kras_browser_owner(&selected, candidate.as_ref()),
				"reject {name}"
			);
		}
	}

	#[test]
	fn local_endpoint_guard_rejects_before_query_or_launch() {
		// 도메인 가드가 PowerShell·브라우저 실행보다 먼저 거절해야 한다.
		for endpoint in [
			"https://127.0.0.1:9222",
			"http://192.0.2.8:9222",
			"http://example.invalid:9222",
			"http://127.0.0.1.example.invalid:9222",
			"http://0.0.0.0:9222",
			"http://[::]:9222",
		] {
			assert!(
				verify_kras_browser_owner(&choice(), endpoint).is_err(),
				"verify {endpoint}"
			);
			assert!(
				ensure_kras_chromium(&choice(), endpoint).is_err(),
				"ensure {endpoint}"
			);
		}
	}

	#[test]
	fn rejects_invalid_ports_before_invoking_powershell() {
		for port in [0.0, -1.0, 65536.0, 1.5, f64::NAN, f64::INFINITY] {
			assert!(read_kras_cdp_owner(port).is_err(), "port {port}");
		}
	}
}
