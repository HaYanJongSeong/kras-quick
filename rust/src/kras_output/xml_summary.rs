use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};

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

static TAG_PATTERNS: LazyLock<HashMap<&'static str, Regex>> = LazyLock::new(|| {
	[
		"UNQ_NO", "JIBN", "LAND_LOC_NM", "BLDG_NM", "JIMOK", "PAREA", "LAREA",
		"BAREA", "GAREA", "FSI", "BLR", "STRU_NM", "ROOF_NM", "BLDG_KIND_CD",
		"HGT", "MAIN_USE_NM", "FLR", "USE_APRV_YMD", "OWN_GBN", "OWNER_NM",
	].into_iter().map(|tag| {
		let escaped = regex::escape(tag);
		let pattern = format!(
			r"(?is)<(?:(?:[A-Za-z_][\w.-]*):)?{}\b[^>]*>([\s\S]*?)</(?:(?:[A-Za-z_][\w.-]*):)?{}\s*>",
			escaped, escaped
		);
		#[expect(clippy::expect_used, reason = "fixed escaped tag names produce valid regex patterns")]
		let compiled = Regex::new(&pattern).expect("static XML tag regex");
		(tag, compiled)
	}).collect()
});

static INNER_TAGS: LazyLock<Regex> = LazyLock::new(|| {
	#[expect(clippy::expect_used, reason = "the fixed inner-tag regex is valid")]
	let compiled = Regex::new(r"<[^>]+>").expect("static inner-tag regex");
	compiled
});

pub fn parse_kras_xml(xml_content: &str) -> KrasXmlData {
	let clean: Cow<'_, str> = if xml_content.contains("ozdata") {
		Cow::Owned(xml_content.replace("ozdata=", "").replace("ozdata", ""))
	} else {
		Cow::Borrowed(xml_content)
	};
	let tag = |name: &str| -> String {
		if let Some(cap) = TAG_PATTERNS.get(name).and_then(|pattern| pattern.captures(&clean)) {
			let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
			let stripped = INNER_TAGS.replace_all(inner, "");
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
	let jibn = if jibn_tag.is_empty() { jibn_from_unq } else { jibn_tag };
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

#[cfg(test)]
mod tests {
	use super::*;

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
