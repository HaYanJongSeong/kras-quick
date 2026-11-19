use kras_quick::output_log;
use serde_json::Value;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

struct SyntheticLog(PathBuf);

impl Drop for SyntheticLog {
	fn drop(&mut self) { std::fs::remove_file(&self.0).unwrap(); }
}

fn old_latest(path: &str, kind: &str) -> Option<Value> {
	let text = std::fs::read_to_string(path).ok()?;
	text.lines().rev().find_map(|line| {
		let mut entry: Value = serde_json::from_str(line).ok()?;
		if entry["kind"].as_str() != Some(kind) { return None; }
		entry.get_mut("data").map(Value::take)
	})
}

#[test]
#[ignore = "synthetic lookup timings; run with --ignored --nocapture"]
fn compare_latest_lookup_on_synthetic_logs() {
	for large_utf8 in [false, true] {
		// Given
		let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		let log = SyntheticLog(std::env::temp_dir().join(format!("kras-lookup-bench-{}-{stamp}-{large_utf8}.jsonl", std::process::id())));
		let file = std::fs::OpenOptions::new().create_new(true).write(true).open(&log.0).unwrap();
		let mut writer = std::io::BufWriter::new(file);
		let payload = if large_utf8 { "한글🙂".repeat(2048) } else { "small".to_owned() };
		for index in 0..10_001 {
			let kind = if index % 2 == 0 { "a" } else { "b" };
			writeln!(writer, "{}", serde_json::json!({"kind": kind, "data": {"index": index, "text": payload}})).unwrap();
		}
		writer.flush().unwrap();
		drop(writer);
		let name = log.0.to_str().unwrap();
		for kind in ["a", "b", "absent"] {
			// When
			let before = Instant::now();
			let expected = old_latest(name, kind);
			let old_ms = before.elapsed().as_secs_f64() * 1000.0;
			let before = Instant::now();
			let actual = output_log::latest(name, kind);
			let new_ms = before.elapsed().as_secs_f64() * 1000.0;
			// Then
			assert_eq!(actual, expected);
			eprintln!("rows=10001 large_utf8={large_utf8} kind={kind} old_ms={old_ms:.3} new_ms={new_ms:.3}");
		}
	}
}
