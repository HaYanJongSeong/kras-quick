use kras_quick::kras_output::{create_oz_structure, parse_kras_xml, KrasXmlData};
use regex::Regex;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[test]
fn first_namespaced_tag_wins_when_case_attributes_and_nested_tags_vary() {
	// Given
	let xml = "<a:land_loc_nm id='1'> 서울 <b>주소</b> &amp; </z:LAND_LOC_NM ><LAND_LOC_NM>다음</LAND_LOC_NM>";
	// When
	let data = parse_kras_xml(xml);
	// Then
	assert_eq!(data.land_loc_nm, "서울 주소 &amp;");
}

#[test]
fn ozdata_replacement_applies_when_markers_occur_inside_values() {
	// Given
	let xml = "ozdata=<ROOT><BLDG_NM>ozdata=앞ozdata뒤</BLDG_NM></ROOT>";
	// When
	let data = parse_kras_xml(xml);
	// Then
	assert_eq!(data.bldg_nm, "앞뒤");
}

#[test]
fn jibn_falls_back_when_tag_is_missing_or_strips_to_empty() {
	// Given
	let cases = [
		("<UNQ_NO>1-2-003-0004</UNQ_NO>", "3-4"),
		("<UNQ_NO>1-2-bad- 9 </UNQ_NO><JIBN><b> </b></JIBN>", "0-9"),
		("<UNQ_NO>1-2-3</UNQ_NO>", ""),
		("<UNQ_NO>1-2-3-4</UNQ_NO><JIBN> 007-8 </JIBN>", "007-8"),
	];
	for (xml, expected) in cases {
		// When
		let data = parse_kras_xml(xml);
		// Then
		assert_eq!(data.jibn, expected);
	}
}

#[test]
fn summary_preserves_strings_when_all_fields_are_present() {
	// Given
	let fields = [
		("LAND_LOC_NM", "address"), ("BLDG_NM", "buildingName"),
		("OWNER_NM", "owner"), ("JIMOK", "landCategory"), ("PAREA", "area"),
		("UNQ_NO", "registrationInfo"), ("BAREA", "buildingArea"),
		("LAREA", "landArea"), ("GAREA", "grossArea"), ("FSI", "floorSpaceRatio"),
		("BLR", "buildingCoverageRatio"), ("STRU_NM", "structure"),
		("ROOF_NM", "roof"), ("BLDG_KIND_CD", "buildingKind"), ("HGT", "height"),
		("MAIN_USE_NM", "mainUse"), ("FLR", "floors"),
		("USE_APRV_YMD", "constructionDate"), ("OWN_GBN", "ownershipType"),
	];
	let xml: String = fields.iter().map(|(tag, value)| format!("<{tag}> 00{value}&amp; </{tag}>")).collect();
	// When
	let summary = create_oz_structure(&parse_kras_xml(&xml));
	// Then
	let expected: serde_json::Map<String, serde_json::Value> = fields.iter()
		.map(|(_, key)| (key.to_string(), serde_json::json!(format!("00{key}&amp;")))).collect();
	assert_eq!(summary, serde_json::Value::Object(expected));
}

#[test]
fn empty_summary_is_string_valued_when_tags_are_unclosed_or_absent() {
	// Given
	let xml = "<LAND_LOC_NM>unclosed<BLDG_NM/>";
	// When
	let summary = create_oz_structure(&parse_kras_xml(xml));
	// Then
	assert_eq!(summary.as_object().unwrap().len(), 19);
	assert!(summary.as_object().unwrap().values().all(|value| value == ""));
}

#[test]
fn cached_parser_matches_frozen_baseline_with_and_without_ozdata_markers() {
	// Given
	let cases = [
		"<LAND_LOC_NM>주소</LAND_LOC_NM><UNQ_NO>1-2-003-04</UNQ_NO>",
		"ozdata=<LAND_LOC_NM>ozdata=주소ozdata</LAND_LOC_NM>",
		"<a:land_loc_nm> 첫 <b>주소</b> </z:LAND_LOC_NM><LAND_LOC_NM>다음</LAND_LOC_NM>",
		"<BLDG_NM>OZDATA ozdataozdata= ozdata=ozdata</BLDG_NM>",
		"<UNQ_NO>1-2-bad-9</UNQ_NO><JIBN> <b/> </JIBN>",
		"<LAND_LOC_NM>unclosed",
		"",
	];
	for xml in cases {
		let baseline = baseline_parse(xml);
		// When
		let optimized = parse_kras_xml(xml);
		// Then
		assert_eq!(create_oz_structure(&optimized), create_oz_structure(&baseline));
		assert_eq!(optimized.jibn, baseline.jibn);
	}
}

// Frozen pre-cache parser: intentionally independent for before/after measurements.
fn baseline_parse(xml_content: &str) -> KrasXmlData {
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

#[test]
#[ignore = "synthetic timing; run explicitly in release mode with --nocapture"]
fn synthetic_parse_median_matches_frozen_baseline() {
	// Given
	let xml = format!("ozdata=<ns:ROOT><ns:LAND_LOC_NM> 서울 <b>주소</b> </ns:LAND_LOC_NM><UNQ_NO>1-2-003-0004</UNQ_NO><OWNER_NM>소유자</OWNER_NM>{}</ns:ROOT>", "<UNUSED>other</UNUSED>".repeat(100));
	let expected = baseline_parse(&xml);
	let expected_structure = create_oz_structure(&expected);
	let mut baseline_times = Vec::new();
	let mut optimized_times = Vec::new();
	black_box(parse_kras_xml(&xml));
	// When
	for _ in 0..101 {
		let start = Instant::now();
		let baseline = baseline_parse(black_box(&xml));
		baseline_times.push(start.elapsed());
		let start = Instant::now();
		let optimized = parse_kras_xml(black_box(&xml));
		optimized_times.push(start.elapsed());
		// Then
		assert_eq!(create_oz_structure(&baseline), expected_structure);
		assert_eq!(create_oz_structure(&optimized), expected_structure);
		assert_eq!(optimized.jibn, expected.jibn);
	}
	baseline_times.sort_unstable();
	optimized_times.sort_unstable();
	let median = |times: &[Duration]| times[times.len() / 2];
	eprintln!("101 parses, synthetic XML {} bytes: baseline median {:?}, optimized median {:?}", xml.len(), median(&baseline_times), median(&optimized_times));
}
