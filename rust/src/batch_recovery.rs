#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupFailure { Temporary, AccessBlocked, LoginExpired, Cancelled, Unknown }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recovery { RecordAndContinue, WaitForLogin, StopAndPreserve }

#[derive(Debug)]
pub struct SiteRequestFailure;

impl std::fmt::Display for SiteRequestFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("사이트 처리 오류가 감지되었습니다.")
    }
}

impl std::error::Error for SiteRequestFailure {}

pub fn classify_notice(text: &str) -> LookupFailure {
    let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.contains("차단") || compact.contains("접근제한") || compact.contains("접속제한") {
        return LookupFailure::AccessBlocked;
    }
    if compact.contains("로그인") && compact.contains("만료") { return LookupFailure::LoginExpired; }
    if compact.contains("오류가발생하였습니다.") && compact.contains("잠시후다시시도") {
        return LookupFailure::Temporary;
    }
    LookupFailure::Unknown
}

pub fn after_lookup_failure(failure: LookupFailure) -> Recovery {
    match failure {
        LookupFailure::Temporary => Recovery::RecordAndContinue,
        LookupFailure::LoginExpired => Recovery::WaitForLogin,
        LookupFailure::AccessBlocked | LookupFailure::Cancelled | LookupFailure::Unknown => Recovery::StopAndPreserve,
    }
}

pub fn wait_for_login(mut read: impl FnMut() -> Option<String>, mut ready: impl FnMut() -> bool) -> bool {
    while let Some(answer) = read() {
        if answer.trim().eq_ignore_ascii_case("q") { return false; }
        if answer.trim().is_empty() && ready() { return true; }
    }
    false
}

pub fn handle_lookup_notice(surface: &crate::cdp_surface::CdpSurface) -> Result<LookupFailure, Box<dyn std::error::Error>> {
    let result = surface.send_cdp("Runtime.evaluate", serde_json::json!({
        "expression": "(()=>{const d=document.querySelector('.swal2-container.swal2-backdrop-show');return {notice:d?.textContent??'',login:/\\/login(?:\\/|$)/i.test(location.pathname)};})()",
        "returnByValue": true
    }))?;
    let value = &result["result"]["value"];
    if value["login"].as_bool() == Some(true) { return Ok(LookupFailure::LoginExpired); }
    let notice = value["notice"].as_str().unwrap_or("");
    let failure = classify_notice(notice);
    if after_lookup_failure(failure) != Recovery::RecordAndContinue { return Ok(failure); }
    let expression = format!("(()=>{{const d=document.querySelector('.swal2-container.swal2-backdrop-show');if(!d||d.textContent!=={})return false;const b=d.querySelector('.swal2-confirm');if(!b||b.disabled)return false;b.click();return true;}})()", serde_json::to_string(notice)?);
    let result = surface.send_cdp("Runtime.evaluate", serde_json::json!({"expression":expression,"returnByValue":true}))?;
    Ok(if result["result"]["value"].as_bool() == Some(true) { LookupFailure::Temporary } else { LookupFailure::Unknown })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn temporary_notice_advances_without_retry() {
        let failure = classify_notice("오류가 발생하였습니다. 잠시 후 다시 시도하여 주시기 바랍니다.");
        assert_eq!(failure, LookupFailure::Temporary);
        assert_eq!(after_lookup_failure(failure), Recovery::RecordAndContinue);
    }
    #[test]
    fn repeated_temporary_failure_still_advances() {
        assert_eq!(after_lookup_failure(LookupFailure::Temporary), Recovery::RecordAndContinue);
    }
    #[test]
    fn safety_stops_never_retry_or_advance() {
        for failure in [LookupFailure::AccessBlocked, LookupFailure::Cancelled, LookupFailure::Unknown] {
            assert_eq!(after_lookup_failure(failure), Recovery::StopAndPreserve);
        }
    }
    #[test]
    fn blocked_notice_wins_over_temporary_error_words() {
        assert_eq!(classify_notice("IP 차단. 오류가 발생하였습니다. 잠시 후 다시 시도하세요."), LookupFailure::AccessBlocked);
    }
    #[test]
    fn evaluation_code_alone_does_not_authorize_retry() {
        assert_eq!(classify_notice("KRAS_EVALUATION_ERROR"), LookupFailure::Unknown);
    }
    #[test]
    fn whitespace_does_not_change_notice_kind() {
        assert_eq!(classify_notice("오류가\n발생하였습니다.\t잠시 후 다시\n시도하여 주시기 바랍니다."), LookupFailure::Temporary);
        assert_eq!(classify_notice("로그인이 만료되었습니다."), LookupFailure::LoginExpired);
    }
    #[test]
    fn expired_login_waits_instead_of_cancelling_or_skipping() {
        assert_eq!(after_lookup_failure(LookupFailure::LoginExpired), Recovery::WaitForLogin);
    }
    #[test]
    fn enter_before_login_keeps_waiting_until_ready() {
        let mut inputs = [String::new(), String::new()].into_iter();
        let mut checks = 0;
        let resumed = wait_for_login(|| inputs.next(), || { checks += 1; checks == 2 });
        assert!(resumed);
        assert_eq!(checks, 2);
    }
    #[test]
    fn eof_preserves_pause_without_checking_or_resuming() {
        assert!(!wait_for_login(|| None, || panic!("EOF must not check or resume")));
    }
    #[test]
    fn explicit_stop_does_not_resume() {
        assert!(!wait_for_login(|| Some("q".into()), || panic!("stop must not resume")));
    }
    #[test]
    fn non_enter_input_does_not_check_login() {
        let mut inputs = ["other".to_string(), String::new()].into_iter();
        let mut checks = 0;
        assert!(wait_for_login(|| inputs.next(), || { checks += 1; true }));
        assert_eq!(checks, 1);
    }
}
