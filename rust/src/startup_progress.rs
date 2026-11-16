// TS scripts/kras-startup-progress.ts — 단계·비율 기반 시작 진행률(시간 채움 금지).

use std::sync::Mutex;
use std::sync::Weak;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::sync::Arc;

use crate::output;
use crate::term_env;

static PLAN: Mutex<Vec<String>> = Mutex::new(Vec::new());
static FRACTION: Mutex<f64> = Mutex::new(0.0);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn configure_startup_progress(stages: &[&str]) {
	*lock(&PLAN) = stages.iter().map(|stage| (*stage).to_string()).collect();
	*lock(&FRACTION) = 0.0;
}

fn clock_fn() -> Box<dyn Fn() -> f64 + Send> {
	let origin = Instant::now();
	Box::new(move || origin.elapsed().as_secs_f64() * 1000.0)
}

// JS toFixed(1): 0.5 이상은 큰 쪽으로(반올림 절반 절상).
fn mb(bytes: u64) -> String {
	let rounded = (bytes as f64 / 1048576.0 * 10.0).round() / 10.0;
	format!("{rounded:.1} MiB")
}

struct Inner {
	label: String,
	received: Option<u64>,
	total: Option<u64>,
	updated: f64,
	finished: bool,
	animated: bool,
	columns: usize,
	clock: Box<dyn Fn() -> f64 + Send>,
}

impl Inner {
	fn render(&self, result: Option<&str>, newline: bool) {
		let width = self.columns;
		let ratio = match (self.received, self.total) {
			(Some(received), Some(total)) if total > 0 => {
				Some((received as f64 / total as f64).min(1.0))
			}
			_ => None,
		};
		let plan = lock(&PLAN);
		let stage_index = plan
			.iter()
			.position(|stage| *stage == self.label)
			.unwrap_or(0);
		let fraction = if result == Some("완료") {
			1.0
		} else {
			ratio.unwrap_or(0.0)
		};
		let measured = if plan.is_empty() {
			fraction
		} else {
			(stage_index as f64 + fraction) / plan.len() as f64
		};
		let overall = if plan.is_empty() {
			measured
		} else {
			lock(&FRACTION).max(measured)
		};
		*lock(&FRACTION) = overall;
		drop(plan);
		let percentage = (overall * 100.0).floor() as u64;
		let filled = (overall * 16.0).floor() as usize;
		let bar: String = (0..16)
			.map(|index| if index < filled { "█" } else { "░" })
			.collect();
		let amount = if self.label == "런타임 압축 해제" {
			match (self.received, self.total) {
				(Some(received), Some(total)) => format!("{received} / {total}개 파일"),
				(Some(received), None) => format!("{received} / ?개 파일"),
				_ => String::new(),
			}
		} else {
			// TS는 ratio가 아니라 total 유무로 갈라진다(total==0이어도 0% x / 0.0 MiB).
			match (self.received, self.total) {
				(None, _) => String::new(),
				(Some(received), None) => mb(received),
				(Some(received), Some(total)) => format!(
					"{}%  {} / {}",
					(ratio.unwrap_or(0.0) * 100.0).floor() as u64,
					mb(received),
					mb(total)
				),
			}
		};
		let waiting = if self.received.is_some()
			&& (self.clock)() - self.updated >= 10000.0
		{
			" · 수신 대기"
		} else {
			""
		};
		let main = match result {
			Some(result) => result.to_string(),
			None => {
				if width >= 80 {
					amount
				} else if ratio.is_none() {
					"처리 중".to_string()
				} else {
					format!("{}%", (ratio.unwrap_or(0.0) * 100.0).floor() as u64)
				}
			}
		};
		let bar_prefix = if width >= 52 {
			format!("[{bar}] ")
		} else {
			String::new()
		};
		let detail = format!("{main}{waiting}");
		let line = format!(
			"{} : {}{} · 전체 {}%",
			self.label, bar_prefix, detail, percentage
		);
		if self.animated {
			let tail = if result.is_some() && newline {
				"\n"
			} else {
				""
			};
			output::write(&format!("\r\u{1b}[2K{line}{tail}"));
		} else {
			// 비TTY: 반복 프레임 없이 시작/완료 한 줄만.
			output::write(&format!(
				"{}: {}\n",
				self.label,
				result.unwrap_or("시작")
			));
		}
	}
}

pub struct StartupProgress {
	inner: Arc<Mutex<Inner>>,
}

impl StartupProgress {
	pub fn new(label: &str) -> Self {
		let config = term_env::config();
		let clock = clock_fn();
		let animated = config.stderr_tty && config.term.as_deref() != Some("dumb");
		let inner = Arc::new(Mutex::new(Inner {
			label: label.to_string(),
			received: None,
			total: None,
			updated: clock(),
			finished: false,
			animated,
			columns: config.columns,
			clock,
		}));
		lock(&inner).render(None, true);
		if animated {
			let weak = Arc::downgrade(&inner);
			thread::spawn(move || {
				animation_loop(weak);
			});
		}
		Self { inner }
	}

	pub fn update(&self, received: u64, total: Option<u64>) {
		let mut inner = lock(&self.inner);
		inner.received = Some(received);
		inner.total = total;
		inner.updated = (inner.clock)();
	}

	pub fn finish(&self, ok: bool, newline: bool) {
		let mut inner = lock(&self.inner);
		inner.finished = true;
		inner.render(Some(if ok { "완료" } else { "실패" }), newline);
	}
}

// TS의 setInterval(120ms) — 끝나거나 인스턴스가 사라지면 스스로 종료한다.
fn animation_loop(weak: Weak<Mutex<Inner>>) {
	loop {
		thread::sleep(Duration::from_millis(120));
		let Some(inner) = weak.upgrade() else {
			return;
		};
		let guard = lock(&inner);
		if guard.finished {
			return;
		}
		guard.render(None, true);
	}
}

pub fn startup_stage<T, E>(
	label: &str,
	action: impl FnOnce(&mut StartupProgress) -> Result<T, E>,
) -> Result<T, E> {
	let mut progress = StartupProgress::new(label);
	let result = action(&mut progress);
	let ok = result.is_ok();
	progress.finish(ok, !ok);
	result
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::{install_buffer, restore_real, take_buffer};
	use crate::term_env::TermConfig;
	use crate::test_serial;
	use std::convert::Infallible;
	use std::sync::MutexGuard;

	// 전역 PLAN/FRACTION·출력 버퍼·터미널 교체를 함께 쓰므로 직렬화하고 반드시 되돌린다.
	struct TestEnv {
		_serial: MutexGuard<'static, ()>,
	}

	impl TestEnv {
		fn new(stderr_tty: bool) -> Self {
			let serial = test_serial::lock();
			configure_startup_progress(&[]);
			install_buffer();
			term_env::set_config(Some(TermConfig {
				stdout_tty: stderr_tty,
				stdin_tty: stderr_tty,
				stderr_tty,
				term: None,
				columns: 100,
				rows: 30,
			}));
			Self { _serial: serial }
		}
	}

	impl Drop for TestEnv {
		fn drop(&mut self) {
			configure_startup_progress(&[]);
			term_env::set_config(None);
			restore_real();
		}
	}

	#[test]
	fn redirected_output_free_of_ansi_and_repeated_frames() {
		let env = TestEnv::new(false);
		let value = startup_stage("확인", |p| {
			p.update(10, Some(20));
			Ok::<u32, Infallible>(42)
		})
		.unwrap();
		assert_eq!(value, 42);
		// 비TTY: ANSI 없이 시작/완료 두 줄만.
		assert_eq!(take_buffer(), "확인: 시작\n확인: 완료\n");
		drop(env);
	}

	#[test]
	fn measured_percentages_unknown_wait_and_stop_after_failure() {
		let env = TestEnv::new(true);
		let progress = StartupProgress::new("다운로드");
		progress.update(1_048_576, Some(2_097_152));
		thread::sleep(Duration::from_millis(150));
		progress.finish(true, true);
		let output = take_buffer();
		assert!(
			output.contains("50%  1.0 MiB / 2.0 MiB"),
			"50% frame: {output:?}"
		);
		// 실패한 단계는 100%/완료를 지어내지 않는다.
		let result = startup_stage("압축 해제", |_| {
			thread::sleep(Duration::from_millis(150));
			Err::<(), String>("fixture failure".to_string())
		});
		assert!(result.is_err());
		let output = take_buffer();
		assert!(output.contains("실패"), "{output:?}");
		assert!(!output.contains("100%") && !output.contains("완료"), "{output:?}");
		// 종료 후 애니메이션 타이머가 더 이상 프레임을 만들지 않는다.
		thread::sleep(Duration::from_millis(160));
		assert_eq!(take_buffer(), "");
		drop(env);
	}

	#[test]
	fn reports_measured_overall_stage_percentages() {
		let env = TestEnv::new(true);
		configure_startup_progress(&["런타임 준비", "런타임 압축 해제", "프로그램 로딩"]);
		for stage in ["런타임 준비", "런타임 압축 해제", "프로그램 로딩"] {
			StartupProgress::new(stage).finish(true, true);
		}
		let output = take_buffer();
		// 실제 단계 완료로만 33→66→100, 시간 채움 없음.
		assert!(output.contains("전체 33%"), "{output:?}");
		assert!(output.contains("전체 66%"), "{output:?}");
		assert!(output.contains("전체 100%"), "{output:?}");
		assert!(!output.contains('초'), "{output:?}");
		drop(env);
	}

	#[test]
	fn overwrites_same_line_across_stages_without_scrolling() {
		let env = TestEnv::new(true);
		startup_stage("다운로드", |_| Ok::<(), Infallible>(())).unwrap();
		startup_stage("압축 해제", |_| Ok::<(), Infallible>(())).unwrap();
		let output = take_buffer();
		// 성공 단계는 개행 없이 같은 줄을 덮어쓴다.
		assert!(!output.contains('\n'), "{output:?}");
		assert!(output.contains("다운로드 : ["), "{output:?}");
		assert!(output.contains("압축 해제 : ["), "{output:?}");
		assert!(output.contains(&"█".repeat(16)), "{output:?}");
		assert!(output.contains("\r\u{1b}[2K"), "{output:?}");
		drop(env);
	}
}
