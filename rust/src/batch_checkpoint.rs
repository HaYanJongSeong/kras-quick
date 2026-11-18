use std::io::{self, Write};
use std::path::Path;

pub fn load(path: &Path) -> io::Result<Vec<String>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

pub fn save(path: &Path, addresses: &[String]) -> io::Result<()> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let bytes = serde_json::to_vec_pretty(addresses).map_err(io::Error::other)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
    let temporary = path.with_extension(format!("{}-{stamp}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        if let Err(error) = std::fs::remove_file(&temporary) {
            if error.kind() != io::ErrorKind::NotFound { eprintln!("재개용 임시 파일 정리 실패: {error}"); }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("kras-checkpoint-{}-{stamp}", std::process::id()))
    }
    #[test]
    fn restart_restores_remaining_addresses_in_order() {
        let root = root();
        let path = root.join("resume.json");
        let original = vec!["서울특별시 종로구 궁정동 17-4".to_string(), "next".to_string()];
        save(&path, &original).unwrap();
        assert_eq!(load(&path).unwrap(), original);
        save(&path, &original[1..]).unwrap();
        assert_eq!(load(&path).unwrap(), original[1..]);
        save(&path, &[]).unwrap();
        assert!(load(&path).unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_checkpoint_is_reported_without_modification() {
        let root = root();
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("resume.json");
        std::fs::write(&path, b"damaged").unwrap();
        assert_eq!(load(&path).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&path).unwrap(), b"damaged");
        std::fs::remove_dir_all(root).unwrap();
    }
}
