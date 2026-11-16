// TS scripts/kras-task-timing.ts — 필지당 평균·남은 예상시간·실제 걸린 시간 계약.
// 시계는 주입한다(테스트는 가상 시계, 실전은 monotonic Instant).

pub struct TaskTiming {
	clock: Box<dyn Fn() -> f64 + Send>,
	started: Option<f64>,
	waiting: f64,
	total: f64,
	samples: u32,
	batch_started: Option<f64>,
	ended: Option<f64>,
}

fn monotonic_clock() -> Box<dyn Fn() -> f64 + Send> {
	let origin = std::time::Instant::now();
	Box::new(move || origin.elapsed().as_secs_f64() * 1000.0)
}

// JS: Math.max(0, Math.round(ms / 1000)) → HH:MM:SS(pad 2).
fn duration(milliseconds: f64) -> String {
	let seconds = (milliseconds / 1000.0).round().max(0.0) as u64;
	format!(
		"{:02}:{:02}:{:02}",
		seconds / 3600,
		(seconds / 60) % 60,
		seconds % 60
	)
}

impl Default for TaskTiming {
	fn default() -> Self {
		Self::new()
	}
}

impl TaskTiming {
	pub fn new() -> Self {
		Self::with_clock(monotonic_clock())
	}

	pub fn with_clock(clock: Box<dyn Fn() -> f64 + Send>) -> Self {
		Self {
			clock,
			started: None,
			waiting: 0.0,
			total: 0.0,
			samples: 0,
			batch_started: None,
			ended: None,
		}
	}

	pub fn begin(&mut self) {
		let now = (self.clock)();
		self.started = Some(now);
		if self.batch_started.is_none() {
			self.batch_started = Some(now);
		}
		self.waiting = 0.0;
	}

	pub fn reset(&mut self) {
		self.total = 0.0;
		self.samples = 0;
		self.batch_started = Some((self.clock)());
		self.ended = None;
	}

	pub fn finish(&mut self) {
		self.ended = Some((self.clock)());
	}

	pub fn elapsed(&self) -> String {
		duration(match self.batch_started {
			None => 0.0,
			Some(start) => self.ended.unwrap_or_else(|| (self.clock)()) - start,
		})
	}

	pub fn exclude_waiting(&mut self, milliseconds: f64) {
		if self.started.is_some() {
			self.waiting += milliseconds;
		}
	}

	pub fn complete(&mut self, success: bool) {
		if let Some(start) = self.started {
			if success {
				self.total += ((self.clock)() - start - self.waiting).max(0.0);
				self.samples += 1;
			}
		}
		self.started = None;
	}

	// `필지당 평균 … / 남은 예상시간 …` 한 줄.
	pub fn label(&self, remaining: u64) -> String {
		let mean = if self.samples == 0 {
			None
		} else {
			Some(self.total / f64::from(self.samples))
		};
		let elapsed = match self.started {
			None => 0.0,
			Some(start) => ((self.clock)() - start - self.waiting).max(0.0),
		};
		let estimate = if remaining == 0 {
			"00:00:00".to_string()
		} else {
			match mean {
				None => "계산 중".to_string(),
				Some(mean) => duration((mean * remaining as f64 - elapsed).max(0.0)),
			}
		};
		format!(
			"필지당 평균 {} / 남은 예상시간 {}",
			mean.map(duration).unwrap_or_else(|| "계산 중".to_string()),
			estimate
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::AtomicU64;
	use std::sync::atomic::Ordering;
	use std::sync::Arc;

	// 가상 시계: 테스트가 값을 바꿔쓰는 공유 셀(Send 필요).
	fn fake_clock() -> (Arc<AtomicU64>, Box<dyn Fn() -> f64 + Send>) {
		let now = Arc::new(AtomicU64::new(0));
		let shared = Arc::clone(&now);
		(
			now,
			Box::new(move || shared.load(Ordering::SeqCst) as f64),
		)
	}

	#[test]
	fn estimates_from_real_success_excluding_manual_waits() {
		// 15초 중 5초는 입력 대기였던 항목.
		let (now, clock) = fake_clock();
		let mut timing = TaskTiming::with_clock(clock);
		timing.begin();
		now.store(15_000, Ordering::SeqCst);
		timing.exclude_waiting(5_000.0);
		timing.complete(true);
		// 평균 10초, 잔여 2건은 20초.
		let label = timing.label(2);
		assert!(label.contains("00:00:10"), "{label}");
		assert!(label.contains("00:00:20"), "{label}");
	}

	#[test]
	fn excludes_cached_and_failed_and_subtracts_current_processing() {
		// 10초 성공 항목 뒤에 캐시 1건.
		let (now, clock) = fake_clock();
		let mut timing = TaskTiming::with_clock(clock);
		timing.begin();
		now.store(10_000, Ordering::SeqCst);
		timing.complete(true);
		timing.begin();
		now.store(11_000, Ordering::SeqCst);
		timing.complete(false);
		// 다음 실제 항목이 3초 처리 중.
		timing.begin();
		now.store(14_000, Ordering::SeqCst);
		// 캐시는 평균을 오염시키지 않고 잔여 시간은 10*2-3=17초.
		let label = timing.label(2);
		assert!(label.contains("00:00:10"), "{label}");
		assert!(label.contains("00:00:17"), "{label}");
	}

	#[test]
	fn resets_batch_without_inventing_a_mean() {
		// 빰 배치: 평균을 지어내지 않고 잔여 0은 00:00:00.
		let (_now, clock) = fake_clock();
		let mut timing = TaskTiming::with_clock(clock);
		timing.reset();
		let label = timing.label(0);
		assert!(label.contains("계산 중"), "{label}");
		assert!(label.contains("00:00:00"), "{label}");
	}

	#[test]
	fn freezes_actual_duration_including_waits_at_completion() {
		// 5초 입력 대기가 포함된 배치.
		let (now, clock) = fake_clock();
		let mut timing = TaskTiming::with_clock(clock);
		timing.reset();
		timing.begin();
		now.store(15_000, Ordering::SeqCst);
		timing.exclude_waiting(5_000.0);
		timing.complete(true);
		// 배치가 끝난 뒤 시간이 흘러도 실제 걸린 시간은 배치 경계에서 멈춘다.
		now.store(18_000, Ordering::SeqCst);
		timing.finish();
		now.store(99_000, Ordering::SeqCst);
		assert_eq!(timing.elapsed(), "00:00:18");
	}
}
