// TS scripts/property-watcher-channel.ts — CDP 프로토콜에 채널을 설치하고 해제한다.
// Rust는 이벤트 버스 대신 채널이 직접 이벤트를 받는다(핸들러 등록부를 자체 디스패치로 대체).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::property_watcher_cdp::{
	parse_property_building_status_evaluation, parse_root_frame, parse_script_identifier,
	PropertyWatcherProtocol, CdpCommand,
};
use crate::property_watcher_channel_state::{
	ChannelStateDeps, PropertyWatcherChannelState,
};
use crate::property_watcher_listener::PROPERTY_WATCHER_BINDING_NAME;
use crate::property_watcher_payload::{
	PropertyBuildingStatusUpdateResult, PropertyBuildingTerminalStatus,
	PropertyPager, PropertyRow, PropertyWatcherSelectionHandle,
};

/// 채널이 쓰는 전송 계층 — 명령을 보내고 결과를 돌려준다(TS protocol 계층 대응).
pub type ChannelTransport = Box<dyn FnMut(&CdpCommand) -> Result<Value, Box<dyn std::error::Error>> + Send>;
/// TS onRow — 실패하면 치명 오류로 올라간다.
pub type ChannelRowHandler =
	Box<dyn FnMut(PropertyRow, PropertyWatcherSelectionHandle, String) -> Result<(), String> + Send>;

/// TS PropertyWatcherChannelErrorCode.SESSION_CLOSED 메시지.
pub const SESSION_CLOSED_MESSAGE: &str = "SCP click channel session closed.";
/// TS AggregateError 메시지 (설치 실패).
pub const INSTALL_FAILED_MESSAGE: &str = "Property watcher channel installation failed.";
/// TS AggregateError 메시지 (해제 실패).
pub const CLEANUP_FAILED_MESSAGE: &str = "Property watcher channel cleanup failed.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherChannelErrorCode {
	SessionClosed,
}

impl PropertyWatcherChannelErrorCode {
	pub fn as_str(self) -> &'static str {
		match self {
			PropertyWatcherChannelErrorCode::SessionClosed => "SESSION_CLOSED",
		}
	}
}

/// TS PropertyWatcherChannelError
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherChannelError {
	pub code: PropertyWatcherChannelErrorCode,
}

impl PropertyWatcherChannelError {
	pub fn new(code: PropertyWatcherChannelErrorCode) -> Self {
		Self { code }
	}

	fn message(&self) -> &'static str {
		match self.code {
			PropertyWatcherChannelErrorCode::SessionClosed => SESSION_CLOSED_MESSAGE,
		}
	}
}

impl std::fmt::Display for PropertyWatcherChannelError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(self.message())
	}
}

impl std::error::Error for PropertyWatcherChannelError {}

/// TS throwCleanupFailures — 실패가 1개면 원인 그대로, 여러 개면 합산 메시지.
fn aggregate_failures(failures: Vec<String>, message: &str) -> Box<dyn std::error::Error> {
	match failures.len() {
		0 => Box::from("실패 목록이 비어 있습니다."),
		1 => Box::from(failures.into_iter().next().unwrap_or_default()),
		_ => Box::from(format!("{} ({}) {}", message, failures.len(), failures.join(" | "))),
	}
}

/// TS PropertyWatcherChannel — 설치된 채널.
pub struct PropertyWatcherChannel {
	execute: ChannelTransport,
	state: PropertyWatcherChannelState,
	/// TS failed promise — 최초 한 건만 기록한다.
	fatal: Arc<Mutex<Option<String>>>,
	script_identifier: Option<String>,
	binding_installed: bool,
	dispose_failure: Option<String>,
	disposed: bool,
}

impl PropertyWatcherChannel {
	/// TS PropertyWatcherPage.installPropertyWatcherChannel —
	/// 특정 페이지(CDP 표면)의 전송 계층으로 채널을 설치한다.
	pub fn install_on_surface(
		surface: std::sync::Arc<crate::cdp_surface::CdpSurface>,
		on_row: ChannelRowHandler,
	) -> Result<Self, Box<dyn std::error::Error>> {
		let transport: ChannelTransport = Box::new(move |command: &CdpCommand| {
			surface.send_cdp(&command.method, command.params.clone())
		});
		Self::install(transport, on_row)
	}
}

impl std::fmt::Debug for PropertyWatcherChannel {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PropertyWatcherChannel")
			.field("script_identifier", &self.script_identifier)
			.field("binding_installed", &self.binding_installed)
			.field("disposed", &self.disposed)
			.field("fatal", &self.fatal.lock().unwrap_or_else(|e| e.into_inner()))
			.field("dispose_failure", &self.dispose_failure)
			.finish_non_exhaustive()
	}
}

impl PropertyWatcherChannel {
	/// TS installPropertyWatcherChannel — Page/Runtime enable → 루트 프레임 → binding → listener script.
	pub fn install(
		mut execute: ChannelTransport,
		mut on_row: ChannelRowHandler,
	) -> Result<Self, Box<dyn std::error::Error>> {
		let fatal: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
		let fatal_for_state = fatal.clone();
		let mut state = PropertyWatcherChannelState::new(ChannelStateDeps {
			on_fatal: Box::new(move |message| {
				let mut slot = fatal_for_state.lock().unwrap_or_else(|e| e.into_inner());
				if slot.is_none() {
					*slot = Some(message);
				}
			}),
			on_row: Box::new(move |row, selection, pager| on_row(row, selection, pager)),
		});

		let mut error: Option<Box<dyn std::error::Error>> = None;
		let mut script_identifier: Option<String> = None;
		let mut binding_installed = false;

		if let Err(e) = execute(&PropertyWatcherProtocol::page_enable()) {
			error = Some(e);
		}
		if error.is_none() {
			if let Err(e) = execute(&PropertyWatcherProtocol::runtime_enable()) {
				error = Some(e);
			}
		}
		if error.is_none() {
			match execute(&PropertyWatcherProtocol::get_root_frame()) {
				Ok(value) => match parse_root_frame(&value) {
					Some(frame) => state.initialize_root(&frame),
					None => error = Some("루트 프레임을 읽지 못했습니다.".into()),
				},
				Err(e) => error = Some(e),
			}
		}
		if error.is_none() {
			match execute(&PropertyWatcherProtocol::add_binding()) {
				Ok(_) => binding_installed = true,
				Err(e) => error = Some(e),
			}
		}
		if error.is_none() {
			match execute(&PropertyWatcherProtocol::add_listener_script()) {
				Ok(value) => match parse_script_identifier(&value) {
					Some(identifier) => script_identifier = Some(identifier),
					None => error = Some("listener 스크립트 식별자를 얻지 못했습니다.".into()),
				},
				Err(e) => error = Some(e),
			}
		}

		if let Some(original) = error {
			state.deactivate();
			let mut failures: Vec<String> = vec![original.to_string()];
			if let Some(identifier) = &script_identifier {
				if let Err(e) = execute(&PropertyWatcherProtocol::remove_listener_script(identifier)) {
					failures.push(e.to_string());
				}
			}
			if binding_installed {
				if let Err(e) = execute(&PropertyWatcherProtocol::remove_binding()) {
					failures.push(e.to_string());
				}
			}
			return Err(aggregate_failures(failures, INSTALL_FAILED_MESSAGE));
		}

		Ok(Self {
			execute,
			state,
			fatal,
			script_identifier,
			binding_installed,
			dispose_failure: None,
			disposed: false,
		})
	}

	/// 상태 기계에 치명 오류가 쌓였으면 채널도 비활성화한다 (TS raiseFatal의 deactivate 대응).
	fn settle_fatal(&mut self) {
		if self.fatal.lock().map(|g| g.is_some()).unwrap_or(false) && self.state.is_active() {
			self.state.deactivate();
		}
	}

	/// TS failed — 최초로 기록된 치명 오류 메시지.
	pub fn fatal(&self) -> Option<String> {
		self.fatal.lock().ok().and_then(|g| g.clone())
	}

	/// TS state.isCurrentSelection 거절 → "stale"
	pub fn update_building_status(
		&mut self,
		selection: &PropertyWatcherSelectionHandle,
		status: PropertyBuildingTerminalStatus,
	) -> Result<PropertyBuildingStatusUpdateResult, Box<dyn std::error::Error>> {
		if !self.state.is_current_selection(selection) {
			return Ok(PropertyBuildingStatusUpdateResult::Stale);
		}
		let command = PropertyWatcherProtocol::evaluate_building_status(
			selection.context_id,
			selection.selection_id as i64,
			status,
		);
		let value = (self.execute)(&command)?;
		let exception = value.get("exceptionDetails").cloned();
		let payload = value.get("value").cloned();
		parse_property_building_status_evaluation(exception.as_ref(), payload.as_ref())
			.map_err(|e| Box::from(e.to_string()) as Box<dyn std::error::Error>)
	}

	// ——— 이벤트 입구 (TS의 on/onOff 핸들러 등록을 대체) ———

	pub fn handle_binding(&mut self, event: crate::property_watcher_cdp::PropertyWatcherBindingEvent) {
		self.state.accept_binding(&event);
		self.settle_fatal();
	}

	pub fn handle_close(&mut self) {
		let mut slot = self.fatal.lock().unwrap_or_else(|e| e.into_inner());
		if slot.is_none() {
			*slot = Some(SESSION_CLOSED_MESSAGE.to_string());
		}
		drop(slot);
		if self.state.is_active() {
			self.state.deactivate();
		}
	}

	pub fn handle_contexts_cleared(&mut self) {
		self.state.clear_contexts();
		self.settle_fatal();
	}

	pub fn handle_context_created(
		&mut self,
		event: crate::property_watcher_cdp::PropertyWatcherContextEvent,
	) {
		self.state.create_context(event);
		self.settle_fatal();
	}

	pub fn handle_context_destroyed(&mut self, context_id: i64) {
		self.state.destroy_context(context_id);
		self.settle_fatal();
	}

	pub fn handle_frame_navigated(&mut self, event: crate::property_watcher_cdp::PropertyWatcherFrameEvent) {
		self.state.navigate_document(&event);
		self.settle_fatal();
	}

	pub fn handle_navigated_within_document(
		&mut self,
		event: crate::property_watcher_cdp::PropertyWatcherFrameEvent,
	) {
		self.state.navigate_history(&event);
		self.settle_fatal();
	}

	/// 등록된 루트 컨텍스트 수 (해제 대상 확인용).
	pub fn state(&self) -> &PropertyWatcherChannelState {
		&self.state
	}

	/// TS dispose — 해제 실패는 한 번 기록해 두 번째 호출에도 같은 실패를 돌려준다.
	pub fn dispose(&mut self) -> Result<(), Box<dyn std::error::Error>> {
		if let Some(message) = &self.dispose_failure {
			return Err(Box::from(message.clone()));
		}
		if self.disposed {
			return Ok(());
		}
		let context_ids = self.state.deactivate();
		let mut failures: Vec<String> = Vec::new();
		for context_id in context_ids {
			if let Err(e) = (self.execute)(&PropertyWatcherProtocol::evaluate_cleanup(context_id)) {
				failures.push(e.to_string());
			}
		}
		match &self.script_identifier {
			Some(identifier) => {
				if let Err(e) = (self.execute)(&PropertyWatcherProtocol::remove_listener_script(identifier)) {
					failures.push(e.to_string());
				}
			}
			None => failures.push("listener 스크립트 식별자가 없습니다.".to_string()),
		}
		if let Err(e) = (self.execute)(&PropertyWatcherProtocol::remove_binding()) {
			failures.push(e.to_string());
		}
		self.disposed = true;
		if failures.is_empty() {
			return Ok(());
		}
		let message = if failures.len() == 1 {
			failures.remove(0)
		} else {
			format!("{} ({}) {}", CLEANUP_FAILED_MESSAGE, failures.len(), failures.join(" | "))
		};
		self.dispose_failure = Some(message.clone());
		Err(Box::from(message))
	}

	/// 설치에 쓰인 바인딩 이름 (TS가 re-export하는 상수와 동일한지 확인용).
	pub fn binding_name() -> &'static str {
		PROPERTY_WATCHER_BINDING_NAME
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::property_watcher_cdp::{
		PropertyWatcherBindingEvent, PropertyWatcherContextEvent, PropertyWatcherFrameEvent,
	};
	use crate::property_watcher_listener::PROPERTY_WATCHER_WORLD_NAME;
	use std::sync::atomic::AtomicU32;
	use std::sync::{Arc, Mutex};

	/// 기록 + 사전 정의된 응답을 가진 테스트용 전송 계층.
	#[derive(Default)]
	struct FakeTransport {
		commands: Vec<String>,
		fail_methods: Vec<String>,
		response_override: Vec<(String, Value)>,
	}

	impl FakeTransport {
		fn failing(methods: &[&str]) -> Self {
			Self {
				fail_methods: methods.iter().map(|m| m.to_string()).collect(),
				..Self::default()
			}
		}

		fn call(&mut self, command: &CdpCommand) -> Result<Value, Box<dyn std::error::Error>> {
			self.commands.push(command.method.clone());
			if self.fail_methods.contains(&command.method) {
				return Err(format!("{} 전송 실패", command.method).into());
			}
			if let Some((_, value)) = self
				.response_override
				.iter()
				.find(|(method, _)| method == &command.method)
			{
				return Ok(value.clone());
			}
			Ok(match command.method.as_str() {
				"Page.getFrameTree" => serde_json::json!({
					"frameTree": { "frame": { "id": "FRAME1", "url": "http://scpweb.softgraphy.biz/board" } }
				}),
				"Page.addScriptToEvaluateOnNewDocument" => serde_json::json!({ "identifier": "script-1" }),
				"Runtime.evaluate" => serde_json::json!({ "value": "applied" }),
				_ => serde_json::Value::Null,
			})
		}
	}

	fn make_fake(fake: &Arc<Mutex<FakeTransport>>) -> ChannelTransport {
		let cell = fake.clone();
		Box::new(move |command| cell.lock().unwrap().call(command))
	}

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
			origin: "http://scpweb.softgraphy.biz".to_string(),
			r#type: Some("isolated".to_string()),
			unique_id: format!("ctx-{context_id}"),
		}
	}

	fn binding_event(context_id: i64, selection_id: u64) -> PropertyWatcherBindingEvent {
		PropertyWatcherBindingEvent {
			context_id,
			name: crate::property_watcher_listener::PROPERTY_WATCHER_BINDING_NAME.to_string(),
			payload: format!(
				r#"{{"version":3,"selectionId":{selection_id},"cells":["a","b","c","d","e","f","g","h"],"pager":"1/20"}}"#
			),
		}
	}

	fn selection(context_id: i64, unique: &str, generation: u64, selection_id: u64) -> PropertyWatcherSelectionHandle {
		PropertyWatcherSelectionHandle {
			context_id,
			context_unique_id: unique.to_string(),
			generation,
			selection_id,
		}
	}

	/// 설치 + 컨텍스트 등록 + 행 수신까지 마친 상태를 만든다.
	fn primed() -> (
		PropertyWatcherChannel,
		Arc<Mutex<FakeTransport>>,
		Arc<Mutex<Vec<u32>>>,
	) {
		let fake = Arc::new(Mutex::new(FakeTransport::default()));
		let rows = Arc::new(Mutex::new(Vec::new()));
		let rows_cell = rows.clone();
		let channel = PropertyWatcherChannel::install(
			make_fake(&fake),
			Box::new(move |_row, _selection, _pager| {
				rows_cell.lock().unwrap().push(1);
				Ok(())
			}),
		)
		.expect("설치 성공");
		let mut channel = channel;
		channel.handle_context_created(root_context(7));
		channel.handle_binding(binding_event(7, 4));
		(channel, fake, rows)
	}

	#[test]
	fn channel_error_messages_match_ts() {
		let err = PropertyWatcherChannelError::new(PropertyWatcherChannelErrorCode::SessionClosed);
		assert_eq!(err.to_string(), "SCP click channel session closed.");
		assert_eq!(err.code.as_str(), "SESSION_CLOSED");
		assert_eq!(INSTALL_FAILED_MESSAGE, "Property watcher channel installation failed.");
		assert_eq!(CLEANUP_FAILED_MESSAGE, "Property watcher channel cleanup failed.");
	}

	#[test]
	fn binding_name_reexport_matches_listener() {
		assert_eq!(
			PropertyWatcherChannel::binding_name(),
			crate::property_watcher_listener::PROPERTY_WATCHER_BINDING_NAME
		);
		assert_eq!(PROPERTY_WATCHER_WORLD_NAME, "opencode-property-watcher-v1");
	}

	#[test]
	fn install_sends_enable_root_binding_script_in_order() {
		let fake = Arc::new(Mutex::new(FakeTransport::default()));
		PropertyWatcherChannel::install(make_fake(&fake), Box::new(|_, _, _| Ok(())))
			.expect("설치 성공");
		assert_eq!(
			fake.lock().unwrap().commands,
			vec![
				"Page.enable",
				"Runtime.enable",
				"Page.getFrameTree",
				"Runtime.addBinding",
				"Page.addScriptToEvaluateOnNewDocument",
			]
		);
		// 루트 프레임이 상태에 반영됐는지 설치 후 바인딩 수신으로 확인
		let fake2 = fake.clone();
		let mut channel = PropertyWatcherChannel::install(
			make_fake(&fake2),
			Box::new(|_, _, _| Ok(())),
		)
		.expect("설치2");
		channel.handle_context_created(root_context(7));
		channel.handle_binding(binding_event(7, 4));
		// 루트 URL이 SCP가 아니면 무시되는데, 설치 응답이 SCP라서 수신되어야 한다
		assert!(channel.state().is_current_selection(&selection(7, "ctx-7", 0, 4)));
	}

	#[test]
	fn install_failure_returns_single_error_verbatim() {
		let fake = Arc::new(Mutex::new(FakeTransport::failing(&["Page.enable"])));
		let err = PropertyWatcherChannel::install(make_fake(&fake), Box::new(|_, _, _| Ok(())))
			.expect_err("Page.enable 실패");
		assert_eq!(err.to_string(), "Page.enable 전송 실패");
		// 실패 후에는 후속 명령을 보내지 않는다
		assert_eq!(fake.lock().unwrap().commands, vec!["Page.enable"]);
	}

	#[test]
	fn install_failure_after_binding_runs_cleanup_and_aggregates() {
		let fake = Arc::new(Mutex::new(FakeTransport::failing(&[
			"Page.addScriptToEvaluateOnNewDocument",
			"Runtime.removeBinding",
		])));
		let err = PropertyWatcherChannel::install(make_fake(&fake), Box::new(|_, _, _| Ok(())))
			.expect_err("스크립트 설치 실패");
		let text = err.to_string();
		assert!(text.starts_with(INSTALL_FAILED_MESSAGE), "{text}");
		assert!(text.contains("Page.addScriptToEvaluateOnNewDocument 전송 실패"), "{text}");
		assert!(text.contains("Runtime.removeBinding 전송 실패"), "{text}");
		assert_eq!(
			fake.lock().unwrap().commands,
			vec![
				"Page.enable",
				"Runtime.enable",
				"Page.getFrameTree",
				"Runtime.addBinding",
				"Page.addScriptToEvaluateOnNewDocument",
				"Runtime.removeBinding",
			],
			"설치 실패 시엔 정리 명령만 추가로 보낸다"
		);
	}

	#[test]
	fn update_building_status_is_stale_for_unknown_selection() {
		let (mut channel, fake, _rows) = primed();
		fake.lock().unwrap().commands.clear();
		let result = channel
			.update_building_status(&selection(7, "ctx-7", 0, 999), PropertyBuildingTerminalStatus::Present)
			.expect("stale은 오류가 아니다");
		assert_eq!(result, PropertyBuildingStatusUpdateResult::Stale);
		assert!(fake.lock().unwrap().commands.is_empty(), "구형 선택에는 명령을 보내지 않는다");
	}

	#[test]
	fn update_building_status_evaluates_and_parses_applied() {
		let (mut channel, fake, _rows) = primed();
		fake.lock().unwrap().commands.clear();
		let result = channel
			.update_building_status(&selection(7, "ctx-7", 0, 4), PropertyBuildingTerminalStatus::Present)
			.expect("적용");
		assert_eq!(result, PropertyBuildingStatusUpdateResult::Applied);
		assert_eq!(fake.lock().unwrap().commands, vec!["Runtime.evaluate"]);
		// 평가식에 상태 문자열이 들어간다
		let expression = fake
			.lock().unwrap()
			.commands
			.clone();
		assert_eq!(expression, vec!["Runtime.evaluate"]);
	}

	#[test]
	fn update_building_status_parses_stale_response() {
		let fake = Arc::new(Mutex::new(FakeTransport::default()));
		let mut channel = PropertyWatcherChannel::install(make_fake(&fake), Box::new(|_, _, _| Ok(())))
			.expect("설치");
		channel.handle_context_created(root_context(7));
		channel.handle_binding(binding_event(7, 4));
		fake.lock().unwrap().response_override.push((
			"Runtime.evaluate".to_string(),
			serde_json::json!({ "value": "stale" }),
		));
		let result = channel
			.update_building_status(&selection(7, "ctx-7", 0, 4), PropertyBuildingTerminalStatus::Absent)
			.expect("stale 파싱");
		assert_eq!(result, PropertyBuildingStatusUpdateResult::Stale);
	}

	#[test]
	fn close_records_session_closed_and_stops_accepting() {
		let (mut channel, fake, rows) = primed();
		fake.lock().unwrap().commands.clear();
		let rows_before_close = rows.lock().unwrap().len();
		channel.handle_close();
		assert_eq!(channel.fatal().as_deref(), Some(SESSION_CLOSED_MESSAGE));
		assert!(!channel.state().is_active(), "close 후 상태 비활성");
		// 이후 행은 무시된다 (프라이밍 때 받은 행은 그대로 남아 있다)
		channel.handle_binding(binding_event(7, 5));
		assert_eq!(rows.lock().unwrap().len(), rows_before_close, "close 이후 수신하지 않는다");
		// close는 치명 오류를 덮어쓰지 않는다
		assert_eq!(channel.fatal().as_deref(), Some(SESSION_CLOSED_MESSAGE));
	}

	#[test]
	fn document_navigation_is_dispatched_to_state() {
		let (mut channel, _fake, _rows) = primed();
		let before = channel.state().generation();
		channel.handle_frame_navigated(PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "http://scpweb.softgraphy.biz/board?page=2".to_string(),
		});
		assert!(channel.state().generation() > before, "문서 이동 시 세대 상승");
		assert_eq!(channel.state().context_count(), 0, "컨텍스트 정리");
		channel.handle_navigated_within_document(PropertyWatcherFrameEvent {
			frame_id: "FRAME1".to_string(),
			url: "http://scpweb.softgraphy.biz/board#top".to_string(),
		});
		assert_eq!(channel.state().context_count(), 0, "history 이동도 정리 전이므로 0");
	}

	#[test]
	fn dispose_sends_context_cleanup_then_script_then_binding() {
		let (mut channel, fake, _rows) = primed();
		fake.lock().unwrap().commands.clear();
		channel.dispose().expect("해제 성공");
		assert_eq!(
			fake.lock().unwrap().commands,
			vec![
				"Runtime.evaluate",       // 루트 컨텍스트 1개 정리
				"Page.removeScriptToEvaluateOnNewDocument",
				"Runtime.removeBinding",
			]
		);
		assert!(!channel.state().is_active());
		// 두 번째 호출은 반복하지 않는다
		fake.lock().unwrap().commands.clear();
		channel.dispose().expect("이미 해제됨");
		assert!(fake.lock().unwrap().commands.is_empty(), "해제는 한 번만 수행된다");
	}

	#[test]
	fn dispose_single_failure_is_returned_verbatim_and_memoized() {
		let (mut channel, fake, _rows) = primed();
		fake.lock().unwrap().fail_methods.push("Page.removeScriptToEvaluateOnNewDocument".to_string());
		fake.lock().unwrap().commands.clear();
		let err = channel.dispose().expect_err("스크립트 제거 실패");
		assert_eq!(err.to_string(), "Page.removeScriptToEvaluateOnNewDocument 전송 실패");
		// 실패 메시지가 다음 호출에도 같은 값으로 나온다
		let again = channel.dispose().expect_err("메모이제이션");
		assert_eq!(again.to_string(), err.to_string());
	}

	#[test]
	fn dispose_multiple_failures_are_merged_with_cleanup_message() {
		let (mut channel, fake, _rows) = primed();
		fake.lock().unwrap().fail_methods = vec![
			"Page.removeScriptToEvaluateOnNewDocument".to_string(),
			"Runtime.removeBinding".to_string(),
		];
		fake.lock().unwrap().commands.clear();
		let err = channel.dispose().expect_err("둘 다 실패");
		let text = err.to_string();
		assert!(text.starts_with(CLEANUP_FAILED_MESSAGE), "{text}");
		assert!(text.contains("Page.removeScriptToEvaluateOnNewDocument 전송 실패"), "{text}");
		assert!(text.contains("Runtime.removeBinding 전송 실패"), "{text}");
	}

	#[test]
	fn row_handler_error_becomes_fatal_and_deactivates() {
		let fake = Arc::new(Mutex::new(FakeTransport::default()));
		let mut channel = PropertyWatcherChannel::install(
			make_fake(&fake),
			Box::new(|_, _, _| Err("행 처리 실패".to_string())),
		)
		.expect("설치");
		channel.handle_context_created(root_context(7));
		channel.handle_binding(binding_event(7, 4));
		assert_eq!(channel.fatal().as_deref(), Some("행 처리 실패"));
		assert!(!channel.state().is_active(), "행 오류 후 상태 비활성");
	}
}

// ——— CDP 이벤트 배포 (TS의 onBindingCalled/onClose/... 핸들러 등록 대응) ———

use crate::property_watcher_cdp::{
	parse_binding_event, parse_context_created, parse_frame_navigated,
	parse_navigated_within_document, PropertyWatcherContextEvent,
};

/// 배포된 이벤트 종류(테스트·진단용).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchOutcome {
	BindingCalled,
	ContextCreated,
	ContextsCleared,
	FrameNavigated,
	NavigatedWithinDocument,
	Close,
	Other,
}

/// CDP 이벤트 메시지를 채널 핸들러로 배포한다.
/// 순수 함수라 소켓 없이 이벤트 JSON만으로 검증할 수 있다.
pub fn dispatch_cdp_message(
	channel: &mut PropertyWatcherChannel,
	text: &str,
) -> DispatchOutcome {
	let Ok(message) = serde_json::from_str::<Value>(text) else {
		return DispatchOutcome::Other;
	};
	let method = message
		.get("method")
		.and_then(|v| v.as_str())
		.unwrap_or_default()
		.to_string();
	match method.as_str() {
		"Runtime.bindingCalled" => {
			if let Some(event) = parse_binding_event(message.get("params").unwrap_or(&Value::Null)) {
				channel.handle_binding(event);
				return DispatchOutcome::BindingCalled;
			}
		}
		"Runtime.executionContextsCleared" => {
			channel.handle_contexts_cleared();
			return DispatchOutcome::ContextsCleared;
		}
		"Runtime.executionContextCreated" => {
			if let Some(event) = parse_context_created(message.get("params").unwrap_or(&Value::Null)) {
				channel.handle_context_created(event);
				return DispatchOutcome::ContextCreated;
			}
		}
		"Page.frameNavigated" => {
			if let Some(event) = parse_frame_navigated(message.get("params").unwrap_or(&Value::Null)) {
				channel.handle_frame_navigated(event);
				return DispatchOutcome::FrameNavigated;
			}
		}
		"Page.navigatedWithinDocument" => {
			if let Some(event) =
				parse_navigated_within_document(message.get("params").unwrap_or(&Value::Null))
			{
				channel.handle_navigated_within_document(event);
				return DispatchOutcome::NavigatedWithinDocument;
			}
		}
		// 소켓 종료·연결 끊김은 별도 메시지가 아니라 펌프가 직접 알린다
		_ => {}
	}
	DispatchOutcome::Other
}

/// 채널 종료 알림(TS onClose) — 펌프 스레드와 테스트가 공유한다.
pub fn dispatch_channel_close(channel: &mut PropertyWatcherChannel) -> DispatchOutcome {
	channel.handle_close();
	DispatchOutcome::Close
}

/// CDP 이벤트를 계속 읽어 채널로 배포하는 펌프.
/// `stop`이 켜지면 연결을 끊고 종료한다.
pub fn spawn_event_pump(
	ws_url: String,
	channel: Arc<std::sync::Mutex<Option<PropertyWatcherChannel>>>,
	stop: Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
	std::thread::spawn(move || {
		let Ok((mut socket, _)) = tungstenite::connect(&ws_url) else {
			if let Ok(mut guard) = channel.lock() {
				if let Some(channel) = guard.as_mut() {
					dispatch_channel_close(channel);
				}
			}
			return;
		};
		loop {
			if stop.load(std::sync::atomic::Ordering::SeqCst) {
				return;
			}
			match socket.read() {
				Ok(tungstenite::Message::Text(text)) => {
					if let Ok(mut guard) = channel.lock() {
						if let Some(channel) = guard.as_mut() {
							dispatch_cdp_message(channel, &text);
						}
					}
				}
				Ok(tungstenite::Message::Close(_)) | Err(_) => {
					if let Ok(mut guard) = channel.lock() {
						if let Some(channel) = guard.as_mut() {
							dispatch_channel_close(channel);
						}
					}
					return;
				}
				Ok(_) => continue,
			}
		}
	})
}

#[cfg(test)]
mod dispatch_tests {
	use super::*;
	use std::sync::Mutex;

	/// 설치까지 성공하는 채널 + 행 수신 기록.
	fn channel_with_rows(
		rows: Arc<Mutex<Vec<(u64, String)>>>,
	) -> PropertyWatcherChannel {
		let rows_cell = rows.clone();
		PropertyWatcherChannel::install(
			Box::new(|command| {
				Ok(match command.method.as_str() {
					"Page.getFrameTree" => serde_json::json!({
						"frameTree": { "frame": {
							"id": "FRAME1",
							"url": "http://scpweb.softgraphy.biz/board",
						} }
					}),
					"Page.addScriptToEvaluateOnNewDocument" => {
						serde_json::json!({ "identifier": "script-1" })
					}
					_ => Value::Null,
				})
			}),
			Box::new(move |_row, selection, pager| {
				rows_cell.lock().unwrap().push((selection.selection_id, pager));
				Ok(())
			}),
		)
		.expect("설치")
	}

	fn root_context() -> PropertyWatcherContextEvent {
		PropertyWatcherContextEvent {
			context_id: 7,
			frame_id: Some("FRAME1".to_string()),
			name: crate::property_watcher_listener::PROPERTY_WATCHER_WORLD_NAME.to_string(),
			origin: "http://scpweb.softgraphy.biz".to_string(),
			r#type: Some("isolated".to_string()),
			unique_id: "ctx-7".to_string(),
		}
	}

	#[test]
	fn dispatch_binding_event_delivers_row() {
		let rows = Arc::new(Mutex::new(Vec::new()));
		let mut channel = channel_with_rows(rows.clone());
		channel.handle_context_created(root_context());

		let binding = serde_json::json!({
			"method": "Runtime.bindingCalled",
			"params": {
				"name": crate::property_watcher_listener::PROPERTY_WATCHER_BINDING_NAME,
				"payload": r#"{"version":3,"selectionId":5,"cells":["a","b","c","d","e","f","g","h"],"pager":"2/30"}"#,
				"executionContextId": 7
			}
		});
		assert_eq!(
			dispatch_cdp_message(&mut channel, &binding.to_string()),
			DispatchOutcome::BindingCalled
		);
		assert_eq!(
			*rows.lock().unwrap(),
			vec![(5u64, "2/30".to_string())],
			"등록된 컨텍스트의 행은 도착한다"
		);
	}

	#[test]
	fn dispatch_binding_from_unregistered_context_is_ignored() {
		let rows = Arc::new(Mutex::new(Vec::new()));
		let mut channel = channel_with_rows(rows.clone());
		// 컨텍스트를 등록하지 않는다
		let binding = serde_json::json!({
			"method": "Runtime.bindingCalled",
			"params": {
				"name": crate::property_watcher_listener::PROPERTY_WATCHER_BINDING_NAME,
				"payload": r#"{"version":3,"selectionId":5,"cells":["a","b","c","d","e","f","g","h"],"pager":"2/30"}"#,
				"executionContextId": 99
			}
		});
		assert_eq!(
			dispatch_cdp_message(&mut channel, &binding.to_string()),
			DispatchOutcome::BindingCalled
		);
		assert!(rows.lock().unwrap().is_empty(), "미등록 컨텍스트는 무시된다");
	}

	#[test]
	fn dispatch_other_methods_are_ignored() {
		let rows = Arc::new(Mutex::new(Vec::new()));
		let mut channel = channel_with_rows(rows.clone());
		let other = serde_json::json!({"method": "Page.loadEventFired", "params": {}});
		assert_eq!(
			dispatch_cdp_message(&mut channel, &other.to_string()),
			DispatchOutcome::Other
		);
		assert_eq!(dispatch_cdp_message(&mut channel, "not json"), DispatchOutcome::Other);
		assert_eq!(
			dispatch_cdp_message(&mut channel, r#"{"method":"Runtime.bindingCalled"}"#),
			DispatchOutcome::Other,
			"params가 없으면 무시한다"
		);
		// 문서 이동은 세대를 올리고 컨텍스트를 비운다
		let nav = serde_json::json!({
			"method": "Page.frameNavigated",
			"params": { "frame": { "id": "FRAME1", "url": "http://scpweb.softgraphy.biz/board?page=2" } }
		});
		assert_eq!(
			dispatch_cdp_message(&mut channel, &nav.to_string()),
			DispatchOutcome::FrameNavigated
		);
		assert_eq!(channel.state().context_count(), 0);
		assert!(rows.lock().unwrap().is_empty());
	}

	#[test]
	fn dispatch_close_marks_channel_closed() {
		let rows = Arc::new(Mutex::new(Vec::new()));
		let mut channel = channel_with_rows(rows.clone());
		assert_eq!(dispatch_channel_close(&mut channel), DispatchOutcome::Close);
		assert_eq!(
			channel.fatal().as_deref(),
			Some(super::SESSION_CLOSED_MESSAGE)
		);
		assert!(!channel.state().is_active());
	}
}
