use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn create(root: &Path) -> std::io::Result<(PathBuf, File)> {
    fs::create_dir_all(root)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    for index in 1u64.. {
        let path = root.join(format!("작업로그_진행중_{}_{stamp}_{index}.jsonl", std::process::id()));
        let mut options = OpenOptions::new();
        options.create_new(true).append(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

pub(super) fn finalize(path: &Path, stamp: &str) -> std::io::Result<PathBuf> {
    let root = path.parent().ok_or_else(|| std::io::Error::other("로그 폴더가 없습니다."))?;
    for index in 1u64.. {
        let suffix = if index == 1 { String::new() } else { format!("_{index}") };
        let destination = root.join(format!("작업로그_{stamp}{suffix}.jsonl"));
        // 기존 이름이 있으면 원자적으로 실패하므로 다른 실행의 로그를 교체하지 않는다.
        match fs::hard_link(path, &destination) {
            Ok(()) => {
                if let Err(error) = fs::remove_file(path) {
                    crate::output::write_err(&format!("진행 중 로그 이름 정리 실패: {error}\n"));
                }
                return Ok(destination);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn same_second_logs_preserve_both_contents() {
        let root = std::env::temp_dir().join(format!("kras-log-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let (first, mut file) = create(&root).unwrap();
        file.write_all(b"first\n").unwrap();
        drop(file);
        let first = finalize(&first, "20261118_153045").unwrap();
        let (second, mut file) = create(&root).unwrap();
        file.write_all(b"second\n").unwrap();
        drop(file);
        let second = finalize(&second, "20261118_153045").unwrap();
        assert_eq!(first.file_name().unwrap(), "작업로그_20261118_153045.jsonl");
        assert_eq!(second.file_name().unwrap(), "작업로그_20261118_153045_2.jsonl");
        assert_eq!(fs::read(first).unwrap(), b"first\n");
        assert_eq!(fs::read(second).unwrap(), b"second\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unfinished_log_remains_readable_and_new_run_is_separate() {
        let root = std::env::temp_dir().join(format!("kras-unfinished-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let (first, mut file) = create(&root).unwrap();
        file.write_all(b"unfinished\n").unwrap();
        drop(file);
        let (second, file) = create(&root).unwrap();
        drop(file);
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"unfinished\n");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_finalization_preserves_active_log() {
        let root = std::env::temp_dir().join(format!("kras-log-failure-{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let (path, mut file) = create(&root).unwrap();
        file.write_all(b"preserved\n").unwrap();
        drop(file);
        assert!(finalize(&path, "missing/invalid").is_err());
        assert_eq!(fs::read(path).unwrap(), b"preserved\n");
        fs::remove_dir_all(root).unwrap();
    }
}
