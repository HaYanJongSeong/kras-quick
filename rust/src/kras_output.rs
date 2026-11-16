// TS scripts/kras-output-writer.ts + kras-output-integrity.ts — 출력 라벨·무결성 검증·경로 계약.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasBuildingOption {
	pub value: String,
	pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasOutputTarget {
	pub building: KrasBuildingOption,
	pub floor_room: Option<KrasBuildingOption>,
}

/// 파일명에 쓸 수 없는 문자 제거 + 끝의 점/공백 제거.
pub fn output_label(target: &KrasOutputTarget) -> String {
	let mut parts = Vec::new();
	if !target.building.label.is_empty() {
		parts.push(target.building.label.as_str());
	}
	if let Some(floor) = &target.floor_room {
		if !floor.label.is_empty() {
			parts.push(floor.label.as_str());
		}
	}
	let joined = parts.join("-");
	// Windows 금지 문자 제거
	let cleaned: String = joined
		.chars()
		.filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
		.collect();
	// 끝의 점/공백 제거
	cleaned.trim_end_matches(|c: char| c == '.' || c == ' ').to_string()
}

pub fn with_building_suffix(path: &str, directory: &str, building_label: &str) -> String {
	if building_label.is_empty() || path.ends_with(".jsonl") {
		return path.to_string();
	}
	let ext = std::path::Path::new(path)
		.extension()
		.and_then(|e| e.to_str())
		.map(|e| format!(".{e}"))
		.unwrap_or_default();
	let stem = path
		.strip_suffix(&ext)
		.unwrap_or(path);
	let base = std::path::Path::new(stem)
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or(stem);
	let _ = directory;
	format!("{directory}\\{base}-{building_label}{ext}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasOutputConflictError {
	pub path: String,
}

impl KrasOutputConflictError {
	pub fn new(path: impl Into<String>) -> Self {
		Self { path: path.into() }
	}
}

impl std::fmt::Display for KrasOutputConflictError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "기존 산출물을 보존하기 위해 저장을 중단합니다: {}", self.path)
	}
}

impl std::error::Error for KrasOutputConflictError {}

pub fn valid_pdf(bytes: &[u8]) -> bool {
	if bytes.len() < 5 {
		return false;
	}
	if &bytes[..5] != b"%PDF-" {
		return false;
	}
	// %%EOF in last 1024 bytes
	let tail = if bytes.len() > 1024 {
		&bytes[bytes.len() - 1024..]
	} else {
		bytes
	};
	let tail_str = String::from_utf8_lossy(tail);
	tail_str.rfind("%%EOF").is_some()
}

pub fn valid_png(bytes: &[u8]) -> bool {
	if bytes.len() < 45 {
		return false;
	}
	let sig = [137u8, 80, 78, 71, 13, 10, 26, 10];
	if bytes[..8] != sig {
		return false;
	}
	// IHDR chunk: bytes 8..12 = length (13), 12..16 = "IHDR"
	if u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) != 13 {
		return false;
	}
	if &bytes[12..16] != b"IHDR" {
		return false;
	}
	let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
	let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
	if w == 0 || h == 0 {
		return false;
	}
	// IEND trailer: last 12 bytes
	if bytes.len() < 12 {
		return false;
	}
	let tail = &bytes[bytes.len() - 12..];
	if tail != [0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130] {
		return false;
	}
	true
}

#[derive(Debug, Clone)]
pub struct SavedArtifact {
	pub file: String,
	pub bytes: usize,
	pub sha256: String,
}

pub fn describe_artifact(structure_path: &str, file: &str, bytes: &[u8]) -> SavedArtifact {
	let rel = relative_path(structure_path, file);
	let mut hasher = Sha256::new();
	hasher.update(bytes);
	let hash = format!("{:x}", hasher.finalize());
	SavedArtifact {
		file: rel,
		bytes: bytes.len(),
		sha256: hash,
	}
}

fn relative_path(structure_path: &str, file: &str) -> String {
	// structure 파일이 있는 디렉터리를 기준으로 대상까지의 상대경로(../ 허용)를 만든다.
	// TS relative(dirname(structurePath), file)와 동일 — 검증 단계가 절대경로를 거부하므로 필수다.
	let target = std::path::Path::new(file);
	if !target.is_absolute() {
		return file.to_string();
	}
	let Some(base) = std::path::Path::new(structure_path).parent() else {
		return file.to_string();
	};
	let base_components: Vec<std::path::Component> = base.components().collect();
	let target_components: Vec<std::path::Component> = target.components().collect();
	let common = base_components
		.iter()
		.zip(target_components.iter())
		.take_while(|(a, b)| a == b)
		.count();
	// 루트(드라이브/최상위)가 다르면 상대경로를 만들 수 없다 — 원본 경로 유지
	if common == 0 {
		return file.to_string();
	}
	let mut parts: Vec<String> = Vec::new();
	for _ in common..base_components.len() {
		parts.push("..".to_string());
	}
	for component in &target_components[common..] {
		parts.push(component.as_os_str().to_string_lossy().to_string());
	}
	if parts.is_empty() {
		return target
			.file_name()
			.map(|n| n.to_string_lossy().to_string())
			.unwrap_or_else(|| file.to_string());
	}
	parts.join("\\")
}

pub fn output_label_str(building_label: &str, floor_room_label: Option<&str>) -> String {
	let mut parts = Vec::new();
	if !building_label.is_empty() {
		parts.push(building_label);
	}
	if let Some(fr) = floor_room_label {
		if !fr.is_empty() {
			parts.push(fr);
		}
	}
	parts.join("-")
}

#[derive(Debug, Clone)]
pub struct PropertyAutoOutputPaths {
	pub directory: String,
	pub other_directory: String,
	pub pdf: String,
	pub page_xml: String,
	pub structure_json: String,
	pub address: String,
}

impl PropertyAutoOutputPaths {
	pub fn page_png(&self, page: usize) -> String {
		let num = safe_page_number(page).unwrap_or_else(|_| "00".to_string());
		format!("{}\\{}_페이지{}.png", self.other_directory, self.address, num)
	}
}

/// safePageNumber: 1 이상 정수, 2자리 0패딩.
fn safe_page_number(page: usize) -> Result<String, String> {
	if page < 1 {
		return Err("페이지 번호가 올바르지 않습니다.".to_string());
	}
	Ok(format!("{page:02}"))
}

pub fn property_auto_output_paths(address: &str, root: &str) -> PropertyAutoOutputPaths {
	let directory = format!("{root}\\{address}");
	let other_directory = format!("{directory}\\기타");
	let pdf = format!("{directory}\\{address}.pdf");
	let page_xml = format!("{other_directory}\\{address}.xml");
	let structure_json = format!("{other_directory}\\작업로그.jsonl");
	let addr = address.to_string();
	PropertyAutoOutputPaths {
		directory,
		other_directory,
		pdf,
		page_xml,
		structure_json,
		address: addr,
	}
}

// ——— OZ XML 파싱·구조 생성·무결성 검증 (TS kras-excel-updater + kras-output-integrity) ———

#[derive(Debug, Clone, Default)]
pub struct KrasXmlData {
	pub unq_no: String,
	pub land_loc_nm: String,
	pub bldg_nm: String,
	pub jibn: String,
	pub jimok: String,
	pub parea: String,
	pub larea: String,
	pub barea: String,
	pub garea: String,
	pub fsi: String,
	pub blr: String,
	pub stru_nm: String,
	pub roof_nm: String,
	pub bldg_kind_cd: String,
	pub hgt: String,
	pub main_use_nm: String,
	pub flr: String,
	pub use_aprv_ymd: String,
	pub own_gbn: String,
	pub owner_nm: String,
}

pub fn parse_kras_xml(xml_content: &str) -> KrasXmlData {
	let clean = xml_content.replace("ozdata=", "").replace("ozdata", "");
	let tag = |t: &str| -> String {
		let escaped = regex::escape(t);
		let pattern = format!(
			r"(?is)<(?:(?:[A-Za-z_][\w.-]*):)?{}\b[^>]*>([\s\S]*?)</(?:(?:[A-Za-z_][\w.-]*):)?{}\s*>",
			escaped, escaped
		);
		let re = Regex::new(&pattern).unwrap();
		if let Some(cap) = re.captures(&clean) {
			let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
			// strip inner tags
			let stripped = Regex::new(r"<[^>]+>").unwrap().replace_all(inner, "");
			stripped.trim().to_string()
		} else {
			String::new()
		}
	};
	let unq_no = tag("UNQ_NO");
	let jibn_parts: Vec<&str> = unq_no.split('-').collect();
	let jibn_from_unq = if jibn_parts.len() >= 4 {
		let main = jibn_parts[2].trim().parse::<i64>().unwrap_or(0);
		let sub = jibn_parts[3].trim().parse::<i64>().unwrap_or(0);
		format!("{main}-{sub}")
	} else {
		String::new()
	};
	let jibn_tag = tag("JIBN");
	let jibn = if jibn_tag.is_empty() {
		jibn_from_unq
	} else {
		jibn_tag
	};
	KrasXmlData {
		unq_no,
		land_loc_nm: tag("LAND_LOC_NM"),
		bldg_nm: tag("BLDG_NM"),
		jibn,
		jimok: tag("JIMOK"),
		parea: tag("PAREA"),
		larea: tag("LAREA"),
		barea: tag("BAREA"),
		garea: tag("GAREA"),
		fsi: tag("FSI"),
		blr: tag("BLR"),
		stru_nm: tag("STRU_NM"),
		roof_nm: tag("ROOF_NM"),
		bldg_kind_cd: tag("BLDG_KIND_CD"),
		hgt: tag("HGT"),
		main_use_nm: tag("MAIN_USE_NM"),
		flr: tag("FLR"),
		use_aprv_ymd: tag("USE_APRV_YMD"),
		own_gbn: tag("OWN_GBN"),
		owner_nm: tag("OWNER_NM"),
	}
}

pub fn create_oz_structure(xml: &KrasXmlData) -> Value {
	json!({
		"address": xml.land_loc_nm,
		"buildingName": xml.bldg_nm,
		"owner": xml.owner_nm,
		"landCategory": xml.jimok,
		"area": xml.parea,
		"registrationInfo": xml.unq_no,
		"buildingArea": xml.barea,
		"landArea": xml.larea,
		"grossArea": xml.garea,
		"floorSpaceRatio": xml.fsi,
		"buildingCoverageRatio": xml.blr,
		"structure": xml.stru_nm,
		"roof": xml.roof_nm,
		"buildingKind": xml.bldg_kind_cd,
		"height": xml.hgt,
		"mainUse": xml.main_use_nm,
		"floors": xml.flr,
		"constructionDate": xml.use_aprv_ymd,
		"ownershipType": xml.own_gbn,
	})
}

// 무결성 검증 (TS verifyKrasOutput)
pub fn verify_kras_output(
	paths: &PropertyAutoOutputPaths,
	structure_path: &str,
	target: &KrasOutputTarget,
) -> bool {
	// contained path check
	let root_real = match std::fs::canonicalize(&paths.directory) {
		Ok(p) => p,
		Err(_) => return false,
	};
	// structure path must be inside root
	let struct_real = match std::fs::canonicalize(structure_path) {
		Ok(p) => p,
		Err(_) => return false,
	};
	let rel = match struct_real.strip_prefix(&root_real) {
		Ok(r) => r,
		Err(_) => return false,
	};
	if rel.as_os_str().is_empty() {
		return false;
	}

	// identity
	let identity = serde_json::to_string(&json!([
		{ "value": target.building.value, "label": target.building.label },
		target.floor_room.as_ref().map(|f| json!({"value": f.value, "label": f.label}))
	])).unwrap_or_default();

	// read structure
	let structure_text = match std::fs::read_to_string(structure_path) {
		Ok(t) => t,
		Err(_) => return false,
	};
	let structure: Value = match serde_json::from_str::<Value>(&structure_text).ok()
		.filter(|value| value.get("_krasIntegrity").is_some()).or_else(|| {
		crate::output_log::entries(structure_path, "output").into_iter().rev().find(|value| {
			let saved = &value["_krasIntegrity"];
			serde_json::to_string(&json!([saved["building"], saved["floorRoom"]])).ok().as_ref() == Some(&identity)
		})
	}) {
		Some(v) => v,
		None => return false,
	};
	let integrity = match structure.get("_krasIntegrity") {
		Some(v) => v.clone(),
		None => return false,
	};
	// version check
	let version = integrity.get("version").and_then(|v| v.as_i64());
	if !matches!(version, Some(1) | Some(2) | Some(3)) {
		return false;
	}
	// identity check
	let stored_identity = serde_json::to_string(&json!([
		integrity.get("building").cloned().unwrap_or(Value::Null),
		integrity.get("floorRoom").cloned().unwrap_or(Value::Null)
	])).unwrap_or_default();
	if stored_identity != identity {
		return false;
	}
	// artifacts
	let artifact_schema = |a: &Value| -> bool {
		a.get("file").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false)
			&& a.get("bytes").and_then(|v| v.as_i64()).map(|n| n > 0).unwrap_or(false)
			&& a.get("sha256").and_then(|v| v.as_str()).map(|s| s.len() == 64).unwrap_or(false)
	};
	let pdf_art = match integrity.get("pdf") { Some(v) => v, None => return false };
	let xml_art = match integrity.get("pageXml") { Some(v) => v, None => return false };
	let json_art = if version == Some(3) {
		match integrity.get("dataJson") { Some(value) => Some(value), None => return false }
	} else { None };
	let pngs_art = match integrity.get("pagePngs").and_then(|v| v.as_array()) {
		Some(a) if !a.is_empty() || matches!(version, Some(2) | Some(3)) => a,
		_ => return false,
	};
	for a in std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art)) {
		if !artifact_schema(a) { return false; }
	}

	// resolve artifact files
	let struct_dir = std::path::Path::new(structure_path).parent().unwrap_or(std::path::Path::new("."));
	let mut files: Vec<PathBuf> = Vec::new();
	for art in std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art)) {
		let f = match art.get("file").and_then(|v| v.as_str()) {
			Some(s) => s,
			None => return false,
		};
		if std::path::Path::new(f).is_absolute() {
			return false;
		}
		let resolved = struct_dir.join(f);
		// contained check
		let resolved_real = match std::fs::canonicalize(&resolved) {
			Ok(p) => p,
			Err(_) => return false,
		};
		if !resolved_real.starts_with(&root_real) {
			return false;
		}
		files.push(resolved_real);
	}
	// uniqueness + not structure itself
	let struct_real_clone = struct_real.clone();
	if files.iter().collect::<std::collections::HashSet<_>>().len() != files.len() {
		return false;
	}
	if files.contains(&struct_real_clone) {
		return false;
	}

	let watched: Vec<PathBuf> = std::iter::once(struct_real.clone()).chain(files.iter().cloned()).collect();

	// fingerprint before
	let fingerprint = |paths: &[PathBuf]| -> Option<String> {
		let parts: Vec<String> = paths
			.iter()
			.map(|p| {
				let meta = std::fs::metadata(p).ok()?;
				let size = meta.len();
				let mtime = meta
					.modified()
					.ok()?
					.duration_since(std::time::UNIX_EPOCH)
					.ok()?
					.as_nanos();
				let ctime = meta
					.created()
					.ok()?
					.duration_since(std::time::UNIX_EPOCH)
					.ok()?
					.as_nanos();
				Some(format!("{}:{}:{}:{}", size, mtime, ctime, p.display()))
			})
			.collect::<Option<Vec<_>>>()?;
		Some(parts.join("|"))
	};
	let before = match fingerprint(&watched) {
		Some(f) => f,
		None => return false,
	};

	// verify bytes + sha256
	let bytes_list: Vec<Vec<u8>> = match files.iter().map(|f| std::fs::read(f)).collect::<Result<Vec<_>, _>>() {
		Ok(b) => b,
		Err(_) => return false,
	};
	let artifacts = std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art));
	for (i, art) in artifacts.enumerate() {
		let data = &bytes_list[i];
		let expected_bytes = art.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
		let expected_sha = art.get("sha256").and_then(|v| v.as_str()).unwrap_or("");
		if data.len() != expected_bytes {
			return false;
		}
		let mut h = Sha256::new();
		h.update(data);
		let actual_sha = format!("{:x}", h.finalize());
		if actual_sha != expected_sha {
			return false;
		}
	}

	// validCapture: pdf, pngs, xml
	let pdf_bytes = &bytes_list[0];
	let xml_bytes = bytes_list.last().unwrap();
	let xml_str = String::from_utf8_lossy(xml_bytes);
	let png_start = if json_art.is_some() { 2 } else { 1 };
	let pngs: Vec<&[u8]> = bytes_list[png_start..bytes_list.len() - 1].iter().map(|v| v.as_slice()).collect();
	if json_art.is_some() {
		let saved = serde_json::from_slice::<Value>(&bytes_list[1]).ok();
		let expected = crate::full_xml_json::convert(&xml_str).ok();
		if saved.is_none() || saved != expected { return false; }
	}
	if !valid_pdf(pdf_bytes) {
		return false;
	}
	for p in &pngs {
		if !valid_png(p) {
			return false;
		}
	}
	let xml_data = parse_kras_xml(&xml_str);
	if !crate::kras_auto_stages::same_parcel_address(&xml_data.land_loc_nm, &paths.address) {
		return false;
	}

	// structure equality
	let saved_structure = {
		let mut s = structure.clone();
		if let Some(obj) = s.as_object_mut() {
			obj.remove("_krasIntegrity");
		}
		s
	};
	let expected_structure = create_oz_structure(&xml_data);
	if saved_structure != expected_structure {
		return false;
	}

	// fingerprint unchanged
	let after = match fingerprint(&watched) {
		Some(f) => f,
		None => return false,
	};
	if after != before {
		return false;
	}

	true
}

pub fn is_kras_stage3_output_complete(
	paths: &PropertyAutoOutputPaths,
	building: &KrasBuildingOption,
	floor_room: Option<&KrasBuildingOption>,
) -> bool {
	let target = KrasOutputTarget {
		building: building.clone(),
		floor_room: floor_room.cloned(),
	};
	let label = output_label(&target);
	let structure_path = with_building_suffix(&paths.structure_json, &paths.other_directory, &label);
	verify_kras_output(paths, &structure_path, &target) || {
		let legacy = format!("{}\\oz-structure.json", paths.other_directory);
		let legacy = with_building_suffix(&legacy, &paths.other_directory, &label);
		verify_kras_output(paths, &legacy, &target)
	}
}
pub fn split_jibn(jibn: &str) -> (String, String) {
	match jibn.split_once('-') {
		Some((main, sub)) => (main.to_string(), sub.to_string()),
		None => (jibn.to_string(), String::new()),
	}
}


pub fn parse_flr(flrcd: &str) -> (String, String) {
	match flrcd.split_once('/') {
		Some((ground, basement)) => (ground.to_string(), basement.to_string()),
		None => (flrcd.to_string(), "0".to_string()),
	}
}

/// TS buildingAge — 건축일자 연도와 현재 연도의 차이.

/// TS buildingAge — 건축일자 연도와 현재 연도의 차이.
pub fn building_age(use_aprv_ymd: &str) -> String {
	if use_aprv_ymd.is_empty() {
		return String::new();
	}
	let re = regex::Regex::new(r"(\d{4})\.(\d{2})\.(\d{2})").expect("static regex");
	let Some(caps) = re.captures(use_aprv_ymd) else {
		return String::new();
	};
	let year: i64 = caps[1].parse().unwrap_or(0);
	let now = time_now_year();
	(now - year).to_string()
}


fn time_now_year() -> i64 {
	let secs = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_secs())
		.unwrap_or(0);
	// 2000-01-01 기준 일수에서 연도 역산 없이 평년 근사 대신 정확 산출:
	// 초를 연도로 바꾸려면 반복 계산이 필요 — 1년 = 31_556_952초(평균)로 근사하면
	// 연말 경계에서 1년 오차가 날 수 있어 반복 보정한다.
	let mut year: i64 = 1970;
	let mut remaining = secs as i64;
	loop {
		let days = if is_leap(year) { 366 } else { 365 };
		let year_secs = days * 86_400;
		if remaining >= year_secs {
			remaining -= year_secs;
			year += 1;
		} else {
			break;
		}
	}
	year
}

fn is_leap(year: i64) -> bool {
	(year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// TS printKrasDataTable — XML 추출값 표 출력.
pub fn print_kras_data_table(data: &KrasXmlData) {
	let (jibn_main, jibn_sub) = split_jibn(&data.jibn);
	let (ground, basement) = parse_flr(&data.flr);
	let land = data
		.land_loc_nm
		.split('(')
		.next()
		.unwrap_or(&data.land_loc_nm)
		.trim()
		.to_string();
	let rows: [(&str, String); 17] = [
		("소재지", land),
		("본번", jibn_main),
		("부번", jibn_sub),
		("공부지목", data.jimok.clone()),
		("연면적", data.garea.clone()),
		("공부면적", data.garea.clone()),
		("집합건물구분", data.bldg_kind_cd.clone()),
		("건물높이", data.hgt.clone()),
		("건축일자", data.use_aprv_ymd.clone()),
		("용적률", data.fsi.clone()),
		("공부용도", data.main_use_nm.clone()),
		("건물연령", building_age(&data.use_aprv_ymd)),
		("건물구조", data.stru_nm.clone()),
		("지붕구조", data.roof_nm.clone()),
		("건폐율", data.blr.clone()),
		("지상층수", ground),
		("지하층수", basement),
	];
	let mut table = String::from("\n=== XML 추출값 ===\n");
	for (k, v) in rows {
		table.push_str(&format!("{:<14}{}\n", k, v));
	}
	crate::task_screen::task_status(&table);
}

#[cfg(test)]
mod tests {

	#[test]
	fn split_jibn_parses_main_and_sub() {
		assert_eq!(split_jibn("392-19"), ("392".to_string(), "19".to_string()));
		assert_eq!(split_jibn("392"), ("392".to_string(), String::new()));
	}

	#[test]
	fn parse_flr_handles_missing_basement() {
		assert_eq!(parse_flr("5/1"), ("5".to_string(), "1".to_string()));
		assert_eq!(parse_flr(""), (String::new(), "0".to_string()));
	}

	#[test]
	fn building_age_computes_year_delta() {
		assert_eq!(building_age("1991.09.11"), (now_year() - 1991).to_string());
		assert_eq!(building_age(""), "");
		assert_eq!(building_age("알수없음"), "");
	}

	fn now_year() -> i64 {
		super::time_now_year()
	}
	use super::*;

	#[test]
	fn output_label_joins_and_sanitizes() {
		let target = KrasOutputTarget {
			building: KrasBuildingOption { value: "".into(), label: "A".into() },
			floor_room: Some(KrasBuildingOption { value: "".into(), label: "B".into() }),
		};
		assert_eq!(output_label(&target), "A-B");
	}

	#[test]
	fn output_label_empty_floor() {
		let target = KrasOutputTarget {
			building: KrasBuildingOption { value: "".into(), label: "".into() },
			floor_room: None,
		};
		assert_eq!(output_label(&target), "");
	}

	#[test]
	fn valid_pdf_checks_header_and_eof() {
		let mut b = b"%PDF-1.4\n".to_vec();
		b.extend_from_slice(b"%%EOF\n");
		assert!(valid_pdf(&b));
		assert!(!valid_pdf(b"not pdf"));
		assert!(!valid_pdf(b"%PDF-x"));
	}

	#[test]
	fn valid_png_checks_signature_and_ihdr() {
		// Minimal valid PNG signature + IHDR + IEND
		let mut b = vec![137u8, 80, 78, 71, 13, 10, 26, 10];
		// IHDR chunk: length 13
		b.extend_from_slice(&13u32.to_be_bytes());
		b.extend_from_slice(b"IHDR");
		b.extend_from_slice(&100u32.to_be_bytes()); // width
		b.extend_from_slice(&50u32.to_be_bytes()); // height
		b.extend_from_slice(&[8, 2, 0, 0, 0]); // bitdepth, colortype, compress, filter, interlace
		// CRC (dummy, not checked)
		b.extend_from_slice(&[0, 0, 0, 0]);
		// IEND
		b.extend_from_slice(&[0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]);
		assert!(valid_png(&b));
		assert!(!valid_png(b"not png"));
		assert!(!valid_png(&[137, 80, 78, 71]));
	}

	#[test]
	fn describe_artifact_hashes() {
		let a = describe_artifact("C:\\dir\\oz-structure.json", "C:\\dir\\file.pdf", b"hello");
		assert_eq!(a.bytes, 5);
		assert_eq!(a.file, "file.pdf");
		// sha256 of "hello"
		assert_eq!(a.sha256, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
	}

	#[test]
	fn property_auto_output_paths_builds() {
		let p = property_auto_output_paths("서울특별시 용산구 용산동2가 5-227", "C:\\root");
		assert_eq!(p.directory, "C:\\root\\서울특별시 용산구 용산동2가 5-227");
		assert!(p.pdf.ends_with(".pdf"));
		assert_eq!(p.page_png(1), "C:\\root\\서울특별시 용산구 용산동2가 5-227\\기타\\서울특별시 용산구 용산동2가 5-227_페이지01.png");
		assert_eq!(p.page_png(2), "C:\\root\\서울특별시 용산구 용산동2가 5-227\\기타\\서울특별시 용산구 용산동2가 5-227_페이지02.png");
	}

	#[test]
	fn safe_page_number_rejects_zero() {
		assert!(safe_page_number(0).is_err());
		assert_eq!(safe_page_number(1).unwrap(), "01");
		assert_eq!(safe_page_number(12).unwrap(), "12");
	}

	#[test]
	fn with_building_suffix_appends_label() {
		let r = with_building_suffix("C:\\dir\\file.pdf", "C:\\dir", "B1");
		assert!(r.contains("B1"), "{r}");
		assert!(r.ends_with(".pdf"), "{r}");
	}

	#[test]
	fn with_building_suffix_empty_label_unchanged() {
		let r = with_building_suffix("C:\\dir\\file.pdf", "C:\\dir", "");
		assert_eq!(r, "C:\\dir\\file.pdf");
	}

	#[test]
	fn parses_kras_xml_and_creates_oz_structure() {
		let xml = "<REAL_LAND_BLDG><LAND_LOC_NM>서울특별시 용산구 용산동2가 5-227</LAND_LOC_NM><UNQ_NO>1-2-3-4</UNQ_NO><BLDG_NM>테스트빌딩</BLDG_NM></REAL_LAND_BLDG>";
		let data = parse_kras_xml(xml);
		assert_eq!(data.land_loc_nm, "서울특별시 용산구 용산동2가 5-227");
		assert_eq!(data.unq_no, "1-2-3-4");
		assert_eq!(data.bldg_nm, "테스트빌딩");
		let structure = create_oz_structure(&data);
		assert_eq!(structure.get("address").and_then(|v| v.as_str()), Some("서울특별시 용산구 용산동2가 5-227"));
		assert_eq!(structure.get("buildingName").and_then(|v| v.as_str()), Some("테스트빌딩"));
	}

	#[test]
	fn parse_kras_xml_handles_namespaced_tags() {
		let xml = "<ns:ROOT><ns:LAND_LOC_NM>주소</ns:LAND_LOC_NM></ns:ROOT>";
		let data = parse_kras_xml(xml);
		assert_eq!(data.land_loc_nm, "주소");
	}
}
