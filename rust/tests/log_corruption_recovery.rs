use kras_quick::output_log::latest;

#[test]
fn latest_record_survives_an_unrelated_invalid_utf8_line() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "kras-corrupt-log-{}-{stamp}.jsonl",
        std::process::id()
    ));
    let mut bytes = b"{\"kind\":\"stage1\",\"data\":{\"code\":\"001\"}}\n".to_vec();
    bytes.extend_from_slice(b"{\"kind\":\"stage2\",\"data\":\"\xff\"}\n");
    std::fs::write(&path, bytes).unwrap();

    let result = latest(path.to_str().unwrap(), "stage1");

    std::fs::remove_file(path).unwrap();
    assert_eq!(result, Some(serde_json::json!({"code":"001"})));
}
