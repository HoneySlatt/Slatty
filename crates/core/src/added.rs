//! Games added by hand: a game from no store, started from the program the user picked. A Windows
//! program runs through umu and Proton in a prefix of its own, a Linux one as GOG's native builds
//! do. Their files are the user's: removing such a game never deletes them, except those
//! SlattyLauncher unpacked itself from a Linux installer or archive (`unpack`).
//!
//! The game is an install (`installs`, with its folder, runner and isolation) whose id is
//! `local-<number>` (`store::local_id`), never given twice; what it starts, and the files unpacked
//! for it, are kept beside the install records, in `added/<id>.json`, relative to its folder. Not
//! in an install record: those make GOG's setup, updates and checks run.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::{Install, Platform};
use crate::installer::leads_inside;
use crate::paths::Dirs;
use crate::runner::Runner;

/// The setting counting the games ever added, from which each takes its number.
const ADDED: &str = "added_games";

/// What a game added by hand starts, inside its folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Where it starts; the folder itself when empty.
    pub working_dir: PathBuf,
    /// The files SlattyLauncher unpacked into the folder, which removing the game deletes while
    /// they are as they were unpacked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unpacked: Vec<UnpackedFile>,
}

/// A file SlattyLauncher unpacked for a game, with its size and modification time just after; a
/// list written before they were kept names the file alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UnpackedFile {
    Stamped {
        path: PathBuf,
        size: u64,
        mtime: i64,
        mtime_nsec: i64,
    },
    Bare(PathBuf),
}

impl UnpackedFile {
    /// `rel` in `folder` as it is now, or `None` when nothing is there.
    fn stamp(folder: &Path, rel: PathBuf) -> Option<UnpackedFile> {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::symlink_metadata(folder.join(&rel)).ok()?;
        Some(UnpackedFile::Stamped {
            path: rel,
            size: m.len(),
            mtime: m.mtime(),
            mtime_nsec: m.mtime_nsec(),
        })
    }

    fn path(&self) -> &Path {
        match self {
            Self::Stamped { path, .. } | Self::Bare(path) => path,
        }
    }
}

/// A Linux installer or archive unpacked for a game not added yet: where, and which files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Unpacked {
    folder: PathBuf,
    files: Vec<UnpackedFile>,
}

/// A game to add.
#[derive(Debug, Clone, Default)]
pub struct NewGame {
    pub title: String,
    /// The program to start: a Windows `.exe`, a Linux program, script or AppImage.
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The game's folder, which the program is in: the program's own folder when not given. An
    /// isolated game can write in it, and nowhere else of the user's.
    pub folder: Option<PathBuf>,
    /// Where the program starts: its own folder when not given.
    pub working_dir: Option<PathBuf>,
    /// The Proton build a Windows program runs with: the default one (Settings) when not given.
    pub proton: Option<PathBuf>,
    /// Whether it runs isolated: as a game installed now would, when not given.
    pub isolated: Option<bool>,
    /// The game an installer was run for (`run_installer`): it takes that id, and the prefix the
    /// installer put it in.
    pub installed_by: Option<String>,
}

/// How a game added as `game_id` runs: natively, or through Proton (the default one when not
/// given) in a prefix of its own.
fn runner_for(
    db: &Db,
    dirs: &Dirs,
    game_id: &str,
    platform: Platform,
    proton: Option<PathBuf>,
) -> Result<Runner> {
    if platform == Platform::Linux {
        return Ok(Runner::Native);
    }
    let proton = match proton {
        Some(proton) => proton,
        None => crate::settings::default_proton(db)?.ok_or_else(|| {
            Error::Refused(
                "a Windows game needs a Proton build: choose one in Settings → Runners".into(),
            )
        })?,
    };
    if !proton.join("proton").is_file() {
        return Err(Error::Refused(format!(
            "{} is not a Proton build",
            proton.display()
        )));
    }
    Ok(Runner::Umu {
        proton,
        prefix: prefix_of(dirs, game_id),
    })
}

fn prefix_of(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("prefixes").join(game_id)
}

/// Runs a Windows installer for a game to add, in a prefix made for it, and waits for it to end:
/// the game's id, and the game's folder. That folder, named after `title` in `into` and made
/// empty, is drive D: in the prefix, where the installer is to be told to install; the game is
/// its folder, writable when isolated, and the installer's own folder is only read. The game is
/// added once its program is picked (`NewGame::installed_by`), or the prefix thrown away
/// (`discard_installer`).
#[allow(clippy::too_many_arguments)]
pub async fn run_installer(
    db: &Db,
    dirs: &Dirs,
    supervisor: &Path,
    installer: &Path,
    into: &Path,
    title: &str,
    proton: Option<PathBuf>,
    isolated: bool,
) -> Result<(String, PathBuf)> {
    let installer = real(installer)?;
    match platform_of(&installer) {
        Ok(Platform::Windows) => {}
        Ok(Platform::Linux) | Err(Error::Refused(_)) => {
            return Err(Error::Refused(format!(
                "{} is no installer or archive SlattyLauncher can use: choose a Windows \
                 installer (.exe), a Linux one (MojoSetup .sh), or a .zip, .tar.gz or .tar.xz \
                 archive",
                installer.display()
            )));
        }
        Err(e) => return Err(e),
    }
    let folder = installer
        .parent()
        .ok_or_else(|| Error::Refused(format!("{} has no folder", installer.display())))?
        .to_path_buf();
    // Shared read-only with an isolated installer: not a folder holding the home folder either.
    check_folder(db, dirs, &folder, None)?;
    crate::paths::ensure_dir(into)?;
    let target = named_folder(&real(into)?, title)?;
    let made = match std::fs::symlink_metadata(&target) {
        Ok(m) if m.is_dir() && std::fs::read_dir(&target).is_ok_and(|mut d| d.next().is_none()) => {
            false
        }
        Ok(_) => {
            return Err(Error::Refused(format!(
                "{} exists already and is not empty",
                target.display()
            )));
        }
        Err(_) => {
            std::fs::create_dir(&target)
                .map_err(|e| Error::io(format!("make {}", target.display()), e))?;
            true
        }
    };
    let ran = install_with(
        db, dirs, supervisor, &installer, &folder, &target, proton, isolated,
    )
    .await;
    // Only an empty folder goes: what the installer put there is the user's.
    if ran.is_err() && made {
        let _ = std::fs::remove_dir(&target);
    }
    ran.map(|id| (id, target))
}

/// `run_installer`, its game's folder `target` ready.
#[allow(clippy::too_many_arguments)]
async fn install_with(
    db: &Db,
    dirs: &Dirs,
    supervisor: &Path,
    installer: &Path,
    folder: &Path,
    target: &Path,
    proton: Option<PathBuf>,
    isolated: bool,
) -> Result<String> {
    use crate::session::SessionHandle;
    check_folder(db, dirs, target, None)?;
    let game_id = next_id(db, dirs)?;
    // What says this prefix holds a game waiting to be added, which alone may be thrown away; with
    // the game's folder, which goes too when the installer put nothing in it.
    crate::fsutil::write_atomic(
        &installer_file(dirs, &game_id),
        target.as_os_str().as_encoded_bytes(),
    )?;
    let install = Install {
        runner: runner_for(db, dirs, &game_id, Platform::Windows, proton)
            .inspect_err(|_| drop(std::fs::remove_file(installer_file(dirs, &game_id))))?,
        game_id,
        title: String::new(),
        platform: Platform::Windows,
        path: target.to_path_buf(),
        client_id: None,
        umu_id: None,
        isolated,
    };
    let _busy = crate::lock::game(dirs, &install.game_id)?;
    let busy = crate::lock::session(dirs, &install.game_id);
    // As before a launch: a runtime umu lacks is installed now, outside the sandbox, rather than
    // the installer refused (`runner::runtime_installed`).
    if install.isolated {
        crate::runner::refresh_runtimes(db, dirs, crate::runner::container(&install)).await;
    }
    // Nothing is installed in the prefix yet: it goes, with its mark, if the installer cannot run.
    let discard = || {
        if let Some(prefix) = install.runner.prefix() {
            let _ = std::fs::remove_dir_all(prefix);
        }
        let _ = std::fs::remove_file(installer_file(dirs, &install.game_id));
    };
    if let Some(init) =
        crate::runner::prefix_init_spec(dirs, &install).inspect_err(|_| discard())?
    {
        let log = crate::logs::of_game(dirs, "prefix", &install.game_id);
        crate::logs::start_run(&log);
        let outcome = SessionHandle::start(supervisor, &init, &log, Some(&busy))
            .await?
            .wait()
            .await?;
        if !outcome.clean || !crate::runner::prefix_ready(&install) {
            discard();
            return Err(Error::Refused(format!(
                "the Wine prefix could not be created; see {}",
                log.display()
            )));
        }
    }
    if let Some(prefix) = install.runner.prefix() {
        map_drive(prefix, target).inspect_err(|_| discard())?;
    }
    let spec = crate::runner::windows_command_reading(
        dirs,
        &install,
        vec![installer.display().to_string()],
        target.to_path_buf(),
        folder,
    )
    .inspect_err(|_| discard())?;
    let log = crate::logs::of_game(dirs, "setup", &install.game_id);
    crate::logs::start_run(&log);
    SessionHandle::start(supervisor, &spec, &log, Some(&busy))
        .await?
        .wait()
        .await?;
    Ok(install.game_id)
}

/// Makes `folder` drive D: in the Wine prefix `prefix`, for an installer to be told to install
/// there. A D: Wine left there is replaced; a `dosdevices` that is no folder of its own is not
/// written through.
pub fn map_drive(prefix: &Path, folder: &Path) -> Result<()> {
    let devices = prefix.join("dosdevices");
    match std::fs::symlink_metadata(&devices) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => {
            return Err(Error::Refused(format!(
                "{} is not a folder of the prefix",
                devices.display()
            )));
        }
        Err(_) => crate::paths::ensure_dir(&devices)?,
    }
    let drive = devices.join("d:");
    if std::fs::symlink_metadata(&drive).is_ok() {
        std::fs::remove_file(&drive)
            .map_err(|e| Error::io(format!("delete {}", drive.display()), e))?;
    }
    std::os::unix::fs::symlink(folder, &drive)
        .map_err(|e| Error::io(format!("link {}", drive.display()), e))
}

/// The folder a game added by hand is installed or unpacked into: named after its title, in
/// `into`.
fn named_folder(into: &Path, title: &str) -> Result<PathBuf> {
    // An isolated launch refuses a folder with `:` in its path (`runner::path_list`).
    let name = title.trim().replace(['/', '\\', '\0', ':'], " ");
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(Error::Refused("the game needs a title".into()));
    }
    Ok(into.join(name))
}

/// Throws away what an installer was run for, when the game is not added after all: its prefix,
/// the saves in it copied first, and its logs.
pub fn discard_installer(db: &Db, dirs: &Dirs, game_id: &str) -> Result<()> {
    if !waiting(db, dirs, game_id)? {
        return Err(Error::Refused(format!(
            "{game_id} is no game an installer was run for"
        )));
    }
    let _busy = crate::lock::game(dirs, game_id)?;
    if let Ok(unpacked) = load_unpacked(dirs, game_id) {
        let files = deletable(&unpacked.folder, &unpacked.files)?;
        let mut report = crate::maintenance::UninstallReport::default();
        crate::maintenance::delete_listed(&unpacked.folder, &files, &mut report)?;
        let _ = std::fs::remove_file(unpacked_file(dirs, game_id));
    }
    let prefix = prefix_of(dirs, game_id);
    if installer_file(dirs, game_id).exists() && prefix.exists() {
        crate::maintenance::back_up_prefix(dirs, game_id, &prefix)?;
        std::fs::remove_dir_all(&prefix)
            .map_err(|e| Error::io(format!("delete {}", prefix.display()), e))?;
    }
    // The game's folder goes when the installer put nothing in it; what it did put is the user's.
    if let Ok(target) = std::fs::read(installer_file(dirs, game_id))
        && !target.is_empty()
    {
        use std::os::unix::ffi::OsStrExt;
        let _ = std::fs::remove_dir(std::ffi::OsStr::from_bytes(&target));
    }
    let _ = std::fs::remove_file(installer_file(dirs, game_id));
    crate::logs::remove_game(dirs, game_id)
}

/// Whether `game_id` is a game an installer was run for, or an archive unpacked for, not added
/// (yet): only such a one may be added under that id, or thrown away.
fn waiting(db: &Db, dirs: &Dirs, game_id: &str) -> Result<bool> {
    Ok(crate::store::is_local(game_id)
        && Install::get(db, game_id)?.is_none()
        && (installer_file(dirs, game_id).is_file() || unpacked_file(dirs, game_id).is_file()))
}

fn installer_file(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("added").join(format!("{game_id}.installer"))
}

/// A number never given, whose id nothing is left of either (a database made again while the
/// data folder stayed would count from 1 again).
fn next_id(db: &Db, dirs: &Dirs) -> Result<String> {
    loop {
        let id = crate::store::local_id(next_number(db)?);
        let left = [
            prefix_of(dirs, &id),
            dirs.data.join("homes").join(&id),
            task_file(dirs, &id),
            installer_file(dirs, &id),
            unpacked_file(dirs, &id),
        ]
        .iter()
        .any(|p| std::fs::symlink_metadata(p).is_ok());
        if !left && Install::get(db, &id)?.is_none() {
            return Ok(id);
        }
    }
}

/// The files of `files`, relative to `folder`, to delete as the game goes: those still as they
/// were unpacked. One the game rewrote since (a save slot, settings it ships) stays, as do those
/// of a list that never said how they were. Refused whole when one would lead out of `folder`.
fn deletable(folder: &Path, files: &[UnpackedFile]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for f in files {
        let rel = crate::installer::safe_relative(&f.path().to_string_lossy())?;
        crate::installer::refuse_outside(folder, &folder.join(&rel))?;
        let unchanged = match f {
            UnpackedFile::Stamped {
                size,
                mtime,
                mtime_nsec,
                ..
            } => UnpackedFile::stamp(folder, rel.clone()).is_some_and(|now| {
                matches!(now, UnpackedFile::Stamped { size: s, mtime: m, mtime_nsec: n, .. }
                    if (s, m, n) == (*size, *mtime, *mtime_nsec))
            }),
            UnpackedFile::Bare(_) => false,
        };
        if unchanged {
            out.push(rel);
        }
    }
    Ok(out)
}

/// Whether SlattyLauncher can unpack this file for a Linux game: a MojoSetup installer (GOG's
/// offline ones, also sold elsewhere), a `.zip`, or a `.tar.gz` / `.tar.xz`.
pub fn is_archive(path: &Path) -> bool {
    let name = path.to_string_lossy().to_lowercase();
    tar_archive(&name)
        || head(path).is_ok_and(|h| h.starts_with(b"PK\x03\x04") || h.starts_with(b"#!"))
            && !name.ends_with(".exe")
}

fn tar_archive(name: &str) -> bool {
    [".tar.gz", ".tgz", ".tar.xz"]
        .iter()
        .any(|ext| name.ends_with(ext))
}

/// Unpacks a Linux game's installer or archive (`is_archive`) for a game to add, into a new
/// folder named after `title` in `into`: the game's id, and that folder. Nothing is run: a
/// MojoSetup installer's script is not, only the game files it holds (`data/noarch`) are taken.
/// Paths that climb out of the folder and links that lead out are left out. The game is then
/// added with its program picked there (`NewGame::installed_by`), or thrown away
/// (`discard_installer`); removing it deletes the files unpacked, and only those.
pub async fn unpack(
    db: &Db,
    dirs: &Dirs,
    archive: &Path,
    into: &Path,
    title: &str,
    cancel: tokio_util::sync::CancellationToken,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(String, PathBuf)> {
    let archive = real(archive)?;
    let into = real(into)?;
    let target = named_folder(&into, title)?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(Error::Refused(format!(
            "{} exists already",
            target.display()
        )));
    }
    check_folder(db, dirs, &target, None)?;
    let game_id = next_id(db, dirs)?;
    // Made new: one already there (a link planted to have files written elsewhere, or an earlier
    // attempt) is not the folder to unpack into.
    let partial = into.join(format!(".{name}.slatty-unpack"));
    std::fs::create_dir(&partial).map_err(|e| {
        let hint = if e.kind() == std::io::ErrorKind::AlreadyExists {
            " (remove it if it is left from before)"
        } else {
            ""
        };
        Error::io(format!("make {}{hint}", partial.display()), e)
    })?;
    let unpacked = async {
        let files = unpack_into(&archive, &partial, cancel, progress).await?;
        // The list holds names as text: a file whose name is not could never be deleted with the
        // game.
        if let Some(odd) = files.iter().find(|f| f.to_str().is_none()) {
            return Err(Error::Refused(format!(
                "{} holds a file whose name is not UTF-8, {}; SlattyLauncher cannot unpack it",
                archive.display(),
                odd.display()
            )));
        }
        // As they are now, to tell later whether the game rewrote one; what is not there was left
        // out.
        let files = files
            .into_iter()
            .filter_map(|rel| UnpackedFile::stamp(&partial, rel))
            .collect();
        crate::fsutil::sync_filesystem(&partial)?;
        // Kept before the folder takes its place: never a game's folder without its list.
        let list = Unpacked {
            folder: target.clone(),
            files,
        };
        save_unpacked(dirs, &game_id, &list)?;
        std::fs::rename(&partial, &target).map_err(|e| {
            let _ = std::fs::remove_file(unpacked_file(dirs, &game_id));
            Error::io(format!("publish {}", target.display()), e)
        })
    }
    .await;
    // Nothing is resumed: what was unpacked of a failed one goes.
    if unpacked.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    unpacked.map(|()| (game_id, target))
}

/// Unpacks `archive` into `partial`: the files it put there.
async fn unpack_into(
    archive: &Path,
    partial: &Path,
    cancel: tokio_util::sync::CancellationToken,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<Vec<PathBuf>> {
    use crate::installer::linux::{self, LinuxDownload, LinuxSet};
    let lower = archive.to_string_lossy().to_lowercase();
    Ok(if tar_archive(&lower) {
        let (from, to) = (archive.to_path_buf(), partial.to_path_buf());
        // The progress reaches the caller from the thread unpacking.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let work = tokio::task::spawn_blocking(move || {
            crate::protons::unpack_tar(&from, &to, false, false, &cancel, &|done, total| {
                let _ = tx.send((done, total));
            })
        });
        tokio::pin!(work);
        let unpacked = loop {
            tokio::select! {
                Some((done, total)) = rx.recv() => progress(done, total),
                r = &mut work => break r,
            }
        };
        unpacked.map_err(|e| Error::io("unpack", std::io::Error::other(e)))??;
        listed_files(partial)
    } else {
        let source = LocalArchive(archive.to_path_buf());
        let mut root = "data/noarch/";
        let mut entries = linux::read_entries_under(&source, 0, root)
            .await
            .map_err(|_| {
                Error::Refused(format!(
                    "{} is no installer or archive SlattyLauncher can unpack",
                    archive.display()
                ))
            })?;
        // A plain zip: all of it.
        if entries.is_empty() {
            root = "";
            entries = linux::read_entries_under(&source, 0, root).await?;
        }
        let set = LinuxSet::from_entries([entries.as_slice()], root)?;
        let report = |p: crate::installer::Progress| progress(p.bytes_done, p.bytes_total);
        // Filled where it is, as a repair fills a game missing every file: the caller puts the
        // folder in its place once it listed what is in it.
        LinuxDownload {
            source: &source,
            cancel,
            progress: &report,
            free_space: &crate::installer::free_space,
        }
        .check_installed(&set, partial, true)
        .await?;
        set.files
            .iter()
            .chain(&set.links)
            .map(|f| f.path.clone())
            .collect()
    })
}

/// The files and links under `folder`, relative to it.
fn listed_files(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![folder.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => dirs.push(path),
                Ok(_) => out.extend(path.strip_prefix(folder).ok().map(Path::to_path_buf)),
                Err(_) => {}
            }
        }
    }
    out
}

/// A zip or MojoSetup installer on this computer, read as GOG's are over the network.
struct LocalArchive(PathBuf);

impl crate::installer::linux::Source for LocalArchive {
    async fn size(&self, _part: usize) -> Result<u64> {
        std::fs::metadata(&self.0)
            .map(|m| m.len())
            .map_err(|e| Error::io(format!("read {}", self.0.display()), e))
    }

    async fn open(
        &self,
        _part: usize,
        start: u64,
        end: u64,
    ) -> Result<futures::stream::BoxStream<'static, Result<Vec<u8>>>> {
        use futures::StreamExt;
        let path = self.0.clone();
        let bytes = tokio::task::spawn_blocking(move || {
            use std::os::unix::fs::FileExt;
            let file = std::fs::File::open(&path)
                .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
            let mut buf = vec![0; end.saturating_sub(start) as usize];
            file.read_exact_at(&mut buf, start)
                .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
            Ok::<_, Error>(buf)
        })
        .await
        .map_err(|e| Error::io("read", std::io::Error::other(e)))??;
        Ok(futures::stream::once(async move { Ok(bytes) }).boxed())
    }
}

fn unpacked_file(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data
        .join("added")
        .join(format!("{game_id}.unpacked.json"))
}

fn save_unpacked(dirs: &Dirs, game_id: &str, unpacked: &Unpacked) -> Result<()> {
    crate::paths::check_owned_id(game_id)?;
    let json = serde_json::to_vec_pretty(unpacked).map_err(|e| Error::parse("unpacked game", e))?;
    crate::fsutil::write_atomic(&unpacked_file(dirs, game_id), &json)
}

fn load_unpacked(dirs: &Dirs, game_id: &str) -> Result<Unpacked> {
    crate::paths::check_owned_id(game_id)?;
    let path = unpacked_file(dirs, game_id);
    let bytes =
        std::fs::read(&path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::parse("unpacked game", e))
}

/// Adds a game, which takes the next number never given; its Wine prefix is made at its first
/// launch.
pub fn add(db: &Db, dirs: &Dirs, new: NewGame) -> Result<Install> {
    let title = new.title.trim().to_string();
    if title.is_empty() {
        return Err(Error::Refused("the game needs a title".into()));
    }
    let program = real(&new.program)?;
    let platform = platform_of(&program)?;
    // What an installer was run for: the prefix it installed into, or the folder it was unpacked
    // into, which is then the game's.
    let unpacked = match &new.installed_by {
        Some(id) => {
            if !waiting(db, dirs, id)? {
                return Err(Error::Refused(format!(
                    "{id} is no game an installer was run for"
                )));
            }
            load_unpacked(dirs, id).ok()
        }
        None => None,
    };
    let folder = match (&unpacked, &new.folder) {
        (Some(unpacked), _) => real(&unpacked.folder)?,
        (None, Some(folder)) => real(folder)?,
        (None, None) => program
            .parent()
            .ok_or_else(|| Error::Refused(format!("{} has no folder", program.display())))?
            .to_path_buf(),
    };
    check_folder(db, dirs, &folder, new.installed_by.as_deref())?;
    let working_dir = match &new.working_dir {
        Some(dir) => real(dir)?,
        None => program.parent().unwrap_or(&folder).to_path_buf(),
    };
    let inside = |path: &Path| path.strip_prefix(&folder).map(Path::to_path_buf).ok();
    let task = Task {
        program: inside(&program).ok_or_else(|| {
            Error::Refused(format!(
                "{} is not in the game's folder {}",
                program.display(),
                folder.display()
            ))
        })?,
        args: new.args,
        working_dir: inside(&working_dir).ok_or_else(|| {
            Error::Refused(format!(
                "{} is not in the game's folder {}",
                working_dir.display(),
                folder.display()
            ))
        })?,
        unpacked: unpacked.map(|u| u.files).unwrap_or_default(),
    };
    if !working_dir.is_dir() {
        return Err(Error::Refused(format!(
            "{} is not a folder",
            working_dir.display()
        )));
    }

    let game_id = match new.installed_by {
        Some(id) => id,
        None => next_id(db, dirs)?,
    };
    let runner = runner_for(db, dirs, &game_id, platform, new.proton)?;
    let isolated = match new.isolated {
        Some(isolated) => isolated,
        None => crate::install::isolated_when_installed(db, platform, &runner)?,
    };
    if platform == Platform::Linux {
        check_executable(&program, &task, isolated)?;
    }
    let install = Install {
        game_id,
        title,
        platform,
        path: folder,
        client_id: None,
        runner,
        umu_id: None,
        isolated,
    };
    save_task(dirs, &install.game_id, &task)?;
    if let Err(e) = install.save(db) {
        let _ = std::fs::remove_file(task_file(dirs, &install.game_id));
        return Err(e);
    }
    // Its files are now the game's, listed with what it starts.
    let _ = std::fs::remove_file(unpacked_file(dirs, &install.game_id));
    let _ = std::fs::remove_file(installer_file(dirs, &install.game_id));
    Ok(install)
}

/// The games added by hand, as the library lists them: with no image of their own (the user's
/// edits give them some) and never matched with a Steam game.
pub fn library(db: &Db) -> Result<Vec<crate::library::LibraryGame>> {
    Ok(Install::list(db)?
        .iter()
        .filter(|i| crate::store::is_local(&i.game_id))
        .map(library_game)
        .collect())
}

/// A game added by hand as the library lists it.
pub fn library_game(install: &Install) -> crate::library::LibraryGame {
    crate::library::LibraryGame {
        id: install.game_id.clone(),
        title: install.title.clone(),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: vec![
            match install.platform {
                Platform::Windows => "windows",
                Platform::Linux => "linux",
            }
            .into(),
        ],
        metadata: crate::library::MetadataSource::Missing,
        steam: Some(Vec::new()),
    }
}

/// Takes a game added by hand out of the library. Its folder is left as it is: its files are the
/// user's. Its prefix, which SlattyLauncher made, is deleted only when asked, its saves copied to
/// the backups first.
pub fn remove(
    db: &Db,
    dirs: &Dirs,
    game_id: &str,
    delete_prefix: bool,
) -> Result<crate::maintenance::UninstallReport> {
    if !crate::store::is_local(game_id) {
        return Err(Error::Refused(format!("{game_id} was not added by hand")));
    }
    // Also refused while the game runs, under its supervisor once its launcher is gone: a session
    // recorded as running with neither is one a crash left.
    let _busy = crate::lock::game(dirs, game_id)?;
    let install = Install::get(db, game_id)?
        .ok_or_else(|| Error::NotFound(format!("{game_id} is not in the library")))?;
    // What SlattyLauncher unpacked for it goes, checked whole before anything is deleted.
    let unpacked = load_task(dirs, game_id)
        .map(|t| t.unpacked)
        .unwrap_or_default();
    let unpacked = deletable(&install.path, &unpacked)?;
    // The marks of an installer run or an archive unpacked for it, which adding it forgets but a
    // crash could leave: kept, they would have its prefix thrown away later as a leftover.
    for mark in [installer_file(dirs, game_id), unpacked_file(dirs, game_id)] {
        match std::fs::remove_file(&mark) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::io(format!("delete {}", mark.display()), e));
            }
            _ => {}
        }
    }
    let mut report = crate::maintenance::UninstallReport::default();
    if let Some(prefix) = install.runner.prefix().filter(|_| delete_prefix) {
        // Its own, as SlattyLauncher named it: no other game's, nor one reached through `..`.
        if prefix != prefix_of(dirs, game_id) {
            return Err(Error::Refused(format!(
                "{} was not made by SlattyLauncher; nothing was deleted",
                prefix.display()
            )));
        }
        report.prefix_backup = crate::maintenance::back_up_prefix(dirs, game_id, prefix)?;
        // An installer put the game itself in it, with whatever it keeps beside it (saves among
        // them): copied too, before the prefix goes. Compared as they are on disk: the game's
        // folder was kept resolved, the data folder may be reached through a link.
        if crate::installer::leads_inside(prefix, &install.path) && install.path.is_dir() {
            let copy = dirs
                .data
                .join("backups/prefixes")
                .join(game_id)
                .join(format!(
                    "{}-game",
                    chrono::Utc::now().format("%Y%m%d-%H%M%S")
                ));
            crate::maintenance::copy_dir(&install.path, &copy)?;
            crate::fsutil::sync_filesystem(&copy)?;
            report.game_backup = Some(copy);
        }
        if prefix.exists() {
            std::fs::remove_dir_all(prefix)
                .map_err(|e| Error::io(format!("delete {}", prefix.display()), e))?;
            report.prefix_removed = true;
        }
    }
    if !unpacked.is_empty() {
        crate::maintenance::delete_listed(&install.path, &unpacked, &mut report)?;
    }
    Install::remove(db, game_id)?;
    let _ = std::fs::remove_file(task_file(dirs, game_id));
    crate::custom::forget(db, dirs, game_id)?;
    crate::logs::remove_game(dirs, game_id)?;
    Ok(report)
}

/// Finds a game added by hand in `folder`, where it was moved: its program must be there, at the
/// same place within it.
pub fn locate(db: &Db, dirs: &Dirs, game_id: &str, folder: &Path) -> Result<Install> {
    let mut install = Install::get(db, game_id)?
        .filter(|i| crate::store::is_local(&i.game_id))
        .ok_or_else(|| Error::NotFound(format!("{game_id} was not added by hand")))?;
    let folder = real(folder)?;
    check_folder(db, dirs, &folder, Some(game_id))?;
    let task = load_task(dirs, game_id)?;
    let program = folder.join(&task.program);
    if !program.is_file() || !leads_inside(&folder, &program) {
        return Err(Error::Refused(format!(
            "{} is not in {}",
            task.program.display(),
            folder.display()
        )));
    }
    if install.platform == Platform::Linux {
        check_executable(&program, &task, install.isolated)?;
    }
    install.path = folder;
    install.save(db)?;
    Ok(install)
}

/// What a game added by hand starts: its program, arguments and working directory, checked to be
/// in its folder still (a link could lead out of it since).
pub fn launch_task(dirs: &Dirs, install: &Install) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    let task = load_task(dirs, &install.game_id)?;
    let folder = &install.path;
    let program = folder.join(&task.program);
    let cwd = folder.join(&task.working_dir);
    for path in [&program, &cwd] {
        if !leads_inside(folder, path) {
            return Err(Error::Refused(format!(
                "{} leads out of the game's folder {}",
                path.display(),
                folder.display()
            )));
        }
    }
    if !program.is_file() {
        return Err(Error::NotFound(format!("{} is missing", program.display())));
    }
    Ok((program, task.args, cwd))
}

/// Whether a Linux program is an AppImage, which needs FUSE unless told to unpack itself.
pub fn is_appimage(program: &Path) -> bool {
    head(program).is_ok_and(|h| h.starts_with(b"\x7fELF") && h.get(8..11) == Some(b"AI\x02"))
}

/// Whether a Linux file is a program (ELF) rather than a script.
pub fn is_program(path: &Path) -> bool {
    head(path).is_ok_and(|h| h.starts_with(b"\x7fELF"))
}

/// Refuses a Linux program that would not start for want of an execute bit: a program (ELF)
/// always, a script unless isolated (then started through its interpreter). One SlattyLauncher
/// unpacked for the game is made executable instead: an archive made on Windows keeps no modes.
fn check_executable(program: &Path, task: &Task, isolated: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(program)
        .map_err(|e| Error::io(format!("read {}", program.display()), e))?
        .permissions()
        .mode();
    if mode & 0o111 != 0 || isolated && !is_program(program) {
        return Ok(());
    }
    if task.unpacked.iter().any(|f| f.path() == task.program) {
        return std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io(format!("make {} executable", program.display()), e));
    }
    Err(Error::Refused(format!(
        "{} is not executable: allow it to run as a program (chmod +x)",
        program.display()
    )))
}

/// Which system a program is for, from its first bytes: a Windows program (`MZ`), else a Linux
/// program (ELF) or script (`#!`).
pub fn platform_of(program: &Path) -> Result<Platform> {
    let head = head(program)?;
    if head.starts_with(b"MZ") {
        Ok(Platform::Windows)
    } else if head.starts_with(b"\x7fELF") || head.starts_with(b"#!") {
        Ok(Platform::Linux)
    } else {
        Err(Error::Refused(format!(
            "{} is not a program SlattyLauncher can start: choose a Windows .exe, or a Linux \
             program or script",
            program.display()
        )))
    }
}

/// The GOG game a folder holds, by the product id its `goggame-<id>.info` names: one better
/// installed from GOG, with its cloud saves and achievements.
pub fn gog_game_in(folder: &Path) -> Option<String> {
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let id = name.strip_prefix("goggame-")?.strip_suffix(".info")?;
            crate::paths::check_game_id(id)
                .is_ok()
                .then(|| id.to_string())
        })
        .next()
}

/// Refuses a game folder an isolated game, which can write in it, could reach the rest of the
/// system through: the whole disk; one holding the home folder, a folder of programs (`PATH`),
/// SlattyLauncher's own data or the session's sockets; one inside what holds settings, sockets,
/// programs or temporary files, or inside SlattyLauncher's data, but for the game's own prefix
/// (`game_id`'s), where its installer put it. Nor may it be, hold or lie inside another game's
/// folder, or hold the folder games are installed in.
fn check_folder(db: &Db, dirs: &Dirs, folder: &Path, game_id: Option<&str>) -> Result<()> {
    let real = |p: &Path| p.canonicalize().ok();
    let home = std::env::var_os("HOME").and_then(|h| real(Path::new(&h)));
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").and_then(|r| real(Path::new(&r)));
    let programs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).filter_map(|p| real(&p)).collect())
        .unwrap_or_default();
    let own = [&dirs.data, &dirs.config, &dirs.cache, &dirs.state];
    // Holding any of these gives them to the game: the temporary folders and the session's hold
    // its sockets (X11, D-Bus).
    let held = home
        .iter()
        .chain(runtime.iter())
        .chain(&programs)
        .cloned()
        .chain(["/tmp", "/var/tmp", "/dev/shm", "/run"].map(PathBuf::from))
        .chain(own.iter().filter_map(|d| real(d)))
        .any(|d| d.starts_with(folder));
    // Nor may it be inside these.
    let mut within: Vec<PathBuf> = ["/dev", "/proc", "/sys", "/etc"]
        .iter()
        .map(PathBuf::from)
        .chain(own[1..].iter().filter_map(|d| real(d)))
        .collect();
    if let Some(home) = &home {
        within.extend(
            [".config", ".local/bin", ".ssh", ".gnupg"]
                .iter()
                .map(|d| home.join(d)),
        );
    }
    let inside_data = real(&dirs.data).is_some_and(|data| {
        folder.starts_with(&data)
            && !game_id.is_some_and(|id| {
                real(&prefix_of(dirs, id)).is_some_and(|prefix| folder.starts_with(prefix))
            })
    });
    if folder.parent().is_none()
        || held
        || inside_data
        || within.iter().any(|w| folder.starts_with(w))
    {
        return Err(Error::Refused(format!(
            "{} is not a game's folder: an isolated game can write in its folder, and from \
             this one reach much more. Put the game in a folder of its own",
            folder.display()
        )));
    }
    // An isolated game could rewrite what another one starts, and removing a game deletes the
    // files listed as unpacked for it wherever its folder is. Compared as they are on disk: a
    // GOG game's folder is kept as it was chosen, links and all.
    for other in Install::list(db)? {
        if Some(other.game_id.as_str()) != game_id
            && (leads_inside(&other.path, folder) || leads_inside(folder, &other.path))
        {
            return Err(Error::Refused(format!(
                "{} holds or is part of {}'s folder; choose the game's own folder",
                folder.display(),
                other.title
            )));
        }
    }
    let library = crate::settings::library_root(db)?;
    if leads_inside(folder, &library) {
        return Err(Error::Refused(format!(
            "{} holds {}, where games are installed; choose the game's own folder",
            folder.display(),
            library.display()
        )));
    }
    Ok(())
}

/// A path as it is on the disk, links followed, once it exists.
fn real(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| Error::io(format!("find {}", path.display()), e))
}

/// The first bytes of a program, which tell what it is.
fn head(program: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
        .open(program)
        .map_err(|e| Error::io(format!("open {}", program.display()), e))?;
    if !file
        .metadata()
        .map_err(|e| Error::io(format!("read {}", program.display()), e))?
        .is_file()
    {
        return Err(Error::Refused(format!(
            "{} is not a file",
            program.display()
        )));
    }
    let mut head = Vec::with_capacity(16);
    file.take(16)
        .read_to_end(&mut head)
        .map_err(|e| Error::io(format!("read {}", program.display()), e))?;
    Ok(head)
}

/// The next number never given to a game added by hand, counted in the database at once, so that
/// two processes adding games never give the same.
fn next_number(db: &Db) -> Result<u64> {
    let n: String = db.conn().query_row(
        "INSERT INTO settings (key, value) VALUES (?1, '1')
         ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1
         RETURNING value",
        [ADDED],
        |r| r.get(0),
    )?;
    n.parse()
        .map_err(|_| Error::Refused(format!("the count of games added is `{n}`")))
}

fn task_file(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("added").join(format!("{game_id}.json"))
}

fn save_task(dirs: &Dirs, game_id: &str, task: &Task) -> Result<()> {
    crate::paths::check_owned_id(game_id)?;
    let json = serde_json::to_vec_pretty(task).map_err(|e| Error::parse("added game", e))?;
    crate::fsutil::write_atomic(&task_file(dirs, game_id), &json)
}

fn load_task(dirs: &Dirs, game_id: &str) -> Result<Task> {
    crate::paths::check_owned_id(game_id)?;
    let path = task_file(dirs, game_id);
    let bytes =
        std::fs::read(&path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::parse("added game", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        root: PathBuf,
        dirs: Dirs,
        db: Db,
        game: PathBuf,
        proton: PathBuf,
    }

    /// A game folder holding a Windows program, a Linux program and a script, and a Proton build.
    fn fake(name: &str) -> Fake {
        let root = std::env::temp_dir().join(format!("slatty-added-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("games/[FAKE] Game");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        std::fs::write(game.join("bin/game.exe"), b"MZ\x90\0").unwrap();
        std::fs::write(game.join("bin/game"), b"\x7fELF\x02\x01\x01").unwrap();
        std::fs::write(game.join("start.sh"), b"#!/bin/sh\nexec ./bin/game\n").unwrap();
        for program in ["bin/game", "start.sh"] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(game.join(program), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let proton = root.join("proton");
        std::fs::create_dir_all(&proton).unwrap();
        std::fs::write(proton.join("proton"), b"").unwrap();
        Fake {
            dirs: Dirs::under(&root.join("slatty")),
            db: Db::in_memory().unwrap(),
            root,
            game,
            proton,
        }
    }

    impl Fake {
        fn windows(&self) -> NewGame {
            NewGame {
                title: " [FAKE] Game ".into(),
                program: self.game.join("bin/game.exe"),
                args: vec!["-windowed".into()],
                folder: Some(self.game.clone()),
                proton: Some(self.proton.clone()),
                isolated: Some(false),
                ..Default::default()
            }
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn programs_are_told_apart_by_their_first_bytes() {
        let f = fake("kinds");
        let at = |rel: &str| f.game.join(rel);
        assert_eq!(platform_of(&at("bin/game.exe")).unwrap(), Platform::Windows);
        assert_eq!(platform_of(&at("bin/game")).unwrap(), Platform::Linux);
        assert_eq!(platform_of(&at("start.sh")).unwrap(), Platform::Linux);
        std::fs::write(at("readme.txt"), b"[FAKE] read me").unwrap();
        assert!(platform_of(&at("readme.txt")).is_err());
        assert!(platform_of(&f.game).is_err(), "a folder");
        assert!(is_program(&at("bin/game")) && !is_program(&at("start.sh")));
        let mut appimage = b"\x7fELF\x02\x01\x01\0AI\x02".to_vec();
        appimage.resize(16, 0);
        std::fs::write(at("game.AppImage"), appimage).unwrap();
        assert!(is_appimage(&at("game.AppImage")) && !is_appimage(&at("bin/game")));
    }

    /// Each game takes a number never given before, even once an earlier one was removed: what was
    /// kept of a removed game (its sessions, its home) never goes to another.
    #[test]
    fn a_game_added_by_hand_takes_a_number_never_given_again() {
        let f = fake("numbers");
        let first = add(&f.db, &f.dirs, f.windows()).unwrap();
        assert_eq!(first.game_id, "local-1");
        assert_eq!(first.title, "[FAKE] Game");
        assert_eq!(first.platform, Platform::Windows);
        assert_eq!(first.path, f.game.canonicalize().unwrap());
        assert_eq!(
            first.runner,
            Runner::Umu {
                proton: f.proton.clone(),
                prefix: f.dirs.data.join("prefixes/local-1"),
            }
        );
        assert_eq!(
            Install::get(&f.db, "local-1").unwrap().as_ref(),
            Some(&first)
        );
        let listed = library(&f.db).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].id.as_str(), listed[0].title.as_str()),
            ("local-1", "[FAKE] Game")
        );
        assert_eq!(listed[0].os, ["windows"]);
        assert_eq!(
            listed[0].steam,
            Some(Vec::new()),
            "never a Steam game's twin"
        );
        // One game per folder.
        remove(&f.db, &f.dirs, "local-1", false).unwrap();
        assert_eq!(add(&f.db, &f.dirs, f.windows()).unwrap().game_id, "local-2");
        remove(&f.db, &f.dirs, "local-2", false).unwrap();
        assert_eq!(add(&f.db, &f.dirs, f.windows()).unwrap().game_id, "local-3");
    }

    /// The game starts as the user said, with GOG left out: no store for umu to look it up in.
    #[test]
    fn a_game_added_by_hand_starts_its_program_with_its_arguments() {
        let f = fake("start");
        let install = add(&f.db, &f.dirs, f.windows()).unwrap();
        let (program, args, cwd) = launch_task(&f.dirs, &install).unwrap();
        let game = f.game.canonicalize().unwrap();
        assert_eq!(program, game.join("bin/game.exe"));
        assert_eq!(args, ["-windowed"]);
        assert_eq!(cwd, game.join("bin"), "the program's folder until chosen");
        assert!(
            crate::doctor::find_in_path("umu-run").is_some(),
            "run in `nix develop`"
        );
        let spec = crate::runner::launch_spec(&f.dirs, &install, None).unwrap();
        assert_eq!(
            spec.args,
            [
                game.join("bin/game.exe").display().to_string(),
                "-windowed".into()
            ]
        );
        assert_eq!(spec.cwd, game.join("bin"));
        let env = |key: &str| {
            spec.env
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(env("GAMEID"), Some(crate::umu::UNKNOWN));
        assert_eq!(env("STORE"), None, "not GOG's");

        // A Linux program, with the folder it is in as the game's: one game per folder.
        remove(&f.db, &f.dirs, &install.game_id, false).unwrap();
        let linux = add(
            &f.db,
            &f.dirs,
            NewGame {
                title: "[FAKE] Native".into(),
                program: f.game.join("bin/game"),
                args: vec!["--fullscreen".into()],
                isolated: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (linux.platform, &linux.runner),
            (Platform::Linux, &Runner::Native)
        );
        assert_eq!(linux.path, game.join("bin"));
        let spec = crate::runner::launch_spec(&f.dirs, &linux, None).unwrap();
        assert!(
            spec.args.ends_with(&["--fullscreen".into()]),
            "{:?}",
            spec.args
        );
        assert!(
            spec.program == game.join("bin/game")
                || spec
                    .args
                    .contains(&game.join("bin/game").display().to_string()),
            "{spec:?}"
        );
    }

    #[test]
    fn a_folder_too_wide_or_a_program_out_of_its_folder_is_refused() {
        let f = fake("folders");
        let mut outside = f.windows();
        outside.folder = Some(f.root.join("games/elsewhere"));
        std::fs::create_dir_all(f.root.join("games/elsewhere")).unwrap();
        assert!(add(&f.db, &f.dirs, outside).is_err());
        let mut root = f.windows();
        root.folder = Some("/".into());
        assert!(add(&f.db, &f.dirs, root).is_err(), "the whole disk");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap())
            .canonicalize()
            .unwrap();
        let refused = |folder: &Path| check_folder(&f.db, &f.dirs, folder, None).is_err();
        assert!(refused(&home), "the home folder");
        assert!(refused(home.parent().unwrap()));
        // What an isolated game could reach the rest through.
        for folder in [
            PathBuf::from("/tmp"),
            PathBuf::from("/run"),
            PathBuf::from("/etc/[FAKE]"),
            home.join(".config/[FAKE]"),
            home.join(".local/bin"),
        ] {
            assert!(refused(&folder), "{}", folder.display());
        }
        let bin = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .find_map(|p| p.canonicalize().ok())
            .unwrap();
        assert!(refused(&bin), "a folder of programs");
        std::fs::create_dir_all(f.dirs.data.join("prefixes/local-7/drive_c/Game")).unwrap();
        let in_prefix = f.dirs.data.join("prefixes/local-7/drive_c/Game");
        assert!(refused(&f.dirs.data), "SlattyLauncher's data");
        assert!(refused(&in_prefix), "another game's prefix");
        assert!(
            check_folder(
                &f.db,
                &f.dirs,
                &in_prefix.canonicalize().unwrap(),
                Some("local-7")
            )
            .is_ok(),
            "its own prefix, where its installer put it"
        );
        assert!(
            !refused(&f.game.canonicalize().unwrap()),
            "a folder of its own"
        );
        let mut untitled = f.windows();
        untitled.title = "  ".into();
        assert!(add(&f.db, &f.dirs, untitled).is_err());
        assert!(Install::list(&f.db).unwrap().is_empty(), "nothing added");
    }

    /// Another game's folder, one inside or holding it, and one holding the folder games are
    /// installed in are refused: an isolated game could rewrite what the other starts, and removing
    /// one could delete the other's files.
    #[test]
    fn a_folder_shared_with_another_game_is_refused() {
        let f = fake("shared");
        let other = f.root.join("games/GOG/[FAKE] Other");
        std::fs::create_dir_all(other.join("bin")).unwrap();
        std::fs::write(other.join("bin/game.exe"), b"MZ").unwrap();
        Install {
            game_id: "1".into(),
            title: "[FAKE] Other".into(),
            platform: Platform::Linux,
            path: other.clone(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: false,
        }
        .save(&f.db)
        .unwrap();
        std::fs::write(f.root.join("games/GOG/[FAKE].exe"), b"MZ").unwrap();
        let mut holding = f.windows();
        holding.program = f.root.join("games/GOG/[FAKE].exe");
        holding.folder = None;
        let refused = add(&f.db, &f.dirs, holding).unwrap_err().to_string();
        assert!(refused.contains("[FAKE] Other"), "{refused}");
        let mut above = f.windows();
        above.folder = Some(f.root.join("games"));
        assert!(add(&f.db, &f.dirs, above).is_err());
        let mut inside = f.windows();
        inside.program = other.join("bin/game.exe");
        inside.folder = None;
        assert!(add(&f.db, &f.dirs, inside).is_err());

        // Nor is a game added by hand located there, though its program is there too.
        add(&f.db, &f.dirs, f.windows()).unwrap();
        assert!(locate(&f.db, &f.dirs, "local-1", &other).is_err());
        assert!(
            locate(&f.db, &f.dirs, "local-1", &f.game).is_ok(),
            "its own"
        );

        let library = f.root.join("lib/GOG");
        std::fs::create_dir_all(&library).unwrap();
        crate::settings::set_library_root(&f.db, &library).unwrap();
        std::fs::write(f.root.join("lib/[FAKE].exe"), b"MZ").unwrap();
        let mut above_library = f.windows();
        above_library.program = f.root.join("lib/[FAKE].exe");
        above_library.folder = None;
        assert!(add(&f.db, &f.dirs, above_library).is_err());
    }

    /// Removing a game added by hand leaves its folder: the files are the user's. Its prefix goes
    /// only when asked, its saves copied first.
    #[test]
    fn removing_a_game_added_by_hand_keeps_its_files() {
        let f = fake("remove");
        let install = add(&f.db, &f.dirs, f.windows()).unwrap();
        let prefix = install.runner.prefix().unwrap().to_path_buf();
        std::fs::create_dir_all(prefix.join("drive_c/users/steamuser")).unwrap();
        std::fs::write(prefix.join("drive_c/users/steamuser/save.dat"), b"[FAKE]").unwrap();
        crate::custom::save(
            &f.db,
            &f.dirs,
            "local-1",
            crate::custom::Changes {
                title: "[FAKE] Mine".into(),
                sort_title: String::new(),
                hidden: false,
                cover: crate::custom::ImageChange::Keep,
                background: crate::custom::ImageChange::Keep,
            },
        )
        .unwrap();
        assert!(remove(&f.db, &f.dirs, "1", false).is_err(), "a GOG game");

        let report = remove(&f.db, &f.dirs, "local-1", true).unwrap();
        assert!(report.prefix_removed && !prefix.exists());
        let backup = report.prefix_backup.unwrap();
        assert!(backup.join("steamuser/save.dat").is_file());
        assert!(
            f.game.join("bin/game.exe").is_file(),
            "the user's files stay"
        );
        assert_eq!(Install::get(&f.db, "local-1").unwrap(), None);
        assert!(load_task(&f.dirs, "local-1").is_err());
        assert!(crate::custom::all(&f.db).unwrap().is_empty());
    }

    /// A session a crashed launcher left as running does not keep the game in the library: one
    /// still running holds the game's session lock, under its supervisor.
    #[test]
    fn a_session_a_crash_left_running_does_not_keep_the_game() {
        let f = fake("stale-session");
        add(&f.db, &f.dirs, f.windows()).unwrap();
        crate::session::record_start(&f.db, "local-1", None).unwrap();
        let running = crate::lock::try_acquire(&crate::lock::session(&f.dirs, "local-1"))
            .unwrap()
            .unwrap();
        assert!(remove(&f.db, &f.dirs, "local-1", false).is_err());
        drop(running);
        remove(&f.db, &f.dirs, "local-1", false).unwrap();
    }

    #[test]
    fn a_game_moved_out_of_its_folder_through_a_link_is_not_started() {
        let f = fake("link");
        let install = add(&f.db, &f.dirs, f.windows()).unwrap();
        let elsewhere = f.root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("game.exe"), b"MZ").unwrap();
        std::fs::remove_dir_all(f.game.join("bin")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, f.game.join("bin")).unwrap();
        assert!(launch_task(&f.dirs, &install).is_err());
    }

    /// A game moved elsewhere is found there again, by its program at the same place.
    #[test]
    fn a_moved_game_is_located_by_its_program() {
        let f = fake("locate");
        let install = add(&f.db, &f.dirs, f.windows()).unwrap();
        let moved = f.root.join("moved/[FAKE] Game");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(&f.game, &moved).unwrap();
        assert!(launch_task(&f.dirs, &install).is_err(), "missing");
        assert!(
            locate(&f.db, &f.dirs, "local-1", &f.root).is_err(),
            "not there"
        );
        let found = locate(&f.db, &f.dirs, "local-1", &moved).unwrap();
        assert_eq!(found.path, moved.canonicalize().unwrap());
        assert_eq!(Install::get(&f.db, "local-1").unwrap(), Some(found.clone()));
        assert!(launch_task(&f.dirs, &found).is_ok());
        assert!(locate(&f.db, &f.dirs, "1", &moved).is_err(), "a GOG game");
    }

    /// A game an installer put in its prefix is added under the id the installer ran for, with
    /// that prefix; or thrown away with it, its saves copied first.
    #[test]
    fn a_game_installed_by_an_installer_keeps_its_prefix() {
        let f = fake("installer");
        let prefix = f.dirs.data.join("prefixes/local-1");
        let game = prefix.join("drive_c/Games/[FAKE] Quest");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("quest.exe"), b"MZ").unwrap();
        next_number(&f.db).unwrap();
        // As `run_installer` leaves them.
        let waits = |id: &str| {
            crate::fsutil::write_atomic(&installer_file(&f.dirs, id), b"").unwrap();
        };
        assert!(
            add(&f.db, &f.dirs, new_quest(&f, &game, "local-1")).is_err(),
            "a prefix no installer was run for"
        );
        waits("local-1");
        let new = |id: &str| NewGame {
            title: "[FAKE] Quest".into(),
            program: game.join("quest.exe"),
            proton: Some(f.proton.clone()),
            installed_by: Some(id.into()),
            ..Default::default()
        };
        assert!(add(&f.db, &f.dirs, new("local-2")).is_err(), "no prefix");
        assert!(add(&f.db, &f.dirs, new("1")).is_err(), "not added by hand");
        let install = add(&f.db, &f.dirs, new("local-1")).unwrap();
        assert_eq!(install.game_id, "local-1");
        assert_eq!(install.runner.prefix(), Some(prefix.as_path()));
        assert!(
            add(&f.db, &f.dirs, new("local-1")).is_err(),
            "added already"
        );
        assert!(
            discard_installer(&f.db, &f.dirs, "local-1").is_err(),
            "a game"
        );

        let other = f.dirs.data.join("prefixes/local-2");
        std::fs::create_dir_all(other.join("drive_c/users/steamuser")).unwrap();
        std::fs::write(other.join("drive_c/users/steamuser/save"), b"[FAKE]").unwrap();
        waits("local-2");
        discard_installer(&f.db, &f.dirs, "local-2").unwrap();
        assert!(!other.exists());
        assert!(f.dirs.data.join("backups/prefixes/local-2").is_dir());

        // Removed with its prefix kept, the game is no installer's to throw away: its prefix,
        // which holds the game, stays.
        remove(&f.db, &f.dirs, "local-1", false).unwrap();
        assert!(discard_installer(&f.db, &f.dirs, "local-1").is_err());
        assert!(game.join("quest.exe").is_file());
        assert!(
            add(&f.db, &f.dirs, new_quest(&f, &game, "local-1")).is_err(),
            "nor taken again"
        );
        // Removed with its prefix, the game in it is copied first.
        let id = {
            let prefix = f.dirs.data.join("prefixes/local-3");
            let game = prefix.join("drive_c/Games/[FAKE] Quest");
            std::fs::create_dir_all(&game).unwrap();
            std::fs::write(game.join("quest.exe"), b"MZ").unwrap();
            std::fs::write(game.join("save.dat"), b"[FAKE]").unwrap();
            next_number(&f.db).unwrap();
            waits("local-3");
            add(&f.db, &f.dirs, new_quest(&f, &game, "local-3"))
                .unwrap()
                .game_id
        };
        let report = remove(&f.db, &f.dirs, &id, true).unwrap();
        assert!(report.prefix_removed);
        let backups = f.dirs.data.join("backups/prefixes/local-3");
        let copy = std::fs::read_dir(&backups)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        assert!(copy.join("save.dat").is_file(), "{}", copy.display());
        assert_eq!(report.game_backup, Some(copy), "told where");
    }

    /// The data folder reached through a link (moved to another disk): the game an installer put
    /// in its prefix is still copied before the prefix is deleted with it.
    #[test]
    fn the_game_in_its_prefix_is_copied_when_the_data_folder_is_a_link() {
        let mut f = fake("linked-data");
        std::fs::create_dir_all(f.root.join("real")).unwrap();
        std::os::unix::fs::symlink(f.root.join("real"), f.root.join("link")).unwrap();
        f.dirs = Dirs::under(&f.root.join("link"));
        let game = f
            .dirs
            .data
            .join("prefixes/local-1/drive_c/Games/[FAKE] Quest");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("quest.exe"), b"MZ").unwrap();
        std::fs::write(game.join("save.dat"), b"[FAKE] progress").unwrap();
        next_number(&f.db).unwrap();
        crate::fsutil::write_atomic(&installer_file(&f.dirs, "local-1"), b"").unwrap();
        add(&f.db, &f.dirs, new_quest(&f, &game, "local-1")).unwrap();
        let report = remove(&f.db, &f.dirs, "local-1", true).unwrap();
        assert!(report.prefix_removed);
        let backups = f.dirs.data.join("backups/prefixes/local-1");
        assert!(
            std::fs::read_dir(&backups)
                .unwrap()
                .flatten()
                .any(|e| e.path().join("save.dat").is_file()),
            "the game and its save copied first"
        );
    }

    /// A crash between adding the game an installer ran for and forgetting the installer's mark
    /// left the mark: removing the game forgets it too, so the prefix holding the game is never
    /// thrown away later as an installer's leftover.
    #[test]
    fn removing_a_game_forgets_the_mark_of_its_installer() {
        let f = fake("stale-mark");
        let game = f
            .dirs
            .data
            .join("prefixes/local-1/drive_c/Games/[FAKE] Quest");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("quest.exe"), b"MZ").unwrap();
        next_number(&f.db).unwrap();
        crate::fsutil::write_atomic(&installer_file(&f.dirs, "local-1"), b"").unwrap();
        add(&f.db, &f.dirs, new_quest(&f, &game, "local-1")).unwrap();
        // As a crash just after adding it would leave it.
        crate::fsutil::write_atomic(&installer_file(&f.dirs, "local-1"), b"").unwrap();
        remove(&f.db, &f.dirs, "local-1", false).unwrap();
        assert!(discard_installer(&f.db, &f.dirs, "local-1").is_err());
        assert!(game.join("quest.exe").is_file());
    }

    fn new_quest(f: &Fake, game: &Path, id: &str) -> NewGame {
        NewGame {
            title: "[FAKE] Quest".into(),
            program: game.join("quest.exe"),
            proton: Some(f.proton.clone()),
            installed_by: Some(id.into()),
            ..Default::default()
        }
    }

    fn zip_file<'a>(name: &'a str, data: &'a [u8]) -> crate::installer::zip::build::File<'a> {
        crate::installer::zip::build::File {
            name,
            data,
            mode: if name.ends_with('/') {
                0o40755
            } else {
                0o100755
            },
            deflate: true,
        }
    }

    async fn unpacked(f: &Fake, archive: &Path, title: &str) -> Result<(String, PathBuf)> {
        let into = f.root.join("library");
        std::fs::create_dir_all(&into).unwrap();
        unpack(
            &f.db,
            &f.dirs,
            archive,
            &into,
            title,
            Default::default(),
            &|_, _| {},
        )
        .await
    }

    /// A MojoSetup installer gives its game files alone, its script never run; the game added
    /// from them is removed with them, and only them.
    #[tokio::test]
    async fn a_linux_installer_is_unpacked_without_running_its_script() {
        let f = fake("mojosetup");
        let marker = f.root.join("script-ran");
        let script = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
        let files = [
            zip_file("scripts/config.lua", b"-- [FAKE] not the game"),
            zip_file("data/noarch/", b""),
            zip_file("data/noarch/start.sh", b"#!/bin/sh\n./game/run\n"),
            zip_file("data/noarch/game/run", b"\x7fELF[FAKE]"),
        ];
        let archive = f.root.join("[FAKE]_game_1.0.sh");
        std::fs::write(
            &archive,
            crate::installer::zip::build::installer(script.as_bytes(), &files, false),
        )
        .unwrap();
        assert!(is_archive(&archive));
        let (id, folder) = unpacked(&f, &archive, "[FAKE] Game").await.unwrap();
        assert_eq!(
            folder,
            f.root.join("library/[FAKE] Game").canonicalize().unwrap()
        );
        assert!(folder.join("start.sh").is_file() && folder.join("game/run").is_file());
        assert!(!folder.join("scripts").exists(), "only the game's files");
        assert!(!marker.exists(), "the script never runs");
        assert!(
            unpacked(&f, &archive, "[FAKE] Game").await.is_err(),
            "there already"
        );

        let install = add(
            &f.db,
            &f.dirs,
            NewGame {
                title: "[FAKE] Game".into(),
                program: folder.join("start.sh"),
                isolated: Some(false),
                installed_by: Some(id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (install.game_id.as_str(), &install.path),
            (id.as_str(), &folder)
        );
        std::fs::write(folder.join("save.dat"), b"[FAKE] mine").unwrap();
        let report = remove(&f.db, &f.dirs, &id, false).unwrap();
        assert_eq!(report.removed_files, 2);
        assert!(!folder.join("start.sh").exists() && !folder.join("game").exists());
        assert!(
            folder.join("save.dat").is_file(),
            "what the game wrote stays"
        );
    }

    /// Removing a game deletes what SlattyLauncher unpacked for it only as it was unpacked: a file
    /// the game rewrote since (a save slot or settings it ships) stays, listed as kept.
    #[tokio::test]
    async fn files_the_game_rewrote_stay_when_it_is_removed() {
        let f = fake("rewritten");
        let zip = f.root.join("[FAKE] game.zip");
        let files = [
            zip_file("Game/run.sh", b"#!/bin/sh\n"),
            zip_file("Game/save.dat", b"[FAKE] empty slot"),
        ];
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(b"", &files, false),
        )
        .unwrap();
        let (id, folder) = unpacked(&f, &zip, "[FAKE] Saved").await.unwrap();
        add(
            &f.db,
            &f.dirs,
            NewGame {
                title: "[FAKE] Saved".into(),
                program: folder.join("Game/run.sh"),
                isolated: Some(false),
                installed_by: Some(id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(folder.join("Game/save.dat"), b"[FAKE] my progress").unwrap();
        let report = remove(&f.db, &f.dirs, &id, false).unwrap();
        assert_eq!(
            std::fs::read(folder.join("Game/save.dat")).unwrap(),
            b"[FAKE] my progress"
        );
        assert!(
            report.kept.contains(&PathBuf::from("Game/save.dat")),
            "{:?}",
            report.kept
        );
        assert!(!folder.join("Game/run.sh").exists());
    }

    /// A game added before the state of its files was kept: its list still reads, and removing it
    /// deletes none of them, rather than files the game may have rewritten.
    #[test]
    fn a_list_without_the_state_of_its_files_deletes_nothing() {
        let task: Task = serde_json::from_str(
            r#"{"program":"run.sh","args":[],"working_dir":"","unpacked":["run.sh"]}"#,
        )
        .unwrap();
        assert_eq!(task.unpacked, [UnpackedFile::Bare("run.sh".into())]);
        let f = fake("bare-list");
        std::fs::write(f.game.join("run.sh"), b"#!/bin/sh\n").unwrap();
        assert!(deletable(&f.game, &task.unpacked).unwrap().is_empty());
    }

    /// A link left out because it led out of the folder was never put there: a file the user puts
    /// at its place later stays when the game is removed.
    #[tokio::test]
    async fn a_link_left_out_of_an_archive_is_no_file_of_slattys() {
        let f = fake("left-out-link");
        let zip = f.root.join("[FAKE] game.zip");
        let files = [
            zip_file("Game/run.sh", b"#!/bin/sh\n"),
            crate::installer::zip::build::File {
                name: "Game/lib.so",
                data: b"/etc/[FAKE]",
                mode: 0o120777,
                deflate: false,
            },
        ];
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(b"", &files, false),
        )
        .unwrap();
        let (id, folder) = unpacked(&f, &zip, "[FAKE] Linked").await.unwrap();
        assert!(std::fs::symlink_metadata(folder.join("Game/lib.so")).is_err());
        add(
            &f.db,
            &f.dirs,
            NewGame {
                title: "[FAKE] Linked".into(),
                program: folder.join("Game/run.sh"),
                isolated: Some(false),
                installed_by: Some(id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(folder.join("Game/lib.so"), b"[FAKE] mine").unwrap();
        remove(&f.db, &f.dirs, &id, false).unwrap();
        assert_eq!(
            std::fs::read(folder.join("Game/lib.so")).unwrap(),
            b"[FAKE] mine"
        );
    }

    /// A Linux program without an execute bit (an AppImage a browser saved) would not start: it is
    /// refused, saying how to allow it, as is such a script not isolated (then started itself, not
    /// through its interpreter). One SlattyLauncher unpacked for the game is made executable.
    #[tokio::test]
    async fn a_linux_program_that_may_not_run_is_refused_unless_unpacked_for_it() {
        use std::os::unix::fs::PermissionsExt;
        let f = fake("not-executable");
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode();
        for program in ["bin/game", "start.sh"] {
            std::fs::set_permissions(f.game.join(program), std::fs::Permissions::from_mode(0o644))
                .unwrap();
        }
        let new = |program: &str, isolated| NewGame {
            title: "[FAKE] Native".into(),
            program: f.game.join(program),
            isolated: Some(isolated),
            ..Default::default()
        };
        let refused = add(&f.db, &f.dirs, new("bin/game", true))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("executable"), "{refused}");
        assert!(add(&f.db, &f.dirs, new("start.sh", false)).is_err());
        add(&f.db, &f.dirs, new("start.sh", true)).unwrap();
        assert_eq!(mode(&f.game.join("bin/game")) & 0o111, 0, "the user's file");

        // A zip made without modes, as on Windows.
        let zip = f.root.join("[FAKE] game.zip");
        let files = [crate::installer::zip::build::File {
            name: "Game/game",
            data: b"\x7fELF[FAKE]",
            mode: 0o100644,
            deflate: true,
        }];
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(b"", &files, false),
        )
        .unwrap();
        let (id, folder) = unpacked(&f, &zip, "[FAKE] Zipped").await.unwrap();
        add(
            &f.db,
            &f.dirs,
            NewGame {
                title: "[FAKE] Zipped".into(),
                program: folder.join("Game/game"),
                isolated: Some(false),
                installed_by: Some(id),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(mode(&folder.join("Game/game")) & 0o777, 0o755);
    }

    /// An installer SlattyLauncher neither runs nor unpacks (a .7z, a .msi) is refused saying
    /// which it takes, not which programs it starts.
    #[tokio::test]
    async fn an_installer_neither_run_nor_unpacked_is_refused_saying_which_are() {
        let f = fake("seven-zip");
        let archive = f.root.join("[FAKE] game.7z");
        std::fs::write(&archive, b"7z\xbc\xaf\x27\x1c[FAKE]").unwrap();
        assert!(!is_archive(&archive));
        let supervisor = f.root.join("[FAKE] supervisor");
        let into = f.root.join("games");
        let refused = run_installer(
            &f.db,
            &f.dirs,
            &supervisor,
            &archive,
            &into,
            "[FAKE] Game",
            None,
            false,
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(refused.contains(".tar.gz"), "{refused}");
    }

    /// A Windows installer installs into a folder of its own named after the game's title: one
    /// already holding files is refused before anything is made, it being someone else's.
    #[tokio::test]
    async fn a_windows_installer_refuses_a_folder_already_holding_files() {
        let f = fake("installer-full-folder");
        let installer = f.root.join("setup/[FAKE] setup.exe");
        std::fs::create_dir_all(installer.parent().unwrap()).unwrap();
        std::fs::write(&installer, b"MZ\x90\0").unwrap();
        let into = f.root.join("games");
        let target = into.join("[FAKE] Quest");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("[FAKE] mine.txt"), b"[FAKE]").unwrap();
        let refused = run_installer(
            &f.db,
            &f.dirs,
            &f.root.join("[FAKE] supervisor"),
            &installer,
            &into,
            " [FAKE] Quest ",
            Some(f.proton.clone()),
            false,
        )
        .await;
        assert!(
            matches!(&refused, Err(Error::Refused(m)) if m.contains("not empty")),
            "{refused:?}"
        );
        assert!(target.join("[FAKE] mine.txt").is_file(), "left as it is");
        assert!(!installer_file(&f.dirs, "local-1").exists());
        assert!(!prefix_of(&f.dirs, "local-1").exists());
    }

    /// A game an installer ran for, thrown away, takes its folder along only when the installer
    /// put nothing in it: what it did put there stays.
    #[test]
    fn a_discarded_installers_folder_goes_only_when_empty() {
        let f = fake("installer-folder-discard");
        for (id, full) in [("local-1", false), ("local-2", true)] {
            let target = f.root.join(format!("games/[FAKE] {id}"));
            std::fs::create_dir_all(&target).unwrap();
            if full {
                std::fs::write(target.join("game.exe"), b"MZ").unwrap();
            }
            let mark = installer_file(&f.dirs, id);
            crate::fsutil::write_atomic(&mark, target.as_os_str().as_encoded_bytes()).unwrap();
            std::fs::create_dir_all(prefix_of(&f.dirs, id).join("drive_c")).unwrap();
            discard_installer(&f.db, &f.dirs, id).unwrap();
            assert_eq!(target.exists(), full, "{id}");
            assert!(!prefix_of(&f.dirs, id).exists());
        }
    }

    /// The game's folder is drive D: in its prefix, where the installer is told to install; a D:
    /// Wine left there goes, the other drives stay.
    #[test]
    fn the_game_folder_is_drive_d_in_its_prefix() {
        let f = fake("drive-d");
        let prefix = f.root.join("prefix");
        let devices = prefix.join("dosdevices");
        std::fs::create_dir_all(&devices).unwrap();
        std::os::unix::fs::symlink("../drive_c", devices.join("c:")).unwrap();
        std::os::unix::fs::symlink("/[FAKE] cdrom", devices.join("d:")).unwrap();
        map_drive(&prefix, &f.game).unwrap();
        assert_eq!(std::fs::read_link(devices.join("d:")).unwrap(), f.game);
        assert_eq!(
            std::fs::read_link(devices.join("c:")).unwrap(),
            Path::new("../drive_c")
        );
        // A prefix whose `dosdevices` is no folder of its own is not written through.
        let odd = f.root.join("odd-prefix");
        std::fs::create_dir_all(&odd).unwrap();
        std::os::unix::fs::symlink(&f.game, odd.join("dosdevices")).unwrap();
        assert!(map_drive(&odd, &f.game).is_err());
        assert!(!f.game.join("d:").exists());
    }

    /// A Windows installer refused before it starts (here, isolated through a Proton build umu
    /// would start outside its container) leaves nothing behind: neither a prefix nor the mark of a
    /// game waiting to be added, which no one could discard, the caller never told its id. umu is
    /// not run: the day's refresh of its runtimes counts as done.
    #[tokio::test]
    async fn an_installer_refused_before_it_starts_leaves_nothing() {
        let f = fake("installer-refused");
        crate::settings::set_umu_runtimes_checked_at(&f.db, chrono::Utc::now().timestamp())
            .unwrap();
        let installer = f.root.join("setup/[FAKE] setup.exe");
        std::fs::create_dir_all(installer.parent().unwrap()).unwrap();
        std::fs::write(&installer, b"MZ\x90\0").unwrap();
        let supervisor = f.root.join("[FAKE] supervisor");
        let into = f.root.join("games");
        let refused = run_installer(
            &f.db,
            &f.dirs,
            &supervisor,
            &installer,
            &into,
            "[FAKE] Quest",
            Some(f.proton.clone()),
            true,
        )
        .await;
        assert!(
            matches!(&refused, Err(Error::Refused(m)) if m.contains("outside umu's container")),
            "{refused:?}"
        );
        assert!(!installer_file(&f.dirs, "local-1").exists());
        assert!(!prefix_of(&f.dirs, "local-1").exists());
        assert!(
            !into.join("[FAKE] Quest").exists(),
            "its folder, made empty"
        );
    }

    /// Only a folder an earlier attempt left is said to be one to remove: a name the disk refuses
    /// (too long, or with characters FAT or SMB refuse) is told plainly.
    #[tokio::test]
    async fn an_unpack_folder_that_cannot_be_made_says_why() {
        let f = fake("unpack-folder");
        let zip = f.root.join("[FAKE] game.zip");
        let files = [zip_file("run.sh", b"#!/bin/sh\n")];
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(b"", &files, false),
        )
        .unwrap();
        let long = "[FAKE] ".repeat(40);
        let refused = unpacked(&f, &zip, &long).await.unwrap_err().to_string();
        assert!(!refused.contains("left from before"), "{refused}");
        std::fs::create_dir_all(f.root.join("library/.[FAKE] Left.slatty-unpack")).unwrap();
        let refused = unpacked(&f, &zip, "[FAKE] Left")
            .await
            .unwrap_err()
            .to_string();
        assert!(refused.contains("left from before"), "{refused}");
    }

    /// A tarball holding a file whose name is not UTF-8, which the list of what was unpacked
    /// cannot hold, is refused whole: neither its folder nor a list is left, so it can be tried
    /// again.
    #[tokio::test]
    async fn an_archive_whose_files_cannot_be_listed_leaves_nothing() {
        use std::os::unix::ffi::OsStrExt;
        let f = fake("not-utf8");
        let mut tar = tar::Builder::new(Vec::new());
        for name in [&b"Game/run.sh"[..], b"Game/\xff[FAKE].dat"] {
            let mut header = tar::Header::new_gnu();
            header.set_size(10);
            header.set_mode(0o755);
            let name = std::ffi::OsStr::from_bytes(name);
            tar.append_data(&mut header, name, &b"#!/bin/sh\n"[..])
                .unwrap();
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar.into_inner().unwrap()).unwrap();
        let tarball = f.root.join("[FAKE] game.tar.gz");
        std::fs::write(&tarball, gz.finish().unwrap()).unwrap();
        let refused = unpacked(&f, &tarball, "[FAKE] Odd")
            .await
            .unwrap_err()
            .to_string();
        assert!(refused.contains("UTF-8"), "{refused}");
        let left: Vec<_> = std::fs::read_dir(f.root.join("library"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(left.is_empty(), "{left:?}");
        assert!(
            std::fs::read_dir(f.dirs.data.join("added")).map_or(true, |mut d| d.next().is_none()),
            "no list"
        );
    }

    /// A title's `:` stays out of the folder's name: an isolated game's folder is shared with its
    /// container through a list `:` separates, which a folder with one in its name cannot join.
    #[tokio::test]
    async fn a_colon_in_the_title_stays_out_of_the_folder_name() {
        let f = fake("colon");
        let zip = f.root.join("[FAKE] game.zip");
        let files = [zip_file("run.sh", b"#!/bin/sh\n")];
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(b"", &files, false),
        )
        .unwrap();
        let (_, folder) = unpacked(&f, &zip, "[FAKE] Quest: Remastered")
            .await
            .unwrap();
        let name = folder.file_name().unwrap().to_string_lossy();
        assert!(!name.contains(':'), "{name}");
    }

    /// A zip and a tarball are unpacked as they are; one never added is thrown away whole.
    #[tokio::test]
    async fn archives_are_unpacked_and_thrown_away_when_not_added() {
        let f = fake("archives");
        let zip = f.root.join("[FAKE] game.zip");
        std::fs::write(
            &zip,
            crate::installer::zip::build::installer(
                b"",
                &[zip_file("Game/run.sh", b"#!/bin/sh\n")],
                false,
            ),
        )
        .unwrap();
        let (_, folder) = unpacked(&f, &zip, "[FAKE] Zipped").await.unwrap();
        assert!(folder.join("Game/run.sh").is_file());

        let mut tar = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(10);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "Game/run.sh", &b"#!/bin/sh\n"[..])
            .unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar.into_inner().unwrap()).unwrap();
        let tarball = f.root.join("[FAKE] game.tar.gz");
        std::fs::write(&tarball, gz.finish().unwrap()).unwrap();
        assert!(is_archive(&tarball));
        let (id, folder) = unpacked(&f, &tarball, "[FAKE] Tarred").await.unwrap();
        assert!(folder.join("Game/run.sh").is_file());
        discard_installer(&f.db, &f.dirs, &id).unwrap();
        assert!(!folder.exists(), "nothing else was in it");

        assert!(!is_archive(&f.game.join("bin/game.exe")));
    }
}
