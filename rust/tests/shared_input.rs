use kras_quick::quick_address::BufferedLineReader;

#[test]
fn menu_captcha_and_return_keep_prefetched_lines_and_leading_zeroes() {
	let mut reader = BufferedLineReader::new(std::io::Cursor::new(b"3\n01234\n2\n"), 8192);
	assert_eq!(reader.read_line(), "3");
	assert_eq!(reader.try_read_line().unwrap(), "01234");
	assert_eq!(reader.read_line(), "2");
}

#[test]
fn captcha_reader_propagates_input_errors() {
	struct Broken;
	impl std::io::Read for Broken {
		fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
			Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
		}
	}
	let mut reader = BufferedLineReader::new(Broken, 8192);
	assert_eq!(reader.try_read_line().unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
	assert!(!reader.is_eof());
}

#[test]
fn production_menu_and_captcha_share_stdin_reader() {
	let main = include_str!("../src/main.rs");
	let captcha = include_str!("../src/stage12.rs");
	assert!(main.contains("quick_address::lock_stdin_reader as lock_reader"));
	assert!(!main.contains("OnceLock"));
	assert!(captcha.contains("crate::quick_address::lock_stdin_reader()"));
	assert!(!captcha.contains("stdin().read_line"));
}
