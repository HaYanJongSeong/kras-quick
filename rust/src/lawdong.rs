use std::collections::HashMap;
use std::sync::OnceLock;

// TS scripts/kras-lawdong.ts(국토부 법정동코드 전체자료 기반)의 코드→법정동명 표.
const LAW_DONG_TSV: &str = include_str!("../data/lawdong.tsv");

static LAW_DONG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static OFFICIAL: OnceLock<std::sync::RwLock<Option<HashMap<String, OfficialCode>>>> = OnceLock::new();

fn official_cache() -> &'static std::sync::RwLock<Option<HashMap<String, OfficialCode>>> {
	OFFICIAL.get_or_init(|| std::sync::RwLock::new(
		std::fs::read_to_string(cache_path()).ok().and_then(|text| parse_official(&text).ok())
	))
}

#[derive(Debug, PartialEq, Eq)]
pub struct OfficialCode {
	pub name: String,
	pub abolished: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CodeDiff {
	pub added: usize,
	pub renamed: usize,
	pub abolished: usize,
	pub missing: usize,
}

pub fn parse_official(text: &str) -> Result<HashMap<String, OfficialCode>, String> {
	let mut lines = text.trim_start_matches('\u{feff}').lines();
	if lines.next() != Some("법정동코드\t법정동명\t폐지여부") {
		return Err("법정동 전체자료 헤더가 올바르지 않습니다.".into());
	}
	let mut result = HashMap::new();
	for (line_number, line) in lines.enumerate() {
		let fields: Vec<_> = line.split('\t').collect();
		if fields.len() != 3 || fields[0].len() != 10 || !fields[0].bytes().all(|b| b.is_ascii_digit())
			|| fields[1].trim().is_empty() || !matches!(fields[2], "존재" | "폐지") {
			return Err(format!("법정동 전체자료 {}행이 올바르지 않습니다.", line_number + 2));
		}
		let entry = OfficialCode { name: fields[1].trim().into(), abolished: fields[2] == "폐지" };
		if result.insert(fields[0].into(), entry).is_some() {
			return Err(format!("중복 법정동코드: {}", fields[0]));
		}
	}
	if result.is_empty() { return Err("법정동 전체자료가 비어 있습니다.".into()); }
	Ok(result)
}

pub fn compare_official(old: &HashMap<String, String>, new: &HashMap<String, OfficialCode>) -> CodeDiff {
	CodeDiff {
		added: new.keys().filter(|code| !old.contains_key(*code)).count(),
		renamed: old.iter().filter(|(code, name)| new.get(*code).is_some_and(|row| row.name != name.trim())).count(),
		abolished: old.keys().filter(|code| new.get(*code).is_some_and(|row| row.abolished)).count(),
		missing: old.keys().filter(|code| !new.contains_key(*code)).count(),
	}
}

#[cfg(test)]
mod update_tests {
	use super::*;
	#[test]
	#[ignore = "공식 사이트 다운로드 및 사용자 자료 교체를 수행하는 실검증"]
	fn live_official_update_and_lookup() {
		let diff = update_official().expect("공식 다운로드와 교체");
		println!("OFFICIAL_DIFF={diff:?}");
		assert_eq!(lookup("1111018100").as_deref(), Some("서울특별시 종로구 행촌동"));
		let bytes = std::fs::read(cache_path()).unwrap();
		let rows = parse_official(std::str::from_utf8(&bytes).unwrap()).unwrap();
		println!("OFFICIAL_ROWS={} LOOKUP={:?}", rows.len(), lookup("1111018100"));
		assert!(install_official(&cache_path(), "invalid download").is_err());
		assert_eq!(std::fs::read(cache_path()).unwrap(), bytes);
		println!("INVALID_DOWNLOAD_PRESERVES_BYTES=true");
	}
	#[test]
	fn invalid_download_preserves_existing_bytes() {
		let path = std::env::temp_dir().join(format!("kras-lawdong-preserve-{}", std::process::id()));
		std::fs::write(&path, b"existing-data").unwrap();
		assert!(install_official(&path, "invalid response").is_err());
		assert_eq!(std::fs::read(&path).unwrap(), b"existing-data");
		assert!(!path.with_extension("pending").exists());
		std::fs::remove_file(path).unwrap();
	}
	#[test]
	fn compares_added_renamed_abolished_and_missing_separately() {
		let old = HashMap::from([("1111018100".into(), "옛 이름".into()), ("1111018200".into(), "구기동".into()), ("1111018300".into(), "누락".into())]);
		let new = parse_official("법정동코드\t법정동명\t폐지여부\r\n1111018100\t새 이름 \t존재\r\n1111018200\t구기동\t폐지\r\n1111018400\t신규\t존재\r\n").unwrap();
		assert_eq!(compare_official(&old, &new), CodeDiff { added: 1, renamed: 1, abolished: 1, missing: 1 });
	}
	#[test]
	fn rejects_duplicate_invalid_code_and_unknown_status() {
		for rows in ["1111018100\t행촌동\t존재\n1111018100\t행촌동\t존재", "abcdefghij\t행촌동\t존재", "1111018100\t행촌동\t미확인", ""] {
			assert!(parse_official(&format!("법정동코드\t법정동명\t폐지여부\n{rows}")).is_err());
		}
	}
}

pub fn lookup(code: &str) -> Option<String> {
	if let Ok(cache) = official_cache().read() {
		if let Some(rows) = cache.as_ref() {
			return rows.get(code).map(|row| row.name.clone());
		}
	}
	LAW_DONG
		.get_or_init(|| {
			LAW_DONG_TSV
				.lines()
				.filter_map(|line| {
					let mut fields = line.splitn(2, '\t');
					Some((fields.next()?, fields.next()?))
				})
				.collect()
		})
		.get(code)
		.map(|name| name.to_string())
}

fn cache_path() -> std::path::PathBuf {
	std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from)
		.unwrap_or_else(std::env::temp_dir).join("kras-quick").join("lawdong-official.tsv")
}

pub fn update_official() -> Result<CodeDiff, Box<dyn std::error::Error>> {
	use std::io::Read;
	let mut response = ureq::post("https://www.code.go.kr/etc/codeFullDown.do")
		.header("Content-Type", "application/x-www-form-urlencoded")
		.send("codeseId=%EB%B2%95%EC%A0%95%EB%8F%99%EC%BD%94%EB%93%9C")?;
	let mut bytes = Vec::new();
	response.body_mut().as_reader().take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
	if bytes.len() > 16 * 1024 * 1024 { return Err("공식 ZIP 크기 제한 초과".into()); }
	let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
	if zip.len() != 1 { return Err("공식 ZIP 항목 수가 예상과 다릅니다.".into()); }
	let mut file = zip.by_index(0)?;
	if !file.name().ends_with(".txt") || file.size() > 16 * 1024 * 1024 { return Err("공식 TXT 형식 또는 크기 오류".into()); }
	let mut raw = Vec::new();
	file.read_to_end(&mut raw)?;
	let (text, _, errors) = encoding_rs::EUC_KR.decode(&raw);
	if errors { return Err("공식 TXT 디코딩 오류".into()); }
	let rows = parse_official(&text)?;
	let mut cache = official_cache().write().map_err(|_| "법정동 캐시 잠금 오류")?;
	let diff = install_official(&cache_path(), &text)?;
	*cache = Some(rows);
	Ok(diff)
}

fn install_official(path: &std::path::Path, text: &str) -> Result<CodeDiff, Box<dyn std::error::Error>> {
	let rows = parse_official(text)?;
	if rows.len() < 10_000 { return Err("공식 전체자료 건수가 너무 적습니다.".into()); }
	let old = match std::fs::read_to_string(path) {
		Ok(saved) => parse_official(&saved)?.into_iter().map(|(code, row)| (code, row.name)).collect(),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => LAW_DONG_TSV.lines().filter_map(|line| line.split_once('\t')).map(|(code, name)| (code.into(), name.into())).collect(),
		Err(error) => return Err(error.into()),
	};
	let diff = compare_official(&old, &rows);
	if diff.missing != 0 { return Err(format!("기존 코드 {}개 누락: 교체하지 않습니다.", diff.missing).into()); }
	std::fs::create_dir_all(path.parent().ok_or("자료 경로 오류")?)?;
	let pending = path.with_extension("pending");
	use std::io::Write;
	let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&pending)?;
	let result = (|| -> std::io::Result<()> {
		file.write_all(text.as_bytes())?;
		file.sync_all()?;
		drop(file);
		#[cfg(windows)]
		{
			use std::os::windows::ffi::OsStrExt;
			let from: Vec<u16> = pending.as_os_str().encode_wide().chain(Some(0)).collect();
			let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
			if unsafe { windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 { return Err(std::io::Error::last_os_error()); }
		}
		#[cfg(not(windows))]
		std::fs::rename(&pending, path)?;
		Ok(())
	})();
	if result.is_err() { let _ = std::fs::remove_file(&pending); }
	result?;
	Ok(diff)
}
