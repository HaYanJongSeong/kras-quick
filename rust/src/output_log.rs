use serde_json::Value;
use std::io::{Read, Seek, SeekFrom, Write};

pub fn append(path: &str, kind: &str, data: &Value) -> std::io::Result<()> {
	if let Some(parent) = std::path::Path::new(path).parent() { std::fs::create_dir_all(parent)?; }
	let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
	let mut line = serde_json::to_vec(&serde_json::json!({"kind": kind, "data": data}))?;
	line.push(b'\n');
	file.write_all(&line)
}

pub fn entries(path: &str, kind: &str) -> Vec<Value> {
	let Ok(text) = std::fs::read_to_string(path) else { return Vec::new(); };
	text.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok())
		.filter(|entry| entry["kind"].as_str() == Some(kind))
		.filter_map(|mut entry| entry.get_mut("data").map(Value::take)).collect()
}

pub fn latest(path: &str, kind: &str) -> Option<Value> {
	let mut file = std::fs::File::open(path).ok()?;
	let mut position = file.seek(SeekFrom::End(0)).ok()?;
	let mut chunk = [0_u8; 64 * 1024];
	let chunk_limit = u64::try_from(chunk.len()).ok()?;
	let mut line = Vec::new();
	while position > 0 {
		let length = usize::try_from(position.min(chunk_limit)).ok()?;
		position -= u64::try_from(length).ok()?;
		file.seek(SeekFrom::Start(position)).ok()?;
		file.read_exact(&mut chunk[..length]).ok()?;
		let mut end = length;
		while let Some(start) = chunk[..end].iter().rposition(|byte| *byte == b'\n') {
			let segment = &chunk[start + 1..end];
			let data = if line.is_empty() {
				line_data(segment, kind)
			} else {
				line.extend(segment.iter().rev());
				line.reverse();
				line_data(&line, kind)
			};
			if data.is_some() { return data; }
			line.clear();
			end = start;
		}
		line.extend(chunk[..end].iter().rev());
	}
	line.reverse();
	line_data(&line, kind)
}

fn line_data(line: &[u8], kind: &str) -> Option<Value> {
	// Decode only complete lines so split UTF-8 survives and corrupt lines can be skipped.
	let text = std::str::from_utf8(line).ok()?;
	let mut entry: Value = serde_json::from_str(text).ok()?;
	if entry["kind"].as_str() != Some(kind) { return None; }
	entry.get_mut("data").map(Value::take)
}

#[cfg(test)]
#[path = "output_log_tests.rs"]
mod lookup_tests;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reverse_lookup_matches_complete_scan_with_trailing_partial_records() {
		let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		let path = std::env::temp_dir().join(format!("kras-log-latest-{}-{stamp}.jsonl", std::process::id()));
		std::fs::write(&path, concat!(
			"{\"kind\":\"stage1\",\"data\":{\"code\":\"001\"}}\n",
			"{\"kind\":\"stage1\",\"data\":{\"code\":\"002\"}}\n",
			"{\"kind\":\"stage2\",\"data\":null}\n",
			"{\"kind\":\"stage1\"}\n{\"kind\":")).unwrap();
		let name = path.to_str().unwrap();
		for kind in ["stage1", "stage2", "absent"] {
			assert_eq!(latest(name, kind), entries(name, kind).pop());
		}
		assert_eq!(latest(name, "stage1").unwrap()["code"], "002");
		std::fs::remove_file(path).unwrap();
	}
}
