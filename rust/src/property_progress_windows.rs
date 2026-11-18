// Windows 공유 모드로 다른 쓰기를 막고, 원본 내용이 일치할 때만 갱신한다.

use std::path::Path;

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
		use std::io::{Read, Seek, SeekFrom, Write};
		use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
		use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};

		let expected = std::fs::read(expected_snapshot_path).map_err(|_| LockedWriteError)?;
		let replacement = std::fs::read(replacement_path).map_err(|_| LockedWriteError)?;
		let mut target = std::fs::OpenOptions::new().read(true).write(true)
			.share_mode(FILE_SHARE_READ).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
			.open(target_path).map_err(|_| LockedWriteError)?;
		let metadata = target.metadata().map_err(|_| LockedWriteError)?;
		if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
			return Err(LockedWriteError);
		}
		if metadata.len() != u64::try_from(expected.len()).map_err(|_| LockedWriteError)? {
			return Ok(LockedWriteResult::SourceChanged);
		}
		let mut actual = Vec::new();
		target.read_to_end(&mut actual).map_err(|_| LockedWriteError)?;
		if actual != expected { return Ok(LockedWriteResult::SourceChanged); }
		target.seek(SeekFrom::Start(0)).map_err(|_| LockedWriteError)?;
		target.write_all(&replacement).map_err(|_| LockedWriteError)?;
		target.set_len(u64::try_from(replacement.len()).map_err(|_| LockedWriteError)?).map_err(|_| LockedWriteError)?;
		target.sync_all().map_err(|_| LockedWriteError)?;
		Ok(LockedWriteResult::Written)
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
