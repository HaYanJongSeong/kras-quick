// TS scripts/kras-numeric-ocr.py — ONNX CPU OCR 전처리·추론·CTC 디코드.
// Python 워커 대신 프로세스 내 ort 세션을 사용한다.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use sha2::{Digest, Sha256};

use crate::numeric_ocr::{NumericMode, OcrOutcome};

const MODEL_SHA256: &str = "d3675cc5336ac085b8b022e71c923e4871ebdd6ea34d20c4aa5e2749dded1a4e";
const MAX_INPUT: usize = 12 * 1024 * 1024;

/// 모델 세션 캐시 — 루트가 바뀌면 다시 로드한다.
static SESSION: Mutex<Option<(PathBuf, ort::session::Session)>> = Mutex::new(None);

pub fn numeric_engine_available(root: &Path) -> bool {
	root.join("number.onnx").is_file()
}

/// SHA256 검증 후 ort 세션을 로드한다(성공 시 캐시).
fn load_session(root: &Path) -> Result<ort::session::Session, Box<dyn std::error::Error>> {
	let model_path = root.join("number.onnx");
	let bytes = std::fs::read(&model_path)?;
	let hash = Sha256::digest(&bytes);
	let actual = format!("{hash:x}");
	if actual != MODEL_SHA256 {
		return Err(format!(
			"Numeric OCR model checksum mismatch: expected {MODEL_SHA256}, got {actual}"
		)
		.into());
	}
	let env = ort::init().build()?;
	let mut builder = ort::session::Session::builder(&env)?;
	builder = builder.with_intra_threads(1)?;
	let session = builder.commit_from_file(&model_path)?;
	Ok(session)
}

fn cached_session(
	root: &Path,
) -> Result<std::sync::MutexGuard<'static, Option<(PathBuf, ort::session::Session)>>, Box<dyn std::error::Error>>
{
	// 락 획득 후 루트가 다르면 세션을 교체한다.
	let mut guard = SESSION.lock().map_err(|e| format!("session lock poisoned: {e}"))?;
	if guard.as_ref().map(|(r, _)| r != root).unwrap_or(true) {
		let session = load_session(root)?;
		*guard = Some((root.to_path_buf(), session));
	}
	Ok(guard)
}

/// RGBA → 흰 배경 알파 합성 → ITU-R 601 Luma (Pillow convert("L") 동일).
fn rgba_to_luma(rgba: &image::RgbaImage) -> Vec<u8> {
	let mut out = vec![0u8; (rgba.width() as usize) * (rgba.height() as usize)];
	for (i, px) in rgba.pixels().enumerate() {
		let [r, g, b, a] = px.0;
		let alpha = a as f32 / 255.0;
		let rf = r as f32 * alpha + 255.0 * (1.0 - alpha);
		let gf = g as f32 * alpha + 255.0 * (1.0 - alpha);
		let bf = b as f32 * alpha + 255.0 * (1.0 - alpha);
		// Pillow: (R*299 + G*587 + B*114) / 1000 (floor)
		out[i] = ((rf * 299.0 + gf * 587.0 + bf * 114.0) / 1000.0).floor() as u8;
	}
	out
}

/// 전처리: base64 PNG → (tensor float32, width). Python prepare_image와 동일 계약.
fn prepare_image(
	png_b64: &str,
	mode: NumericMode,
) -> Result<(Vec<f32>, usize), Box<dyn std::error::Error>> {
	if png_b64.len() > MAX_INPUT {
		return Err("Numeric OCR input size limit".into());
	}
	let bytes = base64::engine::general_purpose::STANDARD
		.decode(png_b64)
		.map_err(|e| format!("Numeric OCR base64 decode failed: {e}"))?;
	if image::guess_format(&bytes).ok() != Some(ImageFormat::Png) {
		return Err("Invalid numeric OCR PNG dimensions".into());
	}
	let img = image::load_from_memory(&bytes)?;
	let (w, h) = img.dimensions();
	if (w as u64) * (h as u64) > 1_000_000 {
		return Err("Invalid numeric OCR PNG dimensions".into());
	}
	let rgba = img.to_rgba8();
	let gray = rgba_to_luma(&rgba);

	let (pixels, mut width, mut height) = match mode {
		NumericMode::Original => (gray, w as usize, h as usize),
		NumericMode::Crop => {
			let w = w as usize;
			let h = h as usize;
			if w % 2 != 0 || h % 2 != 0 {
				return Err("Expected an original 2x capture".into());
			}
			let nw = w / 2;
			let nh = h / 2;
			// nearest-neighbour 2x downscale
			let mut small = vec![0u8; nw * nh];
			for y in 0..nh {
				for x in 0..nw {
					small[y * nw + x] = gray[(y * 2) * w + (x * 2)];
				}
			}
			let mean: f64 =
				small.iter().map(|&v| v as f64).sum::<f64>() / small.len() as f64;
			let mut pixels = small;
			if mean < 145.0 {
				for p in &mut pixels {
					*p = 255 - *p;
				}
			}
			// Otsu-like between-class variance
			let mut hist = [0u64; 256];
			for &p in &pixels {
				hist[p as usize] += 1;
			}
			let total = pixels.len() as u64;
			let mut counts = [0u64; 256];
			let mut sums = [0u64; 256];
			counts[0] = hist[0];
			sums[0] = 0;
			for i in 1..256 {
				counts[i] = counts[i - 1] + hist[i];
				sums[i] = sums[i - 1] + hist[i] * (i as u64);
			}
			let total_sum = sums[255];
			let mut best_t = 0usize;
			let mut best_var = 0.0f64;
			for t in 0..256 {
				let cnt = counts[t];
				let denom = cnt as f64 * (total as f64 - cnt as f64);
				if denom <= 0.0 {
					continue;
				}
				let num = (total_sum as f64 * cnt as f64 - sums[t] as f64 * total as f64).powi(2);
				let var = num / denom;
				if var > best_var {
					best_var = var;
					best_t = t;
				}
			}
			let threshold = best_t as u8;
			// bbox of mask (pixels <= threshold)
			let mut ymin = nh;
			let mut ymax = 0usize;
			let mut xmin = nw;
			let mut xmax = 0usize;
			let mut found = false;
			for y in 0..nh {
				for x in 0..nw {
					if pixels[y * nw + x] <= threshold {
						if !found {
							ymin = y;
							xmin = x;
							found = true;
						}
						ymin = ymin.min(y);
						ymax = ymax.max(y);
						xmin = xmin.min(x);
						xmax = xmax.max(x);
					}
				}
			}
			if !found {
				return Err("Numeric OCR image has no foreground".into());
			}
			let ch = ymax - ymin + 1;
			let cw = xmax - xmin + 1;
			let mut cropped = vec![255u8; ch * cw];
			for y in 0..ch {
				for x in 0..cw {
					cropped[y * cw + x] = pixels[(ymin + y) * nw + (xmin + x)];
				}
			}
			// pad 2 with 255
			let ph = ch + 4;
			let pw = cw + 4;
			let mut padded = vec![255u8; ph * pw];
			for y in 0..ch {
				for x in 0..cw {
					padded[(y + 2) * pw + (x + 2)] = cropped[y * cw + x];
				}
			}
			(padded, pw, ph)
		}
	};

	let resized_w = ((width as f64 * 32.0) / (height as f64)).floor() as usize;
	let resized_w = resized_w.max(8);
	if resized_w > 4096 {
		return Err("Numeric OCR tensor width limit".into());
	}
	let gray_img = image::GrayImage::from_raw(width as u32, height as u32, pixels.clone())
		.ok_or("Numeric OCR gray image construction failed")?;
	let resized = image::imageops::resize(
		&gray_img,
		resized_w as u32,
		32,
		FilterType::Triangle,
	);
	let mut tensor = vec![0.0f32; resized_w * 32];
	for (i, px) in resized.pixels().enumerate() {
		tensor[i] = px.0[0] as f32 / 255.0;
	}
	Ok((tensor, resized_w))
}

/// CTC greedy decode: argmax per timestep, collapse consecutive repeats, skip blank(10), map "1234567890".
fn ctc_decode(scores: &[f32], time_len: usize, classes: usize) -> String {
	let mut indices = Vec::with_capacity(time_len);
	for t in 0..time_len {
		let row = &scores[t * classes..(t + 1) * classes];
		let mut best = 0usize;
		let mut bestv = f32::NEG_INFINITY;
		for (j, &v) in row.iter().enumerate() {
			if v > bestv {
				bestv = v;
				best = j;
			}
		}
		indices.push(best);
	}
	let mut text = String::new();
	let mut prev: Option<usize> = None;
	for &idx in &indices {
		if prev == Some(idx) {
			continue;
		}
		prev = Some(idx);
		if idx == 10 {
			continue;
		}
		let ch = "1234567890".chars().nth(idx).unwrap_or('?');
		text.push(ch);
	}
	text
}

/// Recognize a single captcha. Returns Ok(Some(5 digits)) or Ok(None) or Err.
pub fn recognize_numeric(
	png_b64: &str,
	mode: NumericMode,
) -> OcrOutcome {
	let root = crate::numeric_ocr::numeric_ocr_root();
	let mut guard = cached_session(&root)?;
	let (root_cached, session) = guard.as_mut().ok_or("session unavailable")?;
	let _ = root_cached;
	let (tensor, width) = prepare_image(png_b64, mode)?;
	let lengths = vec![width as i64];
	let input_tensor = ort::value::Tensor::from_array(([1usize, 1, 32, width], tensor))?;
	let length_tensor = ort::value::Tensor::from_array(([1usize], lengths))?;
	let outputs = session.run(ort::inputs!["x" => input_tensor, "input_lengths" => length_tensor])?;

	// scores = outputs[0][0, :lengths_out]
	let scores_view = outputs[0].try_extract_tensor::<f32>()?;
	let shape = scores_view.0;
	let t_total = if shape.len() >= 2 { shape[1] as usize } else { 0 };
	// lengths output dtype may be i64 or i32
	let time_len_raw = match outputs[1].try_extract_tensor::<i64>() {
		Ok(v) => v.1.first().copied().unwrap_or(0) as usize,
		Err(_) => match outputs[1].try_extract_tensor::<i32>() {
			Ok(v) => v.1.first().copied().unwrap_or(0) as usize,
			Err(_) => return Err("Invalid numeric OCR model output".into()),
		},
	};
	let time_len = time_len_raw.min(t_total);
	let data = scores_view.1;
	// first batch item: data layout [1, T, 11]
	let scores = &data[..time_len * 11];
	// finite check
	if !scores.iter().all(|v| v.is_finite()) {
		return Err("Invalid numeric OCR model output".into());
	}
	// validate shape[2] == 11
	if shape.len() < 3 || shape[2] != 11 {
		return Err("Invalid numeric OCR model output".into());
	}
	let text = ctc_decode(scores, time_len, 11);
	if text.len() == 5 && text.bytes().all(|b| b.is_ascii_digit()) {
		Ok(Some(text))
	} else {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use base64::Engine as _;
	use image::{ImageBuffer, Rgba};

	#[test]
	fn rejects_oversized_input() {
		let big = "A".repeat(MAX_INPUT + 1);
		let err = prepare_image(&big, NumericMode::Original).unwrap_err();
		assert!(err.to_string().contains("input size limit"), "{err}");
	}

	#[test]
	fn rejects_non_png() {
		// JPEG magic bytes
		let bytes = [0xFFu8, 0xD8, 0xFF, 0xE0];
		let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
		let err = prepare_image(&b64, NumericMode::Original).unwrap_err();
		assert!(err.to_string().contains("PNG dimensions"), "{err}");
	}

	#[test]
	fn rejects_oversized_dimensions() {
		// 1001x1001 = 1_002_001 > 1_000_000
		let img = DynamicImage::new_rgba8(1001, 1001);
		let mut buf = Vec::new();
		img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
			.unwrap();
		let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
		let err = prepare_image(&b64, NumericMode::Original).unwrap_err();
		assert!(err.to_string().contains("PNG dimensions"), "{err}");
	}

	#[test]
	fn crop_mode_rejects_odd_dimensions() {
		let img = DynamicImage::new_rgba8(5, 4);
		let mut buf = Vec::new();
		img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
			.unwrap();
		let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
		let err = prepare_image(&b64, NumericMode::Crop).unwrap_err();
		assert!(err.to_string().contains("2x capture"), "{err}");
	}

	#[test]
	fn crop_mode_rejects_no_foreground() {
		// all-white image → no foreground
		let img = DynamicImage::new_rgba8(10, 10);
		let mut buf = Vec::new();
		img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
			.unwrap();
		let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
		let err = prepare_image(&b64, NumericMode::Crop).unwrap_err();
		assert!(err.to_string().contains("no foreground"), "{err}");
	}

	#[test]
	fn prepare_image_original_produces_32_height_tensor() {
		let img = DynamicImage::new_rgba8(20, 40);
		let mut buf = Vec::new();
		img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
			.unwrap();
		let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
		let (tensor, w) = prepare_image(&b64, NumericMode::Original).unwrap();
		assert_eq!(tensor.len(), w * 32);
		assert!(w >= 8);
		assert!(w <= 4096);
	}

	#[test]
	fn ctc_decode_collapses_and_maps() {
		// 3 timesteps: [5,5,10] → "5" (collapse 5s, skip blank)
		let mut scores = vec![0.0f32; 3 * 11];
		// t0: class5 high
		scores[4] = 10.0;
		// t1: class4 high (repeat → collapsed)
		scores[1 * 11 + 4] = 10.0;
		// t2: class10 (blank)
		scores[2 * 11 + 10] = 10.0;
		let text = ctc_decode(&scores, 3, 11);
		assert_eq!(text, "5");
	}

	#[test]
	fn ctc_decode_maps_indices_to_digits() {
		// "12345" with no repeats
		let digits = "12345";
		let mut scores = vec![0.0f32; digits.len() * 11];
		for (i, ch) in digits.chars().enumerate() {
			let idx = "1234567890".find(ch).unwrap();
			scores[i * 11 + idx] = 10.0;
		}
		let text = ctc_decode(&scores, digits.len(), 11);
		assert_eq!(text, "12345");
	}

	#[test]
	fn model_checksum_mismatch_errors() {
		let dir = std::env::temp_dir().join("kras-ocr-badmodel");
		let _ = std::fs::create_dir_all(&dir);
		std::fs::write(dir.join("number.onnx"), b"not a model").unwrap();
		let err = load_session(&dir).unwrap_err();
		assert!(err.to_string().contains("checksum mismatch"), "{err}");
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn numeric_engine_available_checks_model_file() {
		let dir = std::env::temp_dir().join("kras-ocr-avail");
		let _ = std::fs::create_dir_all(&dir);
		assert!(!numeric_engine_available(&dir));
		std::fs::write(dir.join("number.onnx"), b"x").unwrap();
		assert!(numeric_engine_available(&dir));
		let _ = std::fs::remove_dir_all(&dir);
	}
}
