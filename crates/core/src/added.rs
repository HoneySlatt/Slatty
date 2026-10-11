//! Games added by hand: a game from no store, already on the disk, started from the program the
//! user picked. A Windows program runs through umu and Proton in a prefix of its own, a Linux one
//! as GOG's native builds do. Their files are the user's: removing such a game never deletes them.
//!
//! The game is an install (`installs`, with its folder, runner and isolation) whose id is
//! `local-<number>` (`store::local_id`), never given twice; what it starts is kept beside the
//! install records, in `added/<id>.json`, relative to its folder.

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
    let folder = match &new.folder {
        Some(folder) => real(folder)?,
        None => program
            .parent()
            .ok_or_else(|| Error::Refused(format!("{} has no folder", program.display())))?
            .to_path_buf(),
    };
    check_folder(&folder)?;
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
    };
    if !working_dir.is_dir() {
        return Err(Error::Refused(format!(
            "{} is not a folder",
            working_dir.display()
        )));
    }

    let game_id = crate::store::local_id(next_number(db)?);
    let runner = match platform {
        Platform::Linux => Runner::Native,
        Platform::Windows => {
            let proton = match new.proton {
                Some(proton) => proton,
                None => crate::settings::default_proton(db)?.ok_or_else(|| {
                    Error::Refused(
                        "a Windows game needs a Proton build: choose one in Settings → Runners"
                            .into(),
                    )
                })?,
            };
            if !proton.join("proton").is_file() {
                return Err(Error::Refused(format!(
                    "{} is not a Proton build",
                    proton.display()
                )));
            }
            Runner::Umu {
                proton,
                prefix: dirs.data.join("prefixes").join(&game_id),
            }
        }
    };
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
    let mut report = crate::maintenance::UninstallReport::default();
    if let Some(prefix) = install.runner.prefix().filter(|_| delete_prefix) {
        if !prefix.starts_with(dirs.data.join("prefixes")) {
            return Err(Error::Refused(format!(
                "{} was not made by SlattyLauncher; nothing was deleted",
                prefix.display()
            )));
        }
        report.prefix_backup = crate::maintenance::back_up_prefix(dirs, game_id, prefix)?;
        if prefix.exists() {
            std::fs::remove_dir_all(prefix)
                .map_err(|e| Error::io(format!("delete {}", prefix.display()), e))?;
            report.prefix_removed = true;
        }
    }
    Install::remove(db, game_id)?;
    let _ = std::fs::remove_file(task_file(dirs, game_id));
    crate::custom::forget(db, dirs, game_id)?;
    crate::logs::remove_game(dirs, game_id)?;
    Ok(report)
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

/// Refuses a game folder an isolated game would be given too much of: the whole disk, or the
/// home folder or one holding it.
fn check_folder(folder: &Path) -> Result<()> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let home = home.as_deref().and_then(|h| h.canonicalize().ok());
    let too_wide = folder.parent().is_none() || home.is_some_and(|h| h.starts_with(folder));
    if too_wide {
        return Err(Error::Refused(format!(
            "{} holds much more than a game: put the game in a folder of its own",
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
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert!(
            check_folder(&home.canonicalize().unwrap()).is_err(),
            "the home folder"
        );
        assert!(check_folder(home.canonicalize().unwrap().parent().unwrap()).is_err());
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
}
