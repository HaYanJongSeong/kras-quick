// CDP 기반 KrasOzViewerSurface 구현 — Page.printToPDF, captureScreenshot, Runtime.evaluate.

use base64::Engine;
use crate::kras_auto_stages::KrasOzViewerSurface;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub struct CdpSurface {
	endpoint: String,
	page_url: String,
	ws_url: String,
	/// 캡처한 페이지 이미지 캐시 — TS의 captureOnce() 메모이제이션과 동일.
	/// (PDF와 PNG 목록이 같은 캡처 결과를 공유하도록)
	page_cache: std::sync::Mutex<Option<Vec<Vec<u8>>>>,
}

#[cfg(test)]
#[path = "capture_memory_tests.rs"]
mod capture_memory_tests;

impl CdpSurface {
	fn capture_pages(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
		crate::oz_capture::capture_oz_pages(self, |done, total| {
			crate::task_screen::task_status(&format!("Stage 3: PNG 캡처 {done}/{total}페이지 완료\n"));
		})
	}

	pub fn close_viewer(&self) -> Result<(), Box<dyn std::error::Error>> {
		if !crate::kras_auto_stages::is_viewer_url(&self.page_url) {
			return Err("OZ 뷰어가 아닌 탭은 닫지 않습니다.".into());
		}
		let id = self.ws_url.rsplit('/').next().filter(|id| !id.is_empty()
			&& id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'))
			.ok_or("OZ 대상 ID 오류")?;
		ureq::get(&format!("{}/json/close/{id}", self.endpoint.trim_end_matches('/'))).call()?;
		for _ in 0..20 {
			if !crate::property_watcher_runtime::list_cdp_pages(&self.endpoint)?.iter().any(|page| page.ws_url == self.ws_url) {
				return Ok(());
			}
			std::thread::sleep(std::time::Duration::from_millis(100));
		}
		Err("OZ 뷰어 닫힘을 확인하지 못했습니다.".into())
	}

	pub fn connect(endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
		// 타깃 목록 수집은 포팅된 runtime의 list_cdp_pages에 위임한다(TS connectOverCDP 대응).
		let pages = crate::property_watcher_runtime::list_cdp_pages(endpoint)?;
		if pages.is_empty() {
			return Err("no page target found".into());
		}
		// 선호 순서: certView.do → 일반 KRAS 페이지(뷰어 제외) → 첫 페이지
		let pick = pages
			.iter()
			.find(|p| p.url.contains("/kras/cert/certView.do"))
			.or_else(|| {
				pages
					.iter()
					.find(|p| p.url.starts_with("https://www.kras.go.kr/") && !p.url.contains("ozhviewer"))
			})
			.unwrap_or(&pages[0]);
		let ws_url = pick.ws_url.clone();
		let page_url = pick.url.clone();
		Ok(Self {
			endpoint: endpoint.to_string(),
			page_url,
			ws_url,
			page_cache: std::sync::Mutex::new(None),
		})
	}

/// 특정 타깃(웹소켓 주소)으로 표면을 만든다 — SCP/KRAS 각각의 페이지에 붙을 때 쓴다.
pub fn from_page(endpoint: &str, page: &crate::property_watcher_runtime::CdpTargetPage) -> Self {
	Self {
		endpoint: endpoint.to_string(),
		page_url: page.url.clone(),
		ws_url: page.ws_url.clone(),
		page_cache: std::sync::Mutex::new(None),
	}
}

pub fn connect_viewer(endpoint: &str, timeout_ms: u64, previous_pages: &[String]) -> Result<Self, Box<dyn std::error::Error>> {
		let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
		loop {
			let url = format!("{}/json/list", endpoint.trim_end_matches('/'));
			if let Ok(resp) = ureq::get(&url).call() {
				if let Ok(body) = resp.into_body().read_to_string() {
					if let Ok(targets) = serde_json::from_str::<Vec<Value>>(&body) {
						for t in &targets {
							if t.get("type").and_then(|v| v.as_str()) == Some("page") {
								let u = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
								if u.contains("ozhviewer") || u.contains("report.jsp") {
									let ws_url = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()).ok_or("no ws")?.to_string();
									if previous_pages.contains(&ws_url) {
										continue;
									}
									let page_url = u.to_string();
									return Ok(Self { endpoint: endpoint.to_string(), page_url, ws_url, page_cache: std::sync::Mutex::new(None) });
								}
							}
						}
					}
				}
			}
			if std::time::Instant::now() >= deadline {
				return Err("OZ viewer not found within timeout".into());
			}
			std::thread::sleep(std::time::Duration::from_millis(1000));
		}
	}

pub fn send_cdp(&self, method: &str, params: Value) -> Result<Value, Box<dyn std::error::Error>> {
		let (mut socket, _) = tungstenite::connect(&self.ws_url)?;
		let cmd = serde_json::json!({
			"id": 1,
			"method": method,
			"params": params,
		});
		socket.send(tungstenite::Message::Text(cmd.to_string().into()))?;
		// read until response with id 1
		loop {
			let msg = socket.read()?;
			if let tungstenite::Message::Text(text) = msg {
				let v: Value = serde_json::from_str(&text)?;
				if v.get("id").and_then(|i| i.as_i64()) == Some(1) {
					if let Some(err) = v.get("error") {
						return Err(format!("CDP error: {err}").into());
					}
					// Runtime.evaluate의 예외는 명령 응답 result 안에 들어온다.
					if let Some(details) = v.get("result").and_then(|result| result.get("exceptionDetails"))
						.or_else(|| v.get("exceptionDetails")) {
						let desc = details
							.get("exception")
							.and_then(|e| {
								e.get("description")
									.or_else(|| e.get("value"))
									.or_else(|| e.get("className"))
							})
							.or_else(|| details.get("text"))
							.and_then(|x| x.as_str())
							.filter(|s| !s.is_empty())
							.unwrap_or("JavaScript evaluation failed");
						return Err(format!("CDP exception: {desc}").into());
					}
					return Ok(v.get("result").cloned().unwrap_or(Value::Null));
				}
			}
		}
	}
}

impl KrasOzViewerSurface for CdpSurface {
	fn url(&self) -> String {
		self.page_url.clone()
	}

	fn has_visible_captcha(&self) -> bool {
		let expr = "document.querySelector('img[id*=\"captcha\" i], img[src*=\"captcha\" i]') !== null";
		let params = serde_json::json!({ "expression": expr, "returnByValue": true });
		match self.send_cdp("Runtime.evaluate", params) {
			Ok(r) => r
				.get("result")
				.and_then(|v| v.get("value"))
				.and_then(|v| v.as_bool())
				.unwrap_or(false),
			Err(_) => false,
		}
	}

	/// TS captureOnce — OZ 뷰어 캡처(줌 300% → 1페이지 복귀 → 페이지 넘김 → 안정 캔버스).
	/// 결과를 캐시해 PDF/PNG 두 경로가 같은 캡처를 공유한다.
	fn capture_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
		if let Ok(cache) = self.page_cache.lock() {
			if let Some(pages) = cache.as_ref() {
				return Ok(pages.clone());
			}
		}
		let pages = self.capture_pages()?;
		if let Ok(mut cache) = self.page_cache.lock() {
			*cache = Some(pages.clone());
		}
		Ok(pages)
	}

	/// TS buildPdfFromPngs — 캡처한 PNG로 PDF를 만든다(뷰어 화면과 동일한 출력).
	fn capture_pdf(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
		let mut cache = self.page_cache.lock().map_err(|error| std::io::Error::other(error.to_string()))?;
		if cache.is_none() { *cache = Some(self.capture_pages()?); }
		let pages = cache.as_deref().ok_or("캡처한 페이지가 없습니다.")?;
		crate::oz_capture::build_pdf_from_pngs(pages)
	}

	fn take_captured_page_pngs(&self) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
		let pages = self.page_cache.lock().map_err(|error| std::io::Error::other(error.to_string()))?.take();
		match pages {
			Some(pages) => Ok(pages),
			None => self.capture_pages(),
		}
	}

	/// TS readReportXml — 전역변수 → 신형 strOZDataXML → 구형 ozdata= → DOM 순.
	fn read_report_xml(&self) -> Result<String, Box<dyn std::error::Error>> {
		crate::oz_capture::read_report_xml(self)
	}
}

// (imports already above)

// ——— CDP 페이지 자동화 헬퍼 (stage-1/2 폼 입력·클릭·읽기) ———

impl CdpSurface {
	/// JS 표현식을 평가하고 결과 value를 반환한다.
	pub fn evaluate(&self, expression: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
		let params = serde_json::json!({ "expression": expression, "returnByValue": true });
		let result = self.send_cdp("Runtime.evaluate", params)?;
		Ok(result.get("result").cloned().unwrap_or(serde_json::Value::Null))
	}

	/// 입력 필드에 값을 설정하고 input/change 이벤트를 발생시킨다.
	pub fn fill_input(&self, selector: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
		let expr = format!(
			r#"(function(){{const el=document.querySelector({sel});if(!el)throw new Error('element not found');el.value={val};el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}})()"#,
			sel = serde_json::to_string(selector).unwrap_or_default(),
			val = serde_json::to_string(value).unwrap_or_default(),
		);
		self.evaluate(&expr)?;
		Ok(())
	}

	/// 버튼/요소를 클릭한다.
	pub fn click(&self, selector: &str) -> Result<(), Box<dyn std::error::Error>> {
		let expr = format!(
			r#"(function(){{const el=document.querySelector({sel});if(!el)throw new Error('element not found');el.click();return true;}})()"#,
			sel = serde_json::to_string(selector).unwrap_or_default(),
		);
		self.evaluate(&expr)?;
		Ok(())
	}

	/// 요소의 textContent를 읽는다.
	pub fn text_content(&self, selector: &str) -> Result<String, Box<dyn std::error::Error>> {
		let expr = format!(
			r#"(function(){{const el=document.querySelector({sel});return el?el.textContent.trim():'';}})()"#,
			sel = serde_json::to_string(selector).unwrap_or_default(),
		);
		let result = self.evaluate(&expr)?;
		Ok(result.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string())
	}

	/// select 옵션 목록을 읽는다.
	pub fn select_options(&self, selector: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
		let expr = format!(
			r#"(function(){{const sel=document.querySelector({sel});if(!sel)return [];return Array.from(sel.options).map(o=>[o.value,(o.textContent||'').trim()]);}})()"#,
			sel = serde_json::to_string(selector).unwrap_or_default(),
		);
		let result = self.evaluate(&expr)?;
		let arr = result.get("value").and_then(|v| v.as_array()).cloned().unwrap_or_default();
		let mut out = Vec::new();
		for item in arr {
			if let Some(pair) = item.as_array() {
				let val = pair.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
				let label = pair.get(1).and_then(|v| v.as_str()).unwrap_or("").to_string();
				out.push((val, label));
			}
		}
		Ok(out)
	}

	/// select에서 특정 라벨과 일치하는 옵션을 선택한다.
	pub fn select_option_by_label(&self, selector: &str, label: &str) -> Result<bool, Box<dyn std::error::Error>> {
		let expr = format!(
			r#"(function(){{const sel=document.querySelector({sel});if(!sel)return false;const opt=Array.from(sel.options).find(o=>(o.textContent||'').trim()==={lbl});if(!opt)return false;sel.value=opt.value;sel.dispatchEvent(new Event('input',{{bubbles:true}}));sel.dispatchEvent(new Event('change',{{bubbles:true}}));return true;}})()"#,
			sel = serde_json::to_string(selector).unwrap_or_default(),
			lbl = serde_json::to_string(label).unwrap_or_default(),
		);
		let result = self.evaluate(&expr)?;
		Ok(result.get("value").and_then(|v| v.as_bool()).unwrap_or(false))
	}
}

/// 남아 있는 OZ 뷰어 탭을 닫는다 (반복 실행 시 이전 세션 잔재 정리).
pub fn close_viewer_tabs(endpoint: &str) -> usize {
	let url = format!("{}/json/list", endpoint.trim_end_matches('/'));
	let targets: Vec<Value> = match ureq::get(&url).call() {
		Ok(resp) => resp
			.into_body()
			.read_to_string()
			.ok()
			.and_then(|b| serde_json::from_str::<Vec<Value>>(&b).ok())
			.unwrap_or_default(),
		Err(_) => return 0,
	};
	let mut closed = 0;
	for t in &targets {
		if t.get("type").and_then(|v| v.as_str()) != Some("page") {
			continue;
		}
		let u = t.get("url").and_then(|v| v.as_str()).unwrap_or("");
		if !u.contains("ozhviewer") && !u.contains("report.jsp") {
			continue;
		}
		if let Some(id) = t.get("id").and_then(|v| v.as_str()) {
			let close_url = format!("{}/json/close/{}", endpoint.trim_end_matches('/'), id);
			if ureq::get(&close_url).call().is_ok() {
				closed += 1;
			}
		}
	}
	closed
}
