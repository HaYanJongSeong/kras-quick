use crate::quick_address::{append_auto_addresses, parse_auto_batch_scope_answer, AutoBatchScope};

pub fn choose_scope(mut read: impl FnMut() -> Option<String>) -> Option<AutoBatchScope> {
    while let Some(answer) = read() {
        if let Some(scope) = parse_auto_batch_scope_answer(&answer) { return Some(scope); }
        println!("1~5 또는 Enter를 입력해 주세요.");
    }
    None
}

pub fn collect(queue: &mut Vec<String>, mut read: impl FnMut(bool) -> Option<String>) -> bool {
    while let Some(input) = read(false) {
        match input.as_str() {
            "시작" => return true,
            "" | "..." => continue,
            _ => {
                let added = append_auto_addresses(queue, &input);
                if added > 0 { println!("AUTO 대기 목록에 {added}건 추가됨 (총 {}건).", queue.len()); }
            }
        }
    }
    false
}

pub fn load_resume(auto_enabled: bool, path: &std::path::Path) -> std::io::Result<Option<Vec<String>>> {
    if !auto_enabled { return Ok(None); }
    crate::batch_checkpoint::load(path).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_queue_keeps_input_prompt_visible() {
        let mut queue = vec!["1114014200100080019".to_string()];
        let original = queue.clone();
        assert!(collect(&mut queue, |suppress| {
            assert!(!suppress, "restored addresses must not hide the batch input prompt");
            Some("시작".to_string())
        }));
        assert_eq!(queue, original);
    }

    #[test]
    fn resume_file_is_not_read_outside_auto_mode() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("kras-resume-gate-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("resume.json");
        std::fs::write(&path, b"damaged").unwrap();
        assert_eq!(load_resume(false, &path).unwrap(), None);
        assert_eq!(load_resume(true, &path).unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_auto_resume_restores_saved_order() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("kras-resume-order-{}-{stamp}", std::process::id()));
        let path = root.join("resume.json");
        let queue = vec!["1114014200100080019".to_string(), "1114014200100080033".to_string()];
        crate::batch_checkpoint::save(&path, &queue).unwrap();
        assert_eq!(load_resume(true, &path).unwrap(), Some(queue));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn eof_keeps_pending_addresses_without_starting() {
        let mut queue = vec!["서울특별시 노원구 월계동 392-19".to_string()];
        let original = queue.clone();
        let started = collect(&mut queue, |_| None);
        assert!(!started);
        assert_eq!(queue, original);
    }

    #[test]
    fn scope_eof_cancels_instead_of_selecting_full() {
        assert!(choose_scope(|| None).is_none());
        let mut answers = [Some("잘못된 선택".to_string()), None].into_iter();
        assert!(choose_scope(|| answers.next().flatten()).is_none());
    }

    #[test]
    fn explicit_start_keeps_queue_order() {
        let mut queue = vec!["first".to_string(), "second".to_string()];
        let original = queue.clone();
        assert!(collect(&mut queue, |_| Some("시작".to_string())));
        assert_eq!(queue, original);
    }

    #[test]
    fn enter_still_selects_default_scope() {
        assert!(matches!(choose_scope(|| Some(String::new())), Some(AutoBatchScope::Full)));
    }
}
