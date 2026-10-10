//! Unfinished installs and updates, kept so they resume with the same build, language and folder.

use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::Install;
use crate::paths::Dirs;

const PARTIAL_SUFFIX: &str = ".slatty-partial";

pub fn partial_dir(root: &Path, directory: &str) -> PathBuf {
    root.join(format!(".{directory}{PARTIAL_SUFFIX}"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallJob {
    pub game_id: String,
    pub build_id: String,
    pub language: String,
    pub root: PathBuf,
    pub directory: String,
    pub state: String,
    pub dlcs: Vec<String>,
}

impl InstallJob {
    /// An update, language or DLC change of an installed game, rather than a first install.
    pub fn is_update(&self) -> bool {
        self.state == crate::maintenance::UPDATING
            || self.state == crate::maintenance::UPDATE_PAUSED
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        crate::store::require_gog(&self.game_id)?;
        db.conn().execute(
            "INSERT INTO install_jobs (game_id, build_id, language, root, directory, state, updated_at, dlcs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(game_id) DO UPDATE SET build_id = excluded.build_id, language = excluded.language,
                root = excluded.root, directory = excluded.directory, state = excluded.state,
                updated_at = excluded.updated_at, dlcs = excluded.dlcs",
            params![
                self.game_id,
                self.build_id,
                self.language,
                self.root.to_string_lossy(),
                self.directory,
                self.state,
                Utc::now().timestamp(),
                self.dlcs.join(",")
            ],
        )?;
        Ok(())
    }

    pub fn load(db: &Db, game_id: &str) -> Result<Option<InstallJob>> {
        Ok(db
            .conn()
            .query_row(
                "SELECT game_id, build_id, language, root, directory, state, dlcs FROM install_jobs WHERE game_id = ?1",
                [game_id],
                |r| {
                    Ok(InstallJob {
                        game_id: r.get(0)?,
                        build_id: r.get(1)?,
                        language: r.get(2)?,
                        root: PathBuf::from(r.get::<_, String>(3)?),
                        directory: r.get(4)?,
                        state: r.get(5)?,
                        dlcs: r
                            .get::<_, String>(6)?
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                            .collect(),
                    })
                },
            )
            .optional()?)
    }

    pub fn list(db: &Db) -> Result<Vec<InstallJob>> {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT game_id FROM install_jobs ORDER BY updated_at")?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        drop(conn);
        ids.iter()
            .filter_map(|id| Self::load(db, id).transpose())
            .collect()
    }

    pub fn is_queued(&self) -> bool {
        self.state == QUEUED
    }

    /// Paused on request (Pause, Ctrl+C), rather than interrupted by a crash or a closed window.
    pub fn is_paused(&self) -> bool {
        self.state == PAUSED || self.state == crate::maintenance::UPDATE_PAUSED
    }

    /// Drops the jobs of installs that were registered just before an interruption left their
    /// job behind.
    pub fn forget_finished(db: &Db) -> Result<()> {
        db.conn().execute(
            "DELETE FROM install_jobs WHERE state NOT IN (?1, ?2)
             AND game_id IN (SELECT game_id FROM installs)",
            [
                crate::maintenance::UPDATING,
                crate::maintenance::UPDATE_PAUSED,
            ],
        )?;
        Ok(())
    }

    pub fn delete(db: &Db, game_id: &str) -> Result<()> {
        db.conn()
            .execute("DELETE FROM install_jobs WHERE game_id = ?1", [game_id])?;
        Ok(())
    }
}

/// State of a job stopped on request.
pub const PAUSED: &str = "paused";
/// State of an install waiting in the download queue: chosen, not started.
pub const QUEUED: &str = "queued";
/// State of an install stopped by an error: resumed only when asked.
pub const FAILED: &str = "failed";

/// Abandons an unfinished install: deletes its hidden partial folder and its job. An unfinished
/// update is refused here, since its files are the installed game itself.
pub fn discard(db: &Db, dirs: &Dirs, game_id: &str) -> Result<Option<PathBuf>> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let Some(job) = InstallJob::load(db, game_id)? else {
        return Ok(None);
    };
    if Install::get(db, game_id)?.is_some() {
        return Err(Error::Refused(format!(
            "{game_id} is installed; an unfinished update must be completed, not discarded"
        )));
    }
    let partial = partial_dir(&job.root, &job.directory);
    if partial.exists() {
        std::fs::remove_dir_all(&partial)
            .map_err(|e| Error::io(format!("delete {}", partial.display()), e))?;
    }
    InstallJob::delete(db, game_id)?;
    Ok(Some(partial))
}
