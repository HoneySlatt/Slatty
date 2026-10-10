//! What slatty wrote for each game, so that only its own files are ever changed or removed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil;
use crate::paths::Dirs;

use super::FileSet;

/// What slatty wrote for a game. Its presence is what marks a game as installed by slatty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub build_id: String,
    pub version: String,
    pub language: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub dlcs: Vec<String>,
    /// Build whose post-install setup has run.
    #[serde(default)]
    pub setup_build: Option<String>,
    pub files: Vec<RecordedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedFile {
    pub path: String,
    pub size: u64,
}

pub fn recorded_files(set: &FileSet) -> Vec<RecordedFile> {
    set.files
        .iter()
        .map(|(p, f)| RecordedFile {
            path: p.to_string_lossy().into_owned(),
            size: f.size(),
        })
        .collect()
}

impl InstallRecord {
    pub fn file(dirs: &Dirs, game_id: &str) -> PathBuf {
        dirs.data.join("manifests").join(format!("{game_id}.json"))
    }

    pub fn load(dirs: &Dirs, game_id: &str) -> Result<Option<InstallRecord>> {
        let path = Self::file(dirs, game_id);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| Error::parse("install record", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
        }
    }

    pub fn save(&self, dirs: &Dirs, game_id: &str) -> Result<()> {
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| Error::parse("install record", e))?;
        fsutil::write_atomic(&Self::file(dirs, game_id), &json)
    }
}
