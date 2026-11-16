// TS scripts/kras-task-screen.ts — 고정 라인 진행 UI·JSONL 작업 로그·Sixel 보존 계약.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::json;

use crate::output;
use crate::task_timing::TaskTiming;
use crate::term_env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchCounts {
	pub processed: u64,
	pub total: u64,
	pub failed: u64,
	pub missing: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	Completed,
	Duplicate,
	Failed,
	Missing,
}

impl Outcome {
	fn as_str(self) -> &'static str {
		match self {
			Outcome::Completed => "completed",
			Outcome::Duplicate => "duplicate",
			Outcome::Failed => "failed",
			Outcome::Missing => "missing",
		}
	}
}

// JS: codePoint >= 0x1100 → 2칸, 초과 시 …를 붙인다.
fn clip(text: &str, width: usize) -> String {
	let mut result = String::new();
	let mut used = 0usize;
	for character in text.chars() {
		let cells = if u32::from(character) >= 0x1100 {
			2
		} else {
			1
		};
		if used + cells > width {
			result.push('…');
			return result;
		}
		result.push(character);
		used += cells;
	}
	result
}

// node:util stripVTControlCharacters — CSI/OSC/DCS 등 제어 시퀀스 제거.
fn strip_vt(text: &str) -> String {
	let mut result = String::new();
	let mut chars = text.chars().peekable();
	while let Some(character) = chars.next() {
		if character != '\u{1b}' {
			result.push(character);
			continue;
		}
		match chars.peek().copied() {
			Some('[') => {
				chars.next();
				for next in chars.by_ref() {
					if ('\u{40}'..='\u{7e}').contains(&next) {
						break;
					}
				}
			}
			Some(']') => {
				chars.next();
				while let Some(next) = chars.next() {
					if next == '\u{7}' {
						break;
					}
					if next == '\u{1b}' {
						if chars.peek() == Some(&'\\') {
							chars.next();
						}
						break;
					}
				}
			}
			Some('P') | Some('X') | Some('^') | Some('_') => {
				chars.next();
				while let Some(next) = chars.next() {
					if next == '\u{1b}' {
						if chars.peek() == Some(&'\\') {
							chars.next();
						}
						break;
					}
				}
			}
			Some(_) => {
				chars.next();
			}
			None => {}
		}
	}
	result
}

// JS Date.prototype.toISOString()
fn iso_now() -> String {
	let since_epoch = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default();
	iso_from_millis(since_epoch.as_millis())
}

fn iso_from_millis(millis: u128) -> String {
	let seconds = millis / 1000;
	let fraction = millis % 1000;
	let days = (seconds / 86_400) as i64;
	let remainder = seconds % 86_400;
	let (year, month, day) = civil_from_days(days);
	format!(
		"{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{fraction:03}Z",
		remainder / 3600,
		(remainder % 3600) / 60,
		remainder % 60
	)
}

// Howard Hinnant days_from_civil의 역변환.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
	let shifted = days + 719_468;
	let era = if shifted >= 0 {
		shifted
	} else {
		shifted - 146_096
	} / 146_097;
	let day_of_era = shifted - era * 146_097;
	let year_of_era =
		(day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
	let year = year_of_era + era * 400;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_index = (5 * day_of_year + 2) / 153;
	let day = day_of_year - (153 * month_index + 2) / 5 + 1;
	let month = if month_index < 10 {
		month_index + 3
	} else {
		month_index - 9
	};
	(year + i64::from(month <= 2), month, day)
}

struct TaskScreen {
	root: PathBuf,
	file: Option<File>,
	address: String,
	phase: String,
	counts: Option<BatchCounts>,
	visible: bool,
	captcha: String,
	finished: bool,
	progress: u32,
	timing: TaskTiming,
	attempt: u32,
	outcomes: BTreeMap<u32, Outcome>,
	active: bool,
	path: PathBuf,
}

impl TaskScreen {
	fn new(root: &str) -> std::io::Result<Self> {
		let directory = Path::new(root).join("기타");
		fs::create_dir_all(root)?;
		let path = directory.join("작업로그.jsonl");
		Ok(Self {
			root: PathBuf::from(root),
			file: None,
			address: String::new(),
			phase: "준비".to_string(),
			counts: None,
			visible: false,
			captcha: String::new(),
			finished: false,
			progress: 0,
			timing: TaskTiming::new(),
			attempt: 0,
			outcomes: BTreeMap::new(),
			active: false,
			path,
		})
	}

	fn record(&mut self, event: &str, detail: &str) -> bool {
		if self.file.is_none() {
			let opened = (|| -> std::io::Result<File> {
				fs::create_dir_all(self.path.parent().ok_or_else(|| std::io::Error::other("로그 폴더가 없습니다."))?)?;
				let mut options = OpenOptions::new();
				options.append(true).create(true);
				#[cfg(unix)] {
					use std::os::unix::fs::OpenOptionsExt;
					options.mode(0o600);
				}
				options.open(&self.path)
			})();
			match opened {
				Ok(file) => self.file = Some(file),
				Err(error) => {
					output::write_err(&format!("작업 로그 저장 실패. 일반 출력으로 전환합니다: {error}\n"));
					return false;
				}
			}
		}
		let Some(mut file) = self.file.take() else {
			return false;
		};
		let mut payload = json!({
			"time": iso_now(),
			"event": event,
			"address": self.address,
			"detail": detail,
		});
		if let Some(counts) = self.counts {
			payload["counts"] = json!({
				"processed": counts.processed,
				"total": counts.total,
				"failed": counts.failed,
				"missing": counts.missing,
			});
		}
		let line = format!("{payload}\n");
		match file.write_all(line.as_bytes()) {
			Ok(()) => {
				self.file = Some(file);
				true
			}
			Err(error) => {
				self.active = false;
				output::write_err(&format!(
					"작업 로그 저장 실패. 일반 출력으로 전환합니다: {error}\n"
				));
				false
			}
		}
	}

	fn close_file(&mut self) {
		if let Some(file) = self.file.take() {
			if let Err(error) = file.sync_all() {
				output::write_err(&format!("로그 닫기 실패: {error}\n"));
			}
		}
	}

	fn begin(&mut self, address: &str) {
		self.pause();
		self.close_file();
		self.address = address.to_string();
		self.attempt += 1;
		let component: String = address.chars().map(|c| {
			if c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }
		}).collect();
		let component = component.trim_end_matches(['.', ' ']);
		let component = if component.is_empty() { "미확인주소" } else { component };
		self.path = self.root.join(component).join("기타").join("작업로그.jsonl");
		self.phase = "조회 준비".to_string();
		self.captcha.clear();
		self.finished = false;
		self.progress = 0;
		self.timing.begin();
		self.record("address.begin", "조회 준비");
	}

	fn set_counts(&mut self, counts: Option<BatchCounts>) {
		if counts.is_none() {
			self.outcomes.clear();
			self.timing.reset();
		}
		self.counts = counts;
	}

	fn advance(&mut self, completed: u64) {
		let value = completed.saturating_mul(25).min(100) as u32;
		self.progress = self.progress.max(value);
	}

	fn outcome(&mut self, result: Outcome, auto_mode: bool) {
		if !self.outcomes.contains_key(&self.attempt) {
			self.timing.complete(result == Outcome::Completed);
		}
		self.outcomes.insert(self.attempt, result);
		if matches!(result, Outcome::Completed | Outcome::Duplicate) {
			self.advance(4);
		}
		self.record("address.result", result.as_str());
		self.active = true;
		self.render(auto_mode);
	}

	fn show_captcha(&mut self, image: &str, auto_mode: bool) {
		self.captcha = image.trim_end().to_string();
		self.active = true;
		self.render(auto_mode);
		self.pause();
	}

	fn finish(&mut self, auto_mode: bool) {
		self.finished = true;
		self.timing.finish();
		self.active = true;
		self.render(auto_mode);
		self.pause();
	}

	fn status(&mut self, text: &str, auto_mode: bool) {
		let clean = strip_vt(text)
			.split_whitespace()
			.collect::<Vec<_>>()
			.join(" ");
		if !self.record("phase", &clean) {
			output::write(text);
			return;
		}
		self.phase = clean;
		self.active = true;
		self.render(auto_mode);
	}

	fn render(&mut self, auto_mode: bool) {
		if !self.active {
			return;
		}
		let config = term_env::config();
		let columns = if config.columns == 0 {
			80
		} else {
			config.columns
		};
		let rows = if config.rows == 0 { 30 } else { config.rows };
		let mode = if auto_mode { "AUTO · 시험 모드" } else { "일반" };
		let results: Vec<Outcome> = self.outcomes.values().copied().collect();
		let count_of = |wanted: Outcome| {
			results
				.iter()
				.filter(|outcome| **outcome == wanted)
				.count() as u64
		};
		let completed = count_of(Outcome::Completed);
		let failed = count_of(Outcome::Failed);
		let missing = count_of(Outcome::Missing);
		let duplicate = count_of(Outcome::Duplicate);
		let total = self.counts.map(|counts| counts.total).unwrap_or(1);
		let remaining = total.saturating_sub(results.len() as u64);
		let width = columns.saturating_sub(2).max(8);
		let filled = ((self.progress as usize) * 16) / 100;
		let counts = format!(
			"완료 {}건(신규{}건/중복{}건) 실패 {}건(지번없음{}건/오류{}건) 잔여 {}건",
			completed + duplicate,
			completed,
			duplicate,
			failed + missing,
			missing,
			failed,
			remaining
		);
		let timing = self.timing.label(remaining);
		let combined = format!("{counts} · {timing}");
		let phase = if clip(&self.phase, width.saturating_sub(13)) == self.phase {
			self.phase.as_str()
		} else if self.phase.contains("실패") || self.phase.contains("차단") {
			"처리 실패 (상세: 작업 로그)"
		} else if self.phase.contains("완료 건물 기록") || self.phase.contains("산출물 저장 완료") {
			"저장 완료"
		} else if self.phase.contains("현재 페이지") || self.phase.contains("Stage 1") {
			"주소 조회"
		} else if self.phase.contains("보안문자") {
			"보안문자 확인"
		} else if self.phase.contains("OZ") || self.phase.contains("뷰어") {
			"문서 준비"
		} else if self.phase.contains("Stage 3") {
			"문서 저장"
		} else {
			"처리 중 (상세: 작업 로그)"
		};
		let mut lines = vec![
			format!("KRAS · {mode} · 단계 기준"),
			format!("주소 : {}", self.address),
			format!(
				"진행 : [{}{}] {}%",
				"█".repeat(filled),
				"░".repeat(16 - filled),
				self.progress
			),
			format!("작업 단계 : {phase}"),
		];
		if clip(&combined, width) == combined {
			lines.push(combined);
		} else {
			lines.push(counts);
			lines.push(timing);
		}
		lines.push("보안문자".to_string());
		lines.push(if self.captcha.is_empty() {
			"(필요할 때 여기에 표시됩니다)".to_string()
		} else {
			self.captcha.clone()
		});
		let reserved = (rows.saturating_sub(3).max(8)).min(18);
		let prefix = if self.visible {
			"\u{1b}[u\u{1b}[J".to_string()
		} else {
			format!(
				"{}\u{1b}[{}A\r\u{1b}[s",
				"\n".repeat(reserved),
				reserved
			)
		};
		let left = (width - 6) / 2;
		let divider = format!(
			"{} 완료 {}",
			"═".repeat(left),
			"═".repeat(width - 6 - left)
		);
		let footer = if self.finished {
			format!("\n실제 걸린 시간 {}\n작업 로그: {}\n{divider}", self.timing.elapsed(), self.path.display())
		} else {
			String::new()
		};
		let content = lines
			.iter()
			.enumerate()
			.map(|(index, line)| {
				// 보안문자(Sixel DCS)는 중간에 잘리면 터미널이 깨지므로 통째로 둔다.
				if index == lines.len() - 1
					|| (!self.captcha.is_empty() && *line == self.captcha)
				{
					line.clone()
				} else {
					clip(line, width)
				}
			})
			.collect::<Vec<_>>()
			.join("\n")
			+ &footer;
		let (open, close) = if auto_mode {
			("\u{1b}[38;5;208m", "\u{1b}[0m")
		} else {
			("", "")
		};
		output::write(&format!("{prefix}{open}{content}{close}\n"));
		self.visible = true;
	}

	fn pause(&mut self) {
		self.active = false;
	}

	fn close(&mut self) {
		self.pause();
		self.close_file();
		if !self.finished {
			output::write(&format!("작업 로그: {}\n", self.path.display()));
		}
	}
}

struct ScreenState {
	screen: Option<Arc<Mutex<TaskScreen>>>,
	auto_mode: bool,
}

impl ScreenState {
	const fn new() -> Self {
		Self {
			screen: None,
			auto_mode: false,
		}
	}
}

static STATE: Mutex<ScreenState> = Mutex::new(ScreenState::new());

fn lock_state() -> std::sync::MutexGuard<'static, ScreenState> {
	STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn snapshot() -> (Option<Arc<Mutex<TaskScreen>>>, bool) {
	let state = lock_state();
	(state.screen.clone(), state.auto_mode)
}

fn lock_screen(screen: &Arc<Mutex<TaskScreen>>) -> std::sync::MutexGuard<'_, TaskScreen> {
	screen.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn open_task_screen(root: &str) {
	let config = term_env::config();
	if !config.stdout_tty || !config.stdin_tty || config.term.as_deref() == Some("dumb") {
		return;
	}
	match TaskScreen::new(root) {
		Ok(screen) => {
			lock_state().screen = Some(Arc::new(Mutex::new(screen)));
		}
		Err(error) => {
			output::write_err(&format!(
				"작업 로그를 열 수 없어 일반 출력으로 진행합니다: {error}\n"
			));
		}
	}
}

pub fn begin_task_address(address: &str) {
	let (screen, _) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).begin(address);
	}
}

pub fn set_task_auto_mode(enabled: bool) {
	lock_state().auto_mode = enabled;
	crate::output::set_auto_color(enabled);
}

pub fn set_task_counts(counts: Option<BatchCounts>) {
	let (screen, _) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).set_counts(counts);
	}
}

pub fn advance_task_progress(completed: u64) {
	let (screen, _) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).advance(completed);
	}
}

pub fn pause_task_screen() {
	let (screen, _) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).pause();
	}
}

pub fn finish_task_address(result: Outcome) {
	let (screen, auto_mode) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).outcome(result, auto_mode);
	}
}

pub fn finish_task_screen() {
	let (screen, auto_mode) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).finish(auto_mode);
	}
}

pub fn task_capture_completed() {
	let (screen, auto_mode) = snapshot();
	if !auto_mode {
		if let Some(screen) = screen {
			lock_screen(&screen).outcome(Outcome::Completed, auto_mode);
		}
	}
}

pub fn show_task_captcha(image: &str) {
	let (screen, auto_mode) = snapshot();
	match screen {
		Some(screen) => lock_screen(&screen).show_captcha(image, auto_mode),
		None => output::write(&format!("\n보안문자 이미지:\n{image}\n")),
	}
}

/// 고정줄 화면이 열려 있을 때만 단계 텍스트를 반영한다.
/// (파이프 실행에서는 아무것도 출력하지 않는다 — 자동화 로그를 건드리지 않는다.)
pub fn update_task_phase(text: &str) {
	let (screen, auto_mode) = snapshot();
	if let Some(screen) = screen {
		lock_screen(&screen).status(text, auto_mode);
	}
}

/// 고정줄 진행 UI가 지금 열려 있는지(Sixel 캡차 표시 여부 판단용).
pub fn task_screen_is_open() -> bool {
	snapshot().0.is_some()
}

pub fn task_status(text: &str) {
	let (screen, auto_mode) = snapshot();
	match screen {
		Some(screen) => lock_screen(&screen).status(text, auto_mode),
		None => output::write(text),
	}
}

pub fn task_summary(text: &str) {
	let (screen, _) = snapshot();
	if screen.is_none() {
		output::write(text);
	}
}

pub fn close_task_screen() {
	let (screen, _) = snapshot();
	lock_state().screen = None;
	if let Some(screen) = screen {
		lock_screen(&screen).close();
	}
}

pub trait Terminal {
	fn question(&self, prompt: &str) -> String;
	fn close(&self);
}

pub struct TaskTerminal<T: Terminal> {
	inner: T,
}

impl<T: Terminal> TaskTerminal<T> {
	pub fn question(&self, prompt: &str) -> String {
		pause_task_screen();
		let (screen, _) = snapshot();
		let started = Instant::now();
		let answer = self.inner.question(prompt);
		if let Some(screen) = screen {
			lock_screen(&screen)
				.timing
				.exclude_waiting(started.elapsed().as_secs_f64() * 1000.0);
		}
		answer
	}

	pub fn close(&self) {
		self.inner.close();
	}
}

pub fn task_terminal<T: Terminal>(inner: T) -> TaskTerminal<T> {
	TaskTerminal { inner }
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::{install_buffer, restore_real, take_buffer};
	use crate::term_env::TermConfig;
	use crate::test_serial;
	use std::sync::MutexGuard;

	struct TestEnv {
		_serial: MutexGuard<'static, ()>,
	}

	impl TestEnv {
		fn new(tty: bool) -> Self {
			let serial = test_serial::lock();
			install_buffer();
			term_env::set_config(Some(TermConfig {
				stdout_tty: tty,
				stdin_tty: tty,
				stderr_tty: tty,
				term: None,
				columns: 80,
				rows: 30,
			}));
			Self { _serial: serial }
		}
	}

	impl Drop for TestEnv {
		fn drop(&mut self) {
			close_task_screen();
			set_task_auto_mode(false);
			term_env::set_config(None);
			restore_real();
		}
	}

	fn temp_root(tag: &str) -> PathBuf {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or_default()
			.as_nanos();
		let root = std::env::temp_dir().join(format!(
			"kras-screen-test-{tag}-{}-{nanos}",
			std::process::id()
		));
		fs::create_dir_all(&root).unwrap();
		root
	}

	fn cleanup(root: &Path) {
		let _ = fs::remove_dir_all(root);
	}

	fn last_frame(output: &str) -> &str {
		output.rsplit("\u{1b}[u\u{1b}[J").next().unwrap_or("")
	}

	#[test]
	fn long_phase_uses_short_label_below_progress_bar() {
		let env = TestEnv::new(true);
		let root = temp_root("short-phase");
		open_task_screen(&root.display().to_string());
		update_task_phase(&format!("현재 페이지: https://www.kras.go.kr/{}", "long/".repeat(30)));
		let rendered = take_buffer();
		let frame = last_frame(&rendered);
		assert!(frame.contains("작업 단계 : 주소 조회"));
		assert!(frame.contains("진행 : ["));
		assert!(!frame.contains("현재 페이지:"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn stage3_error_detail_is_not_truncated_in_disk_log() {
		let env = TestEnv::new(true);
		let root = temp_root("stage3-error");
		open_task_screen(&root.display().to_string());
		let path = lock_screen(&snapshot().0.unwrap()).path.clone();
		let detail = format!("Stage 3 실패: 기존 산출물 보존: C:/{}-existing.pdf", "long-path/".repeat(20));
		update_task_phase(&detail);
		close_task_screen();
		let text = fs::read_to_string(path).unwrap();
		let recorded = text.lines().any(|line| {
			let value: serde_json::Value = serde_json::from_str(line).unwrap();
			value["detail"].as_str() == Some(detail.as_str())
		});
		drop(env);
		cleanup(&root);
		assert!(recorded);
	}

	#[test]
	fn address_logs_stay_in_each_parcel_directory() {
		let env = TestEnv::new(true);
		let root = temp_root("parcel-logs");
		open_task_screen(&root.display().to_string());
		begin_task_address("서울특별시 중구 예장동 7");
		let first = lock_screen(&snapshot().0.unwrap()).path.clone();
		begin_task_address("서울특별시 노원구 월계동 392-19");
		let second = lock_screen(&snapshot().0.unwrap()).path.clone();
		close_task_screen();
		assert!(first.starts_with(root.join("서울특별시 중구 예장동 7").join("기타")));
		assert!(second.starts_with(root.join("서울특별시 노원구 월계동 392-19").join("기타")));
		assert!(fs::read_to_string(&first).unwrap().contains("예장동 7"));
		assert!(fs::read_to_string(&second).unwrap().contains("월계동 392-19"));
		assert!(!root.join("작업기록").exists());
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn final_log_path_precedes_completion_divider_without_close_duplicate() {
		let env = TestEnv::new(true);
		let root = temp_root("log-before-divider");
		open_task_screen(&root.display().to_string());
		finish_task_screen();
		let rendered = take_buffer();
		let frame = last_frame(&rendered);
		assert!(frame.find("작업 로그:").unwrap() < frame.rfind("═").unwrap());
		assert_eq!(frame.matches("작업 로그:").count(), 1);
		close_task_screen();
		assert!(!take_buffer().contains("작업 로그:"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn log_path_is_printed_only_once_at_close() {
		let env = TestEnv::new(true);
		let root = temp_root("log-once");
		open_task_screen(&root.display().to_string());
		let opened = take_buffer();
		close_task_screen();
		close_task_screen();
		let closed = take_buffer();
		drop(env);
		cleanup(&root);
		assert!(!opened.contains("작업 단계 로그:"));
		assert!(!opened.contains("작업 로그:"));
		assert_eq!(closed.matches("작업 로그:").count(), 1);
	}

	#[test]
	fn redirected_output_unchanged_no_log() {
		let env = TestEnv::new(false);
		let root = temp_root("redirect");
		open_task_screen(&root.display().to_string());
		task_status("stage fixture\n");
		let output = take_buffer();
		assert_eq!(output, "stage fixture\n");
		let entries = fs::read_dir(&root).unwrap();
		assert_eq!(entries.count(), 0);
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn auto_status_orange_log_persists_suspends_input() {
		let env = TestEnv::new(true);
		let root = temp_root("auto");
		open_task_screen(&root.display().to_string());
		begin_task_address("fixture-address");
		set_task_auto_mode(true);
		set_task_counts(Some(BatchCounts {
			processed: 2,
			total: 5,
			failed: 1,
			missing: 0,
		}));
		task_status("capture fixture\n");
		let rendered = take_buffer();

		struct FakeTerminal {
			suspended: std::cell::Cell<bool>,
		}
		impl Terminal for FakeTerminal {
			fn question(&self, _prompt: &str) -> String {
				let current = take_buffer();
				self.suspended.set(current.is_empty());
				"secret-answer".to_string()
			}
			fn close(&self) {}
		}
		let terminal = task_terminal(FakeTerminal {
			suspended: std::cell::Cell::new(false),
		});
		assert_eq!(terminal.question("fixture prompt"), "secret-answer");
		let suspended = terminal.inner.suspended.get();
		close_task_screen();

		assert!(rendered.contains("\u{1b}[38;5;208m"));
		assert!(rendered.contains("\u{1b}[s"));
		assert!(suspended);

		let directory = root.join("fixture-address").join("기타");
		let names: Vec<_> = fs::read_dir(&directory)
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert_eq!(names.len(), 1);
		let content = fs::read_to_string(directory.join(&names[0])).unwrap();
		let rows: Vec<serde_json::Value> = content
			.trim()
			.split('\n')
			.map(|line| serde_json::from_str(line).unwrap())
			.collect();
		assert_eq!(rows.len(), 2);
		assert!(content.contains("capture fixture"));
		assert!(!content.contains("secret-answer") && !content.contains("fixture prompt"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn repeated_status_updates_replace_frame_and_keep_disk_log() {
		let env = TestEnv::new(true);
		let root = temp_root("repeated-status");
		open_task_screen(&root.display().to_string());
		begin_task_address("fixture-address");
		task_status("PDF capture started\n");
		let first = take_buffer();
		task_status("PDF capture finished\n");
		let second = take_buffer();
		assert!(first.contains("PDF capture started"));
		assert!(second.contains("PDF capture finished"));
		assert!(!second.contains("PDF capture started"));
		assert!(second.contains('\u{1b}'), "제자리 갱신 제어 코드 필요");
		close_task_screen();
		let entry = fs::read_dir(root.join("fixture-address").join("기타")).unwrap().next().unwrap().unwrap();
		let log = fs::read_to_string(entry.path()).unwrap();
		assert!(log.contains("PDF capture started"));
		assert!(log.contains("PDF capture finished"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn captcha_region_replaced_final_marker_below() {
		let env = TestEnv::new(true);
		let root = temp_root("captcha");
		open_task_screen(&root.display().to_string());
		begin_task_address("fixture-address");
		task_status("lookup");
		show_task_captcha("OLD_IMAGE");
		show_task_captcha("NEW_IMAGE");
		finish_task_address(Outcome::Completed);
		finish_task_screen();
		let output = take_buffer();
		let frame = last_frame(&output);
		assert!(frame.contains("NEW_IMAGE") && !frame.contains("OLD_IMAGE"));
		let lines: Vec<&str> = frame.split('\n').collect();
		let elapsed = lines
			.iter()
			.position(|line| line.starts_with("실제 걸린 시간"))
			.unwrap();
		let divider = lines
			.iter()
			.position(|line| {
				line.starts_with('═') && line.contains(" 완료 ") && line.ends_with('═')
			})
			.unwrap();
		assert!(lines[elapsed + 1].starts_with("작업 로그:"));
		assert_eq!(divider, elapsed + 2);
		assert_eq!(lines[divider].chars().count() + 2, 80 - 2);
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn oversized_captcha_intact_with_footer() {
		let env = TestEnv::new(true);
		let root = temp_root("oversize");
		open_task_screen(&root.display().to_string());
		begin_task_address("fixture-address");
		task_status("lookup");
		let captcha = format!(
			"\u{1b}P0;0;8q\"1;1;320;160{}\u{1b}\\",
			"A".repeat(400)
		);
		show_task_captcha(&captcha);
		finish_task_address(Outcome::Completed);
		finish_task_screen();
		let output = take_buffer();
		let frame = last_frame(&output);
		assert!(frame.contains(&captcha));
		assert!(
			frame.find(&captcha).unwrap() < frame.find("실제 걸린 시간").unwrap()
		);
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn repeated_addresses_counted_separately() {
		let env = TestEnv::new(true);
		let root = temp_root("repeat");
		open_task_screen(&root.display().to_string());
		set_task_counts(Some(BatchCounts {
			processed: 0,
			total: 2,
			failed: 0,
			missing: 0,
		}));
		begin_task_address("same-address");
		finish_task_address(Outcome::Completed);
		begin_task_address("same-address");
		finish_task_address(Outcome::Completed);
		finish_task_screen();
		let output = take_buffer();
		let frame = last_frame(&output);
		assert!(frame.contains("완료 2건"));
		assert!(frame.contains("잔여 0건"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn progress_held_on_error_reset_for_next_address() {
		let env = TestEnv::new(true);
		let root = temp_root("progress");
		open_task_screen(&root.display().to_string());
		begin_task_address("failed-entry");
		advance_task_progress(2);
		advance_task_progress(1);
		finish_task_address(Outcome::Failed);
		finish_task_screen();
		let output = take_buffer();
		let mut frame = last_frame(&output).to_string();
		assert!(frame.contains("50%"));
		assert!(!frame.contains("100%"));
		begin_task_address("next-entry");
		task_status("lookup");
		let output = take_buffer();
		frame = last_frame(&output).to_string();
		assert!(frame.contains("] 0%"));
		drop(env);
		cleanup(&root);
	}

	#[test]
	fn counts_reconcile_duplicate_missing_failed() {
		let env = TestEnv::new(true);
		let root = temp_root("reconcile");
		open_task_screen(&root.display().to_string());
		set_task_counts(Some(BatchCounts {
			processed: 0,
			total: 4,
			failed: 0,
			missing: 0,
		}));
		begin_task_address("first");
		finish_task_address(Outcome::Completed);
		finish_task_address(Outcome::Completed);
		for result in [
			Outcome::Duplicate,
			Outcome::Missing,
			Outcome::Failed,
		] {
			begin_task_address(result.as_str());
			finish_task_address(result);
		}
		finish_task_screen();
		let output = take_buffer();
		let frame = last_frame(&output);
		assert!(frame.contains(
			"완료 2건(신규1건/중복1건) 실패 2건(지번없음1건/오류1건) 잔여 0건"
		));
		drop(env);
		cleanup(&root);
	}
}
