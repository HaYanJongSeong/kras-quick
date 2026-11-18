#![cfg(windows)]

use kras_quick::property_progress_windows::{compare_and_write_locked, LockedWriteError, LockedWriteResult};
use std::os::windows::fs::OpenOptionsExt;
use std::path::PathBuf;
use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

struct Fixture { root: PathBuf }

impl Fixture {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("kras-native-progress-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        for (name, bytes) in [("진행 파일.json", b"original-data".as_slice()), ("expected", b"original-data".as_slice()), ("replacement", b"new".as_slice())] {
            std::fs::write(root.join(name), bytes).unwrap();
        }
        Self { root }
    }
    fn write(&self) -> Result<LockedWriteResult, LockedWriteError> {
        compare_and_write_locked(&self.root.join("진행 파일.json"), &self.root.join("expected"), &self.root.join("replacement"))
    }
    fn contents(&self) -> Vec<u8> { std::fs::read(self.root.join("진행 파일.json")).unwrap() }
}

impl Drop for Fixture {
    fn drop(&mut self) { std::fs::remove_dir_all(&self.root).unwrap(); }
}

#[test]
fn write_works_without_powershell_on_path() {
    if std::env::var_os("KRAS_NATIVE_PROGRESS_CHILD").is_some() {
        let fixture = Fixture::new();
        assert_eq!(fixture.write().unwrap(), LockedWriteResult::Written);
        assert_eq!(fixture.contents(), b"new");
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "write_works_without_powershell_on_path", "--nocapture"])
        .env("KRAS_NATIVE_PROGRESS_CHILD", "1").env("PATH", "").output().unwrap();
    assert!(output.status.success(), "native writer must work with empty PATH:\n{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}

#[test]
fn changed_source_is_preserved() {
    let fixture = Fixture::new();
    std::fs::write(fixture.root.join("진행 파일.json"), b"someone-else").unwrap();
    assert_eq!(fixture.write().unwrap(), LockedWriteResult::SourceChanged);
    assert_eq!(fixture.contents(), b"someone-else");
}

#[test]
fn shorter_replacement_truncates_old_tail() {
    let fixture = Fixture::new();
    assert_eq!(fixture.write().unwrap(), LockedWriteResult::Written);
    assert_eq!(fixture.contents(), b"new");
}

#[test]
fn another_writer_prevents_modification() {
    let fixture = Fixture::new();
    let writer = std::fs::OpenOptions::new().read(true).write(true).share_mode(FILE_SHARE_READ)
        .open(fixture.root.join("진행 파일.json")).unwrap();
    assert_eq!(fixture.write().unwrap_err(), LockedWriteError);
    assert_eq!(fixture.contents(), b"original-data");
    drop(writer);
}

#[test]
fn compatible_reader_does_not_block_write() {
    let fixture = Fixture::new();
    let reader = std::fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(fixture.root.join("진행 파일.json")).unwrap();
    assert_eq!(fixture.write().unwrap(), LockedWriteResult::Written);
    assert_eq!(fixture.contents(), b"new");
    drop(reader);
}
