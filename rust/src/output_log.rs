use serde_json::Value;
use std::io::Write;

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
	let text = std::fs::read_to_string(path).ok()?;
	text.lines().rev().find_map(|line| {
		let mut entry: Value = serde_json::from_str(line).ok()?;
		if entry["kind"].as_str() != Some(kind) { return None; }
		entry.get_mut("data").map(Value::take)
	})
}

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
