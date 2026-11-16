// TS scripts/kras-browser-choice.ts — Chrome·Edge·Brave 설치 발견과 고정 번호 선택.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::output;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KrasBrowserEngine {
	Chromium,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasBrowserChoice {
	pub engine: KrasBrowserEngine,
	pub label: String,
	pub executable_path: String,
	pub profile_directory: String,
}

struct KnownBrowser {
	label: &'static str,
	display_name: &'static str,
	profile: &'static str,
	path: &'static [&'static str],
}

// 설치 확인 대상은 Chrome·Edge·Brave 세 브라우저뿐이다.
const KNOWN_BROWSERS: &[KnownBrowser] = &[
	KnownBrowser {
		label: "Chrome",
		display_name: "크롬",
		profile: ".kras-chrome-profile",
		path: &["Google", "Chrome", "Application", "chrome.exe"],
	},
	KnownBrowser {
		label: "Edge",
		display_name: "엣지",
		profile: ".kras-edge-profile",
		path: &["Microsoft", "Edge", "Application", "msedge.exe"],
	},
	KnownBrowser {
		label: "Brave",
		display_name: "브레이브 브라우저",
		profile: ".kras-brave-profile",
		path: &["BraveSoftware", "Brave-Browser", "Application", "brave.exe"],
	},
];

const SEARCH_DIRECTORIES: &[&str] = &[
	"ProgramW6432",
	"ProgramFiles",
	"ProgramFiles(x86)",
	"LOCALAPPDATA",
];

/// 현재 프로세스 환경변수(진행 시 사용).
pub fn process_environment() -> HashMap<String, String> {
	std::env::vars().into_iter().collect()
}

pub fn discover_kras_browsers(
	root_directory: &Path,
	environment: &HashMap<String, String>,
) -> Vec<KrasBrowserChoice> {
	let mut candidates = Vec::new();
	let mut seen = HashSet::new();
	for browser in KNOWN_BROWSERS {
		for key in SEARCH_DIRECTORIES {
			let Some(directory) = environment.get(*key) else {
				continue;
			};
			if directory.is_empty() || !Path::new(directory).is_absolute() {
				continue;
			}
			let mut executable = PathBuf::from(directory);
			for segment in browser.path {
				executable.push(segment);
			}
			let executable_path = executable.display().to_string();
			if !seen.insert(executable_path.to_lowercase()) {
				continue;
			}
			if !executable.exists() {
				continue;
			}
			candidates.push(KrasBrowserChoice {
				engine: KrasBrowserEngine::Chromium,
				label: browser.label.to_string(),
				executable_path,
				profile_directory: root_directory
					.join(browser.profile)
					.display()
					.to_string(),
			});
			break;
		}
	}
	candidates
}

/// 고정 번호(1~3) 선택 UI. 빈 답이나 0은 취소다.
pub fn choose_kras_browser(
	mut question: impl FnMut(&str) -> String,
	candidates: &[KrasBrowserChoice],
) -> Option<KrasBrowserChoice> {
	let options: Vec<(&KnownBrowser, Option<&KrasBrowserChoice>)> = KNOWN_BROWSERS
		.iter()
		.map(|browser| {
			let choice = candidates.iter().find(|candidate| {
				candidate.label == browser.label
					&& candidate.engine == KrasBrowserEngine::Chromium
			});
			(browser, choice)
		})
		.collect();
	output::write("\n사용하실 브라우저가 무엇인가요?\n");
	for (index, (browser, choice)) in options.iter().enumerate() {
		let state = if choice.is_some() {
			"설치됨"
		} else {
			"설치 안 됨"
		};
		output::write(&format!(
			"{}. {} ({state})\n",
			index + 1,
			browser.display_name
		));
	}
	output::write("0. 취소 (Enter도 취소)\n");
	loop {
		let answer = question("브라우저 번호 > ").trim().to_string();
		if answer.is_empty() || answer == "0" {
			return None;
		}
		let option = match answer.as_str() {
			"1" | "2" | "3" => &options[answer.parse::<usize>().unwrap() - 1],
			_ => {
				output::write("0부터 3까지의 번호를 입력하세요.\n");
				continue;
			}
		};
		if let Some(choice) = option.1 {
			return Some((*choice).clone());
		}
		output::write("설치되어 있지 않습니다. 설치된 브라우저를 선택하세요.\n");
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::output::{install_buffer, restore_real, take_buffer};
	use crate::test_serial;
	use std::fs;
	use std::time::{SystemTime, UNIX_EPOCH};

	fn temp_root(tag: &str) -> PathBuf {
		let nanos = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos();
		let root = std::env::temp_dir().join(format!(
			"kras-browser-{tag}-{}-{nanos}",
			std::process::id()
		));
		fs::create_dir_all(&root).unwrap();
		root
	}

	fn fixture(label: &str) -> KrasBrowserChoice {
		KrasBrowserChoice {
			engine: KrasBrowserEngine::Chromium,
			label: label.to_string(),
			executable_path: label.to_string(),
			profile_directory: label.to_string(),
		}
	}

	#[test]
	fn discovers_only_chrome_edge_brave_one_installation_each() {
		let root = temp_root("three");
		let paths = [
			"Google/Chrome/Application/chrome.exe",
			"Microsoft/Edge/Application/msedge.exe",
			"BraveSoftware/Brave-Browser/Application/brave.exe",
			"Mozilla Firefox/firefox.exe",
			"Vivaldi/Application/vivaldi.exe",
			"Opera/opera.exe",
			"Naver/Naver Whale/Application/whale.exe",
		];
		for base in ["program", "local"] {
			for path in paths {
				let target = root.join(base).join(path);
				fs::create_dir_all(target.parent().unwrap()).unwrap();
				fs::write(&target, "fixture").unwrap();
			}
		}
		let mut environment = HashMap::new();
		environment.insert(
			"ProgramFiles".to_string(),
			root.join("program").display().to_string(),
		);
		environment.insert(
			"LOCALAPPDATA".to_string(),
			root.join("local").display().to_string(),
		);
		let choices = discover_kras_browsers(&root, &environment);
		let labels: Vec<&str> = choices.iter().map(|choice| choice.label.as_str()).collect();
		assert_eq!(labels, ["Chrome", "Edge", "Brave"]);
		// 설치 검색은 절대 경로만 허용하고 프로그램 폴더에서 먼저 찾는다.
		let program_prefix = root.join("program").display().to_string();
		assert!(choices.iter().all(|choice| {
			choice.engine == KrasBrowserEngine::Chromium
				&& choice.executable_path.starts_with(&program_prefix)
		}));
		assert!(discover_kras_browsers(&root, &HashMap::new()).is_empty());
		let mut limited = HashMap::new();
		limited.insert("ProgramFiles".to_string(), String::new());
		limited.insert("LOCALAPPDATA".to_string(), "relative".to_string());
		assert!(discover_kras_browsers(&root, &limited).is_empty());
		let _ = fs::remove_dir_all(&root);
	}

	#[test]
	fn keeps_fixed_browser_numbers_and_cancellation() {
		let serial = test_serial::lock();
		install_buffer();
		let choices: Vec<KrasBrowserChoice> =
			["Chrome", "Edge", "Brave"].iter().map(|label| fixture(label)).collect();
		for answer in ["", "0", " \t ", "1", "2", "3"] {
			let result = choose_kras_browser(|_| answer.to_string(), &choices);
			let expected = match answer.trim() {
				"1" => Some(choices[0].clone()),
				"2" => Some(choices[1].clone()),
				"3" => Some(choices[2].clone()),
				_ => None,
			};
			assert_eq!(result, expected, "answer={answer:?}");
			let output = take_buffer();
			assert!(output.contains("1. 크롬"), "{output:?}");
			assert!(output.contains("2. 엣지"), "{output:?}");
			assert!(output.contains("3. 브레이브 브라우저"), "{output:?}");
			for forbidden in [
				"Firefox",
				"파이어폭스",
				"오페라",
				"비발디",
				"웨일",
				"직접 지정",
			] {
				assert!(!output.contains(forbidden), "{forbidden} in {output:?}");
			}
		}
		restore_real();
		drop(serial);
	}

	#[test]
	fn rejects_invalid_and_uninstalled_selections_before_edge() {
		let serial = test_serial::lock();
		install_buffer();
		let edge = fixture("Edge");
		let answers = ["bad", "4", "1.5", "1", "3", "2"];
		let mut count = 0usize;
		let result = choose_kras_browser(
			|_| {
				let answer = answers[count];
				count += 1;
				answer.to_string()
			},
			std::slice::from_ref(&edge),
		);
		assert_eq!(result, Some(edge));
		assert_eq!(count, answers.len());
		restore_real();
		drop(serial);
	}
}
