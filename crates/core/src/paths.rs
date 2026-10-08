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
