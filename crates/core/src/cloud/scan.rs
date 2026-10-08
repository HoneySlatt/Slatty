use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::fsutil;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub rel: String,
    pub abs: PathBuf,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct LocalScan {
    pub root_exists: bool,
    /// Keyed by lowercase relative path: Windows saves are case-insensitive.
    pub files: BTreeMap<String, ScannedFile>,
    pub skipped: Vec<String>,
}

pub fn key(rel: &str) -> String {
    rel.to_lowercase()
}

pub fn scan(root: &Path) -> Result<LocalScan> {
    let mut out = LocalScan {
        root_exists: root.is_dir(),
        ..Default::default()
    };
    if out.root_exists {
        walk(root, root, &mut out)?;
    }
    Ok(out)
}

fn walk(root: &Path, dir: &Path, out: &mut LocalScan) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| Error::io(format!("list {}", dir.display()), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::io(format!("list {}", dir.display()), e))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| Error::io(format!("stat {}", path.display()), e))?;
        let Some(rel) = relative(root, &path) else {
            out.skipped
                .push(format!("{} (name is not UTF-8)", path.display()));
            continue;
        };
        if file_type.is_symlink() {
            out.skipped.push(format!("{rel} (symbolic link)"));
        } else if file_type.is_dir() {
            walk(root, &path, out)?;
        } else if file_type.is_file() {
            if rel
                .rsplit('/')
                .next()
                .is_some_and(|n| n.contains(".slatty-tmp-"))
            {
                continue;
            }
            let (sha256, size) = hash_file(&path)?;
            let file = ScannedFile {
                rel: rel.clone(),
                abs: path,
                sha256,
                size,
            };
            if let Some(previous) = out.files.insert(key(&rel), file) {
                return Err(Error::Refused(format!(
                    "`{}` and `{rel}` differ only by case; Windows saves cannot hold both",
                    previous.rel
                )));
            }
        }
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    Some(parts?.join("/"))
}

pub fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut f =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((fsutil::hex(&hasher.finalize()), size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_nested_files_with_forward_slashes() {
        let root = std::env::temp_dir().join(format!("slatty-scan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Slot1")).unwrap();
        std::fs::write(root.join("Slot1/Save.DAT"), b"abc").unwrap();
        std::fs::write(root.join(".x.slatty-tmp-1"), b"tmp").unwrap();
        std::os::unix::fs::symlink("/etc/hostname", root.join("link")).unwrap();
        let s = scan(&root).unwrap();
        let f = &s.files["slot1/save.dat"];
        assert_eq!((f.rel.as_str(), f.size), ("Slot1/Save.DAT", 3));
        assert_eq!(f.sha256, fsutil::sha256_hex(b"abc"));
        assert_eq!(s.files.len(), 1);
        assert_eq!(s.skipped, vec!["link (symbolic link)"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_root_is_reported_not_created() {
        let root = std::env::temp_dir().join(format!("slatty-scan-missing-{}", std::process::id()));
        let s = scan(&root).unwrap();
        assert!(!s.root_exists && s.files.is_empty() && !root.exists());
    }

    #[test]
    fn refuses_case_only_duplicates() {
        let root = std::env::temp_dir().join(format!("slatty-scan-case-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.sav"), b"1").unwrap();
        std::fs::write(root.join("A.sav"), b"2").unwrap();
        assert!(scan(&root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
