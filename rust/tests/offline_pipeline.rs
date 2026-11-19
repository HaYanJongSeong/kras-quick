// 라이브(KRAS/브라우저) 없이 처음(주소 조회)~최종 PDF 출력까지 전 구간을 검증하는 통합 테스트.
// 가짜 페이지/뷰어로 Stage 1 → Stage 2 → Stage 3 산출물(PDF·PNG·XML·구조 JSON)과 검증까지 한 흐름으로 돌린다.

use std::cell::RefCell;
use std::path::PathBuf;

use kras_quick::auto_batch::{AutoStage1Result, AutoBuilding};
use kras_quick::kras_auto_stages::{
	property_auto_stage_paths, run_kras_auto_stage3, KrasAutoStage2Result, KrasAutoStage3Result,
	KrasOzViewerSurface,
};
use kras_quick::kras_output::{
	parse_kras_xml, valid_pdf, valid_png, verify_kras_output, KrasBuildingOption,
	KrasOutputTarget,
};
use kras_quick::workflow_watcher::{
	run_kras_workflow_watcher, BuildingChoice, LookupResult, WorkflowHooks, WorkflowPage,
	WorkflowStage2,
};

const ADDRESS: &str = "서울특별시 노원구 월계동 392-19";

/// 건물 1개(일반) + 층-호 1개를 항상 돌려주는 가짜 KRAS 페이지.
struct FakePage {
	building: AutoBuilding,
	floors: Vec<AutoBuilding>,
	floor_selected: RefCell<Option<String>>,
}

impl WorkflowPage for FakePage {
	fn url(&self) -> String {
		"https://www.kras.go.kr/kras/cert/certView.do".to_string()
	}

	fn lookup(&self, _address: &str) -> Result<LookupResult, Box<dyn std::error::Error>> {
		Ok(LookupResult::Lookup {
			has_building: true,
			building_options: vec![self.building.clone()],
			no_parcel: false,
		})
	}

	fn read_building_options(&self) -> Vec<AutoBuilding> {
		vec![self.building.clone()]
	}

	fn select_building(
		&self,
		_value: &str,
	) -> Result<Vec<AutoBuilding>, Box<dyn std::error::Error>> {
		Ok(self.floors.clone())
	}

	fn select_floor_room(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> {
		*self.floor_selected.borrow_mut() = Some(value.to_string());
		Ok(())
	}
}

/// 건물 1번을 고르는 훅(TS chooseBuilding이 index를 돌려주는 자리).
struct PickFirstBuilding;
impl WorkflowHooks for PickFirstBuilding {
	fn choose_building(&mut self, _options: &[AutoBuilding]) -> BuildingChoice {
		BuildingChoice::Index(1)
	}
}

/// 캡처된 PDF·PNG·XML을 미리 만들어 두는 가짜 OZ 뷰어.
struct FakeViewer {
	png: Vec<u8>,
	pdf: Vec<u8>,
	xml: String,
}

impl KrasOzViewerSurface for FakeViewer {
	fn url(&self) -> String {
		"https://kras.go.kr/oz80/ozhviewer/report.jsp".to_string()
	}

	fn has_visible_captcha(&self) -> bool {
		false
	}

	fn capture_pdf(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
		Ok(self.pdf.clone())
	}

	fn capture_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
		Ok(vec![self.png.clone(), self.png.clone()])
	}

	fn read_report_xml(&self) -> Result<String, Box<dyn std::error::Error>> {
		Ok(self.xml.clone())
	}
}

fn tiny_png() -> Vec<u8> {
	let image = image::RgbaImage::from_pixel(16, 12, image::Rgba([180, 40, 40, 255]));
	let dynamic = image::DynamicImage::ImageRgba8(image);
	let mut bytes = Vec::new();
	dynamic
		.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
		.expect("png 인코딩");
	bytes
}

#[test]
fn batch_dash_reuses_the_same_verified_land_output() {
	let root = temp_root("batch-dash");
	let address = kras_quick::quick_address::resolve_work_address("-").unwrap();
	let paths = property_auto_stage_paths(&address, Some(root.to_str().unwrap()));
	assert_eq!(paths.directory, property_auto_stage_paths(ADDRESS, Some(root.to_str().unwrap())).directory);
	let png = tiny_png();
	let viewer = FakeViewer {
		pdf: kras_quick::oz_capture::build_pdf_from_pngs(&[png.clone(), png.clone()]).unwrap(),
		png,
		xml: format!("<REAL_LAND_BLDG><LAND_LOC_NM>{ADDRESS}</LAND_LOC_NM><JIBN>392-19</JIBN></REAL_LAND_BLDG>"),
	};
	let stage2 = KrasAutoStage2Result {
		status: "completed".into(), address,
		building: kras_quick::auto_batch::land_building(), floor_room: None,
	};
	assert!(matches!(run_kras_auto_stage3(&stage2, &viewer, &paths).unwrap(), KrasAutoStage3Result::Completed { .. }));
	assert!(!kras_quick::auto_batch::is_auto_target_new(&paths, &kras_quick::auto_batch::AutoTarget::Land));
	assert!(!root.join("-").exists());
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn all_stage_records_share_one_append_only_log() {
	let root = temp_root("single-log");
	let paths = property_auto_stage_paths(ADDRESS, Some(&root.display().to_string()));
	for path in [&paths.stage1_json, &paths.stage2_json, &paths.stage3_json, &paths.done_buildings_json] {
		assert_eq!(path, &paths.structure_json);
	}
	for (kind, value) in [("stage1", 1), ("stage2", 2), ("stage1", 3)] {
		kras_quick::output_log::append(&paths.structure_json, kind, &serde_json::json!({"value": value})).unwrap();
	}
	assert_eq!(kras_quick::output_log::latest(&paths.structure_json, "stage1").unwrap()["value"], 3);
	assert_eq!(kras_quick::output_log::latest(&paths.structure_json, "stage2").unwrap()["value"], 2);
	assert_eq!(std::fs::read_dir(&paths.other_directory).unwrap().count(), 1);
	std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn existing_pdf_conflict_preserves_original_and_never_completes() {
	let root = temp_root("existing-pdf");
	let paths = property_auto_stage_paths(ADDRESS, Some(&root.display().to_string()));
	let stage2 = KrasAutoStage2Result {
		status: "completed".into(), address: ADDRESS.into(),
		building: KrasBuildingOption { value: String::new(), label: String::new() },
		floor_room: None,
	};
	let png = tiny_png();
	let viewer = FakeViewer {
		pdf: kras_quick::oz_capture::build_pdf_from_pngs(&[png.clone(), png.clone()]).unwrap(),
		png,
		xml: format!("<REAL_LAND_BLDG><LAND_LOC_NM>{ADDRESS}</LAND_LOC_NM><JIBN>392-19</JIBN></REAL_LAND_BLDG>"),
	};
	std::fs::create_dir_all(&paths.directory).unwrap();
	let original = b"existing-output-must-survive";
	std::fs::write(&paths.pdf, original).unwrap();
	let error = run_kras_auto_stage3(&stage2, &viewer, &paths).unwrap_err().to_string();
	let preserved = std::fs::read(&paths.pdf).unwrap();
	let completion_exists = std::path::Path::new(&paths.stage3_json).exists();
	std::fs::remove_dir_all(&root).unwrap();
	assert!(error.contains(&paths.pdf));
	assert_eq!(preserved, original);
	assert!(!completion_exists);
}

#[test]
fn xml_write_failure_never_reports_completed() {
	let root = temp_root("xml-write-failure");
	let paths = property_auto_stage_paths(ADDRESS, Some(&root.display().to_string()));
	let stage2 = KrasAutoStage2Result {
		status: "completed".into(), address: ADDRESS.into(),
		building: KrasBuildingOption { value: "0".into(), label: String::new() },
		floor_room: None,
	};
	let png = tiny_png();
	let viewer = FakeViewer {
		pdf: kras_quick::oz_capture::build_pdf_from_pngs(&[png.clone(), png.clone()]).unwrap(),
		png,
		xml: format!("<REAL_LAND_BLDG><LAND_LOC_NM>{ADDRESS}</LAND_LOC_NM><JIBN>392-19</JIBN></REAL_LAND_BLDG>"),
	};
	std::fs::create_dir_all(&paths.page_xml).unwrap();
	let result = run_kras_auto_stage3(&stage2, &viewer, &paths);
	let write_error = result.as_ref().err().and_then(|error| error.downcast_ref::<std::io::Error>()).is_some();
	let completed = matches!(result, Ok(KrasAutoStage3Result::Completed { .. }));
	let completion_record = std::fs::read_to_string(&paths.stage3_json).unwrap_or_default();
	std::fs::remove_dir_all(&root).unwrap();
	assert!(!completed, "XML write failure must not return Completed");
	assert!(write_error, "must propagate the actual filesystem write error");
	assert!(!completion_record.contains("\"completed\""));
}

fn temp_root(tag: &str) -> PathBuf {
	let nanos = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.expect("시계")
		.as_nanos();
	let dir = std::env::temp_dir().join(format!(
		"kras-offline-e2e-{tag}-{}-{nanos}",
		std::process::id()
	));
	std::fs::create_dir_all(&dir).expect("임시 루트");
	dir
}

#[test]
fn different_document_address_is_saved_without_rewriting_original_data() {
	let root = temp_root("stale-document");
	let paths = property_auto_stage_paths(ADDRESS, Some(root.to_str().expect("root")));
	std::fs::create_dir_all(&paths.other_directory).expect("stage 기록 폴더");
	let png = tiny_png();
	let viewer = FakeViewer {
		pdf: kras_quick::oz_capture::build_pdf_from_pngs(&[png.clone()]).expect("PDF"),
		png,
		xml: "<REAL_LAND_BLDG><LAND_LOC_NM>서울특별시 종로구 행촌동 210-730</LAND_LOC_NM></REAL_LAND_BLDG>".into(),
	};
	let stage2 = KrasAutoStage2Result {
		status: "completed".into(), address: ADDRESS.into(),
		building: KrasBuildingOption { value: String::new(), label: String::new() },
		floor_room: None,
	};
	let result = run_kras_auto_stage3(&stage2, &viewer, &paths).expect("저장 결과");
	let output = match result {
		KrasAutoStage3Result::Completed { output, .. } => output,
		KrasAutoStage3Result::Blocked { reason, .. } => panic!("주소 차이만으로 차단되면 안 됨: {reason}"),
	};
	assert_eq!(std::fs::read(&output.pdf).unwrap(), viewer.pdf);
	assert_eq!(std::fs::read_to_string(&output.page_xml).unwrap(), viewer.xml);
	let data: serde_json::Value = serde_json::from_slice(&std::fs::read(&output.data_json).unwrap()).unwrap();
	assert_eq!(data, kras_quick::full_xml_json::convert(&viewer.xml).unwrap());
	assert!(output.pdf.contains(ADDRESS));
	let record = kras_quick::output_log::latest(&output.structure_json, "output").unwrap();
	assert_eq!(record["address"], "서울특별시 종로구 행촌동 210-730");
	assert_eq!(record["_krasIntegrity"]["requestedAddress"], ADDRESS);
	assert_eq!(record["_krasIntegrity"]["documentAddress"], "서울특별시 종로구 행촌동 210-730");
	assert_eq!(record["_krasIntegrity"]["addressMismatch"], true);
	let target = KrasOutputTarget { building: stage2.building.clone(), floor_room: None };
	assert!(verify_kras_output(&paths.output_paths(), &output.structure_json, &target));
	assert!(kras_quick::kras_output::is_kras_stage3_output_complete(&paths.output_paths(), &stage2.building, None));
	assert!(run_kras_auto_stage3(&stage2, &viewer, &paths).is_err());
	assert_eq!(std::fs::read_to_string(&output.page_xml).unwrap(), viewer.xml);
	for field in ["requestedAddress", "documentAddress", "addressMismatch", "dataJson"] {
		let mut damaged = record.clone();
		damaged["_krasIntegrity"].as_object_mut().unwrap().remove(field);
		kras_quick::output_log::append(&output.structure_json, "output", &damaged).unwrap();
		assert!(!verify_kras_output(&paths.output_paths(), &output.structure_json, &target), "missing {field}");
	}
	for (field, value) in [("requestedAddress", serde_json::json!("other request")),
		("documentAddress", serde_json::json!("other document")), ("addressMismatch", serde_json::json!(false)),
		("version", serde_json::json!(3))] {
		let mut damaged = record.clone();
		damaged["_krasIntegrity"][field] = value;
		kras_quick::output_log::append(&output.structure_json, "output", &damaged).unwrap();
		assert!(!verify_kras_output(&paths.output_paths(), &output.structure_json, &target), "invalid {field}");
	}
	kras_quick::output_log::append(&output.structure_json, "output", &record).unwrap();
	assert!(verify_kras_output(&paths.output_paths(), &output.structure_json, &target));
	std::fs::remove_dir_all(root).expect("테스트 정리");
}

#[test]
fn mismatch_permission_does_not_accept_corrupt_or_missing_capture_data() {
	for invalid in ["pdf", "png", "xml"] {
		let root = temp_root("invalid-mismatch");
		let paths = property_auto_stage_paths(ADDRESS, Some(root.to_str().unwrap()));
		let png = tiny_png();
		let mut viewer = FakeViewer {
			pdf: kras_quick::oz_capture::build_pdf_from_pngs(std::slice::from_ref(&png)).unwrap(), png,
			xml: "<REAL_LAND_BLDG><LAND_LOC_NM>서울특별시 종로구 행촌동 210-730</LAND_LOC_NM></REAL_LAND_BLDG>".into(),
		};
		match invalid { "pdf" => viewer.pdf.clear(), "png" => viewer.png.clear(), "xml" => viewer.xml.clear(), _ => unreachable!() }
		let stage2 = KrasAutoStage2Result { status: "completed".into(), address: ADDRESS.into(),
			building: KrasBuildingOption { value: String::new(), label: String::new() }, floor_room: None };
		let result = run_kras_auto_stage3(&stage2, &viewer, &paths).unwrap();
		assert!(matches!(result, KrasAutoStage3Result::Blocked { reason, .. } if reason == "invalid_capture"));
		assert!(!std::path::Path::new(&paths.pdf).exists());
		assert!(!std::path::Path::new(&paths.page_xml).exists());
		std::fs::remove_dir_all(root).unwrap();
	}
}

#[test]
fn offline_pipeline_from_lookup_to_final_pdf() {
	// ——— Stage 1~2: 조회 → 건물 선택 → 층-호 선택 ———
	let page = FakePage {
		building: AutoBuilding {
			index: 1,
			value: "101219021".to_string(),
			label: "(일반)101219021-건물".to_string(),
		},
		floors: vec![AutoBuilding {
			index: 1,
			value: "401".to_string(),
			label: "401호".to_string(),
		}],
		floor_selected: RefCell::new(None),
	};

	let mut hooks = PickFirstBuilding;
	let mut stage1 = AutoStage1Result::default();
	let mut stage2: Option<WorkflowStage2> = None;
	let result = run_kras_workflow_watcher(
		&page,
		ADDRESS,
		&mut hooks,
		false,
		&mut stage1,
		&mut stage2,
	)
	.expect("워크플로 완주");

	assert_eq!(stage1.status, "completed");
	assert_eq!(stage1.building_options.len(), 1);
	assert_eq!(stage1.building_options[0].label, "(일반)101219021-건물");
	assert!(!result.awaiting_user, "건물 선택으로 사용자 대기를 만들지 않는다");

	let stage2_ws = stage2.expect("stage2 기록");
	assert_eq!(stage2_ws.status, "completed");
	assert_eq!(stage2_ws.building.label, "(일반)101219021-건물");
	// 층-호가 1개면 자동 선택된다
	let floor = stage2_ws.floor_room.as_ref().expect("층-호 자동 선택");
	assert_eq!(floor.label, "401호");
	assert_eq!(page.floor_selected.borrow().as_deref(), Some("401"));

	// main.rs와 동일하게 WorkflowStage2 → KrasAutoStage2Result 매핑
	let stage2 = KrasAutoStage2Result {
		status: stage2_ws.status.clone(),
		address: stage2_ws.address.clone(),
		building: KrasBuildingOption {
			value: stage2_ws.building.value.clone(),
			label: stage2_ws.building.label.clone(),
		},
		floor_room: stage2_ws.floor_room.as_ref().map(|floor| KrasBuildingOption {
			value: floor.value.clone(),
			label: floor.label.clone(),
		}),
	};

	// ——— Stage 3: 캡처 → PDF/PNG/XML/구조 JSON 저장 ———
	let png = tiny_png();
	let pdf = kras_quick::oz_capture::build_pdf_from_pngs(&[png.clone(), png.clone()])
		.expect("PDF 합성");
	let xml = concat!(
		"<?xml version='1.0' encoding='euc-kr'?>",
		"<REAL_LAND_BLDG>",
		"<LAND_LOC_NM>서울특별시 노원구 월계동  392-19</LAND_LOC_NM>",
		"<UNQ_NO>11350-101-392-0019</UNQ_NO>",
		"<JIBN>392-19</JIBN><JIMOK>대</JIMOK>",
		"<GAREA>326.42</GAREA><PAREA>326.42</PAREA><BAREA>65.62</BAREA>",
		"<STRU_NM>철근콘크리트조</STRU_NM><ROOF_NM>평스라부</ROOF_NM>",
		"<FLR>4/1</FLR><USE_APRV_YMD>1991.09.11</USE_APRV_YMD>",
		"</REAL_LAND_BLDG>"
	);
	let viewer = FakeViewer { png: png.clone(), pdf, xml: xml.to_string() };

	let root = temp_root("stage3");
	let root_arg = root.display().to_string();
	let paths = property_auto_stage_paths(ADDRESS, Some(root_arg.as_str()));

	let outcome = run_kras_auto_stage3(&stage2, &viewer, &paths).expect("stage3 완주");
	let (version, status, address, output) = match outcome {
		KrasAutoStage3Result::Completed { version, stage, status, address, output } => {
			assert_eq!(stage, 3);
			(version, status, address, output)
		}
		KrasAutoStage3Result::Blocked { reason, .. } => {
			panic!("stage3 차단: {reason}");
		}
	};

	// 산출물 실측 — 존재·형식·내용 검증
	assert_eq!(version, 1);
	assert_eq!(status, "completed");
	assert_eq!(address, ADDRESS);

	let pdf_bytes = std::fs::read(&output.pdf).expect("PDF 파일");
	assert!(valid_pdf(&pdf_bytes), "PDF 헤더/트레일러 검증");
	assert!(output.pdf.ends_with(".pdf"));

	assert_eq!(output.page_pngs.len(), 0);
	for png_path in &output.page_pngs {
		let bytes = std::fs::read(png_path).expect("PNG 파일");
		assert!(valid_png(&bytes), "PNG 서명 검증: {png_path}");
	}

	let xml_text = std::fs::read_to_string(&output.page_xml).expect("XML 파일");
	let parsed = parse_kras_xml(&xml_text);
	assert_eq!(parsed.land_loc_nm, "서울특별시 노원구 월계동  392-19");
	assert_eq!(parsed.jibn, "392-19");
	assert_eq!(parsed.jimok, "대");

	let structure_text = std::fs::read_to_string(&output.structure_json).expect("구조 JSON");
	let structure = kras_quick::output_log::latest(&output.structure_json, "output").expect("통합 로그 산출물 정보");
	assert_eq!(structure["_krasIntegrity"]["version"], 4);
	let files: Vec<_> = std::fs::read_dir(&paths.other_directory).unwrap()
		.map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
	assert_eq!(files.len(), 3, "기타에는 XML·전체 JSON·통합 로그만 저장: {files:?}");
	let full_json: serde_json::Value = serde_json::from_slice(&std::fs::read(&output.data_json).unwrap()).unwrap();
	assert_eq!(full_json, kras_quick::full_xml_json::convert(&xml_text).unwrap());
	assert_eq!(std::path::Path::new(&output.data_json).file_stem(), std::path::Path::new(&output.page_xml).file_stem());
	assert!(output.page_xml.contains(ADDRESS));
	assert!(output.page_xml.contains("401호"));

	// 산출물 검증기(TS verifyKrasOutput) 통과
	let target = KrasOutputTarget {
		building: stage2.building.clone(),
		floor_room: stage2.floor_room.clone(),
	};
	let verified = verify_kras_output(&paths.output_paths(), &output.structure_json, &target);
	assert!(verified, "verify_kras_output 실패");

	// stage3.json 기록까지 확인
	let stage3_path = &paths.stage3_json;
	let stage3_text = std::fs::read_to_string(&stage3_path).expect("stage3.json");
	assert!(stage3_text.contains("\"completed\""));
	assert!(!structure_text.is_empty());
	assert!(kras_quick::kras_output::is_kras_stage3_output_complete(
		&paths.output_paths(), &stage2.building, stage2.floor_room.as_ref()
	));
	let mut second = stage2.clone();
	second.floor_room = Some(KrasBuildingOption { value: "402".into(), label: "402호".into() });
	let second_result = run_kras_auto_stage3(&second, &viewer, &paths).unwrap();
	assert!(matches!(second_result, KrasAutoStage3Result::Completed { .. }));
	assert_eq!(kras_quick::output_log::entries(&paths.structure_json, "output").len(), 2);
	for target in [&stage2, &second] {
		assert!(kras_quick::kras_output::is_kras_stage3_output_complete(
			&paths.output_paths(), &target.building, target.floor_room.as_ref()
		), "통합 로그에서 각 층호를 독립 검증해야 함");
	}
	let mut legacy = structure.clone();
	legacy["_krasIntegrity"]["version"] = serde_json::json!(2);
	legacy["_krasIntegrity"].as_object_mut().unwrap().remove("dataJson");
	let legacy_path = kras_quick::kras_output::with_building_suffix(
		&format!("{}\\oz-structure.json", paths.other_directory), &paths.other_directory,
		&kras_quick::kras_output::output_label(&target),
	);
	std::fs::write(&legacy_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
	assert!(verify_kras_output(&paths.output_paths(), &legacy_path, &target));
	let json_bytes = std::fs::read(&output.data_json).unwrap();
	std::fs::write(&output.data_json, b"damaged-json").unwrap();
	assert!(!kras_quick::kras_output::is_kras_stage3_output_complete(
		&paths.output_paths(), &stage2.building, stage2.floor_room.as_ref()
	), "legacy verification must not bypass a damaged current JSON");
	std::fs::write(&output.data_json, json_bytes).unwrap();
	std::fs::write(&output.pdf, b"damaged").unwrap();
	assert!(!kras_quick::kras_output::is_kras_stage3_output_complete(
		&paths.output_paths(), &stage2.building, stage2.floor_room.as_ref()
	), "damaged output must not be skipped as completed");

	let _ = std::fs::remove_dir_all(&root);
}
