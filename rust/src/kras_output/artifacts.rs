use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KrasOutputConflictError {
	pub path: String,
}

impl KrasOutputConflictError {
	pub fn new(path: impl Into<String>) -> Self {
		Self { path: path.into() }
	}
}

impl std::fmt::Display for KrasOutputConflictError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "기존 산출물을 보존하기 위해 저장을 중단합니다: {}", self.path)
	}
}

impl std::error::Error for KrasOutputConflictError {}

pub fn valid_pdf(bytes: &[u8]) -> bool {
	if bytes.len() < 5 {
		return false;
	}
	if &bytes[..5] != b"%PDF-" {
		return false;
	}
	let tail = if bytes.len() > 1024 { &bytes[bytes.len() - 1024..] } else { bytes };
	let tail_str = String::from_utf8_lossy(tail);
	tail_str.rfind("%%EOF").is_some()
}

pub fn valid_png(bytes: &[u8]) -> bool {
	if bytes.len() < 45 {
		return false;
	}
	let sig = [137u8, 80, 78, 71, 13, 10, 26, 10];
	if bytes[..8] != sig {
		return false;
	}
	if u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) != 13 {
		return false;
	}
	if &bytes[12..16] != b"IHDR" {
		return false;
	}
	let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
	let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
	if w == 0 || h == 0 {
		return false;
	}
	if bytes.len() < 12 {
		return false;
	}
	let tail = &bytes[bytes.len() - 12..];
	if tail != [0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130] {
		return false;
	}
	true
}

#[derive(Debug, Clone)]
pub struct SavedArtifact {
	pub file: String,
	pub bytes: usize,
	pub sha256: String,
}

pub fn describe_artifact(structure_path: &str, file: &str, bytes: &[u8]) -> SavedArtifact {
	let rel = relative_path(structure_path, file);
	let mut hasher = Sha256::new();
	hasher.update(bytes);
	let hash = format!("{:x}", hasher.finalize());
	SavedArtifact { file: rel, bytes: bytes.len(), sha256: hash }
}

fn relative_path(structure_path: &str, file: &str) -> String {
	// 검증 단계가 절대경로를 거부하므로 structure 디렉터리 기준 상대경로(../ 허용)가 필요하다.
	let target = std::path::Path::new(file);
	if !target.is_absolute() {
		return file.to_string();
	}
	let Some(base) = std::path::Path::new(structure_path).parent() else {
		return file.to_string();
	};
	let base_components: Vec<std::path::Component> = base.components().collect();
	let target_components: Vec<std::path::Component> = target.components().collect();
	let common = base_components.iter().zip(target_components.iter())
		.take_while(|(a, b)| a == b).count();
	// 루트(드라이브/최상위)가 다르면 원본 경로를 유지한다.
	if common == 0 {
		return file.to_string();
	}
	let mut parts: Vec<String> = Vec::new();
	for _ in common..base_components.len() {
		parts.push("..".to_string());
	}
	for component in &target_components[common..] {
		parts.push(component.as_os_str().to_string_lossy().to_string());
	}
	if parts.is_empty() {
		return target.file_name().map(|n| n.to_string_lossy().to_string())
			.unwrap_or_else(|| file.to_string());
	}
	parts.join("\\")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn valid_pdf_checks_header_and_eof() {
		let mut b = b"%PDF-1.4\n".to_vec();
		b.extend_from_slice(b"%%EOF\n");
		assert!(valid_pdf(&b));
		assert!(!valid_pdf(b"not pdf"));
		assert!(!valid_pdf(b"%PDF-x"));
	}

	#[test]
	fn valid_png_checks_signature_and_ihdr() {
		let mut b = vec![137u8, 80, 78, 71, 13, 10, 26, 10];
		b.extend_from_slice(&13u32.to_be_bytes());
		b.extend_from_slice(b"IHDR");
		b.extend_from_slice(&100u32.to_be_bytes());
		b.extend_from_slice(&50u32.to_be_bytes());
		b.extend_from_slice(&[8, 2, 0, 0, 0]);
		// CRC 검증은 기존 계약에 포함되지 않는다.
		b.extend_from_slice(&[0, 0, 0, 0]);
		b.extend_from_slice(&[0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]);
		assert!(valid_png(&b));
		assert!(!valid_png(b"not png"));
		assert!(!valid_png(&[137, 80, 78, 71]));
	}

	#[test]
	fn describe_artifact_hashes() {
		let a = describe_artifact("C:\\dir\\oz-structure.json", "C:\\dir\\file.pdf", b"hello");
		assert_eq!(a.bytes, 5);
		assert_eq!(a.file, "file.pdf");
		assert_eq!(a.sha256, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
	}
}
