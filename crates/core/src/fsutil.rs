use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.slatty-tmp-{}", std::process::id()))
}

/// Flushes everything written to the filesystem holding `path`: one call after many files is far
/// cheaper than syncing each of them.
pub fn sync_filesystem(path: &Path) -> Result<()> {
    let dir =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    nix::unistd::syncfs(&dir).map_err(|e| Error::io(format!("sync {}", path.display()), e.into()))
}

/// Writes to a sibling temp file, fsyncs, then renames over `path`.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::io(
            path.display().to_string(),
            std::io::ErrorKind::InvalidInput.into(),
        )
    })?;
    crate::paths::ensure_dir(parent)?;
    let tmp = temp_sibling(path);
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    result.map_err(|e: std::io::Error| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(format!("write {}", path.display()), e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("slatty-fsutil-{}", std::process::id()));
        let file = dir.join("a.txt");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
