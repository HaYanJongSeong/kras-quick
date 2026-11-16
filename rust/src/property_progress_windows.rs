// TS scripts/property-progress-windows.ts — Windows 잠금 상태에서의 진행 파일 원자 교체.
// PowerShell의 FileShare.Read 열기 + 바이트 비교 후 쓰기를 그대로 재현한다.

use std::path::Path;

use base64::Engine;

/// TS LockedWriteResult
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockedWriteResult {
	Written,
	SourceChanged,
}

impl LockedWriteResult {
	pub fn as_str(self) -> &'static str {
		match self {
			LockedWriteResult::Written => "written",
			LockedWriteResult::SourceChanged => "source_changed",
		}
	}
}

/// TS LockedWriteError
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockedWriteError;

impl std::fmt::Display for LockedWriteError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("Windows locked progress write failed.")
	}
}

impl std::error::Error for LockedWriteError {}

/// UTF-16LE 바이트를 base64로 — TS가 `-EncodedCommand`에 넘기는 형식.
fn utf16le_base64(text: &str) -> String {
	let mut bytes = Vec::with_capacity(text.len() * 2);
	for unit in text.encode_utf16() {
		bytes.extend_from_slice(&unit.to_le_bytes());
	}
	base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// TS createEncodedCommand — 3개 경로를 base64로 넣어 스크립트를 구성한다.
/// 플레이스홀더 치환 방식이라 PowerShell의 중괄호를 그대로 둘 수 있다.
fn create_encoded_command(
	target_path: &str,
	expected_snapshot_path: &str,
	replacement_path: &str,
) -> String {
	let script = r#"
& {
  $ErrorActionPreference = "Stop"
  try {
    [string]$TargetPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("__TARGET__"))
    [string]$ExpectedSnapshotPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("__EXPECTED__"))
    [string]$ReplacementPath = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String("__REPLACEMENT__"))
    $attributes = [System.IO.File]::GetAttributes($TargetPath)
    if (($attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw "reparse target"
    }
    [byte[]]$expected = [System.IO.File]::ReadAllBytes($ExpectedSnapshotPath)
    [byte[]]$replacement = [System.IO.File]::ReadAllBytes($ReplacementPath)
    $target = [System.IO.FileStream]::new(
      $TargetPath,
      [System.IO.FileMode]::Open,
      [System.IO.FileAccess]::ReadWrite,
      [System.IO.FileShare]::Read
    )
    try {
      $matches = $target.Length -eq $expected.LongLength
      if ($matches) {
        for ($index = 0; $index -lt $expected.Length; $index += 1) {
          if ($target.ReadByte() -ne $expected[$index]) {
            $matches = $false
            break
          }
        }
      }
      if (-not $matches) {
        [Console]::Out.Write("source_changed")
        return
      }
      $target.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
      $target.Write($replacement, 0, $replacement.Length)
      $target.SetLength($replacement.LongLength)
      $target.Flush($true)
      [Console]::Out.Write("written")
    } finally {
      $target.Dispose()
    }
  } catch {
    [Console]::Error.Write("io_failure")
    exit 1
  }
}
"#;
	let script = script
		.replace(
			"__TARGET__",
			&base64::engine::general_purpose::STANDARD.encode(target_path.as_bytes()),
		)
		.replace(
			"__EXPECTED__",
			&base64::engine::general_purpose::STANDARD.encode(expected_snapshot_path.as_bytes()),
		)
		.replace(
			"__REPLACEMENT__",
			&base64::engine::general_purpose::STANDARD.encode(replacement_path.as_bytes()),
		);
	utf16le_base64(script.trim_start_matches('\n'))
}

/// TS compareAndWriteLocked — 잠금을 잡고 기대 스냅샷과 일치할 때만 교체한다.
pub fn compare_and_write_locked(
	target_path: &Path,
	expected_snapshot_path: &Path,
	replacement_path: &Path,
) -> Result<LockedWriteResult, LockedWriteError> {
	#[cfg(not(windows))]
	{
		let _ = (target_path, expected_snapshot_path, replacement_path);
		return Err(LockedWriteError);
	}

	#[cfg(windows)]
	{
		let target_meta = std::fs::symlink_metadata(target_path).map_err(|_| LockedWriteError)?;
		// TS: isFile() && !isSymbolicLink()
		if !target_meta.file_type().is_file() {
			return Err(LockedWriteError);
		}

		let Some(target_str) = target_path.to_str() else {
			return Err(LockedWriteError);
		};
		let Some(expected_str) = expected_snapshot_path.to_str() else {
			return Err(LockedWriteError);
		};
		let Some(replacement_str) = replacement_path.to_str() else {
			return Err(LockedWriteError);
		};

		let encoded = create_encoded_command(target_str, expected_str, replacement_str);
		let output = std::process::Command::new("powershell.exe")
			.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
			.output()
			.map_err(|_| LockedWriteError)?;
		if !output.status.success() {
			return Err(LockedWriteError);
		}
		let stdout = String::from_utf8_lossy(&output.stdout);
		match stdout.trim() {
			"written" => Ok(LockedWriteResult::Written),
			"source_changed" => Ok(LockedWriteResult::SourceChanged),
			_ => Err(LockedWriteError),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn locked_write_result_strings_match_ts() {
		assert_eq!(LockedWriteResult::Written.as_str(), "written");
		assert_eq!(LockedWriteResult::SourceChanged.as_str(), "source_changed");
	}

	#[test]
	fn error_message_matches_ts() {
		assert_eq!(LockedWriteError.to_string(), "Windows locked progress write failed.");
	}

	#[test]
	fn encoded_command_embeds_all_three_paths() {
		let encoded = create_encoded_command("C:\\target.txt", "C:\\exp.tmp", "C:\\rep.tmp");
		// base64(UTF-16LE) 길이는 양수여야 하고 디코딩 가능해야 한다
		assert!(!encoded.is_empty());
		let raw: Vec<u8> = base64::engine::general_purpose::STANDARD
			.decode(&encoded)
			.expect("base64");
		assert_eq!(raw.len() % 2, 0, "UTF-16LE은 2바이트 정렬");
		let text = String::from_utf16(
			&raw
				.chunks_exact(2)
				.map(|c| u16::from_le_bytes([c[0], c[1]]))
				.collect::<Vec<_>>(),
		)
		.expect("utf16");
		// 경로는 base64로 들어가므로 원문 경로가 통째로 노출되지 않는다
		assert!(!text.contains("C:\\target.txt"));
		assert!(text.contains("FromBase64String"));
		assert!(text.contains("source_changed"));
		assert!(text.contains("FileShare]::Read"));
		// 디코딩하면 경로가 보인다
		let decoded_target =
			base64::engine::general_purpose::STANDARD.encode("C:\\target.txt".as_bytes());
		assert!(text.contains(&decoded_target));
	}

	#[test]
	fn utf16le_base64_round_trips_korean() {
		let original = "진행 파일을 안전하게 갱신할 수 없습니다.";
		let encoded = utf16le_base64(original);
		let raw: Vec<u8> = base64::engine::general_purpose::STANDARD
			.decode(&encoded)
			.expect("base64");
		let units: Vec<u16> = raw
			.chunks_exact(2)
			.map(|c| u16::from_le_bytes([c[0], c[1]]))
			.collect();
		assert_eq!(String::from_utf16(&units).expect("utf16"), original);
	}

	#[test]
	fn missing_target_is_rejected() {
		let dir = std::env::temp_dir().join("kras-lockedwrite-none");
		let _ = std::fs::create_dir_all(&dir);
		let err = compare_and_write_locked(
			&dir.join("absent.txt"),
			&dir.join("a.tmp"),
			&dir.join("b.tmp"),
		)
		.expect_err("없는 대상은 거부");
		assert_eq!(err, LockedWriteError);
	}
}
