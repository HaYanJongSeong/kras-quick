// TS scripts/kras-oz.ts — OZ 뷰어 캡처 파이프라인(줌·페이지 이동·캔버스 안정 캡처·PDF 합성)과
// readReportXml(전역변수→신형→구형→DOM 순 XML 추출).

use std::io::Write as _;

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::cdp_surface::CdpSurface;

// TS OZ_CAPTURE_TIMEOUT_MS(30초)에 맞춘 캔버스 준비 여유 시간.
// 냉(report 생성 직후) 렌더가 15초를 넘기는 경우가 있어 15초 고정은 제거했다(최적화).
const ZOOM_DEADLINE_MS: u64 = 30_000;
const STABLE_ATTEMPTS: u32 = 40;
const STABLE_DELAY_MS: u64 = 300;
const PAGE_WAIT_MS: u64 = 15_000;

/// TS setOZZoomTo300 — HTML5 뷰어 툴바 select를 300%로 바꾼다.
const SET_ZOOM_JS: &str = r#"() => {
	const select = document.querySelector('select.oz_ui_layout');
	if (select === null) return false;
	select.value = '300%';
	select.dispatchEvent(new Event('change', { bubbles: true }));
	return true;
}"#;

/// 캔버스가 실제로 그려졌는지(폭·높이 > 0).
const CANVAS_READY_JS: &str = r#"() => {
	const canvas = document.querySelector('canvas');
	if (canvas === null) return null;
	return { w: canvas.width, h: canvas.height };
}"#;

/// TS readOZTotalPages — 본문의 "현재 / 전체"에서 전체 페이지 수를 읽는다.
const TOTAL_PAGES_JS: &str = r#"() => {
	const match = (document.body.innerText || '').match(/(\d+)\s*\/\s*(\d+)/);
	return match ? match[2] : '0';
}"#;

/// 현재 페이지 번호 읽기(TS resetOZToFirstPage의 비교값).
const CURRENT_PAGE_JS: &str = r#"() => document.querySelector("input[title='현재 페이지'], input[title='Current page']")?.value ?? ''"#;

const PREV_PAGE_JS: &str = r#"() => {
	const el = document.querySelector("input[title='한 페이지 전으로 이동'], input[title='Move to the previous page']");
	if (el) { el.click(); return true; }
	return false;
}"#;

const NEXT_PAGE_JS: &str = r#"() => {
	const el = document.querySelector("input[title='한 페이지 다음으로 이동'], input[title='Move to the next page']");
	if (el) { el.click(); return true; }
	return false;
}"#;

/// TS captureStableCanvas — canvas.toBlob으로 PNG를 얻어 두 번 연속 같아질 때까지 재시도.
const CAPTURE_CANVAS_JS: &str = r#"async () => {
	const canvas = document.querySelector('canvas');
	if (canvas === null) return null;
	const blob = await new Promise((resolve) => canvas.toBlob((b) => resolve(b ?? null), 'image/png'));
	if (blob === null) return null;
	const bytes = new Uint8Array(await blob.arrayBuffer());
	let bin = '';
	const chunk = 0x8000;
	for (let i = 0; i < bytes.length; i += chunk) {
		bin += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
	}
	return btoa(bin);
}"#;

/// TS readReportXml — 전역변수 → 신형 strOZDataXML → 구형 ozdata= → DOM 후보 순.
const READ_REPORT_XML_JS: &str = r#"() => {
	try {
		const win = window;
		const candidates = [win.ozData, win.OZDATA, win._ozData, win.__ozData, win.ozdata, win.OZ_DATA, win._OZ_DATA, win.reportData, win.REPORT_DATA];
		for (const val of candidates) {
			if (typeof val === 'string' && val.trim().startsWith('<')) return val;
		}
	} catch (e) {}
	const content = document.documentElement.outerHTML;
	const ozVar = content.match(/var\s+strOZDataXML\s*=\s*"((?:[^"\\]|\\.)*)/);
	if (ozVar && ozVar[1]) {
		const raw = ozVar[1].trim();
		if (raw.startsWith('<')) return raw.replace(/\\"/g, '"').replace(/\\\\/g, '\\');
	}
	const legacy = content.match(/ozdata=(<[\s\S]*?)(?=<\/REAL_LAND_BLDG>)/i);
	if (legacy && legacy[1]) return "<?xml version='1.0' encoding='euc-kr'?>" + legacy[1] + '</REAL_LAND_BLDG>';
	const nodes = document.querySelectorAll("pre, textarea, [id*='xml' i], [class*='xml' i]");
	for (const node of nodes) {
		const text = (node.textContent || '').trim();
		if (text.startsWith('<')) return text;
	}
	return '';
}"#;

fn eval_js(surface: &CdpSurface, expression: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
	// CDP Runtime.evaluate는 소스를 "표현식"으로 평가한다.
	// `() => {}`를 그대로 보내면 함수 객체(type: "function")가 돌려져 한 줄도 실행되지 않으므로
	// (expr)() 로 감싸 호출한다(TS page.evaluate(fn)와 동일한 동작).
	let expression = format!("({expression})()");
	let params = serde_json::json!({
		"expression": expression,
		"returnByValue": true,
		"awaitPromise": true
	});
	let result = surface.send_cdp("Runtime.evaluate", params)?;
	if let Some(err) = result.get("exceptionDetails") {
		let desc = err
			.get("exception")
			.and_then(|e| e.get("description").or(e.get("text")))
			.and_then(|v| v.as_str())
			.unwrap_or("evaluate failed");
		return Err(format!("OZ evaluation failed: {desc}").into());
	}
	Ok(result.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

fn eval_str(surface: &CdpSurface, expression: &str) -> Option<String> {
	eval_js(surface, expression)
		.ok()?
		.get("value")?
		.as_str()
		.map(|s| s.to_string())
}

fn sleep_ms(ms: u64) {
	std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// 300%로 줌을 바꾼 뒤 캔버스가 그려질 때까지 기다린다.
pub fn set_oz_zoom_to_300(surface: &CdpSurface) -> Result<(), Box<dyn std::error::Error>> {
	let zoom_set = eval_js(surface, SET_ZOOM_JS)?;
	let started = std::time::Instant::now();
	let deadline = started + std::time::Duration::from_millis(ZOOM_DEADLINE_MS);
	let mut last_report = std::time::Instant::now();
	loop {
		let ready = eval_js(surface, CANVAS_READY_JS)
			.ok()
			.and_then(|value| value.get("value").cloned());
		let dims = ready
			.as_ref()
			.and_then(|value| {
				Some(format!(
					"w={},h={}",
					value.get("w")?.as_u64()?,
					value.get("h")?.as_u64()?
				))
			})
			.unwrap_or_else(|| "없음".to_string());
		if ready
			.as_ref()
			.and_then(|value| value.get("w").and_then(|w| w.as_u64()))
			.map(|w| w > 0)
			.unwrap_or(false)
			&& ready
				.as_ref()
				.and_then(|value| value.get("h").and_then(|h| h.as_u64()))
				.map(|h| h > 0)
				.unwrap_or(false)
		{
			crate::task_screen::task_status(&format!(
				"[OZ 캔버스] 준비 완료 경과 {:.1}초 ({dims})\n",
				started.elapsed().as_secs_f32()
			));
			return Ok(());
		}
		// 5초마다 진행 상태를 남긴다(냉 렌더 시간 측정용)
		if last_report.elapsed() >= std::time::Duration::from_secs(5) {
			crate::task_screen::task_status(&format!(
				"[OZ 캔버스] 대기 중 경과 {:.1}초 canvas={dims} zoomSet={zoom_set}\n",
				started.elapsed().as_secs_f32()
			));
			last_report = std::time::Instant::now();
		}
		if std::time::Instant::now() >= deadline {
			return Err(format!(
				"OZ 캔버스가 그려지지 않았습니다 ({}초 초과, canvas={dims})",
				ZOOM_DEADLINE_MS / 1000
			)
			.into());
		}
		sleep_ms(250);
	}
}

/// TS readOZTotalPages — 총 페이지 수.
pub fn read_oz_total_pages(surface: &CdpSurface) -> Result<u32, Box<dyn std::error::Error>> {
	let raw = eval_str(surface, TOTAL_PAGES_JS).unwrap_or_else(|| "0".to_string());
	let pages: u32 = raw.trim().parse().unwrap_or(0);
	if pages == 0 {
		return Err("OZ 뷰어 총 페이지 수를 읽지 못했습니다.".into());
	}
	Ok(pages)
}

/// TS resetOZToFirstPage — 1페이지가 아닐 때까지 이전 버튼을 누른다.
pub fn reset_oz_to_first_page(surface: &CdpSurface) -> Result<(), Box<dyn std::error::Error>> {
	for _ in 0..50 {
		let current = eval_str(surface, CURRENT_PAGE_JS).unwrap_or_default();
		if current == "1" || current.is_empty() {
			return Ok(());
		}
		eval_js(surface, PREV_PAGE_JS)?;
		sleep_ms(500);
	}
	Ok(())
}

/// TS captureStableCanvas — 연속 두 번 캡처가 같아질 때까지 최대 40회.
pub fn capture_stable_canvas(surface: &CdpSurface) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
	capture_changed_stable_canvas(surface, None)
}

fn capture_changed_stable_canvas(surface: &CdpSurface, prior_page: Option<&[u8]>) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
	let mut previous_fp = String::new();
	for attempt in 0..STABLE_ATTEMPTS {
		let current = eval_str(surface, CAPTURE_CANVAS_JS)
			.and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok());
		if let Some(bytes) = current {
			if prior_page == Some(bytes.as_slice()) {
				previous_fp.clear();
			} else {
			let mut hasher = Sha256::new();
			hasher.update(&bytes);
			let fp = format!("{:x}", hasher.finalize());
			if fp == previous_fp {
				return Ok(Some(bytes));
			}
			previous_fp = fp;
			}
		}
		if attempt + 1 < STABLE_ATTEMPTS {
			sleep_ms(STABLE_DELAY_MS);
		}
	}
	Ok(None)
}

fn click_and_wait(surface: &CdpSurface, expression: &str, target_page: u32) -> Result<(), Box<dyn std::error::Error>> {
	eval_js(surface, expression)?;
	let deadline = std::time::Instant::now() + std::time::Duration::from_millis(PAGE_WAIT_MS);
	loop {
		let current: u32 = eval_str(surface, CURRENT_PAGE_JS)
			.and_then(|v| v.trim().parse().ok())
			.unwrap_or(0);
		if current == target_page {
			return Ok(());
		}
		if std::time::Instant::now() >= deadline {
			return Err(format!("{target_page}페이지로 이동하지 못했습니다 (15초 초과)").into());
		}
		sleep_ms(250);
	}
}

/// TS captureOZPages — 줌·1페이지 복귀 후 페이지를 넘기며 캔버스를 캡처한다.
pub fn capture_oz_pages(
	surface: &CdpSurface,
	mut on_progress: impl FnMut(usize, usize),
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
	set_oz_zoom_to_300(surface)?;
	reset_oz_to_first_page(surface)?;
	let total = read_oz_total_pages(surface)?;
	let mut pages: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
	for index in 1..=total {
		let png = capture_changed_stable_canvas(surface, pages.last().map(Vec::as_slice))?
			.ok_or_else(|| format!("{index}페이지 캔버스 캡처에 실패했습니다."))?;
		pages.push(png);
		on_progress(index as usize, total as usize);
		if index < total {
			click_and_wait(surface, NEXT_PAGE_JS, index + 1)?;
		}
	}
	Ok(pages)
}

/// TS readReportXml — 뷰어에서 증명서 XML을 찾는다.
pub fn read_report_xml(surface: &CdpSurface) -> Result<String, Box<dyn std::error::Error>> {
	Ok(eval_str(surface, READ_REPORT_XML_JS).unwrap_or_default())
}

/// PNG 목록을 하나의 PDF로 합성한다 (TS pdf-lib buildPdfFromPngs와 동일 산출 구조:
/// 페이지 크기 = 픽셀 크기(포인트), 이미지를 원본 크기로 (0,0)에 그린다).
pub fn build_pdf_from_pngs(pngs: &[Vec<u8>]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	build_pdf_with_compression(pngs, flate2::Compression::new(3))
}

fn build_pdf_with_compression(pngs: &[Vec<u8>], compression: flate2::Compression) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	if pngs.is_empty() {
		return Err("PDF로 만들 페이지가 없습니다.".into());
	}
	struct Page {
		width: u32,
		height: u32,
		stream: Vec<u8>,
	}
	let mut pages: Vec<Page> = Vec::with_capacity(pngs.len());
	for png in pngs {
		let decoded = image::load_from_memory(png)?;
		let rgb = decoded.to_rgb8();
		let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), compression);
		encoder.write_all(rgb.as_raw())?;
		pages.push(Page {
			width: rgb.width(),
			height: rgb.height(),
			stream: encoder.finish()?,
		});
	}

	let count = pages.len();
	// 객체 번호: 1 Catalog, 2 Pages, 이후 페이지당 3개(Page/Contents/Image)
	let total_objects = 2 + count * 3;
	let mut buf: Vec<u8> = Vec::new();
	buf.extend_from_slice(b"%PDF-1.4\n");
	let mut offsets: Vec<usize> = Vec::with_capacity(total_objects);

	let mut push = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
		offsets.push(buf.len());
		buf.extend_from_slice(body);
	};

	// 1: Catalog
	push(&mut buf, &mut offsets, b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

	// 2: Pages
	let kids: Vec<String> = (0..count)
		.map(|k| format!("{} 0 R", 3 + k * 3))
		.collect();
	let pages_body = format!(
		"2 0 obj\n<< /Type /Pages /Kids [ {} ] /Count {} >>\nendobj\n",
		kids.join(" "),
		count
	);
	push(&mut buf, &mut offsets, pages_body.as_bytes());

	for (k, page) in pages.iter().enumerate() {
		let page_id = 3 + k * 3;
		let contents_id = page_id + 1;
		let image_id = page_id + 2;

		let page_body = format!(
			"{page_id} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] \
			 /Resources << /XObject << /Im0 {image_id} 0 R >> >> /Contents {contents_id} 0 R >>\nendobj\n",
			w = page.width,
			h = page.height
		);
		push(&mut buf, &mut offsets, page_body.as_bytes());

		let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n", w = page.width, h = page.height);
		let contents_body = format!(
			"{contents_id} 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
			content.len(),
			content
		);
		push(&mut buf, &mut offsets, contents_body.as_bytes());

		let image_header = format!(
			"{image_id} 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
			 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
			page.stream.len(),
			w = page.width,
			h = page.height
		);
		offsets.push(buf.len());
		buf.extend_from_slice(image_header.as_bytes());
		buf.extend_from_slice(&page.stream);
		buf.extend_from_slice(b"\nendstream\nendobj\n");
	}

	let xref_offset = buf.len();
	let xref_header = format!("xref\n0 {}\n", total_objects + 1);
	buf.extend_from_slice(xref_header.as_bytes());
	buf.extend_from_slice(format!("{:010} {:05} f \n", 0, 65535).as_bytes());
	for offset in &offsets {
		buf.extend_from_slice(format!("{:010} {:05} n \n", offset, 0).as_bytes());
	}
	let trailer = format!(
		"trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
		total_objects + 1,
		xref_offset
	);
	buf.extend_from_slice(trailer.as_bytes());
	Ok(buf)
}

#[cfg(test)]
mod performance_tests {
	use super::*;
	#[test]
	#[ignore = "KRAS_BENCH_PNG로 지정한 실제 페이지의 Windows release 측정"]
	fn actual_page_pdf_cost() {
		let png = std::fs::read(std::env::var("KRAS_BENCH_PNG").expect("실제 PNG 경로")).unwrap();
		let expected_rgb = image::load_from_memory(&png).unwrap().to_rgb8().into_raw();
		let mut decode = Vec::new();
		for _ in 0..5 {
			let start = std::time::Instant::now();
			let rgb = image::load_from_memory(&png).unwrap().to_rgb8();
			decode.push(start.elapsed().as_micros());
			std::hint::black_box(rgb);
		}
		decode.sort();
		println!("PNG_DECODE_RGB_MEDIAN_US={}", decode[2]);
		for level in [1, 3, 6] {
			let mut times = Vec::new();
			let mut size = 0;
			for _ in 0..5 {
				let start = std::time::Instant::now();
				let pdf = build_pdf_with_compression(&[png.clone()], flate2::Compression::new(level)).unwrap();
				times.push(start.elapsed().as_micros());
				size = pdf.len();
				assert!(crate::kras_output::valid_pdf(&pdf));
				let marker = b"/Subtype /Image";
				let image_at = pdf.windows(marker.len()).position(|part| part == marker).unwrap();
				let stream_at = image_at + pdf[image_at..].windows(7).position(|part| part == b"stream\n").unwrap() + 7;
				let mut restored = Vec::new();
				std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&pdf[stream_at..]), &mut restored).unwrap();
				assert_eq!(restored, expected_rgb, "PDF 이미지 픽셀 무손실 검증");
			}
			times.sort();
			println!("PDF_LEVEL={level} MEDIAN_US={} BYTES={size}", times[2]);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use image::ImageEncoder;

	fn png_of(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
		let mut buf = std::io::Cursor::new(Vec::new());
		let mut encoder = image::codecs::png::PngEncoder::new(&mut buf);
		let pixels: Vec<u8> = (0..width * height)
			.flat_map(|_| rgb.iter().copied())
			.collect();
		encoder
			.write_image(&pixels, width, height, image::ExtendedColorType::Rgb8)
			.expect("png encode");
		buf.into_inner()
	}

	#[test]
	fn build_pdf_has_valid_header_trailer_and_page_count() {
		let pngs = vec![png_of(4, 3, [255, 0, 0]), png_of(2, 2, [0, 255, 0])];
		let pdf = build_pdf_from_pngs(&pngs).expect("pdf");
		assert!(crate::kras_output::valid_pdf(&pdf));
		let text = String::from_utf8_lossy(&pdf);
		assert!(text.contains("/Type /Catalog"));
		assert!(text.contains("/Count 2"));
		assert!(text.contains("/MediaBox [0 0 4 3]"));
		assert!(text.contains("/MediaBox [0 0 2 2]"));
		assert!(text.contains("/Filter /FlateDecode"));
		assert!(text.contains("startxref"));
		assert!(text.trim_end().ends_with("%%EOF"));
	}

	#[test]
	fn build_pdf_offsets_match_xref_table() {
		let pngs = vec![png_of(2, 2, [10, 20, 30])];
		let pdf = build_pdf_from_pngs(&pngs).expect("pdf");
		// xref 오프셋은 바이트 기준이므로(이미지 스트림에 비ASCII 포함) 손실 디코딩한
		// String 대신 원본 바이트로 검증해야 한다.
		let needle = b"startxref";
		let hit = pdf
			.windows(needle.len())
			.rposition(|w| w == needle)
			.expect("startxref marker");
		let after = String::from_utf8_lossy(&pdf[hit + needle.len()..]);
		let startxref: usize = after
			.split_whitespace()
			.next()
			.and_then(|v| v.parse().ok())
			.expect("startxref number");
		assert_eq!(&pdf[startxref..startxref + 4], b"xref");

		let xref_part = &pdf[startxref..];
		let lines: Vec<&[u8]> = xref_part.split(|b| *b == b'\n').collect();
		// 0: "xref", 1: "0 N", 이후가 엔트리
		for (i, entry) in lines.iter().skip(2).take(5).enumerate() {
			if i == 0 {
				continue; // free entry
			}
			let text = String::from_utf8_lossy(entry);
			let offset: usize = text
				.split_whitespace()
				.next()
				.and_then(|v| v.parse().ok())
				.expect("object offset");
			let rest = &pdf[offset..];
			let prefix = format!("{i} 0 obj");
			assert!(
				rest.starts_with(prefix.as_bytes()),
				"object {i} offset mismatch: {:?}",
				&rest[..rest.len().min(24)]
			);
		}
	}

	#[test]
	fn build_pdf_rejects_empty_input() {
		assert!(build_pdf_from_pngs(&[]).is_err());
	}

	#[test]
	fn zoom_and_page_js_are_valid_iife_shapes() {
		// 문법 형태 보존 확인(브리지가 깨지면 Runtime.evaluate에서 예외가 난다)
		for src in [SET_ZOOM_JS, TOTAL_PAGES_JS, CURRENT_PAGE_JS, PREV_PAGE_JS, NEXT_PAGE_JS] {
			assert!(src.trim_start().starts_with('(') || src.contains("=>"), "bad shape");
		}
		assert!(READ_REPORT_XML_JS.contains("strOZDataXML"));
		assert!(READ_REPORT_XML_JS.contains("REAL_LAND_BLDG"));
		assert!(CAPTURE_CANVAS_JS.contains("toBlob"));
	}
}
