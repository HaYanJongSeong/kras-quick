// TS scripts/scp-usage-watcher.ts — SCP 현장조사 사용량 큐(재산번호/활용방안/종합의견/특기사항)를
// CDP로 반영하는 독립 도구. 큐·커서·로그 파일 규약과 미리보기/적용 모드를 그대로 따른다.
// TS는 playwright locator를 쓰고 여기는 같은 조작을 페이지 주입 JS로 수행한다.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// TS PLACEHOLDER — SCP 검색 입력창의 정확한 placeholder.
pub const SCP_USAGE_PLACEHOLDER: &str = "키워드(재산명/주소/재산번호 등)";
/// TS config.poll_ms 기본값.
pub const SCP_USAGE_DEFAULT_POLL_MS: u64 = 1500;
/// TS CDP endpoint(9222) — property watcher runtime과 같은 브라우저.
pub const SCP_USAGE_CDP_ENDPOINT: &str = "http://127.0.0.1:9222";
/// TS page.setDefaultTimeout(8000)
pub const SCP_USAGE_STEP_TIMEOUT_MS: u64 = 8000;
/// TS 도구 사용법 안내.
pub const SCP_USAGE: &str = "Usage: kras-quick scp-usage watch --config PATH [--commit] [--once]";

/// TS Event — 큐 파일 한 줄의 사용량 이벤트.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct UsageEvent {
	pub property_number: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub jibun: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub pnu: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub usage: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub opinion: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub special: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub usage_changed: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub opinion_changed: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub special_changed: Option<bool>,
}

/// TS Result
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ScpUsageResult {
	pub property_number: String,
	pub status: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub detail: Option<String>,
}

/// TS Config — config 파일의 상대 경로는 config 위치 기준으로 해석한다.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageConfig {
	pub queue_file: PathBuf,
	pub cursor_file: PathBuf,
	pub log_file: PathBuf,
	pub poll_ms: u64,
}

#[derive(Debug, serde::Deserialize)]
struct UsageConfigFile {
	queue_file: String,
	cursor_file: String,
	log_file: String,
	#[serde(default)]
	poll_ms: Option<u64>,
}

/// TS loadConfig — 상대 경로를 config 파일 폴더 기준으로 풀어 절대 경로처럼 쓴다.
pub fn load_config(path: &Path) -> Result<UsageConfig, Box<dyn std::error::Error>> {
	let text = std::fs::read_to_string(path)?;
	let file: UsageConfigFile = serde_json::from_str(&text)?;
	// canonicalize는 Windows에서 \\?\ 접두어를 붙이므로 부모 폴더를 그대로 쓴다(TS path.dirname과 동일)
	let base = path
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(Path::to_path_buf)
		.unwrap_or_else(|| PathBuf::from("."));
	Ok(UsageConfig {
		queue_file: resolve_path(&base, &file.queue_file),
		cursor_file: resolve_path(&base, &file.cursor_file),
		log_file: resolve_path(&base, &file.log_file),
		poll_ms: file.poll_ms.unwrap_or(SCP_USAGE_DEFAULT_POLL_MS),
	})
}

/// TS path.resolve(base, value) — 절대 경로는 그대로, 상대 경로는 base 아래로 붙인다.
fn resolve_path(base: &Path, value: &str) -> PathBuf {
	let candidate = PathBuf::from(value);
	if candidate.is_absolute() {
		candidate
	} else {
		base.join(candidate)
	}
}

/// TS cursor — 기록이 없거나 깨졌으면 0에서 시작한다.
pub fn read_cursor(path: &Path) -> u64 {
	std::fs::read_to_string(path)
		.ok()
		.and_then(|text| serde_json::from_str::<Value>(&text).ok())
		.and_then(|value| value.get("offset").and_then(Value::as_u64))
		.unwrap_or(0)
}

/// TS saveCursor — 임시 파일에 쓴 뒤 원자적으로 교체한다.
pub fn save_cursor(path: &Path, offset: u64) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	let temp = PathBuf::from(format!("{}.tmp", path.display()));
	std::fs::write(&temp, serde_json::to_string(&serde_json::json!({ "offset": offset }))?)?;
	std::fs::rename(&temp, path)?;
	Ok(())
}

/// TS readEvents의 항목 — 이벤트와 그 줄이 끝나는 바이트 위치.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedEvent {
	pub event: UsageEvent,
	pub end: usize,
}

/// TS readEvents — 커서 위치부터 완성된 줄만 읽고, 깨진 줄은 건너뛴다.
/// `end`는 TS와 동일하게 (줄 길이 + 개행 1바이트)를 누적한 값이다.
pub fn read_events(config: &UsageConfig) -> Vec<QueuedEvent> {
	let Ok(bytes) = std::fs::read(&config.queue_file) else {
		return Vec::new();
	};
	let mut offset = read_cursor(&config.cursor_file);
	if offset > bytes.len() as u64 {
		offset = 0;
	}
	let offset = offset as usize;
	let remainder = String::from_utf8_lossy(&bytes[offset..]).into_owned();
	let mut position = offset;
	let mut events = Vec::new();
	for line in remainder.split('\n') {
		position += line.len() + 1;
		if line.trim().is_empty() {
			continue;
		}
		let Ok(value) = serde_json::from_str::<Value>(line) else {
			continue;
		};
		let Some(property_number) = value
			.get("property_number")
			.and_then(Value::as_str)
			.map(str::trim)
		else {
			continue;
		};
		if property_number.is_empty() {
			continue;
		}
		let has_value = ["usage", "opinion", "special"].iter().any(|key| {
			value.get(*key).and_then(Value::as_str).is_some()
		});
		if !has_value {
			continue;
		}
		let Ok(event) = serde_json::from_value::<UsageEvent>(value) else {
			continue;
		};
		events.push(QueuedEvent { event, end: position });
	}
	events
}

/// TS update의 변경 판정부 — 페이지 안에서 값을 읽어온 뒤 호출한다.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangePlan {
	pub special: bool,
	pub usage: bool,
	pub opinion: bool,
	pub detail: Option<String>,
}

impl ChangePlan {
	pub fn any(&self) -> bool {
		self.special || self.usage || self.opinion
	}
}

/// TS update의 변경 규칙 — special/opinion은 *_changed가 true일 때만,
/// usage는 *_changed가 false가 아니면 항상 반영한다.
pub fn plan_changes(
	event: &UsageEvent,
	before_special: &str,
	before_usage: &str,
	before_opinion: &str,
) -> ChangePlan {
	let update_special =
		event.special.is_some() && event.special_changed.unwrap_or(false);
	let update_usage = event.usage.is_some() && event.usage_changed != Some(false);
	let update_opinion =
		event.opinion.is_some() && event.opinion_changed.unwrap_or(false);
	let special_changed =
		update_special && before_special != event.special.as_deref().unwrap_or("");
	let usage_changed = update_usage && before_usage != event.usage.as_deref().unwrap_or("");
	let opinion_changed =
		update_opinion && before_opinion != event.opinion.as_deref().unwrap_or("");
	let detail = [
		special_changed.then(|| {
			format!(
				"{} -> {}",
				before_special,
				event.special.as_deref().unwrap_or("")
			)
		}),
		usage_changed.then(|| {
			format!("{} -> {}", before_usage, event.usage.as_deref().unwrap_or(""))
		}),
		opinion_changed.then(|| {
			format!(
				"{} -> {}",
				before_opinion,
				event.opinion.as_deref().unwrap_or("")
			)
		}),
	]
	.into_iter()
	.flatten()
	.collect::<Vec<String>>()
	.join("; ");
	ChangePlan {
		special: special_changed,
		usage: usage_changed,
		opinion: opinion_changed,
		detail: (!detail.is_empty()).then_some(detail),
	}
}

/// TS log — 로그 줄은 { ...result, ts } 순서를 지킨다.
pub fn append_result_log(
	config: &UsageConfig,
	result: &ScpUsageResult,
) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = config.log_file.parent() {
		std::fs::create_dir_all(parent)?;
	}
	let mut line = serde_json::json!({
		"property_number": result.property_number,
		"status": result.status,
	});
	if let Some(detail) = &result.detail {
		line["detail"] = Value::String(detail.clone());
	}
	line["ts"] = Value::String(iso_utc_now());
	let mut file = std::fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(&config.log_file)?;
	writeln!(file, "{line}")?;
	Ok(())
}

/// ISO 8601 UTC 타임스탬프(밀리초 포함) — TS new Date().toISOString() 대응.
pub fn iso_utc_now() -> String {
	let now = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap_or_default();
	let total = now.as_secs();
	let millis = now.subsec_millis();
	let days = (total / 86_400) as i64;
	let seconds_of_day = total % 86_400;
	let (hour, minute, second) = (
		seconds_of_day / 3600,
		(seconds_of_day % 3600) / 60,
		seconds_of_day % 60,
	);
	let (year, month, day) = civil_from_days(days);
	format!(
		"{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
	)
}

/// Howard Hinnant의 civil_from_days — 1970-01-01 기준 일수를 캘린더 날짜로.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
	let shifted = days + 719_468;
	let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
	let day_of_era = shifted - era * 146_097;
	let year_of_era =
		(day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
	let year = year_of_era + era * 400;
	let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
	let month_prime = (5 * day_of_year + 2) / 153;
	let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
	let month = (if month_prime < 10 { month_prime + 3 } else { month_prime - 9 }) as u32;
	(if month <= 2 { year + 1 } else { year }, month, day)
}

/// TS update가 페이지에 넘기는 요청.
pub struct UpdateRequest<'a> {
	pub event: &'a UsageEvent,
	pub commit: bool,
}

/// TS update의 페이지 조작부 — 주입 JS가 담당한다(결과는 JSON으로 돌려받는다).
const SCP_USAGE_UPDATE_JS: &str = r##"async (request) => {
	const { propertyNumber, special, usage, opinion, specialChanged, usageChanged, opinionChanged, commit, placeholder } = request;
	const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
	const buttons = () => Array.from(document.querySelectorAll("button"));
	const clickText = (text) => {
		const button = buttons().find((item) => (item.innerText || "").trim() === text);
		if (button !== undefined) button.click();
		return button !== undefined;
	};
	const rowByProperty = () =>
		Array.from(document.querySelectorAll("tr")).find((item) =>
			(item.innerText || "").split("\t").some((value) => value.trim() === propertyNumber),
		);
	const waitFor = async (probe, timeoutMs) => {
		const deadline = Date.now() + timeoutMs;
		while (Date.now() < deadline) {
			const value = probe();
			if (value) return value;
			await sleep(200);
		}
		return null;
	};
	const fill = (input, value) => {
		input.value = value;
		input.dispatchEvent(new Event("input", { bubbles: true }));
		input.dispatchEvent(new Event("change", { bubbles: true }));
	};
	const before = (value) => (value ?? "");
	const updateSpecial = special !== undefined && specialChanged === true;
	const updateUsage = usage !== undefined && usageChanged !== false;
	const updateOpinion = opinion !== undefined && opinionChanged === true;
	try {
		clickText("닫기");
		await sleep(300);
		const search = Array.from(document.querySelectorAll("input")).find(
			(item) => (item.getAttribute("placeholder") || "") === placeholder,
		);
		if (search === undefined) return { error: "SCP search field not found" };
		fill(search, propertyNumber);
		clickText("검색");
		await sleep(900);
		const row = await waitFor(rowByProperty, 8000);
		if (row === null) return { error: `SCP row not found: ${propertyNumber}` };
		row.click();
		await sleep(700);
		clickText("현장조사수정");
		await sleep(700);
		const controls = await waitFor(() => {
			const list = Array.from(document.querySelectorAll("input.cell-input"));
			return list.length >= 3 ? list : null;
		}, 8000);
		if (controls === null) return { error: "SCP edit controls not found" };
		const specialTarget = controls[controls.length - 2];
		const usageTarget = controls[controls.length - 3];
		const opinionTarget = controls[controls.length - 1];
		const beforeSpecial = before(specialTarget.value);
		const beforeUsage = before(usageTarget.value);
		const beforeOpinion = before(opinionTarget.value);
		const specialDiff = updateSpecial && beforeSpecial !== special;
		const usageDiff = updateUsage && beforeUsage !== usage;
		const opinionDiff = updateOpinion && beforeOpinion !== opinion;
		if (!specialDiff && !usageDiff && !opinionDiff) {
			clickText("닫기");
			return { status: "unchanged" };
		}
		const detail = [
			specialDiff ? `${beforeSpecial} -> ${special}` : undefined,
			usageDiff ? `${beforeUsage} -> ${usage}` : undefined,
			opinionDiff ? `${beforeOpinion} -> ${opinion}` : undefined,
		]
			.filter((value) => value !== undefined)
			.join("; ");
		if (!commit) {
			clickText("취소");
			await sleep(250);
			clickText("닫기");
			return { status: "preview", detail };
		}
		if (specialDiff) fill(specialTarget, special);
		if (usageDiff) fill(usageTarget, usage);
		if (opinionDiff) fill(opinionTarget, opinion);
		clickText("저장");
		await sleep(1500);
		clickText("확인");
		await sleep(400);
		clickText("닫기");
		await sleep(400);
		const reopened = rowByProperty();
		if (reopened !== undefined) reopened.click();
		await sleep(700);
		const body = (document.querySelector("body")?.innerText ?? "");
		const verified =
			(!specialDiff || body.includes(`특기사항\t${special}`)) &&
			(!usageDiff || body.includes(`활용방안\t${usage}`)) &&
			(!opinionDiff || body.includes(`종합의견\t${opinion}`));
		clickText("닫기");
		if (!verified) return { error: `SCP value verification failed: ${propertyNumber}` };
		return { status: "saved", detail };
	} catch (error) {
		return { error: String(error?.message ?? error) };
	}
}"##;

/// TS update — SCP /board 페이지에서 한 건의 사용량을 미리보기/적용한다.
pub fn update_property_usage(
	surface: &crate::cdp_surface::CdpSurface,
	request: &UpdateRequest<'_>,
) -> Result<ScpUsageResult, String> {
	let event = request.event;
	let payload = serde_json::json!({
		"propertyNumber": event.property_number,
		"special": event.special,
		"usage": event.usage,
		"opinion": event.opinion,
		"specialChanged": event.special_changed,
		"usageChanged": event.usage_changed,
		"opinionChanged": event.opinion_changed,
		"commit": request.commit,
		"placeholder": SCP_USAGE_PLACEHOLDER,
	});
	let expression = format!("({})({})", SCP_USAGE_UPDATE_JS, payload);
	let params = serde_json::json!({
		"expression": expression,
		"returnByValue": true,
		"awaitPromise": true,
	});
	let value = surface
		.send_cdp("Runtime.evaluate", params)
		.map_err(|error| error.to_string())?
		.get("result")
		.cloned()
		.and_then(|value| value.get("value").cloned())
		.unwrap_or(Value::Null);
	if let Some(error) = value.get("error").and_then(Value::as_str) {
		return Err(error.to_string());
	}
	let status = value
		.get("status")
		.and_then(Value::as_str)
		.unwrap_or("failed")
		.to_string();
	let detail = value
		.get("detail")
		.and_then(Value::as_str)
		.map(str::to_string);
	Ok(ScpUsageResult { property_number: event.property_number.clone(), status, detail })
}

/// TS findScpPage — /board 경로의 SCP 페이지가 정확히 하나여야 한다.
pub fn find_scp_board_page(
	pages: &[crate::property_watcher_runtime::CdpTargetPage],
) -> Result<usize, String> {
	let matches: Vec<usize> = pages
		.iter()
		.enumerate()
		.filter(|(_, page)| {
			url_pathname(&page.url).as_deref() == Some("/board")
		})
		.map(|(index, _)| index)
		.collect();
	match matches.as_slice() {
		[index] => Ok(*index),
		other => Err(format!(
			"expected one SCP /board page, found {}",
			other.len()
		)),
	}
}

/// URL pathname만 뽑는다(TS new URL(page.url()).pathname).
fn url_pathname(value: &str) -> Option<String> {
	let rest = value.split_once("//").map(|(_, rest)| rest)?;
	let after_scheme = rest.split_once('/').map(|(_, path)| path)?;
	let path = format!("/{after_scheme}");
	Some(path.split(['?', '#']).next().unwrap_or(&path).to_string())
}

/// TS WatchOptions — CLI 플래그.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScpUsageWatchOptions {
	pub commit: bool,
	pub once: bool,
}

/// TS CLI 명령.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScpUsageCommand {
	Watch { config: PathBuf, options: ScpUsageWatchOptions },
}

/// TS argv 파싱 — `scp-usage` 이후 `watch --config PATH [--commit] [--once]`.
pub fn parse_scp_usage_args(args: &[String]) -> Result<ScpUsageCommand, String> {
	if args.first().map(String::as_str) != Some("watch") {
		return Err(SCP_USAGE.to_string());
	}
	let mut config: Option<PathBuf> = None;
	let mut commit = false;
	let mut once = false;
	let mut index = 1;
	while index < args.len() {
		match args[index].as_str() {
			"--config" => {
				let value = args.get(index + 1).ok_or_else(|| SCP_USAGE.to_string())?;
				config = Some(PathBuf::from(value));
				index += 2;
			}
			"--commit" => {
				commit = true;
				index += 1;
			}
			"--once" => {
				once = true;
				index += 1;
			}
			_ => return Err(SCP_USAGE.to_string()),
		}
	}
	let config = config.ok_or_else(|| "watch requires --config path".to_string())?;
	Ok(ScpUsageCommand::Watch {
		config,
		options: ScpUsageWatchOptions { commit, once },
	})
}

fn read_prompt(prompt: &str) -> String {
	print!("{prompt}");
	let _ = std::io::stdout().flush();
	let mut answer = String::new();
	if std::io::stdin().read_line(&mut answer).is_err() {
		return String::new();
	}
	answer.trim().to_string()
}

/// TS run — 폴링 루프. 실패 시 운영자에게 retry/skip/stop을 묻는다.
pub fn run_scp_usage_watcher(
	config: UsageConfig,
	options: ScpUsageWatchOptions,
) -> Result<(), Box<dyn std::error::Error>> {
	let pages = crate::property_watcher_runtime::list_cdp_pages(SCP_USAGE_CDP_ENDPOINT)?;
	let index = find_scp_board_page(&pages).map_err(|error| error)?;
	let surface =
		crate::cdp_surface::CdpSurface::from_page(SCP_USAGE_CDP_ENDPOINT, &pages[index]);
	println!(
		"[scp-usage] watcher started: {} mode",
		if options.commit { "commit" } else { "preview" }
	);
	// TS: lastHeartbeat = 0 이라 첫 대기 줄이 곧바로 한 번 찍힌다
	let mut last_heartbeat: Option<std::time::Instant> = None;
	loop {
		let events = read_events(&config);
		if last_heartbeat
			.map(|at| at.elapsed() >= std::time::Duration::from_secs(5))
			.unwrap_or(true)
		{
			println!("[scp-usage] waiting: {} queued event(s)", events.len());
			last_heartbeat = Some(std::time::Instant::now());
		}
		for item in &events {
			let label = item
				.event
				.usage
				.clone()
				.or_else(|| item.event.opinion.clone())
				.or_else(|| item.event.special.clone())
				.unwrap_or_default();
			println!("[scp-usage] processing {}: {}", item.event.property_number, label);
			let mut handled = false;
			while !handled {
				match update_property_usage(&surface, &UpdateRequest {
					event: &item.event,
					commit: options.commit,
				}) {
					Ok(result) => {
						append_result_log(&config, &result)?;
						let detail = match &result.detail {
							Some(detail) => format!(" ({detail})"),
							None => String::new(),
						};
						println!(
							"[scp-usage] {}: {}{}",
							result.property_number, result.status, detail
						);
						if options.commit || result.status == "unchanged" {
							save_cursor(&config.cursor_file, item.end as u64)?;
						}
						handled = true;
					}
					Err(error) => {
						let result = ScpUsageResult {
							property_number: item.event.property_number.clone(),
							status: "failed".to_string(),
							detail: Some(error),
						};
						append_result_log(&config, &result)?;
						eprintln!(
							"[scp-usage] {}: failed ({})",
							result.property_number,
							result.detail.as_deref().unwrap_or("")
						);
						let answer = read_prompt("[1] retry  [2] skip  [3] stop > ");
						match answer.as_str() {
							"1" => continue,
							"2" => {
								save_cursor(&config.cursor_file, item.end as u64)?;
								append_result_log(
									&config,
									&ScpUsageResult {
										property_number: item.event.property_number.clone(),
										status: "skipped".to_string(),
										detail: Some("operator choice".to_string()),
									},
								)?;
								println!(
									"[scp-usage] {}: skipped by operator",
									item.event.property_number
								);
								handled = true;
							}
							_ => {
								println!("[scp-usage] stopped by operator");
								return Ok(());
							}
						}
					}
				}
			}
		}
		if options.once {
			return Ok(());
		}
		std::thread::sleep(std::time::Duration::from_millis(config.poll_ms));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn temp_dir(name: &str) -> PathBuf {
		let dir = std::env::temp_dir().join(format!("scp-usage-{name}-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).expect("임시 폴더");
		dir
	}

	fn event(property_number: &str, usage: Option<&str>) -> UsageEvent {
		UsageEvent {
			property_number: property_number.to_string(),
			jibun: None,
			pnu: None,
			usage: usage.map(str::to_string),
			opinion: None,
			special: None,
			usage_changed: None,
			opinion_changed: None,
			special_changed: None,
		}
	}

	#[test]
	fn load_config_resolves_relative_paths_against_config_directory() {
		let dir = temp_dir("config");
		let config_path = dir.join("watch.json");
		std::fs::write(
			&config_path,
			r#"{"queue_file":"queue.jsonl","cursor_file":"state/cursor.json","log_file":"logs/run.log","poll_ms":2500}"#,
		)
		.expect("config 기록");
		let config = load_config(&config_path).expect("config 로드");
		assert_eq!(config.queue_file, dir.join("queue.jsonl"));
		assert_eq!(config.cursor_file, dir.join("state/cursor.json"));
		assert_eq!(config.log_file, dir.join("logs/run.log"));
		assert_eq!(config.poll_ms, 2500);
	}

	#[test]
	fn load_config_defaults_poll_interval() {
		let dir = temp_dir("config-default");
		let config_path = dir.join("watch.json");
		std::fs::write(
			&config_path,
			r#"{"queue_file":"q","cursor_file":"c","log_file":"l"}"#,
		)
		.expect("config 기록");
		let config = load_config(&config_path).expect("config 로드");
		assert_eq!(config.poll_ms, SCP_USAGE_DEFAULT_POLL_MS);
	}

	#[test]
	fn cursor_defaults_to_zero_and_round_trips() {
		let dir = temp_dir("cursor");
		let cursor = dir.join("state/cursor.json");
		assert_eq!(read_cursor(&cursor), 0);
		save_cursor(&cursor, 1234).expect("커서 저장");
		assert_eq!(read_cursor(&cursor), 1234);
		// 깨진 기록도 0으로 되돌아간다(TS try/catch 동일)
		std::fs::write(&cursor, "not json").expect("커서 훼손");
		assert_eq!(read_cursor(&cursor), 0);
	}

	fn config_in(dir: &Path) -> UsageConfig {
		UsageConfig {
			queue_file: dir.join("queue.jsonl"),
			cursor_file: dir.join("cursor.json"),
			log_file: dir.join("run.log"),
			poll_ms: SCP_USAGE_DEFAULT_POLL_MS,
		}
	}

	#[test]
	fn read_events_skips_blank_invalid_and_valueless_lines() {
		let dir = temp_dir("events");
		let config = config_in(&dir);
		let queue = concat!(
			"{\"property_number\":\"10\",\"usage\":\"공공용도\"}\n",
			"\n",
			"{broken json\n",
			"{\"jibun\":\" somewhere\"}\n",
			"{\"property_number\":\"11\"}\n",
			"{\"property_number\":\"  \",\"usage\":\"값\"}\n",
			"{\"property_number\":\"12\",\"special\":\"특기\"}\n"
		);
		std::fs::write(&config.queue_file, queue).expect("큐 기록");
		let events = read_events(&config);
		assert_eq!(events.len(), 2);
		assert_eq!(events[0].event.property_number, "10");
		// end는 첫 줄 바이트 길이 + 개행 1바이트(TS 누적 방식)
		let first_line = queue.split('\n').next().unwrap();
		assert_eq!(events[0].end, first_line.len() + 1);
		assert_eq!(events[1].event.property_number, "12");
		assert_eq!(events[1].event.special.as_deref(), Some("특기"));
	}

	#[test]
	fn read_events_resumes_from_cursor_and_resets_when_cursor_is_beyond_file() {
		let dir = temp_dir("events-cursor");
		let config = config_in(&dir);
		let first = "{\"property_number\":\"1\",\"usage\":\"A\"}\n";
		let second = "{\"property_number\":\"2\",\"usage\":\"B\"}\n";
		std::fs::write(&config.queue_file, format!("{first}{second}")).expect("큐 기록");

		// 커서 0이면 파일 끝까지 한 번에 읽는다(TS readEvents는 EOF까지)
		let first_batch = read_events(&config);
		assert_eq!(first_batch.len(), 2);
		// 첫 줄의 끝 위치로 커서를 옮기면 다음 줄부터 이어 읽는다
		save_cursor(&config.cursor_file, first_batch[0].end as u64).expect("커서 저장");
		let resumed = read_events(&config);
		assert_eq!(resumed.len(), 1);
		assert_eq!(resumed[0].event.property_number, "2");

		// 커서가 파일보다 크면 처음부터 다시 읽는다(TS offset > byteLength → 0)
		save_cursor(&config.cursor_file, 10_000).expect("커서 저장");
		assert_eq!(read_events(&config).len(), 2);
	}

	#[test]
	fn plan_changes_follows_usage_default_and_strict_flags() {
		let mut target = event("10", Some("공공용도"));
		target.opinion = Some("의견A".to_string());
		target.special = Some("특기A".to_string());
		// usage는 changed 플래그가 없어도 반영, special/opinion은 true일 때만
		let plan = plan_changes(&target, "", "", "");
		assert!(plan.usage);
		assert!(!plan.special);
		assert!(!plan.opinion);
		assert_eq!(plan.detail.as_deref(), Some(" -> 공공용도"));

		// usage_changed: false면 반영하지 않는다
		let mut off = event("10", Some("공공용도"));
		off.usage_changed = Some(false);
		assert!(!plan_changes(&off, "", "", "").usage);

		// 값이 같으면 unchanged
		assert!(!plan_changes(&target, "", "공공용도", "").usage);
	}

	#[test]
	fn plan_changes_builds_detail_in_special_usage_opinion_order() {
		let mut target = event("10", Some("활용B"));
		target.special = Some("특기B".to_string());
		target.opinion = Some("의견B".to_string());
		target.special_changed = Some(true);
		target.opinion_changed = Some(true);
		let plan = plan_changes(&target, "특기A", "활용A", "의견A");
		assert!(plan.special && plan.usage && plan.opinion);
		assert_eq!(
			plan.detail.as_deref(),
			Some("특기A -> 특기B; 활용A -> 활용B; 의견A -> 의견B")
		);
	}

	#[test]
	fn append_result_log_writes_ordered_json_line_with_timestamp() {
		let dir = temp_dir("log");
		let config = config_in(&dir);
		append_result_log(
			&config,
			&ScpUsageResult {
				property_number: "10".to_string(),
				status: "saved".to_string(),
				detail: Some("활용A -> 활용B".to_string()),
			},
		)
		.expect("로그 기록");
		let text = std::fs::read_to_string(&config.log_file).expect("로그 읽기");
		let line = text.lines().next().expect("한 줄");
		let value: Value = serde_json::from_str(line).expect("JSON 줄");
		// TS { ...result, ts } 순서
		assert_eq!(
			line.find("\"property_number\"").unwrap() < line.find("\"status\"").unwrap(),
			true
		);
		assert!(line.contains("\"detail\""));
		assert!(line.contains("\"ts\":\""));
		assert_eq!(value["status"], "saved");
		// ISO-8601 Z 형식
		let ts = value["ts"].as_str().expect("ts 문자열");
		assert!(ts.ends_with('Z') && ts.contains('T') && ts.len() == 24, "{ts}");
	}

	#[test]
	fn parse_args_requires_watch_and_config_and_accepts_flags() {
		let args: Vec<String> = ["watch", "--config", "conf.json", "--commit", "--once"]
			.iter()
			.map(|value| value.to_string())
			.collect();
		assert_eq!(
			parse_scp_usage_args(&args).expect("명령"),
			ScpUsageCommand::Watch {
				config: PathBuf::from("conf.json"),
				options: ScpUsageWatchOptions { commit: true, once: true },
			}
		);
		assert_eq!(
			parse_scp_usage_args(&[]).expect_err("명령 없음"),
			SCP_USAGE
		);
		assert_eq!(
			parse_scp_usage_args(&["run".to_string()]).expect_err("watch 아님"),
			SCP_USAGE
		);
		assert_eq!(
			parse_scp_usage_args(&["watch".to_string()]).expect_err("config 없음"),
			"watch requires --config path"
		);
	}

	#[test]
	fn scp_board_page_requires_exactly_one_board_target() {
		let page = |url: &str| crate::property_watcher_runtime::CdpTargetPage {
			id: "t".to_string(),
			url: url.to_string(),
			ws_url: "ws://x".to_string(),
		};
		assert_eq!(
			find_scp_board_page(&[page("https://scpweb.softgraphy.biz/board")]).expect("1개"),
			0
		);
		assert_eq!(
			find_scp_board_page(&[page("https://scpweb.softgraphy.biz/other")])
				.expect_err("0개"),
			"expected one SCP /board page, found 0"
		);
		assert_eq!(
			find_scp_board_page(&[
				page("https://scpweb.softgraphy.biz/board"),
				page("https://scpweb.softgraphy.biz/board?page=2")
			])
			.expect_err("2개"),
			"expected one SCP /board page, found 2"
		);
		// 쿼리는 pathname 비교에 영향 주지 않는다
		assert!(find_scp_board_page(&[page("https://scpweb.softgraphy.biz/board?x=1")]).is_ok());
	}

	#[test]
	fn iso_timestamp_is_utc_calendar_date() {
		let stamp = iso_utc_now();
		assert_eq!(stamp.len(), 24, "{stamp}");
		assert!(stamp.ends_with('Z'));
		let year: i64 = stamp[0..4].parse().expect("연도");
		assert!((2024..=2100).contains(&year), "{stamp}");
		// 1970-01-01 → 0일 기준 검산
		assert_eq!(civil_from_days(0), (1970, 1, 1));
		assert_eq!(civil_from_days(19_723), (2024, 1, 1));
	}
}
