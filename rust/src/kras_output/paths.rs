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
	let cleaned: String = joined
		.chars()
		.filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
		.collect();
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
	let stem = path.strip_suffix(&ext).unwrap_or(path);
	let base = std::path::Path::new(stem)
		.file_name()
		.and_then(|n| n.to_str())
		.unwrap_or(stem);
	format!("{directory}\\{base}-{building_label}{ext}")
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

/// 1 이상 정수, 2자리 0패딩.
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

#[cfg(test)]
mod tests {
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
}
