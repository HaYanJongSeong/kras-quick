use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use sha2::Sha256;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

const ITERATIONS: u32 = 600_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
	version: u32,
	salt: String,
	verifier: String,
}

struct Verifier {
	salt: [u8; 16],
	derived: [u8; 32],
}

fn decode<const N: usize>(text: &str) -> Option<[u8; N]> {
	let bytes = STANDARD.decode(text).ok()?;
	if STANDARD.encode(&bytes) != text { return None; }
	bytes.try_into().ok()
}

fn parse_settings(bytes: &[u8]) -> Option<Verifier> {
	if bytes.len() > 4096 { return None; }
	let settings: Settings = serde_json::from_slice(bytes).ok()?;
	if settings.version != 1 { return None; }
	Some(Verifier { salt: decode(&settings.salt)?, derived: decode(&settings.verifier)? })
}

fn ordinary(metadata: &fs::Metadata) -> bool {
	if metadata.file_type().is_symlink() { return false; }
	#[cfg(windows)]
	{
		use std::os::windows::fs::MetadataExt;
		if metadata.file_attributes() & 0x400 != 0 { return false; }
	}
	true
}

fn load_settings(path: &Path) -> Option<Verifier> {
	for ancestor in path.ancestors() {
		if ancestor.as_os_str().is_empty() { continue; }
		let metadata = fs::symlink_metadata(ancestor).ok()?;
		if !ordinary(&metadata) { return None; }
		if ancestor == path {
			if !metadata.is_file() || metadata.len() > 4096 { return None; }
		} else if !metadata.is_dir() { return None; }
	}
	let file = fs::File::open(path).ok()?;
	let metadata = file.metadata().ok()?;
	if !ordinary(&metadata) || !metadata.is_file() || metadata.len() > 4096 { return None; }
	let mut bytes = Vec::new();
	file.take(4097).read_to_end(&mut bytes).ok()?;
	parse_settings(&bytes)
}

fn settings_path() -> Option<PathBuf> {
	let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
	if !root.is_absolute() { return None; }
	Some(root.join("kras-quick").join("local-access").join("verifier.json"))
}

fn accepts(verifier: &Verifier, input: io::Result<String>) -> bool {
	let Ok(candidate) = input else { return false; };
	if candidate.is_empty() { return false; }
	let mut derived = [0u8; 32];
	pbkdf2::pbkdf2_hmac::<Sha256>(candidate.as_bytes(), &verifier.salt, ITERATIONS, &mut derived);
	bool::from(derived.ct_eq(&verifier.derived))
}

fn read_buffered<R: io::Read>(reader: &mut kras_quick::quick_address::BufferedLineReader<R>) -> io::Result<String> {
	let candidate = reader.try_read_line()?;
	if reader.is_eof() {
		return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
	}
	Ok(candidate)
}

pub fn request_access() -> bool {
	let Some(verifier) = settings_path().and_then(|path| load_settings(&path)) else { return false; };
	kras_quick::task_screen::pause_task_screen();
	kras_quick::output::write("암호 > ");
	if io::stdout().flush().is_err() { return false; }
	let input = if io::stdin().is_terminal() {
		rpassword::read_password()
	} else {
		read_buffered(&mut *kras_quick::quick_address::lock_stdin_reader())
	};
	accepts(&verifier, input)
}

#[cfg(test)]
mod tests {
	use super::*;

	const SECRET: &str = "fixture-only-secret";

	fn fixture() -> Verifier {
		static FIXTURE: std::sync::OnceLock<Verifier> = std::sync::OnceLock::new();
		let verifier = FIXTURE.get_or_init(|| {
			let salt = [0x37; 16];
			let mut derived = [0; 32];
			pbkdf2::pbkdf2_hmac::<Sha256>(SECRET.as_bytes(), &salt, ITERATIONS, &mut derived);
			Verifier { salt, derived }
		});
		Verifier { salt: verifier.salt, derived: verifier.derived }
	}

	fn fixture_json() -> serde_json::Value {
		let verifier = fixture();
		serde_json::json!({"version": 1, "salt": STANDARD.encode(verifier.salt), "verifier": STANDARD.encode(verifier.derived)})
	}

	#[test]
	fn accepts_only_exact_input() {
		let verifier = fixture();
		assert!(accepts(&verifier, Ok(SECRET.into())));
		for candidate in [String::new(), "wrong".into(), SECRET.to_uppercase(), format!(" {SECRET}"), format!("{SECRET} ")] {
			assert!(!accepts(&verifier, Ok(candidate)));
		}
	}

	#[test]
	fn read_errors_and_eof_are_denied() {
		for kind in [io::ErrorKind::UnexpectedEof, io::ErrorKind::Other] {
			assert!(!accepts(&fixture(), Err(io::Error::from(kind))));
		}
	}

	#[test]
	fn buffered_input_preserves_next_line_and_rejects_unterminated_eof() {
		use kras_quick::quick_address::BufferedLineReader;
		let input = format!("{SECRET}\r\nnext\n");
		let mut reader = BufferedLineReader::new(input.as_bytes(), 256);
		assert_eq!(read_buffered(&mut reader).unwrap(), SECRET);
		assert_eq!(reader.try_read_line().unwrap(), "next");
		assert!(read_buffered(&mut reader).is_err());
		let input = SECRET;
		let mut reader = BufferedLineReader::new(input.as_bytes(), 256);
		assert!(read_buffered(&mut reader).is_err());
	}

	#[test]
	fn buffered_read_failure_is_denied() {
		struct FailedRead;
		impl io::Read for FailedRead {
			fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
				Err(io::Error::from(io::ErrorKind::Other))
			}
		}
		let mut reader = kras_quick::quick_address::BufferedLineReader::new(FailedRead, 256);
		assert!(read_buffered(&mut reader).is_err());
	}

	#[test]
	fn settings_are_strict_and_canonical() {
		let valid = fixture_json();
		assert!(parse_settings(&serde_json::to_vec(&valid).unwrap()).is_some());
		for (field, value) in [
			("version", serde_json::json!(2)), ("version", serde_json::json!("1")),
			("salt", serde_json::json!("!")), ("salt", serde_json::json!(STANDARD.encode([0; 15]))),
			("salt", serde_json::json!(STANDARD.encode([0; 16]).trim_end_matches('='))),
			("verifier", serde_json::json!(STANDARD.encode([0; 31]))),
			("extra", serde_json::json!(true)),
		] {
			let mut invalid = valid.clone();
			invalid[field] = value;
			assert!(parse_settings(&serde_json::to_vec(&invalid).unwrap()).is_none());
		}
		for bytes in [b"{".as_slice(), b"{}", &[b' '; 4097]] {
			assert!(parse_settings(bytes).is_none());
		}
	}

	#[test]
	fn files_must_be_regular_and_not_links() {
		let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
		let root = std::env::temp_dir().join(format!("restricted-access-{}-{id}", std::process::id()));
		fs::create_dir(&root).unwrap();
		let file = root.join("verifier.json");
		assert!(load_settings(&file).is_none());
		assert!(load_settings(&root).is_none());
		fs::write(&file, serde_json::to_vec(&fixture_json()).unwrap()).unwrap();
		assert!(load_settings(&file).is_some());
		let link = root.join("link.json");
		#[cfg(unix)]
		let linked = std::os::unix::fs::symlink(&file, &link);
		#[cfg(windows)]
		let linked = std::os::windows::fs::symlink_file(&file, &link);
		#[cfg(any(unix, windows))]
		if linked.is_ok() {
			assert!(load_settings(&link).is_none());
			fs::remove_file(&link).unwrap();
		}
		fs::write(&file, b"corrupt").unwrap();
		assert!(load_settings(&file).is_none());
		fs::write(&file, [b' '; 4097]).unwrap();
		assert!(load_settings(&file).is_none());
		fs::remove_file(&file).unwrap();
		fs::remove_dir(&root).unwrap();
	}
}
