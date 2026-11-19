use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Log(PathBuf);

impl Log {
	fn new(bytes: &[u8]) -> Self {
		static NEXT: AtomicU64 = AtomicU64::new(0);
		let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		let path = std::env::temp_dir().join(format!("kras-log-edge-{}-{stamp}-{}.jsonl", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
		let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
		file.write_all(bytes).unwrap();
		Self(path)
	}

	fn name(&self) -> &str { self.0.to_str().unwrap() }
}

impl Drop for Log {
	fn drop(&mut self) { std::fs::remove_file(&self.0).unwrap(); }
}

#[test]
fn latest_matches_scan_when_records_have_crlf_and_damaged_json() {
	// Given
	let log = Log::new(b"{\"kind\":\"a\",\"data\":1}\r\n{\"kind\":\"b\",\"data\":2}\r\n{\"kind\":\"a\",\"data\":3}\r\n{\"kind\":\"a\"}\r\ninvalid\r\n{\"kind\":");
	// When
	let result = latest(log.name(), "a");
	// Then
	assert_eq!(result, Some(serde_json::json!(3)));
	assert_eq!(result, entries(log.name(), "a").pop());
}

#[test]
fn latest_preserves_null_when_newer_record_has_missing_data() {
	// Given
	let log = Log::new(b"{\"kind\":\"a\",\"data\":1}\n{\"kind\":\"a\",\"data\":null}\n{\"kind\":\"a\"}");
	// When
	let result = latest(log.name(), "a");
	// Then
	assert_eq!(result, Some(Value::Null));
}

#[test]
fn latest_recovers_valid_record_when_utf8_lines_are_corrupt() {
	// Given: recovery differs intentionally from read_to_string's whole-file rejection.
	let log = Log::new(b"\xff\n{\"kind\":\"a\",\"data\":7}\n{\"kind\":\"a\",\"data\":\"\xff\"}\n");
	// When
	let result = latest(log.name(), "a");
	// Then
	assert_eq!(result, Some(serde_json::json!(7)));
}

#[test]
fn latest_reads_large_utf8_record_across_multibyte_alignments() {
	// Given
	let payload = "한🙂".repeat(20_000);
	let record = serde_json::json!({"kind": "a", "data": payload}).to_string();
	for padding in 0..8 {
		let log = Log::new(format!("{record}\r\n{}", " ".repeat(padding)).as_bytes());
		// When
		let result = latest(log.name(), "a");
		// Then
		assert_eq!(result, Some(serde_json::json!(payload)));
	}
}

#[test]
fn latest_recovers_earlier_record_when_large_trailing_lines_do_not_match() {
	// Given
	let other = serde_json::json!({"kind": "other", "data": "🙂".repeat(40_000)});
	let log = Log::new(format!("{{\"kind\":\"a\",\"data\":9}}\n{other}\n{}", "x".repeat(30_000)).as_bytes());
	// When
	let result = latest(log.name(), "a");
	// Then
	assert_eq!(result, Some(serde_json::json!(9)));
}

#[test]
fn latest_returns_none_when_file_has_no_matching_data() {
	for bytes in [b"".as_slice(), b"\r\n\n", b"{\"kind\":\"other\",\"data\":1}\n", b"{\"kind\":\"a\"}"] {
		// Given
		let log = Log::new(bytes);
		// When
		let result = latest(log.name(), "a");
		// Then
		assert_eq!(result, None);
	}
}

#[test]
fn latest_returns_none_when_file_is_missing_or_unreadable() {
	// Given
	let log = Log::new(b"");
	let missing = log.0.with_extension("missing");
	// When
	let results = [latest(missing.to_str().unwrap(), "a"), latest(std::env::temp_dir().to_str().unwrap(), "a")];
	// Then
	assert_eq!(results, [None, None]);
}
