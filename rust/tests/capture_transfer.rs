use kras_quick::kras_auto_stages::{property_auto_stage_paths, run_kras_auto_stage3, KrasAutoStage2Result, KrasAutoStage3Result, KrasOzViewerSurface};
use kras_quick::kras_output::KrasBuildingOption;
use std::cell::RefCell;

struct Viewer {
    pages: RefCell<Option<Vec<Vec<u8>>>>,
    pdf: Vec<u8>,
    xml: String,
}

impl KrasOzViewerSurface for Viewer {
    fn url(&self) -> String { "https://kras.go.kr/oz80/ozhviewer/report.jsp".into() }
    fn has_visible_captcha(&self) -> bool { false }
    fn capture_pdf(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> { Ok(self.pdf.clone()) }
    fn capture_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        panic!("pipeline must transfer captured pages instead of requesting a copy")
    }
    fn take_captured_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
        self.pages.borrow_mut().take().ok_or_else(|| "pages already consumed".into())
    }
    fn read_report_xml(&self) -> Result<String, Box<dyn std::error::Error>> { Ok(self.xml.clone()) }
}

#[test]
fn pipeline_transfers_pngs_and_preserves_pdf_xml_and_full_json() {
    let address = "서울특별시 노원구 월계동 392-19";
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("kras-transfer-{}-{stamp}", std::process::id()));
    let paths = property_auto_stage_paths(address, Some(root.to_str().unwrap()));
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(16, 12, image::Rgb([1, 2, 3])));
    let mut png = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let pages = vec![png];
    let pdf = kras_quick::oz_capture::build_pdf_from_pngs(&pages).unwrap();
    let xml = format!("<REAL_LAND_BLDG><LAND_LOC_NM>{address}</LAND_LOC_NM><JIBN>392-19</JIBN><CODE id=\"001\">0002</CODE></REAL_LAND_BLDG>");
    let viewer = Viewer { pages: RefCell::new(Some(pages)), pdf: pdf.clone(), xml: xml.clone() };
    let stage2 = KrasAutoStage2Result {
        status: "completed".into(), address: address.into(),
        building: KrasBuildingOption { value: String::new(), label: String::new() }, floor_room: None,
    };
    let result = run_kras_auto_stage3(&stage2, &viewer, &paths).unwrap();
    match result {
        KrasAutoStage3Result::Completed { output, .. } => {
            assert_eq!(std::fs::read(&output.pdf).unwrap(), pdf);
            assert_eq!(std::fs::read_to_string(&output.page_xml).unwrap(), xml);
            let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&output.data_json).unwrap()).unwrap();
            assert_eq!(json, kras_quick::full_xml_json::convert(&viewer.xml).unwrap());
            assert!(output.page_pngs.is_empty());
        }
        KrasAutoStage3Result::Blocked { reason, .. } => panic!("unexpected blocked capture: {reason}"),
    }
    assert!(viewer.pages.borrow().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
