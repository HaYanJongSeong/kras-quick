// TS scripts/kras-numeric-ocr.ts — 숫자 보안문자 정책·출력 검증.
// OCR 추론은 프로세스 내 ort CPU 엔진을 사용한다.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericMode {
	Original,
	Crop,
}

impl NumericMode {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Original => "original",
			Self::Crop => "crop",
		}
	}
}

/// 정책 함수 공통 결과: 성공(5자리 또는 null) 또는 오류 전파.
pub type OcrOutcome = Result<Option<String>, Box<dyn std::error::Error>>;

pub type LegacyRecognizer<'a> = &'a dyn Fn(&str, Option<u8>) -> OcrOutcome;
pub type NumericRecognizer<'a> = &'a dyn Fn(&str, NumericMode) -> OcrOutcome;

pub type NumericRunner<'a> =
	 &'a dyn Fn(&str, NumericMode) -> Result<String, Box<dyn std::error::Error>>;

/// 런처가 발견한 OCR 루트 — 한 번 설정되면 이후 모든 추론에 적용된다.
static DISCOVERED_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_numeric_ocr_root(root: PathBuf) {
	let _ = DISCOVERED_ROOT.set(root);
}

pub fn numeric_ocr_root() -> PathBuf {
	if let Ok(root) = std::env::var("KRAS_NUMERIC_OCR_ROOT") {
		if !root.is_empty() {
			return PathBuf::from(root);
		}
	}
	if let Some(root) = DISCOVERED_ROOT.get() {
		return root.clone();
	}
	std::env::current_exe()
		.ok()
		.and_then(|executable| executable.parent().map(|dir| dir.join("numeric-ocr")))
		.unwrap_or_else(|| PathBuf::from("numeric-ocr"))
}

/// number.onnx가 있는 OCR 루트를 찾는다. 없으면 None.
fn candidates_with_model() -> Vec<PathBuf> {
	let mut out: Vec<PathBuf> = Vec::new();
	if let Ok(root) = std::env::var("KRAS_NUMERIC_OCR_ROOT") {
		if !root.is_empty() {
			let p = PathBuf::from(&root);
			if p.join("number.onnx").is_file() {
				out.push(p);
			}
		}
	}
	if let Ok(exe) = std::env::current_exe() {
		if let Some(dir) = exe.parent() {
			let p = dir.join("numeric-ocr");
			if p.join("number.onnx").is_file() {
				out.push(p);
			}
		}
	}
	// 런타임 캐시: LOCALAPPDATA\kras-quick\*\numeric-ocr
	let mut bases: Vec<PathBuf> = Vec::new();
	if let Ok(v) = std::env::var("LOCALAPPDATA") {
		if !v.is_empty() {
			bases.push(PathBuf::from(v).join("kras-quick"));
		}
	}
	if let Ok(v) = std::env::var("TEMP") {
		if !v.is_empty() {
			bases.push(PathBuf::from(v).join("kras-quick"));
		}
	}
	for base in bases {
		if let Ok(entries) = std::fs::read_dir(&base) {
			let mut dirs: Vec<PathBuf> = entries
				.filter_map(|e| e.ok())
				.map(|e| e.path())
				.filter(|p| p.is_dir())
				.collect();
			// 버전 이름 내림차순(最新 런타임 우선)
			dirs.sort();
			dirs.reverse();
			for d in dirs {
				let p = d.join("numeric-ocr");
				if p.join("number.onnx").is_file() && !out.contains(&p) {
					out.push(p);
				}
			}
		}
	}
	out
}

/// 모델이 있는 루트를 찾아 설정하고 경로를 돌려준다.
pub fn discover_numeric_ocr_root() -> Option<PathBuf> {
	let found = candidates_with_model().into_iter().next();
	if let Some(root) = &found {
		set_numeric_ocr_root(root.clone());
	}
	found
}

/// 모델 파일 존재 여부 — Python 실행파일 검사는 Rust 엔지엔에 적용되지 않는다.
pub fn numeric_ocr_available(root: &Path) -> bool {
	root.join("number.onnx").is_file()
}

/// 런처 runner 경로 검증: stdout을 잘라 5자리 숫자인지 판정한다.
pub fn validate_numeric_output(stdout: &str) -> Option<String> {
	let text = stdout.trim();
	if text.len() == 5 && text.bytes().all(|byte| byte.is_ascii_digit()) {
		Some(text.to_string())
	} else {
		None
	}
}

/// runner 주입 경로(TS recognizeNumericCaptcha의 options.runner 부분).
fn run_with_runner(
	png: &str,
	mode: NumericMode,
	_root: &Path,
	_timeout_ms: u64,
	runner: NumericRunner<'_>,
) -> OcrOutcome {
	let stdout = runner(png, mode)?;
	Ok(validate_numeric_output(&stdout))
}

/// 엔진 주입 훅 — ort 인프레스 단계에서 이 함수가 실제 추론으로 교체된다.
fn default_numeric_recognize(png: &str, mode: NumericMode) -> OcrOutcome {
	crate::numeric_engine::recognize_numeric(png, mode)
}

pub fn recognize_numeric_captcha(
	png: &str,
	mode: NumericMode,
	root: &Path,
	timeout_ms: u64,
	runner: Option<NumericRunner<'_>>,
) -> OcrOutcome {
	match runner {
		Some(runner) => run_with_runner(png, mode, root, timeout_ms, runner),
		None => default_numeric_recognize(png, mode),
	}
}

/// AUTO 후보 정책(TS recognizeAutoCaptchaCandidate 동일).
/// numeric이 없고 legacy recognize만 있으면 legacy 경로, 아니면 numeric 경로
/// (slot 짝수 → crop, 홀수 → original). numeric 실패·null은 legacy로 대체하지 않는다.
pub fn recognize_auto_captcha_candidate(
	png: &str,
	slot: u8,
	legacy_attempt: u8,
	recognize: Option<LegacyRecognizer<'_>>,
	numeric: Option<NumericRecognizer<'_>>,
) -> OcrOutcome {
	if numeric.is_none() {
		if let Some(recognize) = recognize {
			return recognize(png, Some(legacy_attempt));
		}
	}
	let mode = if slot % 2 == 0 {
		NumericMode::Crop
	} else {
		NumericMode::Original
	};
	match numeric {
		Some(numeric) => numeric(png, mode),
		None => default_numeric_recognize(png, mode),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;
	use std::cell::RefCell;

	#[test]
	fn uses_only_the_numeric_engine_for_all_three_attempts() {
		let calls: RefCell<Vec<String>> = RefCell::new(Vec::new());
		let legacy = |image: &str, attempt: Option<u8>| -> OcrOutcome {
			calls
				.borrow_mut()
				.push(format!("legacy:{attempt:?}"));
			let _ = image;
			Ok(Some("00017".to_string()))
		};
		let numeric = |image: &str, mode: NumericMode| -> OcrOutcome {
			let _ = image;
			calls.borrow_mut().push(mode.as_str().to_string());
			Ok(Some("00017".to_string()))
		};
		for slot in 1..=3 {
			let result =
				recognize_auto_captcha_candidate("png", slot, 1, Some(&legacy), Some(&numeric))
					.unwrap();
			assert_eq!(result.as_deref(), Some("00017"));
		}
		assert_eq!(*calls.borrow(), ["original", "crop", "original"]);
	}

	#[test]
	fn keeps_an_explicit_existing_recognizer_unchanged() {
		let attempts: RefCell<Vec<Option<u8>>> = RefCell::new(Vec::new());
		let recognize = |image: &str, attempt: Option<u8>| -> OcrOutcome {
			let _ = image;
			attempts.borrow_mut().push(attempt);
			Ok(Some("27277".to_string()))
		};
		for slot in 1..=3 {
			let result =
				recognize_auto_captcha_candidate("png", slot, slot, Some(&recognize), None)
					.unwrap();
			assert_eq!(result.as_deref(), Some("27277"));
		}
		assert_eq!(*attempts.borrow(), [Some(1), Some(2), Some(3)]);
	}

	#[test]
	fn does_not_add_a_legacy_ocr_call_when_numeric_inference_fails() {
		let legacy_calls = Cell::new(0u32);
		let recognize = |_image: &str, _attempt: Option<u8>| -> OcrOutcome {
			legacy_calls.set(legacy_calls.get() + 1);
			Ok(Some("27277".to_string()))
		};
		let numeric = |_image: &str, _mode: NumericMode| -> OcrOutcome {
			Err("numeric process failure".into())
		};
		let result =
			recognize_auto_captcha_candidate("png", 1, 1, Some(&recognize), Some(&numeric));
		let error = result.unwrap_err();
		assert_eq!(error.to_string(), "numeric process failure");
		assert_eq!(legacy_calls.get(), 0);
	}

	#[test]
	fn leaves_a_null_numeric_result_as_null_rather_than_guessing_digits() {
		let legacy_calls = Cell::new(0u32);
		let recognize = |_image: &str, _attempt: Option<u8>| -> OcrOutcome {
			legacy_calls.set(legacy_calls.get() + 1);
			Ok(Some("27277".to_string()))
		};
		let numeric = |_image: &str, _mode: NumericMode| -> OcrOutcome { Ok(None) };
		let result =
			recognize_auto_captcha_candidate("png", 2, 2, Some(&recognize), Some(&numeric))
				.unwrap();
		assert_eq!(result, None);
		assert_eq!(legacy_calls.get(), 0);
	}

	#[test]
	fn retains_exact_numeric_output() {
		for text in ["00000", "00017", "11111", "77777", "27277"] {
			let runner = |_png: &str, _mode: NumericMode| {
				Ok(format!("{text}\n"))
			};
			let result =
				recognize_numeric_captcha("png", NumericMode::Original, Path::new("test-root"), 5000, Some(&runner))
					.unwrap();
			assert_eq!(result.as_deref(), Some(text), "text={text}");
		}
	}

	#[test]
	fn rejects_invalid_numeric_output() {
		for text in ["", "2727", "272777", "27 277", "２７２７７", "a27277"] {
			let fixture = text.to_string();
			let runner = |_png: &str, _mode: NumericMode| {
				Ok(fixture.clone())
			};
			let result =
				recognize_numeric_captcha("png", NumericMode::Crop, Path::new("test-root"), 5000, Some(&runner))
					.unwrap();
			assert_eq!(result, None, "text={text:?}");
		}
	}

	#[test]
	fn passes_png_and_mode_to_in_process_recognizer() {
		let received = Cell::new(false);
		let runner = |png: &str, mode: NumericMode| {
			assert_eq!(png, "sensitive-png");
			assert_eq!(mode, NumericMode::Crop);
			received.set(true);
			Ok("27277".to_string())
		};
		let result = recognize_numeric_captcha(
			"sensitive-png",
			NumericMode::Crop,
			Path::new("test-root"),
			123,
			Some(&runner),
		)
		.unwrap();
		assert!(received.get());
		assert_eq!(result.as_deref(), Some("27277"));
	}
}

#[cfg(test)]
mod discovery_tests {
	use super::*;

	#[test]
	fn discover_numeric_ocr_root_yields_model_when_present() {
		if let Some(root) = discover_numeric_ocr_root() {
			assert!(
				root.join("number.onnx").is_file(),
				"discovered root lacks number.onnx: {}",
				root.display()
			);
			assert_eq!(numeric_ocr_root(), root);
			assert!(crate::numeric_ocr::numeric_ocr_available(&root));
		}
	}
}
