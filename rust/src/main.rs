// 런처 진입점 — CDP 표면 기반 직접 실행 (node/TS 없음).

use kras_quick::auto_batch::{
	format_auto_failures, AutoAddressFailure, AutoAddressFailures, AutoBatchState, AutoBuilding,
	AutoTarget,
};
use kras_quick::cdp_surface::CdpSurface;
use kras_quick::kras_auto_stages::{
	property_auto_stage_paths, run_kras_auto_stage3, KrasAutoStage2Result,
	KrasAutoStage3Result, KrasOzViewerSurface,
};
use kras_quick::kras_output::KrasBuildingOption;
use kras_quick::quick_address::{
	active_address_prompt, append_auto_addresses, classify_menu_input,
	split_quick_address_batch, AutoBatchScope, MenuAction, AUTO_SCOPE_PROMPT, DEFAULT_ADDRESS,
	EXIT_CONFIRM_PROMPT, LOGIN_PROMPT, QUICK_ADDRESS_PROMPT,
};
use kras_quick::stage12::handle_captcha_loop;
use kras_quick::workflow_watcher::{self, BuildingChoice, FloorChoice, WorkflowHooks};
use std::io::{self, Write};

const ENDPOINT: &str = "http://127.0.0.1:9333";

use kras_quick::quick_address::lock_stdin_reader as lock_reader;

fn read_line(prompt: &str) -> String {
	kras_quick::task_screen::pause_task_screen();
	kras_quick::output::write(prompt);
	io::stdout().flush().ok();
	lock_reader().read_line()
}

/// 주소 메뉴 입력 — TS readQuickAddress처럼 버퍼에 남은 줄을 함께 배치로 합친다.
fn read_menu_input(prompt: &str) -> String {
	kras_quick::task_screen::pause_task_screen();
	kras_quick::output::write(prompt);
	io::stdout().flush().ok();
	let input = {
		let mut guard = lock_reader();
		guard.read_line().trim().to_string()
	};
	let pending = lock_reader().drain_pending();
	kras_quick::quick_address::join_pending_lines(&input, &pending)
}

/// 주소 1건 실행 모드.
enum RunMode {
	/// 대화형 — 건물·층호를 사용자가 고른다.
	Interactive,
	FloorReturn { value: String },
	/// AUTO 배치 — 대상(target)에 따라 건물/층호가 정해진다. `building`이 없으면 토지(TS 수동 stage2).
	Auto { building: Option<kras_quick::auto_batch::AutoBuilding>, floor: Option<kras_quick::auto_batch::AutoBuilding> },
}

impl RunMode {
	fn uses_automatic_captcha(&self) -> bool {
		matches!(self, Self::Auto { .. })
	}
}

#[cfg(test)]
mod menu_return_tests {
	use super::*;
	#[test]
	#[ignore = "사용자가 요청한 실패 4개 필지만 실제 복구"]
	fn live_recover_failed_sindang_parcels() {
		std::env::var("KRAS_QUICK_ROOT").expect("복구 저장 루트를 명시해야 합니다.");
		let pnus = ["1114016200100850002", "1114016200100850099", "1114016200100850111", "1114016200100850244"];
		let mut queue = pnus.iter().map(|value| value.to_string()).collect();
		run_auto_batch(&mut queue, AutoBatchScope::Full);
		for pnu in pnus {
			let address = kras_quick::quick_address::resolve_work_address(pnu).unwrap();
			let paths = property_auto_stage_paths(&address, quick_output_root().as_deref());
			let output = paths.output_paths();
			assert!(kras_quick::kras_output::is_kras_stage3_output_complete(&output, &kras_quick::auto_batch::land_building(), None), "토지 복구 실패: {address}");
			let stage1 = read_stage1_from_disk(&address).expect("복구 조회 기록");
			for building in stage1.building_options {
				if building.is_aggregate() {
					let stages = kras_quick::output_log::entries(&paths.stage2_json, "stage2");
					let stage = stages.iter().rev().find(|stage| stage["building"]["value"].as_str() == Some(building.value.as_str())).expect("집합건물 층호 기록");
					let floors = stage["floorRoomOptions"].as_array().expect("층호 목록");
					for floor in floors {
						let floor = KrasBuildingOption { value: floor["value"].as_str().unwrap().into(), label: floor["label"].as_str().unwrap().into() };
						assert!(kras_quick::kras_output::is_kras_stage3_output_complete(&output, &building.to_output_option(), Some(&floor)), "층호 복구 실패: {address} {}", floor.label);
					}
				} else {
					assert!(kras_quick::kras_output::is_kras_stage3_output_complete(&output, &building.to_output_option(), None), "건물 복구 실패: {address}");
				}
			}
			println!("RECOVERY_VERIFIED={address}");
		}
	}
	#[test]
	fn public_modes_require_manual_captcha() {
		assert!(!kras_quick::quick_address::QUICK_ADDRESS_PROMPT.contains("AUTO"));
		assert!(!RunMode::Interactive.uses_automatic_captcha());
		assert!(!RunMode::FloorReturn { value: "building".into() }.uses_automatic_captcha());
		assert!(RunMode::Auto { building: None, floor: None }.uses_automatic_captcha());
	}
	#[test]
	#[ignore = "실제 집합건물 한 동의 전체 층호를 AUTO 큐로 끝까지 출력"]
	fn live_all_rooms_one_building() {
		let root = std::env::var("KRAS_QUICK_ROOT").expect("별도 검증 루트");
		assert!(!std::path::Path::new(&root).exists(), "기존 산출물 재사용 금지");
		let address = "서울특별시 노원구 월계동 18";
		assert!(run_single_address(address, RunMode::Auto { building: None, floor: None }));
		let original = read_stage1_from_disk(address).expect("실제 건물 목록");
		let building = original.building_options.into_iter().find(|b| b.value == "101213228").expect("101동");
		let mut state = kras_quick::auto_batch::AutoBatchState::new();
		let land = AutoTarget::Land;
		state.mark_completed(&land, Some(kras_quick::auto_batch::AutoStage1Result {
			status: "completed".into(), reason: None, building_options: vec![building.clone()],
		}), false);
		let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
		let mut expected = Vec::new();
		let mut completed = Vec::new();
		let mut iterations = 0;
		while let Some(target) = state.next_target(&paths, AutoBatchScope::Full) {
			iterations += 1;
			assert!(iterations < 1000, "AUTO 큐 반복 상한 초과");
			let mut discovered = Vec::new();
			let is_new = state.target_is_new(&paths, &target);
			assert!(run_address_with_options(address, run_mode_for(&target), &mut discovered, true), "대상 실패: {}", kras_quick::auto_batch::auto_target_label(&target));
			state.mark_completed(&target, None, is_new);
			match target {
				AutoTarget::AggregateTotal { .. } => {
					assert!(!discovered.is_empty());
					expected = discovered.clone();
					println!("EXPECTED_ROOM_COUNT={}", expected.len());
					state.set_floor_queue(&building, discovered);
				},
				AutoTarget::AggregateFloor { floor, .. } => {
					assert!(kras_quick::kras_output::is_kras_stage3_output_complete(&paths.output_paths(), &building.to_output_option(), Some(&floor.to_output_option())));
					let label = kras_quick::kras_output::output_label(&kras_quick::kras_output::KrasOutputTarget {
						building: building.to_output_option(), floor_room: Some(floor.to_output_option()),
					});
					let xml_path = kras_quick::kras_output::with_building_suffix(&paths.page_xml, &paths.other_directory, &label);
					let xml = std::fs::read_to_string(xml_path).expect("호별 XML");
					let room = regex::Regex::new(r"<HO_NM>([^<]*)</HO_NM>").unwrap();
					assert_eq!(room.captures(&xml).expect("호명칭").get(1).unwrap().as_str().trim(), floor.label.trim());
					assert!(xml.contains("<DONG_NM>101동</DONG_NM>"));
					assert!(!completed.contains(&floor.value), "층호 중복 완료");
					completed.push(floor.value.clone());
					println!("ROOM_VERIFIED={}/{} VALUE={} LABEL={}", completed.len(), expected.len(), floor.value, floor.label);
				},
				_ => panic!("검증 범위 밖 대상"),
			}
		}
		assert_eq!(completed, expected.iter().map(|floor| floor.value.clone()).collect::<Vec<_>>());
		println!("ALL_ROOMS_VERIFIED={} MISSING=0 DUPLICATE=0", completed.len());
	}
	#[test]
	#[ignore = "로그인된 KRAS 실제 캡처 및 프로그램 자체 OZ 종료 검증"]
	fn live_capture_closes_its_viewer() {
		let root = std::env::var("KRAS_QUICK_ROOT").expect("별도 검증 루트");
		assert!(!std::path::Path::new(&root).exists());
		let before = kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT).unwrap();
		assert!(run_single_address("서울특별시 노원구 월계동 18", RunMode::Auto {
			building: Some(AutoBuilding { index: 2, value: "101213228".into(), label: "(집합)그랑빌아파트 101동".into() }),
			floor: Some(AutoBuilding { index: 1, value: "1012198908".into(), label: "101".into() }),
		}));
		let after = kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT).unwrap();
		assert!(!after.iter().any(|page| kras_quick::kras_auto_stages::is_viewer_url(&page.url)
			&& !before.iter().any(|old| old.ws_url == page.ws_url)));
		for old in before.iter().filter(|page| !kras_quick::kras_auto_stages::is_viewer_url(&page.url)) {
			assert!(after.iter().any(|page| page.ws_url == old.ws_url));
		}
		println!("OWN_VIEWER_CLOSED=true OTHER_PAGES_PRESERVED=true");
	}
	#[test]
	#[ignore = "실제 OZ 동일 마지막 페이지의 줌별 렌더링 진단"]
	fn live_oz_zoom_diagnostic() {
		let root = std::env::var("KRAS_QUICK_ROOT").expect("검증 루트");
		assert!(!std::path::Path::new(&root).exists());
		let ok = run_single_address("서울특별시 노원구 월계동 18", RunMode::Auto {
			building: Some(AutoBuilding { index: 2, value: "101213228".into(), label: "(집합)그랑빌아파트 101동".into() }),
			floor: Some(AutoBuilding { index: 2, value: "1012198834".into(), label: "102".into() }),
		});
		if !ok { kras_quick::cdp_surface::close_viewer_tabs(ENDPOINT); panic!("진단 문서 출력 실패"); }
		let result = (|| -> Result<(), Box<dyn std::error::Error>> {
			let viewer = CdpSurface::connect_viewer(ENDPOINT, 1000, &[])?;
			let instrument = r#"(() => {
				window.__krasFontProbe = {fonts:[], notes:[]};
				const original = CanvasRenderingContext2D.prototype.fillText;
				CanvasRenderingContext2D.prototype.fillText = function(text,x,y,maxWidth) {
					const p=window.__krasFontProbe;
					if(!p.fonts.includes(this.font))p.fonts.push(this.font);
					if(String(text).includes('부동산종합증명서'))p.notes.push({text:String(text),font:this.font,x,y,maxWidth,width:this.measureText(text).width,transform:[this.getTransform().a,this.getTransform().d]});
					return original.apply(this,arguments);
				};
				return true;
			})()"#;
			viewer.send_cdp("Runtime.evaluate", serde_json::json!({"expression":instrument,"returnByValue":true}))?;
			for zoom in [100, 200, 300] {
				let expression = format!("(async()=>{{await document.fonts.ready;const s=document.querySelector('select.oz_ui_layout');if(!s)throw Error('zoom missing');s.value='{zoom}%';s.dispatchEvent(new Event('change',{{bubbles:true}}));await new Promise(r=>setTimeout(r,2500));return {{zoom:s.value,page:document.querySelector(\"input[title='현재 페이지'], input[title='Current page']\")?.value,canvases:[...document.querySelectorAll('canvas')].map(c=>[c.width,c.height])}};}})()");
				let state = viewer.send_cdp("Runtime.evaluate", serde_json::json!({"expression":expression,"awaitPromise":true,"returnByValue":true}))?;
				println!("ZOOM_STATE={state}");
				let fonts = viewer.send_cdp("Runtime.evaluate", serde_json::json!({"expression":"JSON.stringify({probe:window.__krasFontProbe,available:[...document.fonts].map(f=>({family:f.family,status:f.status})),checks:{gulim:document.fonts.check('12px Gulim'),batang:document.fonts.check('12px Batang'),malgun:document.fonts.check('12px \"Malgun Gothic\"')}})","returnByValue":true}))?;
				println!("FONT_PROBE={fonts}");
				let png = kras_quick::oz_capture::capture_stable_canvas(&viewer)?.ok_or("줌 렌더링 불안정")?;
				std::fs::write(std::path::Path::new(&root).join(format!("zoom-{zoom}.png")), png)?;
			}
			Ok(())
		})();
		kras_quick::cdp_surface::close_viewer_tabs(ENDPOINT);
		result.expect("줌 진단");
	}
	#[test]
	#[ignore = "로그인된 KRAS에서 집합건물 전체 및 첫 5호를 실제 출력"]
	fn live_aggregate_first_five_rooms() {
		let root = std::env::var("KRAS_QUICK_ROOT").expect("별도 검증 출력 루트 필요");
		assert!(!std::path::Path::new(&root).exists(), "기존 출력 재사용 금지");
		let address = "서울특별시 노원구 월계동 18";
		let building = AutoBuilding { index: 2, value: "101213228".into(), label: "(집합)그랑빌아파트 101동".into() };
		let mut floors = Vec::new();
		let total_ok = run_single_address_with_floors(address, RunMode::Auto { building: Some(building.clone()), floor: None }, &mut floors);
		kras_quick::cdp_surface::close_viewer_tabs(ENDPOINT);
		assert!(total_ok, "집합건물 전체 출력 실패");
		assert!(floors.len() >= 5, "실제 층호 5개 미만");
		println!("LIVE_DISCOVERED_FLOORS={}", floors.len());
		for floor in floors.into_iter().take(5) {
			let ok = run_single_address(address, RunMode::Auto { building: Some(building.clone()), floor: Some(floor.clone()) });
			kras_quick::cdp_surface::close_viewer_tabs(ENDPOINT);
			assert!(ok, "호별 출력 실패: {}", floor.value);
			println!("LIVE_ROOM_COMPLETED building={} floor={} label={}", building.value, floor.value, floor.label);
		}
	}
	#[test]
	fn auto_collects_every_floor_without_three_item_limit() {
		let mode = RunMode::Auto { building: Some(AutoBuilding { index: 1, value: "aggregate".into(), label: "(집합)A동".into() }), floor: None };
		let options: Vec<_> = (1..=5).map(|index| AutoBuilding { index, value: format!("room-{index}"), label: format!("{index}01") }).collect();
		let mut collected = Vec::new();
		let mut hooks = ModeHooks { mode: &mode, discovered_floors: &mut collected, address: "" };
		assert_eq!(hooks.choose_floor_room(&options, &options[0]), FloorChoice::Index(options[0].index));
		assert_eq!(collected, options);
	}
	#[test]
	fn watcher_child_arguments_require_current_executable() {
		let args = vec!["--title=opencode-property-watcher:test".into(), "self.exe".into(), "watch".into()];
		assert_eq!(normalize_watcher_child_args(args.clone(), "self.exe"), vec!["watch"]);
		assert_eq!(normalize_watcher_child_args(args.clone(), "other.exe"), args);
	}
	#[test]
	fn floor_return_keeps_building_value_when_indices_change() {
		let mode = RunMode::FloorReturn { value: "selected-building".into() };
		let mut floors = Vec::new();
		let mut hooks = ModeHooks { mode: &mode, discovered_floors: &mut floors, address: "" };
		let options = vec![AutoBuilding { index: 9, value: "selected-building".into(), label: "(집합)테스트".into() }];
		assert_eq!(hooks.choose_building(&options), BuildingChoice::Value("selected-building".into()));
	}
}

/// AUTO 대상(target)을 실행 모드로 변환한다.
fn run_mode_for(target: &AutoTarget) -> RunMode {
	match target {
		AutoTarget::Land => RunMode::Auto { building: None, floor: None },
		AutoTarget::Building { building } | AutoTarget::AggregateTotal { building } => {
			RunMode::Auto { building: Some(building.clone()), floor: None }
		}
		AutoTarget::AggregateFloor { building, floor } => RunMode::Auto {
			building: Some(building.clone()),
			floor: Some(floor.clone()),
		},
	}
}

/// 워처 훅 — 모드에 따라 건물/층호 선택을 처리한다.
/// (대화형이면 프롬프트, AUTO면 대상에 지정된 인덱스, 토지는 TS undefined와 동일하게 Reject)
struct ModeHooks<'a> {
	mode: &'a RunMode,
	discovered_floors: &'a mut Vec<AutoBuilding>,
	address: &'a str,
}

fn completed_choices(address: &str) -> (std::collections::HashSet<String>, std::collections::HashSet<(String, String)>) {
	let mut buildings = std::collections::HashSet::new();
	let mut floors = std::collections::HashSet::new();
	if address.is_empty() { return (buildings, floors); }
	let paths = property_auto_stage_paths(address, quick_output_root().as_deref()).output_paths();
	let Ok(entries) = std::fs::read_dir(&paths.other_directory) else { return (buildings, floors); };
	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		let records = if name == "작업로그.jsonl" {
			kras_quick::output_log::entries(&entry.path().to_string_lossy(), "output")
		} else if name.starts_with("oz-structure") && name.ends_with(".json") {
			std::fs::read_to_string(entry.path()).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok()).into_iter().collect()
		} else { continue; };
		for json in records {
		let integrity = &json["_krasIntegrity"];
		let parse = |value: &serde_json::Value| -> Option<KrasBuildingOption> {
			Some(KrasBuildingOption { value: value["value"].as_str()?.into(), label: value["label"].as_str()?.into() })
		};
		let Some(building) = parse(&integrity["building"]) else { continue; };
		let floor = parse(&integrity["floorRoom"]);
		let target = kras_quick::kras_output::KrasOutputTarget { building: building.clone(), floor_room: floor.clone() };
		if kras_quick::kras_output::verify_kras_output(&paths, &entry.path().to_string_lossy(), &target) {
			buildings.insert(building.value.clone());
			if let Some(floor) = floor { floors.insert((building.value, floor.value)); }
		}
		}
	}
	(buildings, floors)
}

fn interactive_building_choice(input: &str, options: &[AutoBuilding]) -> BuildingChoice {
	let number: usize = if input.trim().is_empty() { 0 } else {
		match input.trim().parse() { Ok(number) => number, Err(_) => return BuildingChoice::Absent }
	};
	if number == 0 { return BuildingChoice::Reject; }
	options.get(number - 1)
		.map(|option| BuildingChoice::Index(option.index)).unwrap_or(BuildingChoice::Absent)
}

#[test]
fn interactive_land_and_building_choices_preserve_numbering() {
	let options = vec![AutoBuilding { index: 7, value: "building-a".into(), label: "A동".into() }];
	assert_eq!(interactive_building_choice(" 0 ", &options), BuildingChoice::Reject);
	assert_eq!(interactive_building_choice(" 1 ", &options), BuildingChoice::Index(7));
	assert_eq!(interactive_building_choice("", &options), BuildingChoice::Reject);
	assert_eq!(interactive_building_choice("2", &options), BuildingChoice::Absent);
}

impl WorkflowHooks for ModeHooks<'_> {
	fn choose_building(&mut self, options: &[AutoBuilding]) -> BuildingChoice {
		match self.mode {
			RunMode::FloorReturn { value } => BuildingChoice::Value(value.clone()),
			RunMode::Interactive => {
				if options.is_empty() {
					return BuildingChoice::Absent;
				}
				println!("건물 옵션:");
				let paths = property_auto_stage_paths(self.address, quick_output_root().as_deref());
				if kras_quick::kras_output::is_kras_stage3_output_complete(
					&paths.output_paths(), &kras_quick::auto_batch::land_building(), None,
				) { println!("\x1b[32m  [0] 토지 [출력 있음]\x1b[0m"); }
				else { println!("  [0] 토지"); }
				let (completed, _) = completed_choices(self.address);
				for (i, o) in options.iter().enumerate() {
					let i = i + 1;
					if completed.contains(&o.value) { println!("\x1b[32m  [{i}] {} [출력 있음]\x1b[0m", o.label); }
					else { println!("  [{}] {}", i, o.label); }
				}
				loop {
					let input = read_line("번호 선택 (Enter=0, 토지): ");
					let choice = interactive_building_choice(&input, options);
					if choice != BuildingChoice::Absent {
						kras_quick::task_screen::update_task_phase("선택 적용 중");
						break choice;
					}
					println!("목록에 있는 번호를 입력해 주세요.");
				}
			}
			// TS: target.kind === "land" → chooseBuilding returns undefined → 수동 stage2
			RunMode::Auto { building: None, .. } => BuildingChoice::Reject,
			RunMode::Auto { building: Some(b), .. } => BuildingChoice::Value(b.value.clone()),
		}
	}

	fn choose_floor_room(&mut self, options: &[AutoBuilding], _building: &AutoBuilding) -> FloorChoice {
		*self.discovered_floors = options.to_vec();
		match self.mode {
			RunMode::Interactive | RunMode::FloorReturn { .. } => {
				if options.is_empty() {
					println!("층호가 없습니다.");
					return FloorChoice::None;
				}
				println!("층-호 옵션:");
				let (_, completed) = completed_choices(self.address);
				for (i, o) in options.iter().enumerate() {
					if completed.contains(&(_building.value.clone(), o.value.clone())) { println!("\x1b[32m  [{i}] {} [출력 완료]\x1b[0m", o.label); }
					else { println!("  [{}] {}", i, o.label); }
				}
				let input = read_line("층-호 번호 선택 (Enter=건물 전체): ");
				if input.is_empty() {
					return FloorChoice::None;
				}
				match input.parse::<usize>() {
					Ok(idx) if idx < options.len() => FloorChoice::Index(options[idx].index),
					_ => FloorChoice::None,
				}
			}
			RunMode::Auto { floor: Some(f), .. } => FloorChoice::Index(f.index),
			RunMode::Auto { building: Some(building), floor: None } if building.is_aggregate() => {
				options.first().map(|floor| FloorChoice::Index(floor.index)).unwrap_or(FloorChoice::None)
			}
			RunMode::Auto { floor: None, .. } => FloorChoice::None,
		}
	}
}

/// 뷰어 대기 중 스냅샷 — 현재 페이지 URL·대화상자·탭 목록을 한 줄씩 찍는다.
fn print_viewer_snapshot() {
	let tabs = kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT)
		.map(|pages| {
			pages
				.iter()
				.map(|page| page.url.clone())
				.collect::<Vec<String>>()
				.join(" | ")
		})
		.unwrap_or_else(|error| format!("목록 실패: {error}"));
	let snapshot = CdpSurface::connect(ENDPOINT)
		.ok()
		.and_then(|surface| {
			let expression = r#"JSON.stringify({href: location.href, dialog: (document.querySelector(".swal2-container.swal2-backdrop-show")?.innerText || "").replace(/\s+/g, " ").trim().slice(0, 120)})"#;
			let params = serde_json::json!({ "expression": expression, "returnByValue": true });
			surface
				.send_cdp("Runtime.evaluate", params)
				.ok()
				.and_then(|result| {
					result
						.get("result")
						.and_then(|value| value.get("value"))
						.and_then(|value| value.as_str())
						.map(str::to_string)
				})
		})
		.unwrap_or_else(|| "스냅샷 실패".to_string());
	kras_quick::task_screen::task_status(&format!("[viewer 대기] {snapshot}\n탭: {tabs}\n"));
}

/// 주소 1건을 끝까지 처리한다(Stage 1 → 건물/층호 → 캡차 → 뷰어 → Stage 3 → Excel).
/// 성공이면 true.
fn run_single_address(address: &str, mode: RunMode) -> bool {
	run_single_address_with_floors(address, mode, &mut Vec::new())
}

fn run_single_address_with_floors(address: &str, mode: RunMode, discovered_floors: &mut Vec<AutoBuilding>) -> bool {
	run_address_with_options(address, mode, discovered_floors, false)
}

fn needs_lookup_navigation(url: &str) -> bool {
	url.split(['?', '#']).next() != Some("https://www.kras.go.kr/kras/cert/certView.do")
}

#[test]
fn lookup_navigation_is_not_repeated_on_current_form() {
	assert!(!needs_lookup_navigation("https://www.kras.go.kr/kras/cert/certView.do"));
	assert!(!needs_lookup_navigation("https://www.kras.go.kr/kras/cert/certView.do?x=1"));
	assert!(needs_lookup_navigation("https://www.kras.go.kr/login/loginView.do"));
	assert!(needs_lookup_navigation("https://other.invalid/kras/cert/certView.do"));
}

#[derive(Debug, PartialEq, Eq)]
enum AddressAttempt { Completed, NoParcel, Failed }

fn run_address_with_options(address: &str, mode: RunMode, discovered_floors: &mut Vec<AutoBuilding>, reuse_loaded_options: bool) -> bool {
	run_address_attempt(address, mode, discovered_floors, reuse_loaded_options) == AddressAttempt::Completed
}

fn run_address_attempt(address: &str, mode: RunMode, discovered_floors: &mut Vec<AutoBuilding>, reuse_loaded_options: bool) -> AddressAttempt {
	let started = std::time::Instant::now();
	let mut surface = match CdpSurface::connect(ENDPOINT) {
		Ok(s) => s,
		Err(e) => {
			eprintln!("CDP 연결 실패: {e}");
			eprintln!("브라우저가 실행 중인지 확인하세요 (디버깅 포트 9333).");
			return AddressAttempt::Failed;
		}
	};
	if !reuse_loaded_options && needs_lookup_navigation(&surface.url()) {
	let navigation = surface.send_cdp(
		"Page.navigate",
		serde_json::json!({"url": "https://www.kras.go.kr/kras/cert/certView.do"}),
	);
	if let Err(error) = navigation { eprintln!("KRAS 페이지 이동 실패: {error}"); return AddressAttempt::Failed; }
	let ready = surface.send_cdp("Runtime.evaluate", serde_json::json!({
		"expression": "(async()=>{const end=Date.now()+5000;while(Date.now()<end){if(location.pathname==='/kras/cert/certView.do'&&document.readyState!=='loading'&&typeof certView!=='undefined'&&document.querySelector('#bldgSeCd'))return true;await new Promise(r=>setTimeout(r,50));}return false;})()",
		"returnByValue": true, "awaitPromise": true
	}));
	if !matches!(ready, Ok(ref result) if result["result"]["value"].as_bool() == Some(true)) {
		kras_quick::task_screen::task_status("조회 화면 준비 실패: 로그인 또는 브라우저 상태를 확인해 주세요.\n");
		return AddressAttempt::Failed;
	}
	if let Ok(s) = CdpSurface::connect(ENDPOINT) {
		surface = s;
	}
	}
	kras_quick::task_screen::task_status(&format!("현재 페이지: {}\n", surface.url()));
	kras_quick::task_screen::task_status(&format!("조회 준비 {}ms\n", started.elapsed().as_millis()));

	// Stage 1~2: TS runKrasWorkflowWatcher 루프 — 조회 재시도·건물 선택·층-호 선택.
	let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
	let mut hooks = ModeHooks { mode: &mode, discovered_floors, address };
	let mut stage1_result = kras_quick::auto_batch::AutoStage1Result::default();
	let mut stage2_result: Option<workflow_watcher::WorkflowStage2> = None;
	let workflow = match workflow_watcher::run_kras_workflow_watcher(
		&surface,
		address,
		&mut hooks,
		reuse_loaded_options,
		&mut stage1_result,
		&mut stage2_result,
	) {
		Ok(w) => w,
		Err(e) => {
			eprintln!("Stage 1 실패: {e}");
			kras_quick::task_screen::update_task_phase(&format!("조회 실패: {e}"));
			return AddressAttempt::Failed;
		}
	};
	kras_quick::task_screen::task_status(&format!("Stage 1 결과: {:?}\n", stage1_result));
	kras_quick::task_screen::update_task_phase("Stage 1 조회 완료");
	kras_quick::task_screen::advance_task_progress(1);

	// stage1.json — 차단 상태도 기록한다(TS와 동일)
	let stage1_json = serde_json::json!({
		"version": 1,
		"stage": 1,
		"status": stage1_result.status,
		"address": address,
		"reason": stage1_result.reason,
		"hasBuilding": stage1_result.status == "completed",
		"buildingOptions": stage1_result.building_options,
	});
	if let Some(parent) = std::path::Path::new(&paths.stage1_json).parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	if let Err(error) = kras_quick::output_log::append(&paths.stage1_json, "stage1", &stage1_json) {
		eprintln!("단계 기록 실패: {error}"); return AddressAttempt::Failed;
	}

	if workflow.return_to_address {
		println!("주소 입력 화면으로 돌아갑니다.");
		return AddressAttempt::Failed;
	}
	let Some(stage2_ws) = workflow.stage2 else {
		if stage1_result.reason.as_deref() == Some("no_parcel") { return AddressAttempt::NoParcel; }
		if stage1_result.status != "completed" {
			eprintln!("Stage 1이 차단되었습니다: {:?}", stage1_result.reason);
		} else if workflow.awaiting_user {
			eprintln!("건물 선택이 필요합니다.");
		} else {
			eprintln!("Stage 2를 완료하지 못했습니다.");
		}
		return AddressAttempt::Failed;
	};

	let building_index = stage2_ws.building.index;
	let building = KrasBuildingOption {
		value: stage2_ws.building.value.clone(),
		label: stage2_ws.building.label.clone(),
	};
	let floor_room = stage2_ws.floor_room.as_ref().map(|f| KrasBuildingOption {
		value: f.value.clone(),
		label: f.label.clone(),
	});

	let stage2 = KrasAutoStage2Result {
		status: stage2_ws.status.clone(),
		address: address.to_string(),
		building: building.clone(),
		floor_room: floor_room.clone(),
	};

	// stage2.json
	let stage2_json = serde_json::json!({
		"version": 1,
		"stage": 2,
		"status": stage2.status,
		"address": stage2.address,
		"reason": stage2_ws.reason,
		"building": { "value": stage2.building.value, "label": stage2.building.label },
		"floorRoom": stage2.floor_room.as_ref().map(|f| serde_json::json!({
			"value": f.value, "label": f.label
		})),
		"floorRoomOptions": stage2_ws.floor_room_options.iter().map(|o| serde_json::json!({
			"index": o.index, "value": o.value, "label": o.label
		})).collect::<Vec<_>>(),
	});
	if let Err(error) = kras_quick::output_log::append(&paths.stage2_json, "stage2", &stage2_json) {
		eprintln!("단계 기록 실패: {error}"); return AddressAttempt::Failed;
	}
	kras_quick::task_screen::update_task_phase("Stage 2 선택 완료");
	kras_quick::task_screen::advance_task_progress(2);
	if stage2.building.value.is_empty() {
		if let Err(error) = kras_quick::stage12::select_building(&surface, "") {
			kras_quick::task_screen::task_status(&format!("토지 선택 실패: {error}\n"));
			return AddressAttempt::Failed;
		}
	}

	// 사용자 지정: 검증된 기존 출력은 모든 모드에서 재출력하지 않는다.
	if kras_quick::kras_output::is_kras_stage3_output_complete(
		&paths.output_paths(),
		&stage2.building,
		stage2.floor_room.as_ref(),
	) {
		kras_quick::task_screen::task_status(&format!("이미 출력됨 — 캡처 생략: {address}\n"));
		kras_quick::task_screen::update_task_phase("이미 출력됨 — 캡처 생략");
		return AddressAttempt::Completed;
	}

	// 캡차 처리
	let previous_pages = match kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT) {
		Ok(pages) => pages.into_iter().map(|page| page.ws_url).collect::<Vec<_>>(),
		Err(error) => {
			eprintln!("OZ 기존 탭 확인 실패: {error}");
			return AddressAttempt::Failed;
		}
	};
	kras_quick::task_screen::task_status("보안문자 처리 중...\n");
	kras_quick::task_screen::update_task_phase("보안문자 처리");
	let captcha_result = if mode.uses_automatic_captcha() {
		handle_captcha_loop(&surface)
	} else {
		kras_quick::stage12::manual_captcha_input(&surface, Vec::new())
	};
	if let Err(e) = captcha_result {
		eprintln!("{e}");
		kras_quick::task_screen::update_task_phase(&format!("보안문자 중단: {e}"));
		return AddressAttempt::Failed;
	}
	kras_quick::task_screen::advance_task_progress(3);

	// OZ 뷰어 팝업 대기 + 연결
	kras_quick::task_screen::task_status("OZ 뷰어 팝업 대기 중...\n");
	// 10초 구간으로 쪼개 기다리며 구간마다 탭/대화상자 스냅샷을 찍는다
	// (뷰어가 안 뜨는 원인: 팝업 차단 vs 서버 오류 vs 세션 만료를 구분하기 위한 진단 출력)
	let viewer_outcome: Result<CdpSurface, Box<dyn std::error::Error>> = {
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
		let mut outcome: Result<CdpSurface, Box<dyn std::error::Error>> =
			Err("OZ viewer not found within timeout".into());
		loop {
			match CdpSurface::connect_viewer(ENDPOINT, 10_000, &previous_pages) {
				Ok(surface) => {
					outcome = Ok(surface);
					break;
				}
				Err(error) => {
					outcome = Err(error);
					if std::time::Instant::now() >= deadline {
						break;
					}
					print_viewer_snapshot();
				}
			}
		}
		outcome
	};
	let viewer = match viewer_outcome {
		Ok(v) => {
			kras_quick::task_screen::task_status(&format!("OZ 뷰어 연결: {}\n", v.url()));
			kras_quick::task_screen::update_task_phase("OZ 뷰어 연결");
			v
		}
		Err(e) => {
			eprintln!("OZ 뷰어 연결 실패: {e}");
			return AddressAttempt::Failed;
		}
	};

	// Stage 3: 캡처
	let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
	let stage3_result = run_kras_auto_stage3(&stage2, &viewer, &paths);
	match viewer.close_viewer() {
		Ok(()) => kras_quick::task_screen::task_status("사용한 OZ 뷰어 닫힘 확인\n"),
		Err(error) => eprintln!("OZ 뷰어 정리 실패: {error}"),
	}
	match &stage3_result {
		Ok(result @ KrasAutoStage3Result::Completed { .. }) => {
			kras_quick::task_screen::task_status(&format!("stage-3 완료: {result:?}\n"));
			kras_quick::task_screen::update_task_phase("산출물 저장 완료");
		}
		Ok(KrasAutoStage3Result::Blocked { reason, .. }) => {
			eprintln!("Stage 3 저장 차단: {reason}");
			kras_quick::task_screen::update_task_phase(&format!("산출물 저장 차단: {reason}"));
		}
		Err(e) => {
			eprintln!("stage-3 실패: {e}");
			kras_quick::task_screen::update_task_phase(&format!("Stage 3 실패: {e}"));
		}
	}

	// 완료 건물 기록 (TS appendDoneBuildingIndex)
	if building_index > 0 && matches!(&stage3_result, Ok(KrasAutoStage3Result::Completed { .. })) {
		match kras_quick::kras_auto_stages::append_done_building_value(&paths, &stage2.building.value) {
			Ok(()) => kras_quick::task_screen::task_status(&format!("완료 건물 기록: value={}\n", stage2.building.value)),
			Err(e) => eprintln!("완료 건물 기록 실패: {e}"),
		}
	}

	if matches!(stage3_result, Ok(KrasAutoStage3Result::Completed { .. })) {
		AddressAttempt::Completed
	} else { AddressAttempt::Failed }
}

/// 직접 실행은 EXE 폴더, npx는 실행기가 전달한 호출 폴더를 사용한다.
fn quick_output_root() -> Option<String> {
	let base = std::env::var("KRAS_QUICK_ROOT").ok()
		.filter(|value| !value.trim().is_empty()).map(std::path::PathBuf::from)
		.or_else(|| std::env::current_exe().ok().and_then(|path| path.parent().map(std::path::Path::to_path_buf)))
		.or_else(|| std::env::current_dir().ok())?;
	Some(base.join("KRAS").to_string_lossy().into_owned())
}

#[test]
fn default_output_root_is_beside_executable_not_desktop() {
	let base = std::env::var("KRAS_QUICK_ROOT").ok().filter(|value| !value.trim().is_empty())
		.map(std::path::PathBuf::from).unwrap_or_else(|| std::env::current_exe().unwrap().parent().unwrap().to_path_buf());
	assert_eq!(std::path::PathBuf::from(quick_output_root().unwrap()), base.join("KRAS"));
}

/// 종료 확인(TS readQuickAddress의 빈 입력 확인 루프). true면 종료.
fn confirm_quit() -> bool {
	loop {
		let answer = read_line(EXIT_CONFIRM_PROMPT);
		if answer.is_empty() {
			return true;
		}
		if answer == "0" {
			return false;
		}
		println!("Enter(예) 또는 0(아니오)을 입력해 주세요.");
	}
}

/// 일괄처리 범위 선택(TS askAutoBatchScope).
fn ask_auto_batch_scope() -> Option<AutoBatchScope> {
	kras_quick::batch_input::choose_scope(|| {
		let answer = read_line(AUTO_SCOPE_PROMPT);
		if lock_reader().is_eof() { None } else { Some(answer) }
	})
}

/// 대기 목록 수집(TS collectAutoBatch + readQuickAddress ignoreEmpty).
/// 빈 줄(Enter)은 무시하고 `시작` 또는 EOF에서 끝낸다.
fn collect_auto_batch(queue: &mut Vec<String>) -> bool {
	kras_quick::batch_input::collect(queue, |suppress| {
		let input = read_menu_input(if suppress { "" } else { ACTIVE_AUTO_PROMPT });
		if lock_reader().is_eof() && input.is_empty() { None } else { Some(input) }
	})
}

const ACTIVE_AUTO_PROMPT: &str = kras_quick::quick_address::AUTO_ADDRESS_PROMPT;

/// AUTO 배치 실행(TS runAutoParcelWorkflow의 루프 + runAutoTarget 재시도).
fn run_auto_batch(queue: &mut Vec<String>, scope: AutoBatchScope) {
	let mut state = AutoBatchState::new();
	// TS failedAutoAddresses / missingParcelAddresses
	let mut failures = AutoAddressFailures::default();
	let mut missing_parcel: Vec<String> = Vec::new();

	// 고정줄 진행 UI(카운트 줄·═ 구분선) — stdout/stdin이 TTY일 때만 열린다.
	// 비TTY(파이프·자동화)에서는 open_task_screen이 스스로 조용히 반환한다.
	let screen_root =
		quick_output_root().expect("실행 폴더를 확인할 수 없습니다.");
	let initial_total = queue.len() as u64;
	kras_quick::task_screen::set_task_auto_mode(true);
	kras_quick::task_screen::open_task_screen(&screen_root);
	kras_quick::task_screen::set_task_counts(Some(kras_quick::task_screen::BatchCounts {
		processed: 0,
		total: initial_total,
		failed: 0,
		missing: 0,
	}));
	let mut addresses_done: u64 = 0;
	let mut stopped = false;

	while let Some(raw_address) = queue.first().cloned() {
		queue.remove(0);
		state.begin_address();
		let address = match kras_quick::quick_address::resolve_work_address(&raw_address) {
			Ok(address) => address,
			Err(reason) => {
				kras_quick::task_screen::begin_task_address(&raw_address);
				failures.set(raw_address.clone(), AutoAddressFailure { input: raw_address, reason: reason.clone() });
				addresses_done += 1;
				kras_quick::task_screen::task_status(&format!("PNU 변환 실패: {reason}\n"));
				kras_quick::task_screen::finish_task_address(kras_quick::task_screen::Outcome::Failed);
				continue;
			}
		};
		kras_quick::task_screen::begin_task_address(&address);
		let newly_before = state.newly_added();
		if !queue.is_empty() {
			kras_quick::task_screen::task_status(&format!("AUTO 대기 주소 {}건 남음: {address}\n", queue.len()));
		}
		let paths = property_auto_stage_paths(&address, quick_output_root().as_deref());
		// 아직 산출물이 남아 있으면 진행할 대상이 있는지 확인한다.
		loop {
			match state.next_target(&paths, scope) {
				None => break,
				Some(target) => {
					let label = kras_quick::auto_batch::auto_target_label(&target);
					let target_is_new = state.target_is_new(&paths, &target);
					let reused = state.reuse_loaded_options();
					// 실제 조회는 주소 1건 단위로 수행한다(첫 시도에서 stage1 확보).
					let mut discovered_floors = Vec::new();
					let result = run_address_attempt(&address, run_mode_for(&target), &mut discovered_floors, reused);
					if result == AddressAttempt::Completed {
						// TS: 성공한 주소는 실패 목록에서 지워진다
						failures.delete(&address);
						let stage1 = read_stage1_from_disk(&address);
						state.mark_completed(&target, stage1, target_is_new);
						if let AutoTarget::AggregateTotal { building } = &target {
							state.set_floor_queue(&building, discovered_floors);
						}
						kras_quick::task_screen::task_status(&format!("AUTO 완료: {label} ({address})\n"));
						if matches!(target, AutoTarget::Land) {
							let _ = reused;
						}
						std::thread::sleep(std::time::Duration::from_millis(
							kras_quick::auto_batch::AUTO_PACING_MS,
						));
						continue;
					}
					// 지번 없음은 재시도하지 않는다(TS KrasAutoNoParcelError → 다음 주소)
					if result == AddressAttempt::NoParcel
					{
						if !missing_parcel.contains(&address) {
							missing_parcel.push(address.clone());
						}
						println!(
							"AUTO: {address}을(를) 지번 없음으로 기록하고 다음 주소로 넘어갑니다."
						);
						break;
					}
					let reason = format!("{label} 처리 중단: 화면과 작업 로그를 확인해 주세요. 자동 재시도하지 않습니다.");
					eprintln!("{reason}");
					// TS: failedAutoAddresses.set(quickAddress, { input, reason })
					failures.set(
						address.clone(),
						AutoAddressFailure { input: address.clone(), reason },
					);
					queue.insert(0, raw_address.clone());
					stopped = true;
					break;
				}
			}
		}
		let (total, processed) = state.aggregate_summary(&paths);
		kras_quick::task_screen::task_status(&format!("집합건물 {processed}/{total} 처리: {address}\n"));

		// 주소 단위 결과를 고정줄에 기록한다(TS counts/ outcome와 같은 자리).
		addresses_done += 1;
		let outcome = if missing_parcel.contains(&address) {
			kras_quick::task_screen::Outcome::Missing
		} else if failures.contains(&address) {
			kras_quick::task_screen::Outcome::Failed
		} else if state.newly_added() == newly_before {
			kras_quick::task_screen::Outcome::Duplicate
		} else {
			kras_quick::task_screen::Outcome::Completed
		};
		kras_quick::task_screen::set_task_counts(Some(kras_quick::task_screen::BatchCounts {
			processed: addresses_done,
			total: initial_total,
			failed: failures.len() as u64,
			missing: missing_parcel.len() as u64,
		}));
		kras_quick::task_screen::finish_task_address(outcome);
		if stopped { break; }
	}
	if state.newly_added() > 0 {
		println!("출력완료 - {}건이 추가되었습니다", state.newly_added());
	}
	// TS 일괄처리 종료 요약: 실패 목록 → 지번 없음 목록 → AUTO OFF
	println!("일괄처리 종료.");
	print!("{}", format_auto_failures(&failures));
	failures.clear();
	println!("지번 없음 {}건", missing_parcel.len());
	for missing in &missing_parcel {
		println!("- {missing}");
	}
	kras_quick::task_screen::finish_task_screen();
	kras_quick::task_screen::close_task_screen();
	kras_quick::task_screen::set_task_auto_mode(false);
	println!("=== AUTO 모드: OFF ===");
}

/// stage1.json이 있으면 읽어 배치 상태에 반영한다(없으면 None).
fn read_stage1_from_disk(address: &str) -> Option<kras_quick::auto_batch::AutoStage1Result> {
	let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
	let value = kras_quick::output_log::latest(&paths.stage1_json, "stage1")?;
	Some(kras_quick::auto_batch::AutoStage1Result::from_value(&value))
}

fn normalize_watcher_child_args(args: Vec<String>, executable: &str) -> Vec<String> {
	if args.len() == 3 && args[0].starts_with("--title=opencode-property-watcher:")
		&& args[1] == executable && args[2] == "watch" {
		vec!["watch".into()]
	} else { args }
}

fn main() {
	let version = env!("CARGO_PKG_VERSION");
	let raw_args: Vec<String> = std::env::args().skip(1).collect();
	if raw_args.len() == 1 && matches!(raw_args[0].as_str(), "--version" | "-V") {
		println!("kras-quick v{version}");
		return;
	}
	let raw_args = match std::env::current_exe() {
		Ok(executable) => normalize_watcher_child_args(raw_args, &executable.to_string_lossy()),
		Err(_) => raw_args,
	};
	let entry = match kras_quick::kras_launcher::terminal_entry(&raw_args) {
		Ok(entry) => entry,
		Err(error) => { eprintln!("{error}"); std::process::exit(1); }
	};
	let args = entry.args;
	#[cfg(windows)]
	{
		use std::io::IsTerminal;
		if args.is_empty() && !entry.reentered && std::env::var_os("WT_SESSION").is_none()
			&& std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
		{
			use base64::Engine;
			let payload = base64::engine::general_purpose::STANDARD.encode(
				serde_json::to_vec(&args).expect("터미널 인자 직렬화"),
			);
			let handoff = std::env::current_exe().and_then(|exe| {
				std::process::Command::new("wt.exe")
					.arg("new-tab").arg(exe)
					.arg(kras_quick::kras_launcher::TERMINAL_REENTRY_FLAG)
					.arg(payload).status()
			});
			match handoff {
				Ok(status) if status.success() => return,
				_ => eprintln!("Windows Terminal을 열지 못해 현재 콘솔에서 계속합니다."),
			}
		}
		unsafe {
			use windows_sys::Win32::System::Console::{GetConsoleMode, GetStdHandle, SetConsoleMode, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE};
			for id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
				let handle = GetStdHandle(id);
				let mut mode = 0;
				if GetConsoleMode(handle, &mut mode) != 0 {
					if SetConsoleMode(handle, mode | 0x0004) == 0 {
						eprintln!("ANSI 색상·고정줄 모드를 켜지 못했습니다.");
					}
				}
			}
		}
	}

	// TS scp-usage-watcher.ts — SCP 사용량 반영 도구(독립 CLI).
	if args.first().map(String::as_str) == Some("scp-usage") {
		let outcome = kras_quick::scp_usage_watcher::parse_scp_usage_args(&args[1..]).and_then(
			|command| {
				let kras_quick::scp_usage_watcher::ScpUsageCommand::Watch { config, options } =
					command;
				let config = kras_quick::scp_usage_watcher::load_config(&config)
					.map_err(|error| error.to_string())?;
				kras_quick::scp_usage_watcher::run_scp_usage_watcher(config, options)
					.map_err(|error| error.to_string())
			},
		);
		match outcome {
			Ok(()) => return,
			Err(message) => {
				// TS: 오류를 stderr로 내고 종료 코드 1
				eprintln!("{message}");
				std::process::exit(1);
			}
		}
	}

	// TS kras-runner.ts — 조회/다크 모드 실행 모드(watcher 명령보다 먼저 본다).
	if matches!(
		args.first().map(String::as_str),
		Some("lookup") | Some("dark_mode")
	) {
		let result = kras_quick::kras_runner::run_kras_runner(&args);
		match result {
			// TS: process.stdout.write(output) — 개행 없음
			Ok(output) => {
				kras_quick::kras_runner::print_kras_runner_output(&output);
				return;
			}
			Err(error) => {
				eprintln!("{}", kras_quick::property_watcher_runner::failure_line(error.as_ref()));
				std::process::exit(1);
			}
		}
	}

	// TS property-watcher-runner.ts — watch/dry_run/start/status/stop 단일 명령 인자.
	// 후보는 공백 없는 ASCII 식별자만 — 주소 인자와 겹치지 않는다.
	if args.len() == 1
		&& kras_quick::property_watcher_runner::is_runner_command_argument(&args[0])
	{
		let parsed = kras_quick::property_watcher_runner::parse_property_watcher_command(&args);
		let failure = kras_quick::property_watcher_runner::run_property_watcher_cli(|| {
			let command = parsed
				.map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
			kras_quick::property_watcher_runner::execute_property_watcher_command(
				command,
				&mut kras_quick::property_watcher_runner::CliActions,
			)
		});
		if let Some(line) = failure {
			// TS: stderr 한 줄 + 종료 코드 1
			eprintln!("{line}");
			std::process::exit(1);
		}
		return;
	}

	// --probe: 현재 페이지 진단 출력
	if args.first().map(String::as_str) == Some("--probe") {
		let surface = match CdpSurface::connect(ENDPOINT) {
			Ok(s) => s,
			Err(e) => {
				eprintln!("CDP 연결 실패: {e}");
				std::process::exit(1);
			}
		};
		let expr = r#"(function(){
const out = {};
out.url = location.href;
const cap = document.querySelector('#captcha');
out.captcha = cap ? {value: cap.value, visible: !!(cap.offsetParent)} : null;
const btn = document.querySelector("button[onclick='certView.fnProcessCert();']");
out.viewBtn = btn ? {text: (btn.textContent||'').trim(), disabled: !!btn.disabled} : null;
const dlg = document.querySelector('.swal2-container');
out.dialog = dlg ? (dlg.textContent||'').replace(/\s+/g,' ').trim().slice(0,200) : null;
out.dialogShown = document.querySelector('.swal2-container.swal2-backdrop-show') !== null;
const bldg = document.querySelector('#bldgSeCd');
out.building = bldg ? bldg.value : null;
let popup = null;
try { const w = window.open('about:blank','_blank','width=100,height=100'); popup = !!w; if (w) w.close(); } catch (e) { popup = 'ERR:' + e.message; }
out.popupAllowed = popup;
out.iframes = Array.from(document.querySelectorAll('iframe')).map(f => f.src).slice(0,10);
try { out.hasCertView = (typeof certView !== 'undefined'); } catch (e) { out.hasCertView = false; }
return JSON.stringify(out);
})()"#;
		match surface.evaluate(expr) {
			Ok(v) => println!("{}", v.get("value").and_then(|s| s.as_str()).unwrap_or("<null>")),
			Err(e) => eprintln!("probe 실패: {e}"),
		}
		return;
	}

	println!("kras-quick v{version}");

	// OCR 모델 위치 자동 발견 (런타임 폴더 포함)
	// 최초 실행으로 모델이 없으면 런타임 ZIP을 받아와 준비한다(TS ensureRuntime 동일).
	let mut ocr_root = kras_quick::numeric_ocr::discover_numeric_ocr_root();
	if ocr_root.is_none() {
		println!("OCR 모델이 없어 런타임을 준비합니다 (최초 실행)...");
		match kras_quick::kras_launcher::ensure_bundled_runtime() {
			Ok(_) => ocr_root = kras_quick::numeric_ocr::discover_numeric_ocr_root(),
			Err(error) => eprintln!("경고: OCR 런타임 준비 실패: {error}"),
		}
	}
	if ocr_root.is_none() {
		eprintln!("경고: OCR 모델(number.onnx)을 찾지 못했습니다. 보안문자는 수동 입력이 필요합니다.");
	}

	// 최초 실행/브라우저 미기동: 설치된 브라우저를 골라업무용 프로필로 기동한다.
	// (TS prepareKrasQuickBrowser — CDP가 이미 살아 있으면 건너뛴다.)
	let cdp_ready = kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT)
		.map(|pages| !pages.is_empty())
		.unwrap_or(false);
	if !cdp_ready {
		let browser_root = std::env::var("KRAS_QUICK_ROOT")
			.ok()
			.map(|value| value.trim().trim_end_matches(['\\', '/']).to_string())
			.filter(|value| !value.is_empty())
			.unwrap_or_else(|| {
				std::env::current_dir()
					.map(|path| path.display().to_string())
					.unwrap_or_else(|_| ".".to_string())
			});
		let candidates = kras_quick::kras_browser_choice::discover_kras_browsers(
			std::path::Path::new(&browser_root),
			&kras_quick::kras_browser_choice::process_environment(),
		);
		let choice = kras_quick::kras_browser_choice::choose_kras_browser(
			|prompt| read_line(prompt),
			&candidates,
		);
		match choice {
			Some(choice) => match kras_quick::kras_browser_process::ensure_kras_chromium(&choice, ENDPOINT) {
				Ok(()) => println!("선택한 브라우저: {}", choice.label),
				Err(error) => eprintln!("브라우저 시작 실패: {error}"),
			},
			None => eprintln!("브라우저 선택이 취소되었습니다. 이미 실행 중인 브라우저가 있으면 그대로 씁니다."),
		}
	}

	// 인자로 주소가 주어지면 기존 CLI 동작대로 1회 실행
	if let Some(first) = args.first() {
		if !first.starts_with("--") {
			let address = args.join(" ");
			let ok = run_single_address(&address, RunMode::Interactive);
			if !ok {
				std::process::exit(1);
			}
			return;
		}
	}

	// ---- 주소 메뉴 루프 (TS quick 루프 parity) ----
	if CdpSurface::connect(ENDPOINT).is_err() {
		eprintln!("CDP 연결 실패: {e_msg}", e_msg = "디버깅 포트 9333을 여세요");
		std::process::exit(1);
	}

	let mut previous_address: Option<String> = None;
	let mut auto_mode = false;
	let mut qa_mode = false;
	let mut pending_queue: Vec<String> = Vec::new();
	let mut batch_running = false;

	// 로그인 확인 (TS 로그인 프롬프트)
	loop {
		let answer = read_line(LOGIN_PROMPT);
		match answer.as_str() {
			"*" => {
				println!("아직 주소를 조회할 수 없습니다. 로그인을 먼저 완료해 주세요.");
				continue;
			}
			"0" => {
				println!("로그인을 완료한 후 Enter를 눌러 주세요.");
				continue;
			}
			"" => break,
			_ => println!("Enter(예) 또는 0(아니오)을 입력해 주세요."),
		}
	}

	loop {
		let prompt = active_address_prompt(auto_mode, qa_mode);
		let mut input = read_menu_input(prompt);
		if input.trim() == "+" {
			match kras_quick::quick_address::read_clipboard_text() {
				Ok(text) if !text.trim().is_empty() => {
					println!("클립보드 입력:\n{text}");
					if !read_line("Enter: 입력 사용 / 다른 입력: 취소 > ").is_empty() { continue; }
					input = text.trim().to_string();
				},
				Ok(_) => { eprintln!("클립보드가 비어 있습니다."); continue; },
				Err(error) => { eprintln!("클립보드 읽기 실패: {error}"); continue; },
			}
		}
		if input.trim() == "법정동 코드 업데이트" {
			match kras_quick::lawdong::update_official() {
				Ok(diff) => println!("법정동 반영: 추가 {} / 명칭 변경 {} / 기존 코드 중 폐지 {} / 누락 {}", diff.added, diff.renamed, diff.abolished, diff.missing),
				Err(error) => eprintln!("법정동 업데이트 실패(기존 자료 유지): {error}"),
			}
			continue;
		}

		let lines = split_quick_address_batch(&input);
		if lines.len() > 1 && !auto_mode {
			println!("주소 또는 PNU를 한 건씩 입력해 주세요. 붙여넣은 목록은 실행하지 않았습니다.");
			continue;
		}
		if lines.len() > 1 && auto_mode {
			let added = append_auto_addresses(&mut pending_queue, &input);
			println!("AUTO 대기 목록에 {added}건 추가됨 (총 {}건). '시작' 입력 시 실행합니다.", pending_queue.len());
			continue;
		}

		match classify_menu_input(&input) {
			MenuAction::Quit => {
				if confirm_quit() {
					break;
				}
			}
			MenuAction::DefaultAddress => {
				run_one(DEFAULT_ADDRESS, &mut previous_address);
			}
			MenuAction::Redisplay => {}
			MenuAction::PreviousAddress => match &previous_address {
				Some(prev) => {
					let prev = prev.clone();
					run_one_reusing_options(&prev, &mut previous_address, true);
				}
				None => println!("직전 주소가 없습니다. 먼저 주소를 입력해 주세요."),
			},
			MenuAction::FloorRoomReturn => match &previous_address {
				Some(prev) => {
					let paths = property_auto_stage_paths(prev, quick_output_root().as_deref());
					let saved = kras_quick::output_log::latest(&paths.stage2_json, "stage2");
					let building = saved.as_ref().and_then(|stage| stage.get("building"));
					let value = building.and_then(|b| b.get("value")).and_then(|v| v.as_str()).unwrap_or("");
					let label = building.and_then(|b| b.get("label")).and_then(|v| v.as_str()).unwrap_or("");
					let selected = AutoBuilding { index: 0, value: value.into(), label: label.into() };
					let has_floors = saved.as_ref().and_then(|stage| stage["floorRoomOptions"].as_array())
						.map(|options| !options.is_empty()).unwrap_or(false);
					if value.is_empty() || !selected.is_aggregate() || !has_floors {
						println!("층호가 없습니다.");
						continue;
					}
					let _ = run_address_with_options(prev, RunMode::FloorReturn { value: value.into() }, &mut Vec::new(), true);
				},
				None => println!("직전 주소가 없습니다. 먼저 주소를 입력해 주세요."),
			},
			MenuAction::ViewerManual => {
				let Some(address) = previous_address.as_ref() else {
					println!("직전 주소가 없습니다. 먼저 주소를 조회해 주세요.");
					continue;
				};
				println!("브라우저에서 '{address}'의 OZ 문서를 직접 열어 주세요.");
				if !read_line("Enter: 열린 문서 캡처 / 다른 입력: 취소 > ").is_empty() { continue; }
				let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
					let pages = kras_quick::property_watcher_runtime::list_cdp_pages(ENDPOINT)?;
					if pages.iter().filter(|page| kras_quick::kras_auto_stages::is_viewer_url(&page.url)).count() != 1 {
						return Err("OZ 문서 탭을 정확히 하나만 열어 주세요.".into());
					}
					let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
					let saved = kras_quick::output_log::latest(&paths.stage2_json, "stage2").ok_or("직전 단계 기록이 없습니다.")?;
					if saved.get("address").and_then(|v| v.as_str()) != Some(address.as_str()) {
						return Err("직전 선택 기록의 주소가 일치하지 않습니다.".into());
					}
					let option = |value: &serde_json::Value| -> Result<KrasBuildingOption, Box<dyn std::error::Error>> {
						Ok(KrasBuildingOption {
							value: value.get("value").and_then(|v| v.as_str()).ok_or("건물 식별자 기록 없음")?.into(),
							label: value.get("label").and_then(|v| v.as_str()).ok_or("건물 명칭 기록 없음")?.into(),
						})
					};
					let stage2 = KrasAutoStage2Result {
						status: "completed".into(), address: address.clone(),
						building: option(&saved["building"])?,
						floor_room: match saved.get("floorRoom").filter(|v| !v.is_null()) { Some(value) => Some(option(value)?), None => None },
					};
					let viewer = CdpSurface::connect_viewer(ENDPOINT, 1000, &[])?;
					let captured = run_kras_auto_stage3(&stage2, &viewer, &paths);
					if let Err(error) = viewer.close_viewer() { eprintln!("OZ 뷰어 정리 실패: {error}"); }
					match captured? {
						KrasAutoStage3Result::Completed { .. } => println!("수동 OZ 문서 저장 완료: {address}"),
						KrasAutoStage3Result::Blocked { reason, .. } => return Err(format!("수동 OZ 저장 차단: {reason}").into()),
					}
					Ok(())
				})();
				if let Err(error) = outcome { eprintln!("수동 OZ 처리 실패: {error}"); }
			},
			MenuAction::ClipboardPaste => {
				println!("클립보드 내용이 메뉴 명령 '+'입니다. 주소 또는 PNU를 복사해 주세요.");
			}
			MenuAction::ToggleQa => {
				qa_mode = !qa_mode;
				if qa_mode {
					auto_mode = false;
					println!("=== QA 모드: ON ===");
				} else {
					println!("=== QA 모드: OFF ===");
				}
			}
			MenuAction::ToggleAuto => {
				auto_mode = !auto_mode;
				if auto_mode {
					qa_mode = false;
					kras_quick::task_screen::set_task_auto_mode(true);
					println!("=== AUTO 모드: ON ===");
					println!("AUTO 준비 완료: '...' 입력 시 대규모 처리 모드를 시작합니다.");
				} else {
					batch_running = false;
					kras_quick::task_screen::set_task_auto_mode(false);
					println!("AUTO 모드 종료.");
					println!("=== AUTO 모드: OFF ===");
				}
			}
			MenuAction::EnterBatch => {
				if !auto_mode {
					println!("먼저 AUTO를 입력해 대규모 처리 모드를 켜 주세요.");
					continue;
				}
				if !collect_auto_batch(&mut pending_queue) { continue; }
				if pending_queue.is_empty() {
					println!("대기 중인 주소가 없습니다. 주소를 먼저 추가해 주세요.");
					continue;
				}
				let Some(scope) = ask_auto_batch_scope() else { continue; };
				println!("=== AUTO 모드: ON (일괄처리) ===");
				println!("일괄처리 시작: {}건", pending_queue.len());
				batch_running = true;
				auto_mode = false;
				run_auto_batch(&mut pending_queue, scope);
				batch_running = false;
			}
			MenuAction::StartBatch => {
				if !auto_mode && pending_queue.is_empty() {
					println!("먼저 AUTO를 입력해 대규모 처리 모드를 켜 주세요.");
					continue;
				}
				if pending_queue.is_empty() {
					println!("대기 중인 주소가 없습니다. 주소를 먼저 추가해 주세요.");
					continue;
				}
				let Some(scope) = ask_auto_batch_scope() else { continue; };
				println!("=== AUTO 모드: ON (일괄처리) ===");
				println!("일괄처리 시작: {}건", pending_queue.len());
				batch_running = true;
				auto_mode = false;
				run_auto_batch(&mut pending_queue, scope);
				batch_running = false;
			}
			MenuAction::Pause => {
				println!("일시정지: 현재 주소 완료 후 정지합니다.");
				batch_running = false;
			}
			MenuAction::Resume => {
				if pending_queue.is_empty() {
					println!("재개할 주소가 없습니다. AUTO를 종료합니다.");
					auto_mode = false;
					continue;
				}
				println!("AUTO 재개. 다음 주소 ({}건 남음)", pending_queue.len());
				let Some(scope) = ask_auto_batch_scope() else { continue; };
				batch_running = true;
				auto_mode = false;
				run_auto_batch(&mut pending_queue, scope);
				batch_running = false;
			}
			MenuAction::Run { input } => {
				// AUTO 모드에서 주소를 넣으면 대기 목록으로 모은다
				if auto_mode && !batch_running && pending_queue.len() > 0 {
					let added = append_auto_addresses(&mut pending_queue, &input);
					println!("AUTO 대기 목록에 {added}건 추가됨 (총 {}건). '시작' 입력 시 실행합니다.", pending_queue.len());
					continue;
				}
				// PNU는 주소로 변환
				let address = if kras_quick::kras_pnu::is_pnu_input(&input) {
					match kras_quick::kras_pnu::pnu_to_address(&input) {
						Ok(a) => {
							println!("PNU 변환: {input} → {a}");
							a
						}
						Err(e) => {
							eprintln!("PNU 변환 실패: {e}");
							continue;
						}
					}
				} else {
					input
				};
				if auto_mode {
					pending_queue.push(address.clone());
					run_auto_batch(&mut pending_queue, AutoBatchScope::Full);
					kras_quick::task_screen::set_task_auto_mode(true);
				} else {
					run_one(&address, &mut previous_address);
				}
			}
		}
	}
}

/// 주소 1건 실행 + 직전 주소 기록(TS previousAddress 갱신).
fn run_one(address: &str, previous_address: &mut Option<String>) {
	run_one_reusing_options(address, previous_address, false);
}

fn run_one_reusing_options(address: &str, previous_address: &mut Option<String>, mut reuse_loaded_options: bool) {
	let mut mode = RunMode::Interactive;
	loop {
	let root = quick_output_root().expect("실행 폴더를 확인할 수 없습니다.");
	kras_quick::task_screen::set_task_auto_mode(false);
	kras_quick::task_screen::open_task_screen(&root);
	kras_quick::task_screen::begin_task_address(address);
	let ok = run_address_with_options(address, mode, &mut Vec::new(), reuse_loaded_options);
	kras_quick::task_screen::finish_task_address(if ok {
		kras_quick::task_screen::Outcome::Completed
	} else {
		kras_quick::task_screen::Outcome::Failed
	});
	kras_quick::task_screen::finish_task_screen();
	kras_quick::task_screen::close_task_screen();
	if ok {
		*previous_address = Some(address.to_string());
	}
	if !ok { break; }
	loop {
	match read_line("1: 같은 건물 층호 목록 / 2: 이 주소 건물 목록 / Enter: 주소 입력 > ").trim() {
		"1" => {
			let paths = property_auto_stage_paths(address, quick_output_root().as_deref());
			let saved = kras_quick::output_log::latest(&paths.stage2_json, "stage2");
			let building = saved.as_ref().and_then(|json| json.get("building"));
			let value = building.and_then(|b| b["value"].as_str()).unwrap_or("");
			let label = building.and_then(|b| b["label"].as_str()).unwrap_or("");
			let has_floors = saved.as_ref().and_then(|json| json["floorRoomOptions"].as_array())
				.map(|options| !options.is_empty()).unwrap_or(false);
			if value.is_empty() || !label.contains("(집합)") || !has_floors {
				println!("층호가 없습니다.");
				continue;
			} else { mode = RunMode::FloorReturn { value: value.into() }; }
		},
		"2" => { mode = RunMode::Interactive; },
		_ => return,
	}
	break;
	}
	reuse_loaded_options = true;
	}
}
