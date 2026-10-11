//! Logs of the programs the launcher starts (games, prefix creation, setup, the Galaxy service,
//! Comet). Each run starts a log of its own and the few runs before are kept, `.1` the latest: a
//! game's log holds what the game printed, tickets for the account among it, and a log shared
//! to report a problem should not carry every session since the install.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::paths::Dirs;

/// Runs kept before the current one.
pub const KEPT: u32 = 2;

/// The logs named after a game, each with its kept runs.
const PER_GAME: [&str; 4] = ["game", "prefix", "setup", "galaxy-service"];

/// The log of a game's `kind` of run (`game`, `prefix`, `setup`, `galaxy-service`).
pub fn of_game(dirs: &Dirs, kind: &str, game_id: &str) -> PathBuf {
    dirs.logs().join(format!("{kind}-{game_id}.log"))
}

/// Makes room for a new run at `path`: the current log becomes `.1`, the one before `.2`, and the
/// oldest kept goes. Nothing to do for a log not written yet.
pub fn start_run(path: &Path) {
    let kept = |n: u32| PathBuf::from(format!("{}.{n}", path.display()));
    let _ = std::fs::remove_file(kept(KEPT));
    for n in (1..KEPT).rev() {
        let _ = std::fs::rename(kept(n), kept(n + 1));
    }
    let _ = std::fs::rename(path, kept(1));
}

/// Deletes the logs of an uninstalled game, with their kept runs.
pub fn remove_game(dirs: &Dirs, game_id: &str) -> Result<()> {
    crate::paths::check_owned_id(game_id)?;
    for kind in PER_GAME {
        let log = of_game(dirs, kind, game_id);
        let _ = std::fs::remove_file(&log);
        for n in 1..=KEPT {
            let _ = std::fs::remove_file(format!("{}.{n}", log.display()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each run starts an empty log; the two before are kept, older ones go.
    #[test]
    fn each_run_keeps_the_two_before() {
        let root = std::env::temp_dir().join(format!("slatty-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        std::fs::create_dir_all(dirs.logs()).unwrap();
        let log = of_game(&dirs, "game", "1");
        start_run(&log);
        for run in ["first", "second", "third", "fourth"] {
            start_run(&log);
            std::fs::write(&log, run).unwrap();
        }
        let read = |name: &str| std::fs::read_to_string(dirs.logs().join(name)).ok();
        assert_eq!(read("game-1.log").as_deref(), Some("fourth"));
        assert_eq!(read("game-1.log.1").as_deref(), Some("third"));
        assert_eq!(read("game-1.log.2").as_deref(), Some("second"));
        assert_eq!(read("game-1.log.3"), None);

        std::fs::write(of_game(&dirs, "setup", "1"), "setup").unwrap();
        std::fs::write(of_game(&dirs, "game", "2"), "another game").unwrap();
        remove_game(&dirs, "1").unwrap();
        let mut left: Vec<_> = std::fs::read_dir(dirs.logs())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["game-2.log"], "only the uninstalled game's logs go");
        assert!(remove_game(&dirs, "../1").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
