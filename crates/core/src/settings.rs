use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::error::Result;

const LIBRARY_ROOT: &str = "library_root";
const DEFAULT_PROTON: &str = "default_proton";
const FAVORITES: &str = "favorites";

/// Folder that receives installed games (`~/Games/GOG` until chosen).
pub fn library_root(db: &Db) -> Result<PathBuf> {
    Ok(match db.setting(LIBRARY_ROOT)? {
        Some(p) => PathBuf::from(p),
        None => home().join("Games/GOG"),
    })
}

pub fn set_library_root(db: &Db, path: &Path) -> Result<()> {
    db.set_setting(LIBRARY_ROOT, Some(&path.to_string_lossy()))
}

pub fn default_proton(db: &Db) -> Result<Option<PathBuf>> {
    Ok(db.setting(DEFAULT_PROTON)?.map(PathBuf::from))
}

pub fn set_default_proton(db: &Db, path: &Path) -> Result<()> {
    db.set_setting(DEFAULT_PROTON, Some(&path.to_string_lossy()))
}

/// Game ids marked as favorites.
pub fn favorites(db: &Db) -> Result<Vec<String>> {
    Ok(db
        .setting(FAVORITES)?
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default())
}

pub fn set_favorites(db: &Db, ids: &[String]) -> Result<()> {
    db.set_setting(FAVORITES, Some(&ids.join(",")))
}

/// Proton builds already on disk (Steam compatibility tools folder).
pub fn proton_candidates() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> =
        std::fs::read_dir(home().join(".local/share/Steam/compatibilitytools.d"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("proton").is_file())
            .collect();
    found.sort();
    found
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}
