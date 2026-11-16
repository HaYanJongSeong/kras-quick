// TS scripts/property-watcher-cdp.ts — 격리월드 평가 결과 파싱(타입 있는 프로토콜 오류).

use serde_json::{json, Value};

use crate::property_watcher_listener::{
	cleanup_source, listener_source, PROPERTY_WATCHER_BINDING_NAME, PROPERTY_WATCHER_WORLD_NAME,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyWatcherProtocolErrorCode {
	StatusEvaluationException,
	StatusEvaluationResultInvalid,
}

impl PropertyWatcherProtocolErrorCode {
	fn message(self) -> &'static str {
		match self {
			Self::StatusEvaluationException => {
				"SCP building status evaluation failed."
			}
			Self::StatusEvaluationResultInvalid => {
				"SCP building status evaluation returned an invalid result."
			}
		}
	}
}

/// 상세 예외 내용은 메시지에 담지 않는다(비공개 페이지 정보 유출 방지).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherProtocolError {
	pub code: PropertyWatcherProtocolErrorCode,
	message: String,
}

impl PropertyWatcherProtocolError {
	fn new(code: PropertyWatcherProtocolErrorCode) -> Self {
		Self {
			code,
			message: code.message().to_string(),
		}
	}
	pub fn name(&self) -> &'static str {
		"PropertyWatcherProtocolError"
	}
}

impl std::fmt::Display for PropertyWatcherProtocolError {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.write_str(&self.message)
	}
}

impl std::error::Error for PropertyWatcherProtocolError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyBuildingStatusUpdateResult {
	Applied,
	Stale,
}

impl PropertyBuildingStatusUpdateResult {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Applied => "applied",
			Self::Stale => "stale",
		}
	}
}

pub fn parse_property_building_status_evaluation(
	exception_details: Option<&Value>,
	value: Option<&Value>,
) -> Result<PropertyBuildingStatusUpdateResult, PropertyWatcherProtocolError> {
	if exception_details.is_some() {
		return Err(PropertyWatcherProtocolError::new(
			PropertyWatcherProtocolErrorCode::StatusEvaluationException,
		));
	}
	match value.and_then(Value::as_str) {
		Some("applied") => Ok(PropertyBuildingStatusUpdateResult::Applied),
		Some("stale") => Ok(PropertyBuildingStatusUpdateResult::Stale),
		_ => Err(PropertyWatcherProtocolError::new(
			PropertyWatcherProtocolErrorCode::StatusEvaluationResultInvalid,
		)),
	}
}

// ——— CDP 명령 계층 ———
// 전송(소켓) 없이 명령값만 만든다. 허용 명령 8개 외에는 이 모듈이 만들지 않는다.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyBuildingTerminalStatus {
	Absent,
	Failed,
	Present,
}

impl PropertyBuildingTerminalStatus {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Absent => "absent",
			Self::Failed => "failed",
			Self::Present => "present",
		}
	}
}

/// CDP 명령 한 건 — 이후 전송 계층이 method·params만 그대로 보내면 된다.
#[derive(Debug, Clone, PartialEq)]
pub struct CdpCommand {
	pub method: String,
	pub params: Value,
}

fn send(method: &str, params: Value) -> CdpCommand {
	CdpCommand {
		method: method.to_string(),
		params,
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherFrameEvent {
	pub frame_id: String,
	pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherBindingEvent {
	pub context_id: i64,
	pub name: String,
	pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyWatcherContextEvent {
	pub context_id: i64,
	pub frame_id: Option<String>,
	pub name: String,
	pub origin: String,
	pub r#type: Option<String>,
	pub unique_id: String,
}

/// 프로토콜 명령 생성기(TS createPropertyWatcherProtocol의 명령 부분).
pub struct PropertyWatcherProtocol;

impl PropertyWatcherProtocol {
	pub fn page_enable() -> CdpCommand {
		send("Page.enable", json!({}))
	}

	pub fn runtime_enable() -> CdpCommand {
		send("Runtime.enable", json!({}))
	}

	pub fn get_root_frame() -> CdpCommand {
		send("Page.getFrameTree", json!({}))
	}

	pub fn add_binding() -> CdpCommand {
		send(
			"Runtime.addBinding",
			json!({
				"name": PROPERTY_WATCHER_BINDING_NAME,
				"executionContextName": PROPERTY_WATCHER_WORLD_NAME,
			}),
		)
	}

	pub fn add_listener_script() -> CdpCommand {
		send(
			"Page.addScriptToEvaluateOnNewDocument",
			json!({
				"source": listener_source(),
				"worldName": PROPERTY_WATCHER_WORLD_NAME,
				"runImmediately": true,
			}),
		)
	}

	pub fn evaluate_cleanup(context_id: i64) -> CdpCommand {
		send(
			"Runtime.evaluate",
			json!({
				"expression": cleanup_source(),
				"contextId": context_id,
				"returnByValue": true,
			}),
		)
	}

	pub fn evaluate_building_status(
		context_id: i64,
		selection_id: i64,
		status: PropertyBuildingTerminalStatus,
	) -> CdpCommand {
		let expression = format!(
			"globalThis.__opencodePropertyWatcherListenerV3.updateBuildingStatus({}, {})",
			json!(selection_id),
			json!(status.as_str())
		);
		send(
			"Runtime.evaluate",
			json!({
				"expression": expression,
				"contextId": context_id,
				"returnByValue": true,
			}),
		)
	}

	pub fn remove_listener_script(identifier: &str) -> CdpCommand {
		send(
			"Page.removeScriptToEvaluateOnNewDocument",
			json!({ "identifier": identifier }),
		)
	}

	pub fn remove_binding() -> CdpCommand {
		send(
			"Runtime.removeBinding",
			json!({ "name": PROPERTY_WATCHER_BINDING_NAME }),
		)
	}
}

// 응답·이벤트 매핑 — 전송 계층이 JSON을 주면 값을 돌려준다.

pub fn parse_root_frame(result: &Value) -> Option<PropertyWatcherFrameEvent> {
	let frame = result.get("frameTree")?.get("frame")?;
	Some(PropertyWatcherFrameEvent {
		frame_id: frame.get("id")?.as_str()?.to_string(),
		url: frame.get("url")?.as_str()?.to_string(),
	})
}

pub fn parse_script_identifier(result: &Value) -> Option<String> {
	result.get("identifier")?.as_str().map(str::to_string)
}

pub fn parse_binding_event(params: &Value) -> Option<PropertyWatcherBindingEvent> {
	Some(PropertyWatcherBindingEvent {
		context_id: params.get("executionContextId")?.as_i64()?,
		name: params.get("name")?.as_str()?.to_string(),
		payload: params.get("payload")?.as_str()?.to_string(),
	})
}

pub fn parse_context_created(params: &Value) -> Option<PropertyWatcherContextEvent> {
	let context = params.get("context")?;
	let aux = context.get("auxData");
	let aux_field = |key: &str| {
		aux.and_then(|value| value.get(key))
			.and_then(Value::as_str)
			.map(str::to_string)
	};
	Some(PropertyWatcherContextEvent {
		context_id: context.get("id")?.as_i64()?,
		frame_id: aux_field("frameId"),
		name: context
			.get("name")
			.and_then(Value::as_str)
			.unwrap_or_default()
			.to_string(),
		origin: context
			.get("origin")
			.and_then(Value::as_str)
			.unwrap_or_default()
			.to_string(),
		r#type: aux_field("type"),
		unique_id: context
			.get("uniqueId")
			.and_then(Value::as_str)
			.unwrap_or_default()
			.to_string(),
	})
}

pub fn parse_frame_navigated(params: &Value) -> Option<PropertyWatcherFrameEvent> {
	let frame = params.get("frame")?;
	Some(PropertyWatcherFrameEvent {
		frame_id: frame.get("id")?.as_str()?.to_string(),
		url: frame.get("url")?.as_str()?.to_string(),
	})
}

pub fn parse_navigated_within_document(
	params: &Value,
) -> Option<PropertyWatcherFrameEvent> {
	Some(PropertyWatcherFrameEvent {
		frame_id: params.get("frameId")?.as_str()?.to_string(),
		url: params.get("url")?.as_str()?.to_string(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn accepts_isolated_evaluator_results_unchanged() {
		assert_eq!(
			parse_property_building_status_evaluation(None, Some(&json!("applied"))),
			Ok(PropertyBuildingStatusUpdateResult::Applied)
		);
		assert_eq!(
			parse_property_building_status_evaluation(None, Some(&json!("stale"))),
			Ok(PropertyBuildingStatusUpdateResult::Stale)
		);
	}

	#[test]
	fn rejects_exception_details_with_typed_error_without_leaking_details() {
		let error = parse_property_building_status_evaluation(
			Some(&json!({ "text": "private exception" })),
			None,
		)
		.unwrap_err();
		assert_eq!(
			error.code,
			PropertyWatcherProtocolErrorCode::StatusEvaluationException
		);
		assert!(!error.to_string().contains("private exception"));
	}

	#[test]
	fn rejects_unexpected_value_with_typed_error() {
		let error =
			parse_property_building_status_evaluation(None, Some(&json!("unexpected")))
				.unwrap_err();
		assert_eq!(
			error.code,
			PropertyWatcherProtocolErrorCode::StatusEvaluationResultInvalid
		);
	}

	#[test]
	fn builds_isolated_world_commands_with_required_options() {
		// 격리월드 바인딩: 이름과 executionContextName이 함께 간다.
		let binding = PropertyWatcherProtocol::add_binding();
		assert_eq!(binding.method, "Runtime.addBinding");
		assert_eq!(binding.params["name"], json!(PROPERTY_WATCHER_BINDING_NAME));
		assert_eq!(
			binding.params["executionContextName"],
			json!(PROPERTY_WATCHER_WORLD_NAME)
		);

		// 리스너 주입: worldName + runImmediately: true가 필수다.
		let script = PropertyWatcherProtocol::add_listener_script();
		assert_eq!(script.method, "Page.addScriptToEvaluateOnNewDocument");
		assert_eq!(script.params["worldName"], json!(PROPERTY_WATCHER_WORLD_NAME));
		assert_eq!(script.params["runImmediately"], json!(true));
		assert_eq!(script.params["source"], json!(listener_source()));

		let cleanup = PropertyWatcherProtocol::evaluate_cleanup(7);
		assert_eq!(cleanup.method, "Runtime.evaluate");
		assert_eq!(cleanup.params["contextId"], json!(7));
		assert_eq!(cleanup.params["returnByValue"], json!(true));
		assert_eq!(cleanup.params["expression"], json!(cleanup_source()));

		// 상태 평가 식은 TS 템플릿과 동일한 형태여야 한다.
		let status = PropertyWatcherProtocol::evaluate_building_status(
			7,
			3,
			PropertyBuildingTerminalStatus::Present,
		);
		assert_eq!(
			status.params["expression"],
			json!("globalThis.__opencodePropertyWatcherListenerV3.updateBuildingStatus(3, \"present\")")
		);
		assert_eq!(status.params["contextId"], json!(7));
		assert_eq!(status.params["returnByValue"], json!(true));

		let remove = PropertyWatcherProtocol::remove_listener_script("id-1");
		assert_eq!(remove.method, "Page.removeScriptToEvaluateOnNewDocument");
		assert_eq!(remove.params["identifier"], json!("id-1"));

		let unbind = PropertyWatcherProtocol::remove_binding();
		assert_eq!(unbind.method, "Runtime.removeBinding");
		assert_eq!(unbind.params["name"], json!(PROPERTY_WATCHER_BINDING_NAME));

		assert_eq!(PropertyWatcherProtocol::page_enable().method, "Page.enable");
		assert_eq!(PropertyWatcherProtocol::runtime_enable().method, "Runtime.enable");
		assert_eq!(
			PropertyWatcherProtocol::get_root_frame().method,
			"Page.getFrameTree"
		);
	}

	#[test]
	fn parses_frame_binding_and_context_events() {
		let root = parse_root_frame(&json!({
			"frameTree": { "frame": { "id": "F1", "url": "http://scpweb.softgraphy.biz/board" } }
		}))
		.unwrap();
		assert_eq!(root.frame_id, "F1");
		assert_eq!(root.url, "http://scpweb.softgraphy.biz/board");

		let binding = parse_binding_event(&json!({
			"executionContextId": 5, "name": "n", "payload": "{}"
		}))
		.unwrap();
		assert_eq!(binding.context_id, 5);
		assert_eq!(binding.name, "n");
		assert_eq!(binding.payload, "{}");

		let context = parse_context_created(&json!({
			"context": {
				"id": 9,
				"name": "opencode-property-watcher-v1",
				"origin": "http://scpweb.softgraphy.biz",
				"uniqueId": "u-1",
				"auxData": { "frameId": "F1", "type": "default" }
			}
		}))
		.unwrap();
		assert_eq!(context.context_id, 9);
		assert_eq!(context.frame_id.as_deref(), Some("F1"));
		assert_eq!(context.r#type.as_deref(), Some("default"));
		assert_eq!(context.unique_id, "u-1");

		// auxData 없는 컨텍스트도 이벤트로 남는다(TS ?? {}와 동일).
		let bare = parse_context_created(&json!({
			"context": { "id": 2, "name": "", "origin": "", "uniqueId": "" }
		}))
		.unwrap();
		assert_eq!(bare.frame_id, None);
		assert_eq!(bare.r#type, None);

		let navigated =
			parse_frame_navigated(&json!({ "frame": { "id": "F2", "url": "u" } })).unwrap();
		assert_eq!(navigated.frame_id, "F2");
		assert_eq!(navigated.url, "u");

		let within =
			parse_navigated_within_document(&json!({ "frameId": "F3", "url": "v" })).unwrap();
		assert_eq!(within.frame_id, "F3");
		assert_eq!(within.url, "v");

		let identifier = parse_script_identifier(&json!({ "identifier": "script-1" })).unwrap();
		assert_eq!(identifier, "script-1");
		assert!(parse_script_identifier(&json!({})).is_none());
		assert!(parse_root_frame(&json!({})).is_none());
	}
}
