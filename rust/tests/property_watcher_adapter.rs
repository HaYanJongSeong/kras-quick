// TS tests/property-watcher-cdp.test.ts 계약: 허용 명령 목록 스캔·격리월드 옵션·금지 패턴.
// 소스 스캔은 통합 테스트에서 수행한다(테스트 문자열이 스캔 대상 src에 섞이지 않게).

use std::collections::BTreeSet;

use kras_quick::property_watcher_cdp::PropertyWatcherProtocol;

const ALLOWED_COMMANDS: &[&str] = &[
	"Page.addScriptToEvaluateOnNewDocument",
	"Page.enable",
	"Page.getFrameTree",
	"Page.removeScriptToEvaluateOnNewDocument",
	"Runtime.addBinding",
	"Runtime.enable",
	"Runtime.evaluate",
	"Runtime.removeBinding",
];

fn read_source(relative: &str) -> String {
	std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
		.unwrap()
}

#[test]
fn uses_only_the_fixed_command_allowlist_and_required_isolated_world_options() {
	let source = read_source("src/property_watcher_cdp.rs");
	// fn send("...") 호출부에서 명령 이름만 뽑아 허용 목록과 대조한다.
	let pattern = regex::Regex::new(r#"send\(\s*"([^"]+)""#).unwrap();
	let commands: BTreeSet<String> = pattern
		.captures_iter(&source)
		.map(|captures| captures[1].to_string())
		.collect();
	let allowed: BTreeSet<String> = ALLOWED_COMMANDS
		.iter()
		.map(|text| text.to_string())
		.collect();
	assert_eq!(commands, allowed, "commands={commands:?}");

	// 격리월드 필수 옵션은 소스에 그대로 남는다(TS 계약 동일).
	assert!(source.contains("\"executionContextName\": PROPERTY_WATCHER_WORLD_NAME"));
	assert!(source.contains("\"worldName\": PROPERTY_WATCHER_WORLD_NAME"));
	assert!(source.contains("\"runImmediately\": true"));

	// 그리고 실제 명령값에도 들어간다.
	let binding = PropertyWatcherProtocol::add_binding();
	assert_eq!(
		binding.params["executionContextName"],
		serde_json::json!("opencode-property-watcher-v1")
	);
	let script = PropertyWatcherProtocol::add_listener_script();
	assert_eq!(
		script.params["worldName"],
		serde_json::json!("opencode-property-watcher-v1")
	);
	assert_eq!(script.params["runImmediately"], serde_json::json!(true));
}

#[test]
fn contains_no_page_binding_main_world_init_script_generic_transport_or_lifecycle_action() {
	let joined = [
		"src/property_watcher_cdp.rs",
		"src/property_watcher_listener.rs",
	]
	.into_iter()
	.map(read_source)
	.collect::<Vec<_>>()
	.join("\n");
	for forbidden in [
		".addInitScript(",
		".bringToFront(",
		".click(",
		".close(",
		".detach(",
		".disconnect(",
		".exposeBinding(",
		".focus(",
		".goto(",
		".newPage(",
		".reload(",
	] {
		assert!(!joined.contains(forbidden), "{forbidden}");
	}
}
