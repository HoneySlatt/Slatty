use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub fn temp_sibling(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut name = std::ffi::OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".slatty-tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

/// Restarts a partial download without truncating an existing link's target. The game lock
/// serializes downloads; create_new also refuses a file inserted between unlink and open.
pub(crate) fn restart_staging(path: &Path) -> Result<std::fs::File> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io(format!("remove temporary {}", path.display()), e)),
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Error::io(format!("create {}", path.display()), e))
}

/// Largest file a game writes that is read whole (its `goggame-*.info`, its `start.sh`): such files
/// are a few kilobytes, and a game could make one as large as the disk allows.
const GAME_FILE_LIMIT: u64 = 16 << 20;

/// Reads a file a game can write (its folder); `None` when it is not a regular file, as reading a
/// pipe would wait forever for a writer, or when it is larger than `GAME_FILE_LIMIT`. It is opened
/// without waiting, then checked, so it cannot be swapped for a pipe in between.
pub fn read_game_file(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > GAME_FILE_LIMIT {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    file.take(GAME_FILE_LIMIT).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Flushes everything written to the filesystem holding `path`: one call after many files is far
/// cheaper than syncing each of them.
pub fn sync_filesystem(path: &Path) -> Result<()> {
    let dir =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    nix::unistd::syncfs(&dir).map_err(|e| Error::io(format!("sync {}", path.display()), e.into()))
}

/// Writes to a sibling temp file, fsyncs, then renames over `path`.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::io(
            path.display().to_string(),
            std::io::ErrorKind::InvalidInput.into(),
        )
    })?;
    crate::paths::ensure_dir(parent)?;
    let tmp = temp_sibling(path);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| Error::io(format!("create {}", tmp.display()), e))?;
    let result = (|| {
        f.write_all(data)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    result.map_err(|e: std::io::Error| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(format!("write {}", path.display()), e)
    })
}

/// Runs `f` on a thread of its own at the lowest priority (nice 19), and the threads it starts:
/// reading and hashing a whole game (Verify, a resumed download, an update's check) or making the
/// library's covers at their size keeps several cores busy for minutes, and then only takes the
/// time the desktop and games leave. A thread of its own, as one cannot raise its priority back.
pub async fn in_background<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("slatty-background".into())
        .spawn(move || {
            // SAFETY: sets the calling thread's nice value, which Linux keeps per thread.
            unsafe { nix::libc::setpriority(nix::libc::PRIO_PROCESS, 0, 19) };
            let _ = tx.send(f());
        })
        .expect("start a thread");
    rx.await.expect("background thread panicked")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn background_work_runs_at_the_lowest_priority_and_leaves_the_caller_as_it_was() {
        // SAFETY: reads the calling thread's nice value.
        let nice = || unsafe { nix::libc::getpriority(nix::libc::PRIO_PROCESS, 0) };
        let before = nice();
        assert_eq!(in_background(nice).await, 19);
        assert_eq!(
            in_background(move || std::thread::spawn(nice).join().unwrap()).await,
            19,
            "the threads it starts too"
        );
        assert_eq!(nice(), before);
    }

    #[test]
    fn sha256_of_empty_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("slatty-fsutil-{}", std::process::id()));
        let file = dir.join("a.txt");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A game writes its `goggame-*.info` and `start.sh`, and could make one as large as its disk
    /// allows in an instant (a sparse file): such a file is not read into memory.
    #[test]
    fn a_game_file_too_large_is_not_read() {
        let dir = std::env::temp_dir().join(format!("slatty-game-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("goggame-1.info");
        std::fs::File::create(&file)
            .unwrap()
            .set_len(GAME_FILE_LIMIT + 1)
            .unwrap();
        assert!(read_game_file(&file).unwrap().is_none());
        std::fs::write(&file, b"{}").unwrap();
        assert_eq!(read_game_file(&file).unwrap().unwrap(), b"{}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A temporary name is never reused, and `write_atomic` creates its file only if the name is
    /// free: whatever already sits at a temporary name, a link included, is left alone.
    #[test]
    fn temporary_names_are_never_reused() {
        let file = Path::new("/saves/save.dat");
        let (a, b) = (temp_sibling(file), temp_sibling(file));
        assert_ne!(a, b);
        assert_eq!(a.parent(), file.parent());
        assert!(
            a.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".save.dat.slatty-tmp-")
        );
    }
}
