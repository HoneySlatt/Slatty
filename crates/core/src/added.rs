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
    /// The files SlattyLauncher unpacked into the folder, which removing the game deletes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unpacked: Vec<PathBuf>,
}

/// A Linux installer or archive unpacked for a game not added yet: where, and which files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Unpacked {
    folder: PathBuf,
    files: Vec<PathBuf>,
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
/// the game's id, whose prefix now holds what it installed (its `drive_c`). The game is added
/// once its program is picked there (`NewGame::installed_by`), or the prefix thrown away
/// (`discard_installer`). The installer's folder is the one it can write in when isolated.
pub async fn run_installer(
    db: &Db,
    dirs: &Dirs,
    supervisor: &Path,
    installer: &Path,
    proton: Option<PathBuf>,
    isolated: bool,
) -> Result<String> {
    use crate::session::SessionHandle;
    let installer = real(installer)?;
    if platform_of(&installer)? != Platform::Windows {
        return Err(Error::Refused(format!(
            "{} is not a Windows installer",
            installer.display()
        )));
    }
    let folder = installer
        .parent()
        .ok_or_else(|| Error::Refused(format!("{} has no folder", installer.display())))?
        .to_path_buf();
    check_folder(dirs, &folder, None)?;
    let game_id = next_id(db, dirs)?;
    // What says this prefix holds a game waiting to be added, which alone may be thrown away.
    crate::fsutil::write_atomic(&installer_file(dirs, &game_id), b"")?;
    let install = Install {
        runner: runner_for(db, dirs, &game_id, Platform::Windows, proton)?,
        game_id,
        title: String::new(),
        platform: Platform::Windows,
        path: folder.clone(),
        client_id: None,
        umu_id: None,
        isolated,
    };
    let _busy = crate::lock::game(dirs, &install.game_id)?;
    let busy = crate::lock::session(dirs, &install.game_id);
    if let Some(init) = crate::runner::prefix_init_spec(dirs, &install)? {
        let log = crate::logs::of_game(dirs, "prefix", &install.game_id);
        crate::logs::start_run(&log);
        let outcome = SessionHandle::start(supervisor, &init, &log, Some(&busy))
            .await?
            .wait()
            .await?;
        if !outcome.clean || !crate::runner::prefix_ready(&install) {
            // Nothing was installed in it yet.
            if let Some(prefix) = install.runner.prefix() {
                let _ = std::fs::remove_dir_all(prefix);
            }
            let _ = std::fs::remove_file(installer_file(dirs, &install.game_id));
            return Err(Error::Refused(format!(
                "the Wine prefix could not be created; see {}",
                log.display()
            )));
        }
    }
    let spec = crate::runner::windows_command(
        dirs,
        &install,
        vec![installer.display().to_string()],
        folder,
    )?;
    let log = crate::logs::of_game(dirs, "setup", &install.game_id);
    crate::logs::start_run(&log);
    SessionHandle::start(supervisor, &spec, &log, Some(&busy))
        .await?
        .wait()
        .await?;
    Ok(install.game_id)
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
        let files = checked_files(&unpacked.folder, &unpacked.files)?;
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

/// `files`, relative to `folder`, refused whole when one would lead out of it.
fn checked_files(folder: &Path, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    files
        .iter()
        .map(|f| {
            let rel = crate::installer::safe_relative(&f.to_string_lossy())?;
            crate::installer::refuse_outside(folder, &folder.join(&rel))?;
            Ok(rel)
        })
        .collect()
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
    let name = title.trim().replace(['/', '\\', '\0'], " ");
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err(Error::Refused("the game needs a title".into()));
    }
    let target = into.join(name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(Error::Refused(format!(
            "{} exists already",
            target.display()
        )));
    }
    check_folder(dirs, &target, None)?;
    // Made new: one already there (a link planted to have files written elsewhere, or an earlier
    // attempt) is not the folder to unpack into.
    let partial = into.join(format!(".{name}.slatty-unpack"));
    std::fs::create_dir(&partial).map_err(|e| {
        Error::io(
            format!(
                "make {} (remove it if it is left from before)",
                partial.display()
            ),
            e,
        )
    })?;
    let game_id = next_id(db, dirs)?;
    let files = unpack_into(&archive, &partial, &target, cancel, progress).await;
    // Nothing is resumed: what was unpacked of a failed one goes.
    if files.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    save_unpacked(
        dirs,
        &game_id,
        &Unpacked {
            folder: target.clone(),
            files: files?,
        },
    )?;
    Ok((game_id, target))
}

/// Unpacks `archive` into `partial`, then renames it to `target`: the files it put there.
async fn unpack_into(
    archive: &Path,
    partial: &Path,
    target: &Path,
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
            crate::protons::unpack_tar(&from, &to, false, &cancel, &|done, total| {
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
        std::fs::rename(partial, target)
            .map_err(|e| Error::io(format!("publish {}", target.display()), e))?;
        crate::fsutil::sync_filesystem(target)?;
        listed_files(target)
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
        LinuxDownload {
            source: &source,
            cancel,
            progress: &report,
            free_space: &crate::installer::free_space,
        }
        .run(&set, partial, target)
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
    check_folder(dirs, &folder, new.installed_by.as_deref())?;
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
    let _busy = crate::lock::game(dirs, game_id)?;
    let install = Install::get(db, game_id)?
        .ok_or_else(|| Error::NotFound(format!("{game_id} is not in the library")))?;
    if crate::session::unfinished(db)?
        .iter()
        .any(|s| s.game_id == game_id)
    {
        return Err(Error::Refused(format!("{} is running", install.title)));
    }
    // What SlattyLauncher unpacked for it goes, checked whole before anything is deleted.
    let unpacked = load_task(dirs, game_id)
        .map(|t| t.unpacked)
        .unwrap_or_default();
    let unpacked = checked_files(&install.path, &unpacked)?;
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
        // them): copied too, before the prefix goes.
        if install.path.starts_with(prefix) && install.path.is_dir() {
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
            report.prefix_backup.get_or_insert(copy);
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
    check_folder(dirs, &folder, Some(game_id))?;
    let task = load_task(dirs, game_id)?;
    let program = folder.join(&task.program);
    if !program.is_file() || !leads_inside(&folder, &program) {
        return Err(Error::Refused(format!(
            "{} is not in {}",
            task.program.display(),
            folder.display()
        )));
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
/// (`game_id`'s), where its installer put it.
fn check_folder(dirs: &Dirs, folder: &Path, game_id: Option<&str>) -> Result<()> {
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

        // A Linux program, with the folder it is in as the game's.
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
        let refused = |folder: &Path| check_folder(&f.dirs, folder, None).is_err();
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
            check_folder(&f.dirs, &in_prefix.canonicalize().unwrap(), Some("local-7")).is_ok(),
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
