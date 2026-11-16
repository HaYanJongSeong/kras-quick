use crate::quick_address::{append_auto_addresses, parse_auto_batch_scope_answer, AutoBatchScope};

pub fn choose_scope(mut read: impl FnMut() -> Option<String>) -> Option<AutoBatchScope> {
    while let Some(answer) = read() {
        if let Some(scope) = parse_auto_batch_scope_answer(&answer) { return Some(scope); }
        println!("1, 2 또는 3을 입력해 주세요.");
    }
    None
}

pub fn collect(queue: &mut Vec<String>, mut read: impl FnMut(bool) -> Option<String>) -> bool {
    while let Some(input) = read(!queue.is_empty()) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
