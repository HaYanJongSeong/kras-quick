use super::KrasXmlData;

pub fn split_jibn(jibn: &str) -> (String, String) {
	match jibn.split_once('-') {
		Some((main, sub)) => (main.to_string(), sub.to_string()),
		None => (jibn.to_string(), String::new()),
	}
}

pub fn parse_flr(flrcd: &str) -> (String, String) {
	match flrcd.split_once('/') {
		Some((ground, basement)) => (ground.to_string(), basement.to_string()),
		None => (flrcd.to_string(), "0".to_string()),
	}
}

/// 건축일자 연도와 현재 연도의 차이.
pub fn building_age(use_aprv_ymd: &str) -> String {
	if use_aprv_ymd.is_empty() {
		return String::new();
	}
	#[expect(clippy::expect_used, reason = "the fixed date regex is valid")]
	let re = regex::Regex::new(r"(\d{4})\.(\d{2})\.(\d{2})").expect("static regex");
	let Some(caps) = re.captures(use_aprv_ymd) else {
		return String::new();
	};
	let year: i64 = caps[1].parse().unwrap_or(0);
	let now = time_now_year();
	(now - year).to_string()
}

fn time_now_year() -> i64 {
	let secs = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_secs())
		.unwrap_or(0);
	// 평균 초/년 환산은 연말 경계에서 오차가 나므로 윤년을 반영해 반복 계산한다.
	let mut year: i64 = 1970;
	let mut remaining = secs as i64;
	loop {
		let days = if is_leap(year) { 366 } else { 365 };
		let year_secs = days * 86_400;
		if remaining >= year_secs {
			remaining -= year_secs;
			year += 1;
		} else {
			break;
		}
	}
	year
}

fn is_leap(year: i64) -> bool {
	(year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn print_kras_data_table(data: &KrasXmlData) {
	let (jibn_main, jibn_sub) = split_jibn(&data.jibn);
	let (ground, basement) = parse_flr(&data.flr);
	let land = data.land_loc_nm.split('(').next().unwrap_or(&data.land_loc_nm).trim().to_string();
	let rows: [(&str, String); 17] = [
		("소재지", land),
		("본번", jibn_main),
		("부번", jibn_sub),
		("공부지목", data.jimok.clone()),
		("연면적", data.garea.clone()),
		("공부면적", data.garea.clone()),
		("집합건물구분", data.bldg_kind_cd.clone()),
		("건물높이", data.hgt.clone()),
		("건축일자", data.use_aprv_ymd.clone()),
		("용적률", data.fsi.clone()),
		("공부용도", data.main_use_nm.clone()),
		("건물연령", building_age(&data.use_aprv_ymd)),
		("건물구조", data.stru_nm.clone()),
		("지붕구조", data.roof_nm.clone()),
		("건폐율", data.blr.clone()),
		("지상층수", ground),
		("지하층수", basement),
	];
	let mut table = String::from("\n=== XML 추출값 ===\n");
	for (k, v) in rows {
		table.push_str(&format!("{:<14}{}\n", k, v));
	}
	crate::task_screen::task_status(&table);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn split_jibn_parses_main_and_sub() {
		assert_eq!(split_jibn("392-19"), ("392".to_string(), "19".to_string()));
		assert_eq!(split_jibn("392"), ("392".to_string(), String::new()));
	}

	#[test]
	fn parse_flr_handles_missing_basement() {
		assert_eq!(parse_flr("5/1"), ("5".to_string(), "1".to_string()));
		assert_eq!(parse_flr(""), (String::new(), "0".to_string()));
	}

	#[test]
	fn building_age_computes_year_delta() {
		assert_eq!(building_age("1991.09.11"), (time_now_year() - 1991).to_string());
		assert_eq!(building_age(""), "");
		assert_eq!(building_age("알수없음"), "");
	}
}
