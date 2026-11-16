// TS scripts/kras-workflow-watcher.ts — Sixel 인코더·ANSI 블록 렌더러.
// 순수 TS/Rust 포팅: 새 컴퓨터에 설치할 네이티브 의존성이 없어야 한다.

/// Windows Terminal(1.22+)은 Sixel을 그린다. 그 밖의 콘솔은 기존 색블록을쓴다.
pub fn captcha_terminal_mode(wt_session: Option<&str>) -> &'static str {
	match wt_session {
		Some(value) if !value.is_empty() => "sixel",
		_ => "block",
	}
}

pub fn captcha_terminal_mode_from_env() -> &'static str {
	captcha_terminal_mode(std::env::var("WT_SESSION").ok().as_deref())
}

// JS Math.round: 정확히 .5인 경우는 이 공식에서 발생하지 않는다(v*100이 항상 정수).
fn channel(value: u8) -> u64 {
	((f64::from(value) * 100.0) / 255.0).round() as u64
}

// JS ArrayLike 접근: 범위 밖은 undefined → 0.
fn pixel(data: &[u8], base: usize) -> [u8; 4] {
	[
		data.get(base).copied().unwrap_or(0),
		data.get(base + 1).copied().unwrap_or(0),
		data.get(base + 2).copied().unwrap_or(0),
		data.get(base + 3).copied().unwrap_or(0),
	]
}

// RLE 조각:999개 단위로 끊고, 4개 이상만 !n 압축(JS 동일).
fn flush(segment: &mut String, previous: &mut Option<char>, run: &mut usize) {
	let mut remaining = *run;
	let Some(previous_char) = *previous else {
		*run = 0;
		return;
	};
	while remaining > 0 {
		let take = remaining.min(999);
		if take >= 4 {
			segment.push_str(&format!("!{take}{previous_char}"));
		} else {
			for _ in 0..take {
				segment.push(previous_char);
			}
		}
		remaining -= take;
	}
	*run = 0;
	*previous = None;
}

fn nearest_index(
	palette: &[u32],
	nearest: &mut std::collections::HashMap<u32, usize>,
	rgb: u32,
) -> usize {
	if let Some(hit) = nearest.get(&rgb) {
		return *hit;
	}
	let mut best = 0usize;
	let mut best_distance = i32::MAX;
	for (i, candidate) in palette.iter().enumerate() {
		let dr = ((rgb >> 16) & 255) as i32 - ((candidate >> 16) & 255) as i32;
		let dg = ((rgb >> 8) & 255) as i32 - ((candidate >> 8) & 255) as i32;
		let db = (rgb & 255) as i32 - (candidate & 255) as i32;
		let distance = dr * dr + dg * dg + db * db;
		if distance < best_distance {
			best_distance = distance;
			best = i;
		}
	}
	nearest.insert(rgb, best);
	best
}

/// 순수 Sixel 인코더. RGBA 데이터를 DCS 그래프로 만든다.
pub fn render_captcha_sixel(data: &[u8], width: usize, height: usize) -> String {
	if width < 1 || height < 1 {
		return String::new();
	}
	let pixels = width * height;
	// 투명(alpha<128)은 무시하고 첫 발견 순서를 유지하며 빈도를 센다(JS Map은 삽입순).
	let mut order: Vec<u32> = Vec::new();
	let mut counts: std::collections::HashMap<u32, u64> = std::collections::HashMap::new();
	for i in 0..pixels {
		let [r, g, b, a] = pixel(data, i * 4);
		if a < 128 {
			continue;
		}
		let rgb = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
		if !counts.contains_key(&rgb) {
			order.push(rgb);
		}
		*counts.entry(rgb).or_insert(0) += 1;
	}
	let palette: Vec<u32> = if counts.len() <= 256 {
		order
	} else {
		// 256개를 넘으면 많이 쓰는 색만 팔레트로 남긴다(안정 정렬 = 동률은 첫 발견 순).
		let mut ranked = order;
		ranked.sort_by(|a, b| counts[b].cmp(&counts[a]));
		ranked.truncate(256);
		ranked
	};
	let mut nearest: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
	for (i, rgb) in palette.iter().enumerate() {
		nearest.insert(*rgb, i);
	}
	// JS Int32Array 기본값 0 — 투명 픽셀도 0을 가진다(원본 TS 동일).
	let mut indices = vec![0usize; pixels];
	let mut used = vec![false; palette.len()];
	for i in 0..pixels {
		let [r, g, b, a] = pixel(data, i * 4);
		if a < 128 {
			continue;
		}
		let rgb = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
		let index = nearest_index(&palette, &mut nearest, rgb);
		indices[i] = index;
		used[index] = true;
	}
	let mut out = format!("\u{1b}P0;0;8q\"1;1;{width};{height}");
	for (i, rgb) in palette.iter().enumerate() {
		if !used[i] {
			continue;
		}
		out.push_str(&format!(
			"#{i};2;{};{};{}",
			channel(((rgb >> 16) & 255) as u8),
			channel(((rgb >> 8) & 255) as u8),
			channel((rgb & 255) as u8)
		));
	}
	// Sixel 밴드는 6픽셀 높이씩 그린다. 각 색은 왼쪽에서 시작해야 하므로 $로 되돌린다.
	let mut y = 0;
	while y < height {
		let mut segments: Vec<String> = Vec::new();
		for i in 0..palette.len() {
			if !used[i] {
				continue;
			}
			let mut segment = String::new();
			let mut previous: Option<char> = None;
			let mut run = 0usize;
			let mut drawn = false;
			for x in 0..width {
				let mut bits = 0u8;
				for row in 0..6 {
					let py = y + row;
					if py >= height {
						break;
					}
					if indices[py * width + x] == i {
						bits |= 1 << row;
					}
				}
				if bits != 0 {
					drawn = true;
				}
				let ch = if bits == 0 {
					'?'
				} else {
					char::from_u32(63 + u32::from(bits)).unwrap_or('?')
				};
				if previous == Some(ch) {
					run += 1;
				} else {
					flush(&mut segment, &mut previous, &mut run);
					previous = Some(ch);
					run = 1;
				}
			}
			flush(&mut segment, &mut previous, &mut run);
			if drawn {
				segments.push(format!("#{i}{segment}"));
			}
		}
		if segments.is_empty() {
			out.push('-');
		} else {
			out.push_str(&segments.join("$"));
			out.push_str("$-");
		}
		y += 6;
	}
	out.push_str("\u{1b}\\");
	out
}

/// Sixel을 못 그리는 콘솔용: ANSI 절반 블록(▀)으로 줄마다 상·하 픽셀을 합친다.
pub fn render_captcha_terminal_pixels(data: &[u8], width: usize, height: usize) -> String {
	let mut lines: Vec<String> = Vec::new();
	let mut y = 0;
	while y < height {
		let mut line = String::new();
		for x in 0..width {
			let top = (y * width + x) * 4;
			let has_bottom = y + 1 < height;
			let bottom = if has_bottom {
				((y + 1) * width + x) * 4
			} else {
				top
			};
			let [r1, g1, b1, a1] = pixel(data, top);
			let (r2, g2, b2, a2) = if has_bottom {
				let [r, g, b, a] = pixel(data, bottom);
				(r, g, b, a)
			} else {
				(0, 0, 0, 0)
			};
			if a1 == 0 && a2 == 0 {
				line.push(' ');
				continue;
			}
			let background = if a2 == 0 {
				"\u{1b}[40m".to_string()
			} else {
				format!("\u{1b}[48;2;{r2};{g2};{b2}m")
			};
			let foreground = if a1 == 0 {
				"\u{1b}[37m".to_string()
			} else {
				format!("\u{1b}[38;2;{r1};{g1};{b1}m")
			};
			line.push_str(&format!("{foreground}{background}▀\u{1b}[0m"));
		}
		lines.push(line);
		y += 2;
	}
	lines.join("\n")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rgba(pixels: &[[u8; 4]]) -> Vec<u8> {
		pixels.iter().flat_map(|pixel| pixel.iter().copied()).collect()
	}

	#[test]
	fn detects_windows_terminal_for_sixel_and_falls_back_elsewhere() {
		assert_eq!(captcha_terminal_mode(Some("{guid}")), "sixel");
		assert_eq!(captcha_terminal_mode(None), "block");
		assert_eq!(captcha_terminal_mode(Some("")), "block");
	}

	#[test]
	fn encodes_captcha_as_framed_sixel_graphic() {
		// 흰색 위·검은색 아래 1×2 이미지.
		let data = rgba(&[[255, 255, 255, 255], [0, 0, 0, 255]]);
		let rendered = render_captcha_sixel(&data, 1, 2);
		assert!(
			rendered.starts_with("\u{1b}P0;0;8q\"1;1;1;2"),
			"{rendered:?}"
		);
		assert!(rendered.ends_with("\u{1b}\\"), "{rendered:?}");
		assert!(
			rendered.contains("#0;2;100;100;100#1;2;0;0;0"),
			"{rendered:?}"
		);
		// 밴드: 행0=흰색(bit0 → @), 행1=검은색(bit1 → A), 각 줄은 왼쪽 여백에서 시작.
		assert!(rendered.contains("#0@$#1A$-"), "{rendered:?}");
	}

	#[test]
	fn compresses_sixel_runs_and_keeps_palette_within_256_colors() {
		// 흰색 10픽셀 1줄 → !10@ 압축.
		let run = render_captcha_sixel(&rgba(&vec![[255, 255, 255, 255]; 10]), 10, 1);
		assert!(run.contains("#0!10@$"), "{run:?}");
		// 306색 → 팔레트 정의는 256개 이하.
		let mut pixels = Vec::with_capacity(306 * 4);
		for i in 0..306 {
			pixels.push((i & 255) as u8);
			pixels.push((i >> 8) as u8);
			pixels.push(((i * 7) & 255) as u8);
			pixels.push(255);
		}
		let many = render_captcha_sixel(&pixels, 306, 1);
		let pattern = regex::Regex::new(r"#\d+;2;\d+;\d+;\d+").unwrap();
		let defs = pattern.find_iter(&many).count();
		assert!(defs > 0 && defs <= 256, "defs={defs}");
	}

	#[test]
	fn renders_captcha_with_colored_ansi_half_blocks() {
		let data = rgba(&[[255, 255, 255, 255], [0, 0, 0, 255]]);
		let rendered = render_captcha_terminal_pixels(&data, 1, 2);
		assert!(
			rendered.contains("38;2;255;255;255m\u{1b}[48;2;0;0;0m▀"),
			"{rendered:?}"
		);
		assert!(rendered.contains("\u{1b}[0m"), "{rendered:?}");
	}
}
