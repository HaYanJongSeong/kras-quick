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
	entries(path, kind).pop()
}
