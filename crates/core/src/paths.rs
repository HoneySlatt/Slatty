use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Dirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
    pub runtime: Option<PathBuf>,
}

impl Dirs {
    pub fn from_system() -> Result<Self> {
        let p = ProjectDirs::from("", "", "slatty")
            .ok_or_else(|| Error::Unsupported("no home directory".into()))?;
        Ok(Self {
            config: p.config_dir().to_path_buf(),
            data: p.data_dir().to_path_buf(),
            cache: p.cache_dir().to_path_buf(),
            state: p.state_dir().unwrap_or(p.data_local_dir()).to_path_buf(),
            runtime: p.runtime_dir().map(Path::to_path_buf),
        })
    }

    pub fn under(root: &Path) -> Self {
        Self {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
            state: root.join("state"),
            runtime: Some(root.join("runtime")),
        }
    }

    /// Creates SlattyLauncher's folders readable by the user only (0700), and closes those an
    /// earlier version left readable by others: they hold the library, play times, saves
    /// backups and Wine prefixes.
    pub fn keep_private(&self) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        for dir in [&self.config, &self.data, &self.cache, &self.state] {
            ensure_dir(dir)?;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| Error::io(format!("protect {}", dir.display()), e))?;
        }
        Ok(())
    }

    pub fn db_file(&self) -> PathBuf {
        self.data.join("state.db")
    }

    pub fn account_cache(&self, user_id: &str) -> PathBuf {
        self.cache.join(user_id)
    }

    pub fn backups(&self, user_id: &str, game_id: &str) -> PathBuf {
        self.data.join("backups").join(user_id).join(game_id)
    }

    pub fn locks(&self) -> PathBuf {
        self.state.join("locks")
    }

    pub fn logs(&self) -> PathBuf {
        self.state.join("logs")
    }
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|e| Error::io(format!("create {}", path.display()), e))
}

/// Refuses a game id that could not safely name a file: GOG's ids are letters and digits.
pub fn check_game_id(game_id: &str) -> Result<()> {
    if game_id.is_empty() || !game_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Refused(format!("unexpected game id `{game_id}`")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn game_ids_unsafe_in_a_file_name_are_refused() {
        assert!(check_game_id("1456487183").is_ok());
        for id in ["", "..", "../1", "1/2", "1.2", "-1", "4 4", "é"] {
            assert!(
                matches!(check_game_id(id), Err(Error::Refused(_))),
                "{id:?}"
            );
        }
    }

    #[test]
    fn slatty_files_are_readable_by_the_user_only() {
        let root = std::env::temp_dir().join(format!("slatty-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        // Left open by an earlier version.
        ensure_dir(&dirs.data).unwrap();
        std::fs::set_permissions(&dirs.data, std::fs::Permissions::from_mode(0o755)).unwrap();
        dirs.keep_private().unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        for dir in [&dirs.config, &dirs.data, &dirs.cache, &dirs.state] {
            assert_eq!(mode(dir), 0o700, "{}", dir.display());
        }
        let db = crate::db::Db::open(&dirs.db_file()).unwrap();
        drop(db);
        assert_eq!(mode(&dirs.db_file()), 0o600);
        std::fs::remove_dir_all(root).unwrap();
    }
}
