use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn write_new(path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
    OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::write_new;
    use std::io::ErrorKind;
    use std::sync::{Arc, Barrier};

    fn directory() -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("kras-exclusive-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn existing_output_is_preserved_when_writer_arrives_late() {
        let root = directory();
        let path = root.join("report.xml");
        assert!(!path.exists());
        std::fs::write(&path, b"original").unwrap();

        let error = write_new(&path, b"replacement").unwrap_err();

        assert_eq!(error.kind(), ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_writers_produce_one_whole_output() {
        let root = directory();
        let path = root.join("report.pdf");
        let barrier = Arc::new(Barrier::new(2));
        let payloads = [vec![b'A'; 65536], vec![b'B'; 65536]];

        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = payloads.iter().map(|payload| {
                let barrier = Arc::clone(&barrier);
                let path = &path;
                scope.spawn(move || { barrier.wait(); write_new(path, payload) })
            }).collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
        });

        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter_map(|result| result.as_ref().err()).next().unwrap().kind(), ErrorKind::AlreadyExists);
        let winner = results.iter().position(|result| result.is_ok()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), payloads[winner]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
