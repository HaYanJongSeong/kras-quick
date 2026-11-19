// TS scripts/kras-auto-stages.ts — stage-3 캡처 오케스트레이션·경로·무결성.

use std::path::{Path, PathBuf};

use crate::kras_output::{
	create_oz_structure, is_kras_stage3_output_complete, output_label, parse_kras_xml,
	property_auto_output_paths, verify_kras_output, with_building_suffix, KrasBuildingOption,
	KrasOutputTarget, PropertyAutoOutputPaths, SavedArtifact, describe_artifact, valid_pdf,
	valid_png,
};
use crate::numeric_ocr::OcrOutcome;

#[derive(Debug, Clone)]
pub struct KrasAutoStagePaths {
	pub directory: String,
	pub other_directory: String,
	pub pdf: String,
	pub page_png: String, // template; use page_png_fn
	pub page_xml: String,
	pub structure_json: String,
	pub stage1_json: String,
	pub stage2_json: String,
	pub stage3_json: String,
	pub done_buildings_json: String,
	pub address: String,
	pub other_dir_clone: String,
}

// page_png is a function in TS; store address+other_directory and compute.
impl KrasAutoStagePaths {
	pub fn page_png_path(&self, page: usize) -> String {
		let num = if page < 1 {
			"00".to_string()
		} else {
			format!("{page:02}")
		};
		format!("{}\\{}_페이지{}.png", self.other_directory, self.address, num)
	}

	/// 완료 판정용 출력 경로 집합으로 변환한다.
	pub fn output_paths(&self) -> crate::kras_output::PropertyAutoOutputPaths {
		crate::kras_output::PropertyAutoOutputPaths {
			directory: self.directory.clone(),
			other_directory: self.other_directory.clone(),
			pdf: self.pdf.clone(),
			page_xml: self.page_xml.clone(),
			structure_json: self.structure_json.clone(),
			address: self.address.clone(),
		}
	}
}

pub fn property_auto_stage_paths(address: &str, root: Option<&str>) -> KrasAutoStagePaths {
	let root = root.unwrap_or("C:\\Users\\admin\\Desktop\\시유재산\\부동산종합증명서");
	let base = property_auto_output_paths(address, root);
	KrasAutoStagePaths {
		directory: base.directory.clone(),
		other_directory: base.other_directory.clone(),
		pdf: base.pdf.clone(),
		page_png: base.page_png(1),
		page_xml: base.page_xml.clone(),
		structure_json: base.structure_json.clone(),
		stage1_json: base.structure_json.clone(),
		stage2_json: base.structure_json.clone(),
		stage3_json: base.structure_json.clone(),
		done_buildings_json: base.structure_json.clone(),
		address: address.to_string(),
		other_dir_clone: base.other_directory.clone(),
	}
}

pub fn is_viewer_url(value: &str) -> bool {
	// origin https://kras.go.kr + pathname /oz80/ozhviewer/report.jsp
	if let Some(rest) = value.strip_prefix("https://kras.go.kr") {
		let path = rest.split('?').next().unwrap_or(rest);
		let path = path.split('#').next().unwrap_or(path);
		path == "/oz80/ozhviewer/report.jsp"
	} else {
		false
	}
}

pub(crate) fn same_parcel_address(left: &str, right: &str) -> bool {
	let normalize = |v: &str| -> String {
		v.trim().split_whitespace().collect::<Vec<_>>().join(" ")
	};
	let strip = |v: String| -> String {
		if let Some(idx) = v.find('(') {
			v[..idx].trim().to_string()
		} else {
			v
		}
	};
	let nl = strip(normalize(left));
	let nr = strip(normalize(right));
	if nl == nr {
		return true;
	}
	// fallback: compare normalized (no PNU parsing in Rust port yet)
	false
}

// Browser surface trait — TS KrasOzViewerSurface
pub trait KrasOzViewerSurface {
	fn url(&self) -> String;
	fn has_visible_captcha(&self) -> bool;
	fn capture_pdf(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>>;
	fn capture_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>>;
	fn take_captured_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
		self.capture_page_pngs()
	}
	fn read_report_xml(&self) -> Result<String, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct KrasOzCapture {
	pub pdf: Vec<u8>,
	pub page_pngs: Vec<Vec<u8>>,
	pub page_xml: String,
}

#[derive(Debug, Clone)]
pub struct KrasAutoStage2Result {
	pub status: String,
	pub address: String,
	pub building: KrasBuildingOption,
	pub floor_room: Option<KrasBuildingOption>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct KrasAutoStage3Output {
	pub data_json: String,
	pub pdf: String,
	pub page_pngs: Vec<String>,
	pub page_xml: String,
	pub structure_json: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum KrasAutoStage3Result {
	Completed {
		version: i32,
		stage: i32,
		status: String,
		address: String,
		output: KrasAutoStage3Output,
	},
	Blocked {
		version: i32,
		stage: i32,
		status: String,
		address: String,
		reason: String,
	},
}

pub fn valid_capture(capture: &KrasOzCapture) -> bool {
	crate::kras_output::valid_pdf(&capture.pdf)
		&& !capture.page_pngs.is_empty()
		&& capture.page_pngs.iter().all(|p| crate::kras_output::valid_png(p))
		&& !parse_kras_xml(&capture.page_xml).land_loc_nm.is_empty()
}

/// runKrasAutoStage3 orchestration (TS parity). Surface is the browser abstraction.
pub fn run_kras_auto_stage3<S: KrasOzViewerSurface>(
	stage2: &KrasAutoStage2Result,
	surface: &S,
	paths: &KrasAutoStagePaths,
) -> Result<KrasAutoStage3Result, Box<dyn std::error::Error>> {
	if stage2.status != "completed" {
		return Err("Stage 2 must complete before Stage 3".into());
	}
	let result = if !is_viewer_url(&surface.url()) {
		KrasAutoStage3Result::Blocked {
			version: 1,
			stage: 3,
			status: "blocked".to_string(),
			address: stage2.address.clone(),
			reason: "viewer_not_open".to_string(),
		}
	} else if surface.has_visible_captcha() {
		KrasAutoStage3Result::Blocked {
			version: 1,
			stage: 3,
			status: "blocked".to_string(),
			address: stage2.address.clone(),
			reason: "captcha_required".to_string(),
		}
	} else {
		crate::task_screen::task_status("Stage 3: PDF 캡처 중...\n");
		let pdf = surface.capture_pdf()?;
		crate::task_screen::task_status("Stage 3: PDF 캡처 완료\n");
		crate::task_screen::task_status("Stage 3: PNG 캡처 중...\n");
		let page_pngs = surface.take_captured_page_pngs()?;
		crate::task_screen::task_status("Stage 3: PNG 캡처 완료\n");
		crate::task_screen::task_status("Stage 3: XML 읽기 중...\n");
		let page_xml = surface.read_report_xml()?;
		crate::task_screen::task_status("Stage 3: XML 읽기 완료\n");

		let capture = KrasOzCapture {
			pdf,
			page_pngs,
			page_xml,
		};
		let xml_data = parse_kras_xml(&capture.page_xml);
		let address_mismatch = !same_parcel_address(&xml_data.land_loc_nm, &stage2.address);
		if !valid_capture(&capture) {
			KrasAutoStage3Result::Blocked {
				version: 1,
				stage: 3,
				status: "blocked".to_string(),
				address: stage2.address.clone(),
				reason: "invalid_capture".to_string(),
			}
		} else {
			if address_mismatch {
				let addresses = serde_json::json!({"requestedAddress":stage2.address,"documentAddress":xml_data.land_loc_nm});
				crate::task_screen::task_status(&format!("주소 불일치 기록 후 저장: {addresses}\n"));
			}
			drop(capture.page_pngs);
			// saveKrasOutput equivalent: write files
			let label = output_label(&KrasOutputTarget {
				building: stage2.building.clone(),
				floor_room: stage2.floor_room.clone(),
			});
			let pdf_path = with_building_suffix(&paths.pdf, &paths.directory, &label);
			let xml_path = with_building_suffix(&paths.page_xml, &paths.other_directory, &label);
			let structure_path = with_building_suffix(&paths.structure_json, &paths.other_directory, &label);
			let data_json_path = format!("{}.json", xml_path.strip_suffix(".xml").unwrap_or(&xml_path));
			let data_json_bytes = serde_json::to_vec_pretty(&crate::full_xml_json::convert(&capture.page_xml)?)?;

			std::fs::create_dir_all(&paths.directory)?;
			std::fs::create_dir_all(&paths.other_directory)?;

			// resolve conflict: throw if exists (TS resolveDestination default)
			if let Some(conflict) = [&pdf_path, &xml_path, &data_json_path].into_iter().find(|path| std::path::Path::new(path).is_file()) {
				return Err(format!("기존 산출물을 보존하기 위해 저장을 중단합니다: {conflict}").into());
			}
			use crate::exclusive_output::write_new;
			write_new(&pdf_path, &capture.pdf)?;
			let png_paths: Vec<String> = Vec::new();
			let xml_bytes = capture.page_xml.as_bytes();
			write_new(&xml_path, xml_bytes)?;
			write_new(&data_json_path, &data_json_bytes)?;
			let oz = create_oz_structure(&xml_data);
			// TS integritySchema(version/building/floorRoom/pdf/pagePngs/pageXml) —
			// 아티팩트(파일·바이트·sha256)가 있어야 verifyKrasOutput 통과 후 완료 판정이 된다.
			let artifact = |path: &str, bytes: &[u8]| -> serde_json::Value {
				let a = describe_artifact(&structure_path, path, bytes);
				serde_json::json!({ "file": a.file, "bytes": a.bytes, "sha256": a.sha256 })
			};
			let page_png_artifacts: Vec<serde_json::Value> = Vec::new();
			let mut integrity = serde_json::json!({
				"version": 4,
				"requestedAddress": stage2.address,
				"documentAddress": xml_data.land_loc_nm,
				"addressMismatch": address_mismatch,
				"building": { "value": stage2.building.value, "label": stage2.building.label },
				"floorRoom": stage2.floor_room.as_ref().map(|f| serde_json::json!({"value": f.value, "label": f.label})),
			});
			if let Some(obj) = integrity.as_object_mut() {
				obj.insert("pdf".to_string(), artifact(&pdf_path, &capture.pdf));
				obj.insert(
					"pagePngs".to_string(),
					serde_json::Value::Array(page_png_artifacts),
				);
				obj.insert("pageXml".to_string(), artifact(&xml_path, xml_bytes));
				obj.insert("dataJson".to_string(), artifact(&data_json_path, &data_json_bytes));
			}
			let mut struct_obj = oz.clone();
			if let Some(obj) = struct_obj.as_object_mut() {
				obj.insert("_krasIntegrity".to_string(), integrity);
			}
			crate::output_log::append(&structure_path, "output", &struct_obj)?;
			let target = KrasOutputTarget {
				building: stage2.building.clone(),
				floor_room: stage2.floor_room.clone(),
			};
			if !verify_kras_output(&paths.output_paths(), &structure_path, &target) {
				return Err("저장된 산출물 검증에 실패했습니다. 완료 처리하지 않습니다.".into());
			}

			KrasAutoStage3Result::Completed {
				version: 1,
				stage: 3,
				status: "completed".to_string(),
				address: stage2.address.clone(),
				output: KrasAutoStage3Output {
					data_json: data_json_path,
					pdf: pdf_path,
					page_pngs: png_paths,
					page_xml: xml_path,
					structure_json: structure_path,
				},
			}
		}
	};

	// write stage3 result
	crate::output_log::append(&paths.stage3_json, "stage3", &serde_json::to_value(&result)?)?;
	Ok(result)
}

pub fn write_kras_auto_stage_result(path: &str, result: &KrasAutoStage3Result) -> Result<(), Box<dyn std::error::Error>> {
	crate::output_log::append(path, "stage3", &serde_json::to_value(result)?)?;
	Ok(())
}

// need url crate? is_viewer_url uses url parsing. Implement manually.
// (is_viewer_url already uses url::Url — add dependency or manual parse)

// ——— done-buildings.json 판독·추가 (TS readDoneBuildingIndexes / appendDoneBuildingIndex) ———

/// 완료 건물 value 목록. 과거 숫자 인덱스는 식별자로 재사용하지 않는다.
pub fn read_done_building_values(paths: &KrasAutoStagePaths) -> Vec<String> {
	let Some(value) = crate::output_log::latest(&paths.done_buildings_json, "done-buildings") else {
		return Vec::new();
	};
	let Some(arr) = value.as_array() else {
		return Vec::new();
	};
	arr.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()
}

/// 완료 건물 value를 중복 없이 기록한다.
pub fn append_done_building_value(paths: &KrasAutoStagePaths, value: &str) -> std::io::Result<()> {
	let mut done = read_done_building_values(paths);
	if !done.iter().any(|saved| saved == value) {
		done.push(value.to_owned());
	}
	if let Some(idx) = paths.done_buildings_json.rfind(['\\', '/']) {
		if idx > 0 {
			let dir = &paths.done_buildings_json[..idx];
			std::fs::create_dir_all(dir)?;
		}
	}
	crate::output_log::append(&paths.done_buildings_json, "done-buildings", &serde_json::to_value(&done)?)
}

#[cfg(test)]
mod done_buildings_tests {
	use super::*;

	fn paths_in(dir: &std::path::Path) -> KrasAutoStagePaths {
		let mut p = property_auto_stage_paths("주소", Some(dir.to_str().unwrap()));
		p.done_buildings_json = dir.join("done-buildings.json").to_string_lossy().to_string();
		p
	}

	#[test]
	fn missing_file_returns_empty() {
		let dir = std::env::temp_dir().join("kras-done-missing");
		let _ = std::fs::create_dir_all(&dir);
		let p = paths_in(&dir);
		assert!(read_done_building_values(&p).is_empty());
	}

	#[test]
	fn append_creates_file_and_dedupes() {
		let dir = std::env::temp_dir().join("kras-done-append");
		let _ = std::fs::create_dir_all(&dir);
		let p = paths_in(&dir);
		let _ = std::fs::remove_file(&p.done_buildings_json);

		append_done_building_value(&p, "1002113972").unwrap();
		append_done_building_value(&p, "10021100187359").unwrap();
		append_done_building_value(&p, "1002113972").unwrap();

		assert_eq!(read_done_building_values(&p), vec!["1002113972", "10021100187359"]);
	}

	#[test]
	fn invalid_json_returns_empty() {
		let dir = std::env::temp_dir().join("kras-done-invalid");
		let _ = std::fs::create_dir_all(&dir);
		let p = paths_in(&dir);
		std::fs::write(&p.done_buildings_json, "not json").unwrap();
		assert!(read_done_building_values(&p).is_empty());
	}
}
