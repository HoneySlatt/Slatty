use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};

/// Advisory lock released when dropped.
#[derive(Debug)]
pub struct FileLock(#[allow(dead_code)] File);

pub fn try_acquire(path: &Path) -> Result<Option<FileLock>> {
    if let Some(parent) = path.parent() {
        crate::paths::ensure_dir(parent)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| Error::io(format!("open lock {}", path.display()), e))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(FileLock(file))),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(Error::io(format!("lock {}", path.display()), e)),
    }
}

/// Held while a game is installed, changed, repaired, uninstalled, synced or played, so that two of
/// these never touch the same game at once, even from different processes.
pub fn game(dirs: &crate::paths::Dirs, game_id: &str) -> Result<FileLock> {
    crate::store::of(game_id)?;
    let busy = || {
        Error::Refused(
            "another operation on this game is running (install, update, repair, sync or play)"
                .into(),
        )
    };
    let lock = try_acquire(&dirs.locks().join(format!("game-{game_id}.lock")))?.ok_or_else(busy)?;
    // A game whose launcher closed or crashed still runs under its session supervisor.
    if try_acquire(&session(dirs, game_id))?.is_none() {
        return Err(busy());
    }
    Ok(lock)
}

/// Held by the session supervisor for as long as a game (or its setup) runs, even once the
/// launcher that started it is gone.
pub fn session(dirs: &crate::paths::Dirs, game_id: &str) -> std::path::PathBuf {
    dirs.locks().join(format!("session-{game_id}.lock"))
}

pub async fn acquire(path: &Path, timeout: Duration) -> Result<FileLock> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(lock) = try_acquire(path)? {
            return Ok(lock);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Refused(format!(
                "{} is held by another operation",
                path.display()
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_is_refused_until_release() {
        let dir = std::env::temp_dir().join(format!("slatty-lock-{}", std::process::id()));
        let path = dir.join("a.lock");
        let first = try_acquire(&path).unwrap().unwrap();
        assert!(try_acquire(&path).unwrap().is_none());
        drop(first);
        assert!(try_acquire(&path).unwrap().is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
