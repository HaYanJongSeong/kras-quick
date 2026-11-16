// TS scripts/property-watcher-channel-state.ts — SCP 행 바인딩 수신 상태 기계.
// 세대(generation)·루트 프레임·컨텍스트 수명으로 위변조 행 수신을 차단한다.

use std::collections::HashMap;

use crate::property_watcher_cdp::{
	PropertyWatcherBindingEvent, PropertyWatcherContextEvent, PropertyWatcherFrameEvent,
};
use crate::property_watcher_listener::{
	PROPERTY_WATCHER_BINDING_NAME, PROPERTY_WATCHER_WORLD_NAME,
};
use crate::property_watcher_page::{is_scp_target_url, url_origin, SCP_ORIGIN};
use crate::property_watcher_payload::{
	parse_property_watcher_payload, PropertyRow, PropertyWatcherSelectionHandle,
};

/// 상태 이벤트 처리 실패 시 호출된다(TS onFatal).
pub type FatalHandler = Box<dyn FnMut(String) + Send>;
/// 행 수신 콜백(TS onRow). Err면 onFatal로 전달된다.
pub type RowHandler = Box<
	dyn FnMut(PropertyRow, PropertyWatcherSelectionHandle, String) -> Result<(), String> + Send,
>;

pub struct ChannelStateDeps {
	pub on_fatal: FatalHandler,
	pub on_row: RowHandler,
}

struct LiveContext {
	context: PropertyWatcherContextEvent,
	generation: u64,
	latest_selection_id: Option<u64>,
}

/// TS PropertyWatcherChannelState — 전부 `&mut self`로 상태를 바꾼다.
pub struct PropertyWatcherChannelState {
	deps: ChannelStateDeps,
	active: bool,
	cleanup_context_ids: Option<Vec<i64>>,
	generation: u64,
	root_frame_id: Option<String>,
	root_url: Option<String>,
	contexts: HashMap<i64, LiveContext>,
}

impl PropertyWatcherChannelState {
	pub fn new(deps: ChannelStateDeps) -> Self {
		Self {
			deps,
			active: true,
			cleanup_context_ids: None,
			generation: 0,
			root_frame_id: None,
			root_url: None,
			contexts: HashMap::new(),
		}
	}

	/// TS isWatcherRootContext — 워처 전용 isolated world이며 루트 프레임/오리진과 일치해야 한다.
	fn is_watcher_root_context(&self, context: &PropertyWatcherContextEvent) -> bool {
		context.name == PROPERTY_WATCHER_WORLD_NAME
			&& context.r#type.as_deref() == Some("isolated")
			&& context.frame_id == self.root_frame_id
			&& context.origin == SCP_ORIGIN
	}

	/// TS isLiveSource — 활성·세대·루트 URL·컨텍스트 조건이 모두 맞아야 행을 인정한다.
	fn is_live_source(&self, event: &PropertyWatcherBindingEvent) -> bool {
		if !self.active || event.name != PROPERTY_WATCHER_BINDING_NAME {
			return false;
		}
		let Some(live) = self.contexts.get(&event.context_id) else {
			return false;
		};
		if live.generation != self.generation {
			return false;
		}
		let Some(root_url) = self.root_url.as_deref() else {
			return false;
		};
		if !is_scp_target_url(root_url) {
			return false;
		}
		self.is_watcher_root_context(&live.context)
			&& url_origin(root_url).as_deref() == Some(live.context.origin.as_str())
	}

	/// TS isCurrentSelection — 전달받은 핸들이 현재 세대의 최신 선택인지 판별한다.
	pub fn is_current_selection(&self, selection: &PropertyWatcherSelectionHandle) -> bool {
		if !self.active {
			return false;
		}
		let Some(live) = self.contexts.get(&selection.context_id) else {
			return false;
		};
		live.generation == self.generation
			&& live.generation == selection.generation
			&& live.context.unique_id == selection.context_unique_id
			&& live.latest_selection_id == Some(selection.selection_id)
			&& self.is_watcher_root_context(&live.context)
			&& self.root_url.as_deref().is_some_and(is_scp_target_url)
	}

	/// TS acceptBinding — 페이로드를 파싱해 행 콜백을 부른다.
	pub fn accept_binding(&mut self, event: &PropertyWatcherBindingEvent) {
		if !self.is_live_source(event) {
			return;
		}
		let Some(payload) = parse_property_watcher_payload(&event.payload) else {
			return;
		};
		let Some(live) = self.contexts.get_mut(&event.context_id) else {
			return;
		};
		live.latest_selection_id = Some(payload.selection_id);
		let selection = PropertyWatcherSelectionHandle {
			context_id: live.context.context_id,
			context_unique_id: live.context.unique_id.clone(),
			generation: live.generation,
			selection_id: payload.selection_id,
		};
		let result = (self.deps.on_row)(payload.cells, selection, payload.pager);
		if let Err(message) = result {
			(self.deps.on_fatal)(message);
		}
	}

	/// TS clearContexts — 세대를 올리고 컨텍스트를 비운다.
	pub fn clear_contexts(&mut self) {
		if !self.active {
			return;
		}
		self.generation += 1;
		self.contexts.clear();
	}

	/// TS createContext
	pub fn create_context(&mut self, event: PropertyWatcherContextEvent) {
		if !self.active {
			return;
		}
		self.contexts.insert(
			event.context_id,
			LiveContext { context: event, generation: self.generation, latest_selection_id: None },
		);
	}

	/// TS deactivate — 루트 컨텍스트 id를 돌려주고 이후 모든 처리를 무시한다.
	pub fn deactivate(&mut self) -> Vec<i64> {
		if !self.active {
			return self.cleanup_context_ids.clone().unwrap_or_default();
		}
		let ids: Vec<i64> = self
			.contexts
			.values()
			.filter(|live| self.is_watcher_root_context(&live.context))
			.map(|live| live.context.context_id)
			.collect();
		self.cleanup_context_ids = Some(ids.clone());
		self.active = false;
		self.generation += 1;
		ids
	}

	/// TS destroyContext
	pub fn destroy_context(&mut self, context_id: i64) {
		if !self.active {
			return;
		}
		self.contexts.remove(&context_id);
	}

	/// TS initializeRoot — 루트 프레임과 URL을 고정한다.
	pub fn initialize_root(&mut self, event: &PropertyWatcherFrameEvent) {
		if !self.active {
			return;
		}
		self.root_frame_id = Some(event.frame_id.clone());
		self.root_url = Some(event.url.clone());
	}

	/// TS navigateDocument — 루트가 이동하면 세대를 올리고 컨텍스트를 모두 버린다.
	pub fn navigate_document(&mut self, event: &PropertyWatcherFrameEvent) {
		if !self.active {
			return;
		}
		if Some(event.frame_id.as_str()) != self.root_frame_id.as_deref() {
			return;
		}
		self.generation += 1;
		self.root_url = Some(event.url.clone());
		self.contexts.clear();
	}

	pub fn navigate_history(&mut self, event: &PropertyWatcherFrameEvent) {
		if !self.active {
			return;
		}
		if Some(event.frame_id.as_str()) != self.root_frame_id.as_deref() {
			return;
		}
		self.generation += 1;
		self.root_url = Some(event.url.clone());
		for live in self.contexts.values_mut() {
			live.generation = self.generation;
		}
	}

	/// 현재 세대 번호 (테스트·진단용).
	pub fn generation(&self) -> u64 {
		self.generation
	}

	/// 활성 여부 (테스트·진단용).
	pub fn is_active(&self) -> bool {
		self.active
	}

	/// 등록된 컨텍스트 수 (테스트용).
	pub fn context_count(&self) -> usize {
		self.contexts.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::AtomicU32;
	use std::sync::{Arc, Mutex};

	fn root_frame() -> PropertyWatcherFrameEvent {
		PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "http://scpweb.softgraphy.biz/board".to_string(),
		}
	}

	fn root_context(context_id: i64) -> PropertyWatcherContextEvent {
		PropertyWatcherContextEvent {
			context_id,
			frame_id: Some("FRAME1".to_string()),
			name: PROPERTY_WATCHER_WORLD_NAME.to_string(),
			origin: SCP_ORIGIN.to_string(),
			r#type: Some("isolated".to_string()),
			unique_id: format!("ctx-{context_id}"),
		}
	}

	fn binding(context_id: i64, payload: &str) -> PropertyWatcherBindingEvent {
		PropertyWatcherBindingEvent {
			context_id,
			name: PROPERTY_WATCHER_BINDING_NAME.to_string(),
			payload: payload.to_string(),
		}
	}

	fn valid_payload(selection_id: u64) -> String {
		format!(
			r#"{{"version":3,"selectionId":{selection_id},"cells":["a","b","c","d","e","f","g","h"],"pager":"1/20"}}"#
		)
	}

	struct Recorder {
		rows: Arc<Mutex<Vec<(u64, String, String)>>>,
		fatals: Arc<Mutex<Vec<String>>>,
	}

	fn make_state() -> (PropertyWatcherChannelState, Recorder) {
		let rows = Arc::new(Mutex::new(Vec::new()));
		let fatals = Arc::new(Mutex::new(Vec::new()));
		let rows_cell = rows.clone();
		let fatals_cell = fatals.clone();
		let state = PropertyWatcherChannelState::new(ChannelStateDeps {
			on_fatal: Box::new(move |message| fatals_cell.lock().unwrap().push(message)),
			on_row: Box::new(move |_cells, selection, pager| {
				rows_cell.lock().unwrap().push((selection.selection_id, pager, selection.context_unique_id.clone()));
				Ok(())
			}),
		});
		(state, Recorder { rows, fatals })
	}

	fn live_state() -> (PropertyWatcherChannelState, Recorder) {
		let (mut state, rec) = make_state();
		state.initialize_root(&root_frame());
		state.create_context(root_context(7));
		(state, rec)
	}

	#[test]
	fn accepts_binding_from_live_scp_root_context() {
		let (mut state, rec) = live_state();
		state.accept_binding(&binding(7, &valid_payload(4)));
		assert_eq!(*rec.rows.lock().unwrap(), vec![(4, "1/20".to_string(), "ctx-7".to_string())]);
		assert!(rec.fatals.lock().unwrap().is_empty());
		// 최신 선택 id가 기록된다
		assert!(state.is_current_selection(&PropertyWatcherSelectionHandle {
			context_id: 7,
			context_unique_id: "ctx-7".to_string(),
			generation: state.generation(),
			selection_id: 4,
		}));
	}

	#[test]
	fn ignores_wrong_binding_name() {
		let (mut state, rec) = live_state();
		let mut event = binding(7, &valid_payload(4));
		event.name = "somethingElse".to_string();
		state.accept_binding(&event);
		assert!(rec.rows.lock().unwrap().is_empty());
	}

	#[test]
	fn ignores_unknown_context() {
		let (mut state, rec) = live_state();
		state.accept_binding(&binding(99, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty());
	}

	#[test]
	fn clear_contexts_invalidates_previous_registrations() {
		let (mut state, rec) = live_state();
		assert_eq!(state.context_count(), 1);
		// clearContexts는 세대를 올리면서 컨텍스트를 비운다 — 이전 등록분은 즉시 무효
		state.clear_contexts();
		assert_eq!(state.context_count(), 0, "이전 컨텍스트가 제거되어야 한다");
		state.accept_binding(&binding(7, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty(), "등록 해제된 컨텍스트는 무시된다");

		// 같은 id로 재등록하면 새 세대 컨텍스트로 인정된다(TS와 동일)
		state.create_context(root_context(7));
		state.accept_binding(&binding(7, &valid_payload(4)));
		assert_eq!(rec.rows.lock().unwrap().len(), 1, "재등록 후엔 수신 인정");
	}

	#[test]
	fn ignores_non_scp_root_url() {
		let (mut state, rec) = make_state();
		state.initialize_root(&PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "https://example.com/board".to_string(),
		});
		state.create_context(root_context(7));
		state.accept_binding(&binding(7, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty(), "SCP가 아닌 루트는 무시");
	}

	#[test]
	fn ignores_context_that_is_not_isolated_world_on_root_frame() {
		let (mut state, rec) = live_state();
		let mut wrong_world = root_context(8);
		wrong_world.name = "other-world".to_string();
		state.create_context(wrong_world);
		state.accept_binding(&binding(8, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty());

		let mut wrong_type = root_context(9);
		wrong_type.r#type = Some("main".to_string());
		state.create_context(wrong_type);
		state.accept_binding(&binding(9, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty(), "isolated world가 아니면 무시");

		let mut wrong_frame = root_context(10);
		wrong_frame.frame_id = Some("FRAME2".to_string());
		state.create_context(wrong_frame);
		state.accept_binding(&binding(10, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty(), "루트 프레임이 아니면 무시");
	}

	#[test]
	fn ignores_malformed_payload_without_fatal() {
		let (mut state, rec) = live_state();
		state.accept_binding(&binding(7, "{broken"));
		state.accept_binding(&binding(7, &valid_payload(0).replace("selectionId\":0", "selectionId\":0")));
		assert!(rec.rows.lock().unwrap().is_empty());
		assert!(rec.fatals.lock().unwrap().is_empty(), "형식 오류는 조용히 무시한다");
	}

	#[test]
	fn inactive_state_ignores_everything() {
		let (mut state, rec) = make_state();
		state.initialize_root(&root_frame());
		state.create_context(root_context(7));
		let ids = state.deactivate();
		assert_eq!(ids, vec![7]);
		assert!(!state.is_active());
		// deactivate 이후 전부 no-op
		state.accept_binding(&binding(7, &valid_payload(4)));
		state.create_context(root_context(8));
		state.clear_contexts();
		assert!(rec.rows.lock().unwrap().is_empty());
		assert_eq!(state.context_count(), 1, "deactivate 후 컨텍스트 추가 불가");
		// deactivate 재호출은 이전 결과를 반환한다
		assert_eq!(state.deactivate(), vec![7]);
	}

	#[test]
	fn document_navigation_drops_contexts_and_bumps_generation() {
		let (mut state, rec) = live_state();
		let before = state.generation();
		state.navigate_document(&PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "http://scpweb.softgraphy.biz/board?page=2".to_string(),
		});
		assert_eq!(state.context_count(), 0, "문서 이동 시 컨텍스트 소멸");
		assert!(state.generation() > before);
		// 새 세대에 컨텍스트를 만들어도 인정 (rootUrl이 SCP 유지)
		state.create_context(root_context(7));
		state.accept_binding(&binding(7, &valid_payload(1)));
		assert_eq!(rec.rows.lock().unwrap().len(), 1);
	}

	#[test]
	fn history_navigation_keeps_contexts_but_retags_generation() {
		let (mut state, rec) = live_state();
		let before = state.generation();
		state.navigate_history(&PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "http://scpweb.softgraphy.biz/board#top".to_string(),
		});
		assert_eq!(state.context_count(), 1, "history 이동은 컨텍스트를 유지한다");
		assert!(state.generation() > before);
		// retag 되어 그대로 인정된다
		state.accept_binding(&binding(7, &valid_payload(9)));
		assert_eq!(rec.rows.lock().unwrap().len(), 1);
	}

	#[test]
	fn navigation_on_other_frame_is_ignored() {
		let (mut state, _rec) = live_state();
		let before = state.generation();
		state.navigate_document(&PropertyWatcherFrameEvent {
			frame_id: "OTHER".to_string(),
			url: "http://scpweb.softgraphy.biz/board?page=9".to_string(),
		});
		state.navigate_history(&PropertyWatcherFrameEvent {
			frame_id: "OTHER".to_string(),
			url: "http://scpweb.softgraphy.biz/board?page=9".to_string(),
		});
		assert_eq!(state.context_count(), 1);
		assert_eq!(state.generation(), before, "루트 프레임이 아니면 무시");
	}

	#[test]
	fn current_selection_requires_matching_latest_selection() {
		let (mut state, _rec) = live_state();
		let generation = state.generation();
		let handle = |selection_id: u64| PropertyWatcherSelectionHandle {
			context_id: 7,
			context_unique_id: "ctx-7".to_string(),
			generation,
			selection_id,
		};
		assert!(!state.is_current_selection(&handle(1)), "수신 전이라 최신 없음");
		state.accept_binding(&binding(7, &valid_payload(1)));
		assert!(state.is_current_selection(&handle(1)));
		assert!(!state.is_current_selection(&handle(2)), "다른 선택 id");

		// 세대가 바뀌면 무효
		let later = state.generation();
		assert!(!state.is_current_selection(&PropertyWatcherSelectionHandle {
			context_id: 7,
			context_unique_id: "ctx-7".to_string(),
			generation: later + 1,
			selection_id: 1,
		}));
		// uniqueId 불일치
		assert!(!state.is_current_selection(&PropertyWatcherSelectionHandle {
			context_id: 7,
			context_unique_id: "other".to_string(),
			generation: state.generation(),
			selection_id: 1,
		}));
	}

	#[test]
	fn destroy_context_removes_it() {
		let (mut state, rec) = live_state();
		state.destroy_context(7);
		assert_eq!(state.context_count(), 0);
		state.accept_binding(&binding(7, &valid_payload(4)));
		assert!(rec.rows.lock().unwrap().is_empty());
	}

	#[test]
	fn row_handler_error_is_reported_through_on_fatal() {
		let fatals = Arc::new(Mutex::new(Vec::new()));
		let fatals_cell = fatals.clone();
		let mut state = PropertyWatcherChannelState::new(ChannelStateDeps {
			on_fatal: Box::new(move |m| fatals_cell.lock().unwrap().push(m)),
			on_row: Box::new(|_, _, _| Err("handler 실패".to_string())),
		});
		state.initialize_root(&root_frame());
		state.create_context(root_context(7));
		state.accept_binding(&binding(7, &valid_payload(3)));
		assert_eq!(*fatals.lock().unwrap(), vec!["handler 실패".to_string()]);
	}
}
