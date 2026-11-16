use kras_quick::cdp_surface::CdpSurface;
use kras_quick::property_watcher_runtime::CdpTargetPage;
use kras_quick::stage12::KrasPage;

#[test]
fn kras_page_url_returns_target_without_recursive_trait_dispatch() {
    let url = "https://www.kras.go.kr/kras/cert/certView.do";
    let target = CdpTargetPage {
        id: "offline".to_string(),
        url: url.to_string(),
        ws_url: "ws://127.0.0.1:1/offline".to_string(),
    };
    let surface = CdpSurface::from_page("http://127.0.0.1:1", &target);
    let page: &dyn KrasPage = &surface;
    assert_eq!(page.url(), url);
}
