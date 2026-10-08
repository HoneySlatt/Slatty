use std::collections::BTreeSet;
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use super::scan;
use super::sync::remote_map;
use super::transport::CloudTransport;
use crate::error::Result;
use crate::fsutil;
use crate::gameinfo::resolve_relative;

#[derive(Debug, Clone)]
pub struct LocalSide {
    pub size: u64,
    pub sha256: String,
    pub modified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct RemoteSide {
    pub listed_hash: String,
    pub last_modified: Option<String>,
    pub size: usize,
    pub sha256: String,
    /// Downloaded bytes still look gzip-compressed (transport decoding problem).
    pub gzip_magic: bool,
    pub copy: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FileComparison {
    pub path: String,
    pub local: Option<LocalSide>,
    pub remote: Option<RemoteSide>,
}

impl FileComparison {
    pub fn identical(&self) -> bool {
        matches!((&self.local, &self.remote), (Some(l), Some(r)) if l.sha256 == r.sha256)
    }
}

/// Read-only: downloads cloud copies into `copies`, never touches the save folder.
pub async fn compare<T: CloudTransport>(
    cloud: &T,
    location: &str,
    root: &std::path::Path,
    copies: &std::path::Path,
    filter: Option<&str>,
) -> Result<Vec<FileComparison>> {
    let local = scan::scan(root)?;
    let remote = remote_map(cloud.list().await?, location)?;
    let filter = filter.map(str::to_lowercase);
    let keys: BTreeSet<&String> = local.files.keys().chain(remote.keys()).collect();
    let mut out = Vec::new();
    for key in keys
        .into_iter()
        .filter(|k| filter.as_ref().is_none_or(|f| k.contains(f.as_str())))
    {
        let local_side = local.files.get(key).map(|f| LocalSide {
            size: f.size,
            sha256: f.sha256.clone(),
            modified: std::fs::metadata(&f.abs)
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::from),
        });
        let remote_side = match remote.get(key) {
            None => None,
            Some(r) => {
                let bytes = cloud.download(&r.name).await?;
                let copy = resolve_relative(copies, &r.rel)?;
                fsutil::write_atomic(&copy, &bytes)?;
                Some(RemoteSide {
                    listed_hash: r.hash.clone(),
                    last_modified: r.last_modified.clone(),
                    size: bytes.len(),
                    sha256: fsutil::sha256_hex(&bytes),
                    gzip_magic: bytes.starts_with(&[0x1f, 0x8b]),
                    copy,
                })
            }
        };
        let path = local
            .files
            .get(key)
            .map(|f| f.rel.clone())
            .or_else(|| remote.get(key).map(|r| r.rel.clone()))
            .unwrap_or_else(|| key.clone());
        out.push(FileComparison {
            path,
            local: local_side,
            remote: remote_side,
        });
    }
    Ok(out)
}
