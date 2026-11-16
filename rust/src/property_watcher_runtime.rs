// TS scripts/property-watcher-runtime.ts — CDP 브라우저 표면 연결과 dry-run.
//
// kras-quick 진입점(property-auto-runner)은 이 모듈에서 connectPropertyWatcherBrowser만 쓴다.
// startLivePropertyWatcher / startPropertyWatcherRuntime / adaptPlaywrightBrowser는
// property-watcher-lifecycle·runner(SCP 러너 전용 엔트리)가 쓰므로 해당 그룹에서 포팅한다.

use serde_json::Value;

use crate::property_watcher_page::{
	discover_property_watcher_targets, PropertyWatcherError,
};

/// TS CDP_ENDPOINT — runtime이 연결하는 기본 디버깅 엔드포인트.
pub const CDP_ENDPOINT: &str = "http://127.0.0.1:9222";

/// TS DRY_RUN_RESULT
pub const DRY_RUN_RESULT: &str = r#"{"scpPages":1,"krasPages":1,"status":"ready"}"#;

/// TS CdpPageSurface — CDP 페이지 타깃 표면.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdpTargetPage {
	pub id: String,
	pub url: String,
	pub ws_url: String,
}

impl CdpTargetPage {
	pub fn url(&self) -> &str {
		&self.url
	}
}

/// TS connectPropertyWatcherBrowser의 타깃 수집부 — `chromium.connectOverCDP` 대응.
/// `/json/list`에서 page 타깃만 골라 반환한다.
pub fn list_cdp_pages(endpoint: &str) -> Result<Vec<CdpTargetPage>, Box<dyn std::error::Error>> {
	let url = format!("{}/json/list", endpoint.trim_end_matches('/'));
	let resp = ureq::get(&url).call().map_err(|e| format!("CDP 페이지 목록 조회 실패: {e}"))?;
	let body: String = resp.into_body().read_to_string()?;
	let targets: Vec<Value> = serde_json::from_str(&body)
		.map_err(|e| format!("CDP 페이지 목록 해석 실패: {e}"))?;
	Ok(targets
		.iter()
		.filter(|t| t.get("type").and_then(|v| v.as_str()) == Some("page"))
		.filter_map(|t| {
			Some(CdpTargetPage {
				id: t.get("id")?.as_str()?.to_string(),
				url: t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
				ws_url: t.get("webSocketDebuggerUrl")?.as_str()?.to_string(),
			})
		})
		.collect())
}

/// TS connectPropertyWatcherBrowser — 기본 엔드포인트에 연결해 페이지 표면을 돌려준다.
pub fn connect_property_watcher_browser(
	endpoint: &str,
) -> Result<Vec<CdpTargetPage>, Box<dyn std::error::Error>> {
	list_cdp_pages(endpoint)
}

/// TS runPropertyWatcherDryRun — SCP/KRAS 타깃이 각각 정확히 1개인지 확인한 뒤 결과를 출력한다.
pub fn run_property_watcher_dry_run(
	pages: &[CdpTargetPage],
) -> Result<&'static str, PropertyWatcherError> {
	let urls: Vec<String> = pages.iter().map(|p| p.url.clone()).collect();
	discover_property_watcher_targets(&urls)?;
	Ok(DRY_RUN_RESULT)
}

/// TS adaptPropertyWatcherBrowser — 타깃 목록을 컨텍스트 1개짜리 브라우저 표면으로 적응한다.
/// (Rust에서는 Playwright 세션 대신 CDP 타깃 자체가 페이지 표면이므로 구조만 동일하게 만든다.)
#[derive(Debug, Clone, Default)]
pub struct CdpBrowserSurface {
	pub contexts: Vec<CdpContextSurface>,
}

#[derive(Debug, Clone, Default)]
pub struct CdpContextSurface {
	pub pages: Vec<CdpTargetPage>,
}

impl CdpBrowserSurface {
	pub fn page_count(&self) -> usize {
		self.contexts.iter().map(|c| c.pages.len()).sum()
	}

	/// 타깃 목록을 모두 하나의 컨텍스트로 담은 표면을 만든다.
	pub fn from_pages(pages: Vec<CdpTargetPage>) -> Self {
		Self { contexts: vec![CdpContextSurface { pages }] }
	}

	/// 모든 페이지 URL (URL 목록 기반 발견 함수와 연결할 때 쓴다).
	pub fn page_urls(&self) -> Vec<String> {
		self.contexts
			.iter()
			.flat_map(|context| context.pages.iter().map(|page| page.url.clone()))
			.collect()
	}

	/// TS discoverPropertyWatcherTargets(browser) — 컨텍스트를 넘어 타깃을 찾는다.
	pub fn discover_targets(&self) -> Result<(usize, usize), PropertyWatcherError> {
		discover_property_watcher_targets(&self.page_urls())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn page(id: &str, url: &str) -> CdpTargetPage {
		CdpTargetPage {
			id: id.to_string(),
			url: url.to_string(),
			ws_url: format!("ws://127.0.0.1:9333/devtools/page/{id}"),
		}
	}

	fn ready_surface() -> CdpBrowserSurface {
		CdpBrowserSurface::from_pages(vec![
			page("a", "http://scpweb.softgraphy.biz/board"),
			page("b", "https://www.kras.go.kr/kras/cert/certView.do"),
			page("c", "https://www.kras.go.kr/mainView.do"),
		])
	}

	#[test]
	fn dry_run_result_matches_ts_constant() {
		assert_eq!(DRY_RUN_RESULT, r#"{"scpPages":1,"krasPages":1,"status":"ready"}"#);
		assert_eq!(CDP_ENDPOINT, "http://127.0.0.1:9222");
	}

	#[test]
	fn dry_run_succeeds_with_exactly_one_of_each_target() {
		let surface = ready_surface();
		assert_eq!(
			run_property_watcher_dry_run(&surface.contexts[0].pages).expect("ready"),
			DRY_RUN_RESULT
		);
	}

	#[test]
	fn dry_run_fails_when_a_target_is_missing() {
		let surface = CdpBrowserSurface::from_pages(vec![page(
			"a",
			"http://scpweb.softgraphy.biz/board",
		)]);
		let err = run_property_watcher_dry_run(&surface.contexts[0].pages)
			.expect_err("KRAS 없음");
		assert_eq!(err.code, crate::property_watcher_page::PropertyWatcherErrorCode::TargetCountInvalid);
	}

	#[test]
	fn browser_surface_adapts_pages_into_single_context() {
		let surface = ready_surface();
		assert_eq!(surface.contexts.len(), 1, "타깃 목록은 하나의 컨텍스트로 적응된다");
		assert_eq!(surface.page_count(), 3);
		assert_eq!(surface.page_urls().len(), 3);
		assert_eq!(surface.page_urls()[0], "http://scpweb.softgraphy.biz/board");
	}

	#[test]
	fn discover_targets_on_surface_uses_page_discovery() {
		let surface = ready_surface();
		let (kras_index, scp_index) = surface.discover_targets().expect("1:1");
		assert_eq!(surface.contexts[0].pages[kras_index].url, "https://www.kras.go.kr/kras/cert/certView.do");
		assert_eq!(surface.contexts[0].pages[scp_index].url, "http://scpweb.softgraphy.biz/board");
	}

	#[test]
	fn target_page_url_accessor_matches_ts_url_function() {
		let p = page("x", "https://www.kras.go.kr/kras/cert/certView.do");
		assert_eq!(p.url(), "https://www.kras.go.kr/kras/cert/certView.do");
		assert_eq!(p.id, "x");
		assert!(p.ws_url.contains("/devtools/page/x"));
	}
}

// ——— SCP 러너 전용 라이브 배선 (TS startPropertyWatcherRuntime / startLivePropertyWatcher) ———

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::cdp_surface::CdpSurface;
use crate::property_progress::{commit_progress_update, prepare_progress_update, PreparedProgressUpdate};
use crate::property_watcher::{
	PropertyWatcher, PropertyWatcherCallbacks, PropertyWatcherEvent, PropertyWatcherTarget,
	WatcherFailure,
};
use crate::property_watcher_channel::{
	ChannelRowHandler, PropertyWatcherChannel,
};
use crate::property_watcher_lifecycle::LifecycleWatcher;
use crate::property_watcher_payload::{
	PropertyBuildingStatusUpdateResult, PropertyBuildingTerminalStatus, PropertyRow,
	PropertyWatcherSelectionHandle,
};

/// TS PropertyWatcherRuntimeDependencies
pub struct PropertyWatcherRuntimeDependencies {
	/// TS connectPropertyWatcherBrowser의 엔드포인트.
	pub endpoint: String,
	/// TS progressPath — 기본은 PROPERTY_PROGRESS_FILE.
	pub progress_path: PathBuf,
	/// TS logger.log(event, count)
	pub logger: Box<dyn FnMut(PropertyWatcherEvent, u32) -> Result<(), WatcherFailure> + Send>,
}

impl Default for PropertyWatcherRuntimeDependencies {
	fn default() -> Self {
		Self {
			endpoint: "http://127.0.0.1:9222".to_string(),
			progress_path: PathBuf::from(crate::property_watcher_process::PROPERTY_WATCHER_PID_FILE)
				.parent()
				.map(|p| p.join("runtime-progress.txt"))
				.unwrap_or_else(|| PathBuf::from("runtime-progress.txt")),
			logger: Box::new(|_, _| Ok(())),
		}
	}
}

/// KRAS 조회 오류를 복구 가능 판정용 값으로 승격한다.
fn map_lookup_error(error: Box<dyn std::error::Error>) -> WatcherFailure {
	use crate::kras_page::{KrasLookupError, KrasLookupTargetError};
	if error.downcast_ref::<KrasLookupError>().is_some() {
		WatcherFailure::KrasLookup
	} else if error.downcast_ref::<KrasLookupTargetError>().is_some() {
		WatcherFailure::KrasLookupTarget
	} else {
		WatcherFailure::Other(error.to_string())
	}
}

/// 라이브 워처 + 이벤트 펌프를 한 묶음으로 다루는 어댑터(TS PropertyWatcher의 stopped/stop 대응).
pub struct LivePropertyWatcher {
	watcher: Arc<Mutex<PropertyWatcher<Arc<CdpSurface>, PreparedProgressUpdate>>>,
	pump_stop: Arc<AtomicBool>,
}

impl LifecycleWatcher for LivePropertyWatcher {
	fn take_stop_outcome(&mut self) -> Option<Result<(), String>> {
		self.watcher
			.lock()
			.ok()
			.and_then(|mut watcher| watcher.take_stop_outcome())
	}

	fn request_stop(&mut self) -> Result<(), String> {
		// 펌프에 종료를 알린다 — 다음 메시지/연결 종료 시 스레드가 스스로 끝난다
		self.pump_stop.store(true, Ordering::SeqCst);
		match self.watcher.lock() {
			Ok(mut watcher) => watcher.request_stop(),
			Err(_) => Err("워처 잠금이 손상되었습니다.".to_string()),
		}
	}

	fn is_active(&self) -> bool {
		self.watcher.lock().map(|watcher| watcher.is_running()).unwrap_or(false)
	}
}

impl Drop for LivePropertyWatcher {
	fn drop(&mut self) {
		self.pump_stop.store(true, Ordering::SeqCst);
	}
}

/// TS startPropertyWatcherRuntime — CDP 연결 → SCP 채널 설치 → 워처 생성 → 이벤트 펌프 가동.
/// KRAS 조회는 설치에서 확보한 KRAS 표면으로 내부 배선한다(TS lookupKrasBuilding과 동일 경로).
pub fn start_property_watcher_runtime(
	dependencies: &mut PropertyWatcherRuntimeDependencies,
) -> Result<Box<dyn LifecycleWatcher>, Box<dyn std::error::Error>> {
	// 1) 타깃 수집(TS connectOverCDP 대응)
	let pages = list_cdp_pages(&dependencies.endpoint)?;
	if pages.is_empty() {
		return Err("CDP 페이지 타깃이 없습니다.".into());
	}
	let (kras_index, scp_index) = {
		let urls: Vec<String> = pages.iter().map(|p| p.url.clone()).collect();
		crate::property_watcher_page::discover_property_watcher_targets(&urls)?
	};

	// 2) 설치 순서 역전 문제: 채널이 워처보다 먼저 필요하므로 슬롯+대기열로 해결한다.
	let row_slot: Arc<Mutex<Option<ChannelRowHandler>>> = Arc::new(Mutex::new(None));
	let pending: Arc<Mutex<Vec<(PropertyRow, PropertyWatcherSelectionHandle, String)>>> =
		Arc::new(Mutex::new(Vec::new()));
	let slot_for_rows = row_slot.clone();
	let pending_for_rows = pending.clone();
	let on_row: ChannelRowHandler = Box::new(move |row, selection, pager| {
		let mut handler = slot_for_rows.lock().map_err(|e| e.to_string())?;
		if let Some(handler) = handler.as_mut() {
			handler(row, selection, pager)
		} else {
			// 워처가 아직 준비되지 않았으면 순서를 지키며 모아 둔다(TS는 enqueue가 먼저 존재한다)
			pending_for_rows
				.lock()
				.map_err(|e| e.to_string())?
				.push((row, selection, pager));
			Ok(())
		}
	});

	// 3) SCP 페이지에 채널 설치
	let surfaces: Vec<Arc<CdpSurface>> = pages
		.iter()
		.map(|page| Arc::new(CdpSurface::from_page(&dependencies.endpoint, page)))
		.collect();
	let scp_surface = surfaces
		.get(scp_index)
		.ok_or("SCP 타깃 인덱스가 범위를 벗어났습니다.")?
		.clone();
	let kras_surface = surfaces
		.get(kras_index)
		.ok_or("KRAS 타깃 인덱스가 범위를 벗어났습니다.")?
		.clone();
	let channel = PropertyWatcherChannel::install_on_surface(scp_surface, on_row)?;

	// 4) 채널을 공유 슬롯에 두고 이벤트 펌프를 가동한다
	let channel_slot: Arc<Mutex<Option<PropertyWatcherChannel>>> =
		Arc::new(Mutex::new(Some(channel)));
	let pump_stop = Arc::new(AtomicBool::new(false));
	let channel_for_pump = channel_slot.clone();
	let stop_for_pump = pump_stop.clone();
	let ws_url = scp_surface_page_ws(&pages, scp_index)?;
	let _pump = crate::property_watcher_channel::spawn_event_pump(
		ws_url,
		channel_for_pump,
		stop_for_pump,
	);

	// 5) 워처용타깃 — 상태 기록·해제를 채널에 연결한다
	let channel_for_status = channel_slot.clone();
	let target = PropertyWatcherTarget {
		target: kras_surface.clone(),
		update_building_status: Box::new(
			move |selection: &PropertyWatcherSelectionHandle, status| {
				let mut slot = channel_for_status
					.lock()
					.map_err(|e| WatcherFailure::Other(e.to_string()))?;
				match slot.as_mut() {
					Some(mut channel) => channel
						.update_building_status(selection, status)
						.map_err(|e| WatcherFailure::Other(e.to_string())),
					None => Ok(PropertyBuildingStatusUpdateResult::Stale),
				}
			},
		),
		dispose: Box::new(move || {
			let mut slot = channel_slot
				.lock()
				.map_err(|e| WatcherFailure::Other(e.to_string()))?;
			match slot.take() {
				Some(mut channel) => {
					channel.dispose().map_err(|e| WatcherFailure::Other(e.to_string()))
				}
				None => Ok(()),
			}
		}),
	};

	// 6) 의존 동작을꺼내 워처 콜백을 만든다(TS prepareProgress/lookupKras/commitProgress/logger)
	let progress_path = dependencies.progress_path.clone();
	let mut logger = std::mem::replace(&mut dependencies.logger, Box::new(|_, _| Ok(())));
	let kras_for_lookup = kras_surface.clone();
	let callbacks = PropertyWatcherCallbacks {
		prepare_progress: Box::new(move |address, pager| {
			prepare_progress_update(&progress_path, address.as_str(), pager).map_err(Into::into)
		}),
		// TS lookupKrasBuilding(page, address) — KRAS 표면에서 직접 조회한다.
		lookup_kras: Box::new(move |_surface, address| {
			let _ = _surface;
			crate::stage12::lookup_kras_building(kras_for_lookup.as_ref(), address.as_str())
				.map_err(map_lookup_error)
		}),
		commit_progress: Box::new(|update| commit_progress_update(update).map_err(Into::into)),
		is_recoverable: None,
		log: logger,
	};

	// 7) 워처 생성 후 수신슬롯을 채우고, 그 전에 모인 행을 순서대로 넘긴다
	let watcher = Arc::new(Mutex::new(PropertyWatcher::new(target, callbacks)));
	{
		let watcher_for_rows = watcher.clone();
		let mut slot = row_slot.lock().map_err(|e| e.to_string())?;
		*slot = Some(Box::new(move |row, selection, pager| {
			watcher_for_rows
				.lock()
				.map_err(|e| e.to_string())?
				.enqueue(&row, selection, pager);
			Ok(())
		}));
	}
	let queued = pending.lock().map_err(|e| e.to_string())?.drain(..).collect::<Vec<_>>();
	if let Ok(mut slot) = row_slot.lock() {
		if let Some(mut handler) = slot.as_mut() {
			for (row, selection, pager) in queued {
				let _ = handler(row, selection, pager);
			}
		}
	}

	Ok(Box::new(LivePropertyWatcher { watcher, pump_stop }))
}

/// SCP 타깃의 웹소켓 주소를 찾는다.
fn scp_surface_page_ws(pages: &[CdpTargetPage], scp_index: usize) -> Result<String, Box<dyn std::error::Error>> {
	pages
		.get(scp_index)
		.map(|page| page.ws_url.clone())
		.filter(|ws| !ws.is_empty())
		.ok_or("SCP 타깃의 웹소켓 주소가 없습니다.".into())
}

/// TS startLivePropertyWatcher — 고정 엔드포인트·진행 파일·기본 로거로 러스타임을 연다.
pub fn start_live_property_watcher(
	logger: Box<dyn FnMut(PropertyWatcherEvent, u32) -> Result<(), WatcherFailure> + Send>,
) -> Result<Box<dyn LifecycleWatcher>, Box<dyn std::error::Error>> {
	let mut dependencies = PropertyWatcherRuntimeDependencies {
		endpoint: CDP_ENDPOINT.to_string(),
		progress_path: PathBuf::from(crate::property_progress::PROPERTY_PROGRESS_FILE),
		logger,
	};
	start_property_watcher_runtime(&mut dependencies)
}
