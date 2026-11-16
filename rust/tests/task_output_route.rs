#[test]
fn status_output_without_screen_preserves_multiline_details() {
    kras_quick::output::install_buffer();
    let details = "Stage 3: PDF 캡처 완료\n=== XML 추출값 ===\n본번 00017\n";
    kras_quick::task_screen::task_status(details);
    let captured = kras_quick::output::take_buffer();
    kras_quick::output::restore_real();
    assert_eq!(captured, details);
}
