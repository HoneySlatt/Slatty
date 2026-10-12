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

/// Written into a partial folder as its download starts, with the game's id, then the build on a
/// line of its own, carried into the game folder by the rename and removed once the game is
/// registered: a game folder holding it was published by SlattyLauncher for that game and is
/// finished by resuming its job; one without it, or with another game's id (two products sharing a
/// folder name), is not its own, never adopted.
pub const STAGED_MARK: &str = ".slatty-staged";

/// Makes `partial` with its mark, or keeps them as an earlier attempt for the same game left them.
pub(crate) fn mark_staged(partial: &Path, game_id: &str, build_id: &str) -> Result<()> {
    use std::io::Write;
    crate::paths::ensure_dir(partial)?;
    let mark = partial.join(STAGED_MARK);
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&mark)
    {
        Ok(mut file) => file
            .write_all(format!("{game_id}\n{build_id}").as_bytes())
            .map_err(|e| Error::io(format!("write {}", mark.display()), e)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if staged_here(partial, game_id) {
                Ok(())
            } else {
                Err(Error::Refused(format!(
                    "{} holds the download of another game going into a folder of the same name",
                    partial.display()
                )))
            }
        }
        Err(e) => Err(Error::io(format!("create {}", mark.display()), e)),
    }
}

/// Whether SlattyLauncher published `target` for `game_id` (see `STAGED_MARK`).
pub(crate) fn staged_here(target: &Path, game_id: &str) -> bool {
    staged_mark(target).is_some_and(|(id, _)| id == game_id)
}

/// The build `partial` was started with for `game_id`, as its mark says; `None` for a mark an
/// earlier version wrote, without it, or another game's.
pub(crate) fn staged_build(partial: &Path, game_id: &str) -> Option<String> {
    staged_mark(partial)
        .filter(|(id, _)| id == game_id)
        .and_then(|(_, build)| build)
}

fn staged_mark(folder: &Path) -> Option<(String, Option<String>)> {
    let mark = folder.join(STAGED_MARK);
    if !std::fs::symlink_metadata(&mark).is_ok_and(|m| m.is_file() && m.len() <= 256) {
        return None;
    }
    let text = String::from_utf8(std::fs::read(&mark).ok()?).ok()?;
    let (id, build) = match text.split_once('\n') {
        Some((id, build)) => (id, Some(build.to_owned())),
        None => (text.as_str(), None),
    };
    Some((id.to_owned(), build))
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
    /// The Proton build chosen for a Windows build; `None` when not known (an update, a job
    /// written by an older version).
    pub proton: Option<PathBuf>,
}

impl InstallJob {
    /// An update, language or DLC change of an installed game, rather than a first install.
    pub fn is_update(&self) -> bool {
        self.state == crate::maintenance::UPDATING
            || self.state == crate::maintenance::UPDATE_PAUSED
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        crate::paths::check_game_id(&self.game_id)?;
        db.conn().execute(
            "INSERT INTO install_jobs (game_id, build_id, language, root, directory, state, updated_at, dlcs, proton)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(game_id) DO UPDATE SET build_id = excluded.build_id, language = excluded.language,
                root = excluded.root, directory = excluded.directory, state = excluded.state,
                updated_at = excluded.updated_at, dlcs = excluded.dlcs, proton = excluded.proton",
            params![
                self.game_id,
                self.build_id,
                self.language,
                self.root.to_string_lossy(),
                self.directory,
                self.state,
                Utc::now().timestamp(),
                self.dlcs.join(","),
                self.proton.as_ref().map(|p| p.to_string_lossy())
            ],
        )?;
        Ok(())
    }

    pub fn load(db: &Db, game_id: &str) -> Result<Option<InstallJob>> {
        Ok(db
            .conn()
            .query_row(
                "SELECT game_id, build_id, language, root, directory, state, dlcs, proton
                 FROM install_jobs WHERE game_id = ?1",
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
                        proton: r.get::<_, Option<String>>(7)?.map(PathBuf::from),
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

/// State of an install downloading, or cut off (a closed window, a crash, GOG out of reach):
/// resumed by itself.
pub const DOWNLOADING: &str = "downloading";
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
    let target = job.root.join(&job.directory);
    // The download finished and took the game folder's place; only the job lets an install finish
    // it, since a new one refuses an existing folder. A folder it did not publish is not its own.
    if !partial.exists() && staged_here(&target, game_id) {
        return Err(Error::Refused(format!(
            "{} is already downloaded; install the game again to finish it",
            target.display()
        )));
    }
    if partial.exists() {
        std::fs::remove_dir_all(&partial)
            .map_err(|e| Error::io(format!("delete {}", partial.display()), e))?;
    }
    InstallJob::delete(db, game_id)?;
    Ok(Some(partial))
}
