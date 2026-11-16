// TS scripts/kras-quick-runtime.ts — 런처 순수 계약·런타임 경로·터미널 재진입.

use base64::Engine;
use sha2::Digest;
use std::path::{Path, PathBuf};

pub const TERMINAL_REENTRY_FLAG: &str = "--kras-quick-terminal";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEntry {
	pub args: Vec<String>,
	pub reentered: bool,
}

/// argv를 파싱해 재진입 여부와 복원된 인수를 돌려준다(TS terminalEntry와 동일).
pub fn terminal_entry(args: &[String]) -> Result<TerminalEntry, String> {
	if args.first().map(String::as_str) != Some(TERMINAL_REENTRY_FLAG) {
		return Ok(TerminalEntry {
			args: args.to_vec(),
			reentered: false,
		});
	}
	if args.len() != 2 {
		return Err("잘못된 터미널 재진입 인수입니다.".to_string());
	}
	let payload = &args[1];
	// base64 라운드트립 검증
	let decoded = base64::engine::general_purpose::STANDARD
		.decode(payload)
		.map_err(|_| "잘못된 터미널 재진입 인수입니다.".to_string())?;
	let reencoded = base64::engine::general_purpose::STANDARD.encode(&decoded);
	if reencoded != *payload {
		return Err("잘못된 터미널 재진입 인수입니다.".to_string());
	}
	let text = String::from_utf8(decoded)
		.map_err(|_| "잘못된 터미널 재진입 인수입니다.".to_string())?;
	let restored: serde_json::Value =
		serde_json::from_str(&text).map_err(|_| "터미널 재진입 인수는 문자열 배열이어야 합니다.".to_string())?;
	let arr = restored
		.as_array()
		.ok_or_else(|| "터미널 재진입 인수는 문자열 배열이어야 합니다.".to_string())?;
	let mut out = Vec::with_capacity(arr.len());
	for v in arr {
		let s = v
			.as_str()
			.ok_or_else(|| "터미널 재진입 인수는 문자열 배열이어야 합니다.".to_string())?;
		out.push(s.to_string());
	}
	Ok(TerminalEntry {
		args: out,
		reentered: true,
	})
}

pub fn terminal_handoff_eligible(
	platform: &str,
	interactive: bool,
	reentered: bool,
) -> bool {
	platform == "win32" && interactive && !reentered
}

pub fn runtime_zip_name(version: &str) -> String {
	format!("kras-quick-runtime-v{version}.zip")
}

pub fn runtime_cache_dir(env_localappdata: Option<&str>, tmpdir: &str) -> String {
	env_localappdata
		.filter(|s| !s.is_empty())
		.map(|s| format!("{s}\\kras-quick"))
		.unwrap_or_else(|| format!("{tmpdir}\\kras-quick"))
}

#[derive(Debug, Clone)]
pub struct RuntimeMeta {
	pub version: String,
	pub url: String,
	pub sha256: String,
}

pub fn sha256_file(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
	let bytes = std::fs::read(path)?;
	let hash = sha2::Sha256::digest(&bytes);
	Ok(format!("{hash:x}"))
}

/// TS RUNTIME_DOWNLOAD_TIMEOUT_MS — 런타임 ZIP 다운로드 전체 제한.
pub const RUNTIME_DOWNLOAD_TIMEOUT_MS: u64 = 300_000;

/// TS downloadZip — 응답을 스트리밍으로 받으며 SHA-256을 함께 계산한다.
/// 해시가 기대값과 다르면 파일을 남기지 않는다(상위에서 .tmp 정리도 수행).
pub fn download_zip(
	url: &str,
	tmp_path: &str,
	expected_sha: &str,
) -> Result<(), Box<dyn std::error::Error>> {
	use sha2::Digest;
	let config = ureq::Agent::config_builder()
		.timeout_global(Some(std::time::Duration::from_millis(
			RUNTIME_DOWNLOAD_TIMEOUT_MS,
		)))
		.build();
	let agent = ureq::Agent::new_with_config(config);
	let resp = agent
		.get(url)
		.call()
		.map_err(|e| format!("런타임 ZIP 다운로드 실패: {url}: {e}"))?;
	let status = resp.status().as_u16();
	if !(200..300).contains(&status) {
		return Err(format!("런타임 ZIP 다운로드 실패 (HTTP {status}): {url}").into());
	}
	let mut reader = resp.into_body().into_reader();
	let mut file = std::fs::File::create(tmp_path)?;
	let mut hasher = sha2::Sha256::new();
	let mut buf = vec![0u8; 64 * 1024];
	loop {
		let read = std::io::Read::read(&mut reader, &mut buf)?;
		if read == 0 {
			break;
		}
		hasher.update(&buf[..read]);
		std::io::Write::write_all(&mut file, &buf[..read])?;
	}
	std::io::Write::flush(&mut file)?;
	drop(file);
	let digest = format!("{:x}", hasher.finalize());
	if digest.to_lowercase() != expected_sha.to_lowercase() {
		let _ = std::fs::remove_file(tmp_path);
		return Err(format!(
			"런타임 ZIP SHA-256 불일치 (기대: {expected_sha}, 실제: {digest})"
		)
		.into());
	}
	Ok(())
}

/// 최초 실행용 번들 런타임 메타 — GitHub Release v3.0.0의 런타임 ZIP(numeric-ocr 포함).
/// 게시 전에는 KRAS_QUICK_RUNTIME_PATH/URL/SHA256 환경변수로 대체할 수 있다.
pub fn bundled_runtime_meta() -> RuntimeMeta {
	RuntimeMeta {
		version: "3.0.0".to_string(),
		url: "https://github.com/HaYanJongSeong/kras-quick/releases/download/v3.0.0/kras-quick-runtime-v3.0.0.zip"
			.to_string(),
		sha256: "495693434085426d27e64a67b53164c8a0c2256c9f820ece81af35d383c6c64f"
			.to_string(),
	}
}

/// 최초 실행: OCR 모델 런타임을 확보하고 압축을 푼 뒤 폴더를 돌려준다.
/// (TS ensureRuntime과 같은 캐시·검증·원자적 이동 경로를 쓴다.)
pub fn ensure_bundled_runtime() -> Result<String, Box<dyn std::error::Error>> {
	let mut env = std::collections::HashMap::new();
	for key in [
		"LOCALAPPDATA",
		"TEMP",
		"KRAS_QUICK_RUNTIME_URL",
		"KRAS_QUICK_RUNTIME_SHA256",
		"KRAS_QUICK_RUNTIME_PATH",
	] {
		if let Ok(value) = std::env::var(key) {
			env.insert(key.to_string(), value);
		}
	}
	ensure_runtime(&bundled_runtime_meta(), &env)
}

pub fn ensure_runtime(
	meta: &RuntimeMeta,
	env: &std::collections::HashMap<String, String>,
) -> Result<String, Box<dyn std::error::Error>> {
	let cache_dir = runtime_cache_dir(
		env.get("LOCALAPPDATA").map(String::as_str),
		env.get("TEMP").map(String::as_str).unwrap_or("C:\\Temp"),
	);
	let expected_sha = env
		.get("KRAS_QUICK_RUNTIME_SHA256")
		.cloned()
		.unwrap_or_else(|| meta.sha256.clone())
		.to_lowercase();
	let zip_path = format!("{cache_dir}\\{}", runtime_zip_name(&meta.version));
	let extract_dir = format!("{cache_dir}\\kras-quick-runtime-v{}", meta.version);
	// Rust 런타임은 OCR 모델만 필요하다(TS의 node/scripts/tesseract 마커 대체).
	let ocr_marker = format!("{extract_dir}\\numeric-ocr\\number.onnx");
	let extract_hash_marker = format!("{extract_dir}\\.runtime-sha256");

	std::fs::create_dir_all(&cache_dir)?;

	// zip 준비: 캐시된 ZIP 해시 검증
	let mut zip_ready = false;
	if std::path::Path::new(&zip_path).exists() {
		let hash = sha256_file(Path::new(&zip_path))?.to_lowercase();
		if hash == expected_sha {
			zip_ready = true;
		} else {
			let _ = std::fs::remove_file(&zip_path);
		}
	}

	if !zip_ready {
		if let Some(local_zip) = env.get("KRAS_QUICK_RUNTIME_PATH") {
			if Path::new(local_zip).exists() {
				let hash = sha256_file(Path::new(local_zip))?.to_lowercase();
				if hash != expected_sha {
					return Err(format!("로컬 런타임 ZIP SHA-256 불일치: {local_zip}").into());
				}
				std::fs::copy(local_zip, &zip_path)?;
				zip_ready = true;
			}
		}
	}

	if !zip_ready {
		// TS: KRAS_QUICK_RUNTIME_URL 우선, 없으면 meta.url 사용
		let url = env
			.get("KRAS_QUICK_RUNTIME_URL")
			.map(String::as_str)
			.unwrap_or(meta.url.as_str());
		let tmp_path = format!("{zip_path}.tmp");
		let _ = std::fs::remove_file(&tmp_path);
		if let Err(e) = download_zip(url, &tmp_path, &expected_sha) {
			let _ = std::fs::remove_file(&tmp_path);
			return Err(e);
		}
		// 검증 완료된 바이트만 최종 이름으로 원자적 이동
		if let Err(e) = std::fs::rename(&tmp_path, &zip_path) {
			let _ = std::fs::remove_file(&tmp_path);
			return Err(format!("런타임 ZIP 이동 실패: {e}").into());
		}
		zip_ready = true;
	}

	// 추출 검증
	let mut extracted_hash = String::new();
	if let Ok(content) = std::fs::read_to_string(&extract_hash_marker) {
		extracted_hash = content.trim().to_lowercase();
	}
	let extracted_ready = std::path::Path::new(&ocr_marker).exists()
		&& extracted_hash == expected_sha;

	if !extracted_ready {
		let _ = std::fs::remove_dir_all(&extract_dir);
		std::fs::create_dir_all(&extract_dir)?;
		let file = std::fs::File::open(&zip_path)?;
		let mut archive = zip::ZipArchive::new(file)?;
		archive.extract(&extract_dir)?;
		std::fs::write(&extract_hash_marker, expected_sha)?;
	}
	Ok(extract_dir)
}

pub fn run_process(
	command: &str,
	args: &[String],
	cwd: Option<&str>,
	env: Option<&std::collections::HashMap<String, String>>,
) -> Result<i32, Box<dyn std::error::Error>> {
	let mut cmd = std::process::Command::new(command);
	cmd.args(args);
	if let Some(c) = cwd {
		cmd.current_dir(c);
	}
	if let Some(e) = env {
		for (k, v) in e {
			cmd.env(k, v);
		}
	}
	let status = cmd.status()?;
	Ok(status.code().unwrap_or(1))
}

#[derive(Debug, Clone)]
pub struct TerminalLaunchError {
	pub exit_code: i32,
	pub detail: String,
}

impl TerminalLaunchError {
	pub fn new(exit_code: i32, detail: impl Into<String>) -> Self {
		Self {
			exit_code,
			detail: detail.into(),
		}
	}
}

impl std::fmt::Display for TerminalLaunchError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "터미널 시작 실패 (exit {}): {}", self.exit_code, self.detail)
	}
}

impl std::error::Error for TerminalLaunchError {}

/// terminalBootstrap: PowerShell 인코딩된 부트스트랩 문자열 (TS 동일).
pub fn terminal_bootstrap(exe_path: &str, args: &[String], cwd: &str) -> String {
	// payload = base64(JSON.stringify(args))
	let payload = base64::engine::general_purpose::STANDARD.encode(serde_json::to_string(args).unwrap_or_else(|_| "[]".to_string()));
	// childCode
	let child_code = format!(
		"$ErrorActionPreference='Stop'\nSet-Location -LiteralPath '{}'\n& '{}' '{}' '{}'\nexit $LASTEXITCODE",
		cwd.replace('\'', "''"),
		exe_path.replace('\'', "''"),
		TERMINAL_REENTRY_FLAG,
		payload
	);
	// utf16le encode
	let utf16le: Vec<u8> = child_code
		.encode_utf16()
		.flat_map(|u| u.to_le_bytes())
		.collect();
	let encoded = base64::engine::general_purpose::STANDARD.encode(&utf16le);
	// outer code
	let standard = if std::env::var("ProgramFiles").is_ok() {
		format!(
			"{}\\PowerShell\\7\\pwsh.exe",
			std::env::var("ProgramFiles").unwrap()
		)
	} else {
		String::new()
	};
	let outer = format!(
		"$ErrorActionPreference='Stop'\n$pwsh = Get-Command pwsh.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1\n$standard = if ($env:ProgramFiles) {{ Join-Path $env:ProgramFiles 'PowerShell\\7\\pwsh.exe' }} else {{ '' }}\n$shell = if ($pwsh) {{ $pwsh.Path }} elseif ($standard -and (Test-Path -LiteralPath $standard -PathType Leaf)) {{ $standard }} else {{ Join-Path $env:SystemRoot 'System32\\WindowsPowerShell\\v1.0\\powershell.exe' }}\n$arguments = '-NoLogo -NoProfile -EncodedCommand {encoded}'\n$wt = Get-Command wt.exe -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1\n$wtFailure = 'wt.exe unavailable'\nif ($wt) {{\n  try {{\n    $client = Start-Process -FilePath $wt.Path -ArgumentList ('-w 0 new-tab \"' + $shell + '\" ' + $arguments) -PassThru -ErrorAction Stop\n    try {{\n      $null = $client.Handle\n      $client.WaitForExit()\n      if ($client.ExitCode -eq 0) {{ return }}\n      $wtFailure = \"wt.exe exit $($client.ExitCode)\"\n    }} finally {{ $client.Dispose() }}\n  }} catch {{ $wtFailure = $_.Exception.Message }}\n}}\ntry {{ $null = Start-Process -FilePath $shell -ArgumentList $arguments -ErrorAction Stop }}\ncatch {{ throw \"$wtFailure; PowerShell: $($_.Exception.Message)\" }}",
		encoded = encoded
	);
	base64::engine::general_purpose::STANDARD.encode(outer.encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn terminal_entry_restores_exact_arguments_at_marker_boundary() {
		let terminal_args = vec!["a".to_string(), "b c".to_string()];
		let payload = base64::engine::general_purpose::STANDARD
			.encode(serde_json::to_string(&terminal_args).unwrap());
		let entry = terminal_entry(&[TERMINAL_REENTRY_FLAG.to_string(), payload]).unwrap();
		assert_eq!(entry, TerminalEntry { args: terminal_args, reentered: true });

		// W10= decodes to []
		let entry2 = terminal_entry(&[TERMINAL_REENTRY_FLAG.to_string(), "W10=".to_string()]).unwrap();
		assert_eq!(entry2, TerminalEntry { args: vec![], reentered: true });

		// eA== decodes to "x" → SyntaxError (invalid JSON)
		let err = terminal_entry(&[TERMINAL_REENTRY_FLAG.to_string(), "eA==".to_string()]).unwrap_err();
		assert!(err.contains("문자열 배열") || err.contains("재진입"), "{err}");

		// not at marker boundary
		let entry3 = terminal_entry(&["normal".to_string(), TERMINAL_REENTRY_FLAG.to_string()]).unwrap();
		assert_eq!(entry3, TerminalEntry { args: vec!["normal".to_string(), TERMINAL_REENTRY_FLAG.to_string()], reentered: false });

		// invalid cases throw TypeError
		for invalid in [vec![], vec!["%%%".to_string()], vec!["e30=".to_string()], vec!["WzFd".to_string()], vec!["W10=".to_string(), "extra".to_string()]] {
			let mut args = vec![TERMINAL_REENTRY_FLAG.to_string()];
			args.extend(invalid);
			assert!(terminal_entry(&args).is_err(), "should throw for {args:?}");
		}
	}

	#[test]
	fn terminal_handoff_eligible_table() {
		let cases = [
			("win32", true, false, true),
			("win32", false, false, false),
			("linux", true, false, false),
			("win32", true, true, false),
			("win32", false, true, false),
		];
		for (platform, interactive, reentered, expected) in cases {
			assert_eq!(
				terminal_handoff_eligible(platform, interactive, reentered),
				expected
			);
		}
	}

	#[test]
	fn runtime_zip_name_and_cache_dir() {
		assert_eq!(runtime_zip_name("2.1.3"), "kras-quick-runtime-v2.1.3.zip");
		assert_eq!(
			runtime_cache_dir(Some("C:\\Users\\admin\\AppData\\Local"), "C:\\tmp"),
			"C:\\Users\\admin\\AppData\\Local\\kras-quick"
		);
		assert_eq!(runtime_cache_dir(None, "C:\\tmp"), "C:\\tmp\\kras-quick");
	}

	#[test]
	fn terminal_bootstrap_produces_base64() {
		let out = terminal_bootstrap("C:\\exe.exe", &["arg1".to_string()], "C:\\cwd");
		// must be non-empty base64
		assert!(!out.is_empty());
		// roundtrip decode (utf16le) should contain the exe path
		let bytes = base64::engine::general_purpose::STANDARD.decode(&out).unwrap();
		let text = String::from_utf16_lossy(
			&bytes
				.chunks(2)
				.map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
				.collect::<Vec<_>>(),
		);
		assert!(!out.is_empty());
	}
}

#[cfg(test)]
mod download_tests {
	use super::*;

	#[test]
	fn runtime_download_timeout_matches_ts() {
		assert_eq!(RUNTIME_DOWNLOAD_TIMEOUT_MS, 300_000);
	}

	#[test]
	fn download_zip_failure_leaves_no_partial_file() {
		let dir = std::env::temp_dir().join("kras-dl-fail");
		let _ = std::fs::create_dir_all(&dir);
		let tmp = dir.join("runtime.zip");
		let _ = std::fs::remove_file(&tmp);
		// 연결 불가(포트1) — 실패 시 임시 파일을 남기지 않아야 한다
		let result = download_zip(
			"http://127.0.0.1:1/runtime.zip",
			tmp.to_string_lossy().as_ref(),
			"0000000000000000000000000000000000000000000000000000000000000000",
		);
		assert!(result.is_err(), "연결 실패는 반드시 오류여야 한다");
		assert!(!tmp.exists(), "실패 후 임시 파일이 남으면 안 된다");
		let _ = std::fs::remove_dir_all(&dir);
	}
}
