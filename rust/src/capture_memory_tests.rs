use super::*;
use image::ImageEncoder;
use sha2::{Digest, Sha256};

fn png(width: u32, height: u32) -> Vec<u8> {
    png_fixture(width, height, "rgb")
}

fn png_fixture(width: u32, height: u32, fixture: &str) -> Vec<u8> {
    let mut state = 123456789u32;
    let channels = if fixture == "rgba" { 4 } else { 3 };
    let rgb: Vec<u8> = (0..width * height * channels).map(|offset| {
        if fixture == "document" {
            let pixel = offset / channels;
            let x = pixel % width;
            let y = pixel / width;
            return if x > 70 && x < width - 70 && y % 40 < 3 && x % 9 < 5 { 0 } else { 255 };
        }
        if channels == 4 && offset % channels == 3 { return 255; }
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state.to_le_bytes()[0]
    }).collect();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&rgb, width, height, if channels == 4 { image::ColorType::Rgba8.into() } else { image::ColorType::Rgb8.into() }).unwrap();
    bytes
}

fn cached(pages: Vec<Vec<u8>>) -> CdpSurface {
    CdpSurface {
        endpoint: String::new(), page_url: String::new(), ws_url: String::new(),
        page_cache: std::sync::Mutex::new(Some(pages)),
    }
}

#[test]
fn captured_pngs_transfer_the_original_buffers() {
    let page = png(8, 8);
    let pointer = page.as_ptr();
    let surface = cached(vec![page]);
    let pdf = surface.capture_pdf().unwrap();
    assert!(crate::kras_output::valid_pdf(&pdf));
    let pages = surface.take_captured_page_pngs().unwrap();
    assert_eq!(pages[0].as_ptr(), pointer);
    assert!(surface.page_cache.lock().unwrap().is_none());
}

#[test]
#[ignore = "합성 PNG 4페이지의 release 메모리·PDF 처리 측정; 실사이트 요청 없음"]
fn synthetic_capture_benchmark() {
    let fixture = std::env::var("KRAS_BENCH_FIXTURE").unwrap_or_else(|_| "rgb".into());
    assert!(["rgb", "rgba", "document"].contains(&fixture.as_str()));
    let page = png_fixture(1024, 1536, &fixture);
    let pages = vec![page.clone(), page.clone(), page.clone(), page];
    let input_bytes: usize = pages.iter().map(Vec::len).sum();
    let surface = cached(pages);
    let start = std::time::Instant::now();
    let pdf = surface.capture_pdf().unwrap();
    let pages = surface.take_captured_page_pngs().unwrap();
    let elapsed = start.elapsed().as_micros();
    assert_eq!(pages.len(), 4);
    assert!(crate::kras_output::valid_pdf(&pdf));
    println!("BENCH_CAPTURE_US={elapsed} FIXTURE={fixture} INPUT_PNG_BYTES={input_bytes} PDF_BYTES={} PDF_SHA256={:x}", pdf.len(), Sha256::digest(&pdf));
    std::hint::black_box((pdf, pages));
}

#[test]
fn pdf_bytes_match_test7_for_rgb_rgba_and_document_fixtures() {
    for (fixture, expected) in [
        ("rgb", "480907bfaf21a64f029148e591e5214d831501472dc7fa68f1bb67609c460e15"),
        ("rgba", "480907bfaf21a64f029148e591e5214d831501472dc7fa68f1bb67609c460e15"),
        ("document", "422f2ae4c4454f4068d400b1a72420984f226693d3cc7417d53933e77ae7b895"),
    ] {
        let page = png_fixture(1024, 1536, fixture);
        let pages = vec![page.clone(), page.clone(), page.clone(), page];
        let pdf = crate::oz_capture::build_pdf_from_pngs(&pages).unwrap();
        assert_eq!(format!("{:x}", Sha256::digest(&pdf)), expected, "{fixture}");
    }
}

#[test]
#[ignore = "합성 JSONL 20000행의 최신 기록 검색 측정; 실사이트 요청 없음"]
fn synthetic_latest_log_benchmark() {
    use std::io::Write;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("kras-latest-bench-{}-{stamp}.jsonl", std::process::id()));
    let mut file = std::io::BufWriter::new(std::fs::OpenOptions::new().create_new(true).write(true).open(&path).unwrap());
    let line = serde_json::to_vec(&serde_json::json!({"kind":"stage1","data":{"address":"synthetic","padding":"x".repeat(1000)}})).unwrap();
    for _ in 0..20000 { file.write_all(&line).unwrap(); file.write_all(b"\n").unwrap(); }
    file.flush().unwrap();
    drop(file);
    let name = path.to_str().unwrap();
    let mut timings = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let result = crate::output_log::latest(name, "stage1").unwrap();
        timings.push(start.elapsed().as_micros());
        assert_eq!(result["address"], "synthetic");
    }
    timings.sort_unstable();
    println!("BENCH_LATEST_MEDIAN_US={} LOG_BYTES={}", timings[2], std::fs::metadata(&path).unwrap().len());
    std::fs::remove_file(path).unwrap();
}
