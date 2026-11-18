use super::*;

#[test]
fn auto_floor_selection_follows_value_after_option_order_changes() {
    let selected = AutoBuilding { index: 1, value: "room-a".into(), label: "101호".into() };
    let building = AutoBuilding { index: 1, value: "building".into(), label: "A동 (집합)".into() };
    let mode = RunMode::Auto { building: Some(building.clone()), floor: Some(selected.clone()) };
    let mut discovered = Vec::new();
    let mut hooks = ModeHooks { mode: &mode, discovered_floors: &mut discovered, address: "" };
    let options = [AutoBuilding { index: 1, value: "room-b".into(), label: "102호".into() },
        AutoBuilding { index: 2, ..selected }];
    assert_eq!(hooks.choose_floor_room(&options, &building), FloorChoice::Index(2));
}

#[test]
fn auto_floor_selection_rejects_disappeared_room() {
    let building = AutoBuilding { index: 1, value: "building".into(), label: "A동 (집합)".into() };
    let mode = RunMode::Auto { building: Some(building.clone()), floor: Some(AutoBuilding { index: 1, value: "missing".into(), label: "101호".into() }) };
    let mut discovered = Vec::new();
    let mut hooks = ModeHooks { mode: &mode, discovered_floors: &mut discovered, address: "" };
    assert_eq!(hooks.choose_floor_room(&[AutoBuilding { index: 1, value: "room-b".into(), label: "102호".into() }], &building), FloorChoice::Reject);
}

#[test]
fn invalid_pnus_remain_in_memory_and_checkpoint_after_batch_ends() {
    if std::env::var_os("KRAS_BATCH_AUDIT_CHILD").is_some() {
        let original = vec!["9999999999100010000".to_string(), "9999999999100020000".to_string()];
        let mut queue = original.clone();
        run_auto_batch(&mut queue, AutoBatchScope::LandOnly);
        assert_eq!(queue, original);
        assert_eq!(kras_quick::batch_checkpoint::load(&auto_checkpoint_path().unwrap()).unwrap(), original);
        return;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("kras-batch-audit-{}-{stamp}", std::process::id()));
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "batch_audit_tests::invalid_pnus_remain_in_memory_and_checkpoint_after_batch_ends", "--nocapture"])
        .env("KRAS_BATCH_AUDIT_CHILD", "1").env("KRAS_QUICK_ROOT", &root).output().unwrap();
    std::fs::remove_dir_all(root).unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
}
