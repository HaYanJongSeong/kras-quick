#[test]
fn startup_does_not_close_preexisting_viewer_tabs() {
	let source = include_str!("../src/main.rs");
	let startup = source.rsplit("println!(\"kras-quick v{version}\");").next().unwrap();
	assert!(!startup.contains("close_viewer_tabs("), "interactive startup must preserve preexisting viewer tabs");
	assert!(startup.contains("viewer.close_viewer()"), "captured viewer must still be closed by its owner");
}
