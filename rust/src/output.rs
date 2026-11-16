// 표준 출력·표준 에러 기록 지점 — 테스트는 버퍼로 교체한다.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static AUTO_COLOR: AtomicBool = AtomicBool::new(false);
const ORANGE: &str = "\x1b[38;5;208m";

pub fn set_auto_color(enabled: bool) {
	AUTO_COLOR.store(enabled, Ordering::Relaxed);
	write(if enabled { ORANGE } else { "\x1b[0m" });
}

fn colored_text(text: &str, auto: bool) -> String {
	if auto { format!("{ORANGE}{}{ORANGE}", text.replace("\x1b[0m", ORANGE)) }
	else { text.to_string() }
}

enum Sink {
	Real,
	Buffer(String),
}

static SINK: Mutex<Sink> = Mutex::new(Sink::Real);

fn lock_sink() -> std::sync::MutexGuard<'static, Sink> {
	SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn write(text: &str) {
	let text = colored_text(text, AUTO_COLOR.load(Ordering::Relaxed));
	match &mut *lock_sink() {
		Sink::Real => {
			let stdout = std::io::stdout();
			let mut handle = stdout.lock();
			let _ = handle.write_all(text.as_bytes());
			let _ = handle.flush();
		}
		Sink::Buffer(buffer) => buffer.push_str(&text),
	}
}

#[cfg(test)]
mod auto_color_tests {
	use super::{colored_text, ORANGE};

	#[test]
	fn auto_keeps_orange_after_embedded_reset() {
		let rendered = colored_text("AUTO\x1b[0m 다음 입력", true);
		assert!(rendered.starts_with(ORANGE));
		assert!(rendered.ends_with(ORANGE));
		assert!(!rendered.contains("\x1b[0m"));
		assert_eq!(colored_text("일반", false), "일반");
	}
}

pub fn write_err(text: &str) {
	// 에러 경로는 항상 실제 stderr로 남긴다(테스트가 잡아도 조용히 사라지지 않게).
	eprint!("{text}");
}

/// 테스트 전용: 출력을 버퍼로 돌리고 이전 버퍼를 돌려준다.
pub fn install_buffer() -> String {
	match std::mem::replace(&mut *lock_sink(), Sink::Buffer(String::new())) {
		Sink::Buffer(buffer) => buffer,
		Sink::Real => String::new(),
	}
}

/// 테스트 전용: 버퍼 내용을 비우고 돌려준다.
pub fn take_buffer() -> String {
	match std::mem::replace(&mut *lock_sink(), Sink::Buffer(String::new())) {
		Sink::Buffer(buffer) => buffer,
		Sink::Real => String::new(),
	}
}

/// 테스트 전용: 버퍼를 실제 출력으로 되돌린다.
pub fn restore_real() {
	*lock_sink() = Sink::Real;
}
