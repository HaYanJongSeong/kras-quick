//! KRAS Quick Rust 포팅: 1단계 주소·PNU 파싱, 2단계 카운트/진행 UI·타이밍.

pub mod kras_browser_choice;
pub mod kras_browser_process;
pub mod auto_batch;
pub mod cdp_surface;
pub mod kras_auto_stages;
mod exclusive_output;
pub mod batch_input;
pub mod batch_recovery;
pub mod batch_checkpoint;
pub mod property_watcher_lifecycle;
pub mod kras_launcher;
pub mod kras_dark;
pub mod kras_domain;
pub mod kras_lookup;
pub mod kras_runner;
pub mod kras_bridge;
pub mod kras_output;
pub mod output_log;
pub mod full_xml_json;
pub mod kras_page;
pub mod kras_pnu;
pub mod lawdong;
pub mod numeric_engine;
pub mod numeric_ocr;
pub mod oz_capture;
pub mod property_address;
pub mod property_progress;
pub mod property_progress_windows;
pub mod property_watcher_channel_state;
pub mod property_watcher;
pub mod property_watcher_channel;
pub mod property_watcher_control;
pub mod property_watcher_cdp;
pub mod property_watcher_guidance;
pub mod property_watcher_listener;
pub mod property_watcher_page;
pub mod property_watcher_process;
pub mod property_watcher_runtime;
pub mod property_watcher_runner;
pub mod property_watcher_start;
pub mod property_watcher_state;
pub mod property_watcher_payload;

pub mod output;
pub mod quick_address;
pub mod scp_usage_watcher;
pub mod stage12;
pub mod sixel;
pub mod startup_progress;
pub mod task_screen;
pub mod task_timing;
pub mod term_env;
pub mod workflow_watcher;

pub use kras_pnu::{is_pnu_input, parse_pnu, pnu_to_address, LandType, PnuParts};
pub use property_address::{
	parse_parcel_address, parse_property_row, ParcelAddress, PropertyAddressError,
	PropertyAddressErrorCode,
};

/// 전역 상태(출력 버퍼·터미널 교체·작업 화면)를 함께 쓰는 테스트의 순서 보장.
#[cfg(test)]
pub mod test_serial {
	use std::sync::Mutex;
	use std::sync::MutexGuard;

	static SERIAL: Mutex<()> = Mutex::new(());

	pub fn lock() -> MutexGuard<'static, ()> {
		SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
	}
}
