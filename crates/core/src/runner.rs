use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::doctor::find_in_path;
use crate::error::{Error, Result};
use crate::gameinfo::{self, GameInfo};
use crate::install::{Install, Platform};
use crate::paths::Dirs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Runner {
    Native,
    Umu { proton: PathBuf, prefix: PathBuf },
    Wine { wine: PathBuf, prefix: PathBuf },
}

impl Runner {
    pub fn prefix(&self) -> Option<&Path> {
        match self {
            Runner::Native => None,
            Runner::Umu { prefix, .. } | Runner::Wine { prefix, .. } => Some(prefix),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

/// How to start a game: as `choice` (one of its `launch_options`) when given and still offered,
/// else as its main task.
pub fn launch_spec(dirs: &Dirs, install: &Install, choice: Option<&str>) -> Result<LaunchSpec> {
    let added = crate::store::is_local(&install.game_id);
    match (&install.platform, &install.runner) {
        (Platform::Linux, Runner::Native) => {
            // GOG's builds start through their script; a game added by hand, through the program
            // or script the user picked.
            let (script, extra, cwd) = if added {
                crate::added::launch_task(dirs, install)?
            } else {
                let script = install.path.join("start.sh");
                if !script.is_file() {
                    return Err(Error::NotFound(format!("{} is missing", script.display())));
                }
                (script, Vec::new(), install.path.clone())
            };
            // Only umu's container isolates. Elsewhere on NixOS, which keeps neither `/bin/bash`
            // (which GOG's scripts ask for) nor the libraries games load (OpenGL, sound) where
            // they look, games run in a usual Linux layout.
            let through_umu = |umu: PathBuf| -> Result<Wrapper> {
                let tool = runtime_tool(dirs, NATIVE_RUNTIME).expect("umu knows steamrt3")?;
                Ok(Wrapper::Umu(umu, tool))
            };
            let wrapper = if install.isolated {
                let umu = find_in_path("umu-run").ok_or_else(|| {
                    Error::NotFound("umu-run is needed to run a game isolated".into())
                })?;
                runtime_installed(&umu_data(), NATIVE_RUNTIME)?;
                Some(through_umu(umu)?)
            } else if Path::new("/etc/NIXOS").exists() {
                match find_in_path("steam-run") {
                    Some(run) => Some(Wrapper::SteamRun(run)),
                    None => find_in_path("umu-run").map(through_umu).transpose()?,
                }
            } else {
                None
            };
            let tool = match &wrapper {
                Some(Wrapper::Umu(_, tool)) => Some(tool.clone()),
                _ => None,
            };
            let (mut program, mut args, mut env) = native_command(&script, wrapper);
            // An AppImage mounts itself through FUSE, which neither the container nor every system
            // offers: it unpacks itself instead.
            if added && crate::added::is_appimage(&script) {
                env.push(("APPIMAGE_EXTRACT_AND_RUN".into(), "1".into()));
            }
            if install.isolated {
                env.extend(isolation_env(dirs, install, None)?);
                // Without one, umu takes `~/Games/umu/<id>` or the user's `$WINEPREFIX` as the
                // prefix, and the container shares it: the game's own home takes that place.
                env.push((
                    "WINEPREFIX".into(),
                    isolated_home(dirs, &install.game_id)?.display().to_string(),
                ));
                // Given a file, umu would share the whole filesystem it is on: a script goes to
                // its interpreter, which umu looks for inside the container, and a program to a
                // shell that starts it.
                if crate::added::is_program(&script) {
                    args.splice(0..0, ["sh", "-c", "exec \"$0\" \"$@\""].map(String::from));
                } else {
                    args.insert(0, interpreter_name(&script)?);
                }
                args.extend(extra);
                check_shared(install, &args, &cwd)?;
                env.push(("UMU_RUNTIME_UPDATE".into(), "0".into()));
                (program, args) = read_only_tools(program, args, tool.as_deref())?;
            } else {
                args.extend(extra);
            }
            Ok(LaunchSpec {
                program,
                args,
                cwd,
                env,
            })
        }
        (Platform::Windows, Runner::Umu { .. } | Runner::Wine { .. }) => {
            let (exe, args, cwd) = if added {
                crate::added::launch_task(dirs, install)?
            } else {
                windows_task(install, choice)?
            };
            windows_spec(dirs, install, prepend(exe, args), cwd, true)
        }
        (platform, runner) => Err(Error::Unsupported(format!(
            "{platform:?} game with runner {runner:?}"
        ))),
    }
}

/// What a native game runs inside, where the system alone cannot run it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Wrapper {
    /// NixOS's FHS environment of Steam's libraries (`steam-run`, or `steam-run-free` without
    /// Steam).
    SteamRun(PathBuf),
    /// umu without Proton: the Steam Linux Runtime 3.0 (sniper) container, which umu already
    /// downloads for Windows games, through SlattyLauncher's own tool for it (`runtime_tool`).
    Umu(PathBuf, PathBuf),
}

/// How to start a native game's script: inside `wrapper` when there is one, else through the
/// interpreter its first line names, found in PATH by name when that path does not exist.
fn native_command(
    script: &Path,
    wrapper: Option<Wrapper>,
) -> (PathBuf, Vec<String>, Vec<(String, String)>) {
    let script_arg = vec![script.display().to_string()];
    match wrapper {
        Some(Wrapper::SteamRun(run)) => return (run, script_arg, Vec::new()),
        Some(Wrapper::Umu(umu, tool)) => {
            let env = vec![
                ("PROTONPATH".into(), tool.display().to_string()),
                ("GAMEID".into(), "umu-0".into()),
            ];
            return (umu, script_arg, env);
        }
        None => {}
    }
    let first = crate::fsutil::read_game_file(script)
        .ok()
        .flatten()
        .and_then(|b| b.split(|c| *c == b'\n').next().map(<[u8]>::to_vec))
        .map(|l| String::from_utf8_lossy(&l).into_owned())
        .unwrap_or_default();
    let interpreter = first
        .strip_prefix("#!")
        .and_then(|l| l.split_whitespace().next())
        .map(PathBuf::from);
    match interpreter {
        Some(i) if !i.exists() => match i
            .file_name()
            .and_then(|n| find_in_path(&n.to_string_lossy()))
        {
            Some(found) => (found, script_arg, Vec::new()),
            None => (script.to_path_buf(), Vec::new(), Vec::new()),
        },
        _ => (script.to_path_buf(), Vec::new(), Vec::new()),
    }
}

/// The name of the interpreter a script's first line asks for (`bash` for GOG's, also through
/// `/usr/bin/env`), else `sh`. Only a shell is accepted: umu reads its first argument as one of
/// its own options or verbs (`--config` with the script as its configuration, `winetricks`,
/// Proton's `run`), and the game can rewrite its script between two sessions.
fn interpreter_name(script: &Path) -> Result<String> {
    let bytes = crate::fsutil::read_game_file(script)
        .ok()
        .flatten()
        .unwrap_or_default();
    let first = bytes.split(|c| *c == b'\n').next().unwrap_or_default();
    let line = String::from_utf8_lossy(first);
    let mut words = line
        .strip_prefix("#!")
        .unwrap_or_default()
        .split_whitespace()
        .map(|w| w.rsplit('/').next().unwrap_or(w));
    let name = match words.next() {
        Some("env") => words.next(),
        other => other,
    }
    .filter(|w| !w.is_empty())
    .unwrap_or("sh");
    if !["sh", "bash", "dash"].contains(&name) {
        return Err(Error::Refused(format!(
            "{} asks for `{name}`, which an isolated game cannot be started with",
            script.display()
        )));
    }
    Ok(name.to_string())
}

/// Whether the game's Wine prefix has been created (always true for native games). Nothing may be
/// written into a prefix before, or Proton would take it for created and leave it half set up.
pub fn prefix_ready(install: &Install) -> bool {
    install
        .runner
        .prefix()
        .is_none_or(|p| p.join("drive_c/users").is_dir())
}

/// `wineboot` for a prefix that has never been created, so save folders exist before the first launch.
pub fn prefix_init_spec(dirs: &Dirs, install: &Install) -> Result<Option<LaunchSpec>> {
    let Some(prefix) = install.runner.prefix().filter(|_| !prefix_ready(install)) else {
        return Ok(None);
    };
    crate::paths::ensure_dir(prefix)?;
    windows_command(
        dirs,
        install,
        vec!["wineboot".into(), "-u".into()],
        install.path.clone(),
    )
    .map(Some)
}

/// A program other than the game run in its prefix (setup, redistributables, `reg`, `wineboot`).
pub fn windows_command(
    dirs: &Dirs,
    install: &Install,
    args: Vec<String>,
    cwd: PathBuf,
) -> Result<LaunchSpec> {
    windows_spec(dirs, install, args, cwd, false)
}

/// Proton's fixes for the game apply to the game only: some add arguments to whatever runs with
/// its id (DOOM's `+r_renderAPI 1`), which other programs refuse.
fn windows_spec(
    dirs: &Dirs,
    install: &Install,
    args: Vec<String>,
    cwd: PathBuf,
    game: bool,
) -> Result<LaunchSpec> {
    match &install.runner {
        Runner::Umu { proton, prefix } => {
            if !proton.join("proton").is_file() {
                return Err(Error::NotFound(format!(
                    "no `proton` script in {}",
                    proton.display()
                )));
            }
            let runtime = if install.isolated {
                Some(container_of(proton)?)
            } else {
                None
            };
            let umu = find_in_path("umu-run")
                .ok_or_else(|| Error::NotFound("umu-run is not in PATH".into()))?;
            if let Some(runtime) = runtime {
                runtime_installed(&umu_data(), runtime)?;
            }
            let mut args = args;
            // Given a file of this computer, umu would share the whole filesystem it is on:
            // Proton finds the program by its Windows path instead, through Z: in the container.
            if install.isolated
                && let Some(program) = args.first_mut()
                && Path::new(program.as_str()).is_absolute()
            {
                *program = windows_path(Path::new(program.as_str()));
            }
            if install.isolated {
                check_shared(install, &args, &cwd)?;
            }
            let mut env = umu_env(dirs, install, proton, prefix)?;
            if !game {
                env.push(("PROTONFIXES_DISABLE".into(), "1".into()));
            }
            let (program, args) = if install.isolated {
                env.push(("UMU_RUNTIME_UPDATE".into(), "0".into()));
                read_only_tools(umu, args, Some(proton))?
            } else {
                (umu, args)
            };
            Ok(LaunchSpec {
                program,
                args,
                cwd,
                env,
            })
        }
        Runner::Wine { .. } if install.isolated => Err(Error::Refused(format!(
            "{} runs through Wine alone, which cannot isolate it; use Proton or turn isolation off",
            install.title
        ))),
        Runner::Wine { wine, prefix } => Ok(LaunchSpec {
            program: wine.clone(),
            args,
            cwd,
            env: vec![("WINEPREFIX".into(), prefix.display().to_string())],
        }),
        Runner::Native => Err(Error::Unsupported("Windows command without Wine".into())),
    }
}

/// `/games/Game/game.exe` as Wine names it: `Z:\games\Game\game.exe`.
fn windows_path(path: &Path) -> String {
    format!("Z:{}", path.display()).replace('/', "\\")
}

fn prepend(exe: PathBuf, args: Vec<String>) -> Vec<String> {
    std::iter::once(exe.display().to_string())
        .chain(args)
        .collect()
}

/// The ways a Windows game can be started, its main one first: the game, and tools such as a
/// configuration program. None for a Linux build, which starts through its script.
pub fn launch_options(install: &Install) -> Vec<String> {
    if install.platform != Platform::Windows {
        return Vec::new();
    }
    gameinfo::read(&install.path, Some(&install.game_id))
        .map(|info| {
            info.launch_tasks()
                .iter()
                .map(|t| t.label().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn windows_task(
    install: &Install,
    choice: Option<&str>,
) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    let info: GameInfo = gameinfo::read(&install.path, Some(&install.game_id))?;
    let chosen = choice.and_then(|c| info.launch_tasks().into_iter().find(|t| t.label() == c));
    let task = chosen.or_else(|| info.primary_task()).ok_or_else(|| {
        Error::NotFound(format!(
            "no launchable task in goggame-{}.info",
            install.game_id
        ))
    })?;
    let exe = gameinfo::resolve_relative(&install.path, task.path.as_deref().unwrap_or_default())?;
    if !exe.is_file() {
        return Err(Error::NotFound(format!("{} is missing", exe.display())));
    }
    let cwd = gameinfo::resolve_relative(&install.path, task.working_dir.as_deref().unwrap_or(""))?;
    let args = task
        .arguments
        .as_deref()
        .map(gameinfo::split_args)
        .unwrap_or_default();
    Ok((exe, args, cwd))
}

/// What umu needs: the prefix, the Proton build, and the game's id so umu applies its fixes.
fn umu_env(
    dirs: &Dirs,
    install: &Install,
    proton: &Path,
    prefix: &Path,
) -> Result<Vec<(String, String)>> {
    let game_id = install
        .umu_id
        .clone()
        .unwrap_or_else(|| crate::umu::UNKNOWN.into());
    // Isolated, the build a `-latest` link leads to now, as it is mounted read-only: one linked
    // in its place before umu starts would run writable in the container.
    let proton = match install.isolated {
        true => proton
            .canonicalize()
            .unwrap_or_else(|_| proton.to_path_buf()),
        false => proton.to_path_buf(),
    };
    let mut env = vec![
        ("WINEPREFIX".into(), prefix.display().to_string()),
        ("PROTONPATH".into(), proton.display().to_string()),
        ("GAMEID".into(), game_id),
    ];
    // Where umu looks the game up for its fixes; a game added by hand is from no store.
    if !crate::store::is_local(&install.game_id) {
        env.push(("STORE".into(), "gog".into()));
    }
    // Given this, umu would share the whole filesystem the game is on (`/home`, a library disk),
    // writable: an isolated game's folder is shared on its own instead. Proton's fixes then find
    // the game from its working directory.
    if install.isolated {
        env.extend(isolation_env(dirs, install, Some(&proton))?);
    } else {
        check_steam_sdk_in(
            Path::new(&std::env::var_os("HOME").unwrap_or_default()),
            &proton,
        )?;
        env.push((
            "STEAM_COMPAT_INSTALL_PATH".into(),
            install.path.display().to_string(),
        ));
    }
    Ok(env)
}

/// Whether `proton` is one of Valve's own builds, which Steam keeps in a library's
/// `steamapps/common`. Their `steam.exe` stops unless it loads Steam's client library from
/// `~/.steam/sdk64`; the builds made for use outside Steam (GE-Proton, Proton-CachyOS,
/// UMU-Proton) go on without it.
fn valve_proton(proton: &Path) -> bool {
    fn name(p: Option<&Path>) -> Option<&str> {
        p.and_then(Path::file_name).and_then(|n| n.to_str())
    }
    name(proton.parent()) == Some("common")
        && name(proton.parent().and_then(Path::parent)) == Some("steamapps")
}

/// Refuses one of Valve's builds for a game run with the user's own `home`, when it lacks the
/// `~/.steam/sdk64` only Steam installed natively makes (its Flatpak keeps its own under
/// `~/.var/app`).
fn check_steam_sdk_in(home: &Path, proton: &Path) -> Result<()> {
    if valve_proton(proton) && !home.join(".steam/sdk64/steamclient.so").is_file() {
        return Err(Error::Refused(
            "Valve's Proton builds need Steam's client library in ~/.steam/sdk64, which only \
             Steam installed natively makes: turn isolation on for this game, which gives it the \
             library, or choose another Proton build"
                .into(),
        ));
    }
    Ok(())
}

/// `~/.steam/sdk64/steamclient.so` and the 32-bit one in the game's own home, leading to Steam's
/// in `libs` (`linux64`, `linux32`), which the container is given read-only. Folders holding a
/// link, not links to Steam's folders: pressure-vessel removes a link it finds at `~/.steam/sdk64`
/// in a home of its own (checked). A game could have replaced them with links out of its home.
fn steam_sdk_links(home: &Path, libs: &[PathBuf; 2]) -> Result<()> {
    for (sdk, lib) in ["sdk64", "sdk32"].into_iter().zip(libs) {
        let dir = home.join(".steam").join(sdk);
        if !crate::installer::leads_inside(home, &dir) {
            return Err(Error::Refused(format!(
                "{} leads out of the game's home",
                dir.display()
            )));
        }
        crate::paths::ensure_dir(&dir)?;
        let link = dir.join("steamclient.so");
        let target = lib.join("steamclient.so");
        if std::fs::read_link(&link).ok() == Some(target.clone()) {
            continue;
        }
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link)
            .map_err(|e| Error::io(format!("link {}", link.display()), e))?;
    }
    Ok(())
}

/// Takes away the links in `home/.steam`, and that folder itself when it is one: pressure-vessel
/// looks at `$HOME/.steam` from outside the container to share the Steam installation, and a game
/// writes its own home, so it could have linked the user's there (the client's settings and
/// tokens). SlattyLauncher's own `sdk64` and `sdk32` are folders, which stay.
fn forget_steam_links(home: &Path) -> Result<()> {
    let steam = home.join(".steam");
    let unlink = |path: &Path| {
        std::fs::remove_file(path).map_err(|e| Error::io(format!("delete {}", path.display()), e))
    };
    match std::fs::symlink_metadata(&steam) {
        Ok(m) if m.is_symlink() => unlink(&steam),
        Ok(m) if m.is_dir() => {
            for entry in std::fs::read_dir(&steam).into_iter().flatten().flatten() {
                if entry.file_type().is_ok_and(|t| t.is_symlink()) {
                    unlink(&entry.path())?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Where an isolated game finds a home folder of its own, in place of the user's. It holds what
/// the game writes there, Linux games' saves among them.
pub fn isolated_home(dirs: &Dirs, game_id: &str) -> Result<PathBuf> {
    crate::paths::check_owned_id(game_id)?;
    Ok(dirs.data.join("homes").join(game_id))
}

/// What the container umu runs games in (pressure-vessel, from the Steam Runtime) is told for an
/// isolated game: a home folder of its own, its folder writable, and read-only what umu and the
/// post-install setup need. The rest of the user's files, `/tmp` included, stays out of it. One
/// of Valve's Proton builds is given Steam's client library too (`steam_sdk_links`).
fn isolation_env(
    dirs: &Dirs,
    install: &Install,
    proton: Option<&Path>,
) -> Result<Vec<(String, String)>> {
    let home = isolated_home(dirs, &install.game_id)?;
    crate::paths::ensure_dir(&home)?;
    forget_steam_links(&home)?;
    let mut read_only = vec![
        umu_data(),
        crate::setup::redist_dir(dirs),
        crate::installer::support_dir(dirs, &install.game_id),
    ];
    if proton.is_some_and(valve_proton)
        && let Some(libs) = crate::settings::steam_client_libs()
    {
        steam_sdk_links(&home, &libs)?;
        read_only.extend(libs);
    }
    let home = home.display().to_string();
    let mut env = vec![
        ("PRESSURE_VESSEL_HOME".into(), home.clone()),
        // The user's own environment could ask the container to share the home folder after all.
        ("PRESSURE_VESSEL_SHARE_HOME".into(), "0".into()),
        // The user's other programs stay out of reach: seeing their process ids, a game could
        // stop them (a signal needs only the same user) and read their command lines.
        ("PRESSURE_VESSEL_SHARE_PID".into(), "0".into()),
        (
            "PRESSURE_VESSEL_FILESYSTEMS_RW".into(),
            path_list(std::slice::from_ref(&install.path))?,
        ),
        (
            "PRESSURE_VESSEL_FILESYSTEMS_RO".into(),
            path_list(&read_only)?,
        ),
        // The container shares the user's Steam installation (`~/.steam` and where its links
        // lead, the client's settings among them) with every game, read-only, from the home
        // folder `HOME` names: the game's own home has none. umu still finds its runtimes and
        // keeps its cache where they are.
        ("HOME".into(), home.clone()),
        ("XDG_DATA_HOME".into(), data_home().display().to_string()),
        ("XDG_CACHE_HOME".into(), cache_home().display().to_string()),
        // The container shares the folders these name with the game (checked: any of them in the
        // home folder showed all of it); its own `/tmp` is private.
        ("TMPDIR".into(), "/tmp".into()),
        ("TMP".into(), "/tmp".into()),
        ("TEMP".into(), "/tmp".into()),
        ("TEMPDIR".into(), "/tmp".into()),
        // Proton's logs, when asked for, go to the game's home rather than to a shared folder.
        ("PROTON_LOG_DIR".into(), home),
    ];
    // Every other folder the container would share, writable, from the user's environment
    // (pressure-vessel's `known_required_env`), and the one umu takes a game folder from.
    for shared in [
        "STEAM_COMPAT_INSTALL_PATH",
        "STEAM_COMPAT_LIBRARY_PATHS",
        "STEAM_COMPAT_CLIENT_INSTALL_PATH",
        "STEAM_COMPAT_MOUNT_PATHS",
        "STEAM_COMPAT_TOOL_PATH",
        "STEAM_COMPAT_APP_LIBRARY_PATH",
        "STEAM_COMPAT_APP_LIBRARY_PATHS",
        "STEAM_EXTRA_COMPAT_TOOLS_PATHS",
        "STEAM_RUNTIME_SCOUT",
    ] {
        env.push((shared.into(), String::new()));
    }
    env.extend([
        // Through the D-Bus buses a program could have other services act for it, outside the
        // container (systemd starts commands; the system's rules may let the session suspend or
        // mount drives). Wine, Proton and umu do not need them (checked: Wine saw the same network
        // adapters and drives without the system bus); D-Bus's own "no bus" address keeps them
        // out (a missing socket would stop the container).
        ("DBUS_SESSION_BUS_ADDRESS".into(), "disabled:".into()),
        ("DBUS_SYSTEM_BUS_ADDRESS".into(), "disabled:".into()),
    ]);
    Ok(env)
}

/// What umu and its container would share with an isolated game beyond what it is given,
/// refused. umu takes the game's folder from its first argument when that names a file of this
/// computer, relative to the working directory or to the home folder (`~`), and shares the whole
/// filesystem under it. The container shares the working directory as it is on disk, through
/// any link. A game can plant either in its own folder between two sessions.
fn check_shared(install: &Install, args: &[String], cwd: &Path) -> Result<()> {
    if install.path.to_str().is_none() {
        return Err(Error::Refused(format!(
            "{} cannot be shared with the container: its path is not UTF-8",
            install.path.display()
        )));
    }
    if let Some(program) = args.first()
        && (program.starts_with('~') || cwd.join(program).exists())
    {
        return Err(Error::Refused(format!(
            "`{program}` names a file in {}, through which umu would share the whole disk with \
             the isolated game",
            cwd.display()
        )));
    }
    if !crate::installer::leads_inside(&install.path, cwd) {
        return Err(Error::Refused(format!(
            "{} leads out of the game folder through a link; the container would share where it \
             leads",
            cwd.display()
        )));
    }
    Ok(())
}

/// `umu` and its arguments, run inside bubblewrap with umu's runtimes and the Proton build
/// read-only. umu has the container share both writable (STEAM_COMPAT_MOUNTS), and programs of
/// the runtime run outside the container at every launch: an isolated game could otherwise change
/// them, and have its code run outside its container at the next launch, or in other games. What
/// pressure-vessel writes as a game starts (its copy of the runtime in `var`, its `.ref` locks)
/// stays writable; umu leaves the runtime as it is meanwhile (`UMU_RUNTIME_UPDATE=0`, see
/// `refresh_runtimes`).
/// The bubblewrap an isolated game runs in, once tried: a system where it cannot hold one (no
/// AppArmor profile on Ubuntu) says why before the launch, rather than the game failing as it
/// starts. Never a launch without isolation instead.
fn sandbox(bwrap: Option<PathBuf>) -> Result<PathBuf> {
    let bwrap = bwrap.ok_or_else(|| {
        Error::NotFound("bubblewrap (bwrap) is needed to run a game isolated".into())
    })?;
    match crate::doctor::sandbox_problem(&bwrap) {
        None => Ok(bwrap),
        Some(problem) => Err(Error::Refused(format!(
            "{problem}. Isolation can be turned off for this game in its settings, at the cost of \
             the protection it gives"
        ))),
    }
}

/// bubblewrap's arguments mounting `path` over itself, by where it leads: bubblewrap cannot mount
/// over a link, which a `<project>-latest` build is. The build stays read-only through the link.
fn bind(out: &mut Vec<String>, how: &str, path: &Path) -> Result<()> {
    let real = path
        .canonicalize()
        .map_err(|e| Error::io(format!("resolve {}", path.display()), e))?;
    let real = real.display().to_string();
    out.extend([how.to_string(), real.clone(), real]);
    Ok(())
}

/// What Python's `str(os.path.getmtime(…))` gives for a file's time: CPython adds the nanoseconds
/// as `sec + nsec * 1e-9` and prints the shortest form that reads back the same, as Rust does.
fn python_time(sec: i64, nsec: u32) -> String {
    let text = (sec as f64 + f64::from(nsec) * 1e-9).to_string();
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// `steampipe_fixups.json`: what SteamPipe, which ships Valve's builds, cannot keep.
#[derive(Deserialize)]
struct SteamPipeFixups {
    empty_dirs: Vec<String>,
    no_write_paths: Vec<String>,
}

/// Sets up one of Valve's Proton builds as its `proton` script would at a launch, before an
/// isolated one runs it read-only: SteamPipe's fixups once after each Steam update (empty folders
/// made again, some files made read-only, as `steampipe_fixups.py` does; BSD-3-Clause, Valve), and
/// its `dist.lock`. Whether `proton` is one of Valve's builds. One Proton never set up (no default
/// prefix yet, or a `dist` folder of old versions) is refused: Proton would write in it.
fn set_up_valve_build(proton: &Path) -> Result<bool> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let json = proton.join("steampipe_fixups.json");
    let Ok(meta) = std::fs::metadata(&json) else {
        return Ok(false);
    };
    if !proton.join("files/share/default_pfx").is_dir() || proton.join("dist").exists() {
        return Err(Error::Refused(format!(
            "{} has not set itself up yet: start a game with it once from Steam, or this game once \
             with isolation off",
            proton.display()
        )));
    }
    let io = |what: &str, p: &Path, e| Error::io(format!("{what} {}", p.display()), e);
    let lock = proton.join("dist.lock");
    if !lock.exists() {
        std::fs::File::create(&lock).map_err(|e| io("create", &lock, e))?;
    }
    let stamp = proton.join("files/steampipe_fixups_mtime");
    let now = python_time(meta.mtime(), meta.mtime_nsec() as u32);
    let done = std::fs::read_to_string(&stamp).unwrap_or_default();
    if done.lines().next().map(str::trim) == Some(now.as_str()) {
        return Ok(true);
    }
    let bytes = std::fs::read(&json).map_err(|e| io("read", &json, e))?;
    let fixups: SteamPipeFixups = serde_json::from_slice(&bytes)
        .map_err(|e| Error::parse("steampipe_fixups.json", e.to_string()))?;
    let inside = |p: &str| {
        Path::new(p)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
            .then(|| proton.join(p))
    };
    for dir in fixups.empty_dirs.iter().filter_map(|d| inside(d)) {
        let _ = std::fs::create_dir_all(dir);
    }
    for file in fixups.no_write_paths.iter().filter_map(|f| inside(f)) {
        let meta = std::fs::symlink_metadata(&file).map_err(|e| io("read", &file, e))?;
        if meta.is_symlink() {
            continue;
        }
        let mode = meta.permissions().mode() & !0o222;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode))
            .map_err(|e| io("make read-only", &file, e))?;
    }
    std::fs::write(&stamp, format!("{now}\n")).map_err(|e| io("write", &stamp, e))?;
    Ok(true)
}

fn read_only_tools(
    umu: PathBuf,
    args: Vec<String>,
    proton: Option<&Path>,
) -> Result<(PathBuf, Vec<String>)> {
    let bwrap = sandbox(find_in_path("bwrap"))?;
    let mut out: Vec<String> = ["--dev-bind", "/", "/"].map(String::from).to_vec();
    for runtime in runtimes() {
        bind(&mut out, "--ro-bind", &runtime)?;
        let var = runtime.join("var");
        crate::paths::ensure_dir(&var)?;
        bind(&mut out, "--bind", &var)?;
        for lock in ref_locks(&runtime)? {
            bind(&mut out, "--bind", &lock)?;
        }
    }
    if let Some(proton) = proton {
        bind(&mut out, "--ro-bind", proton)?;
        // Valve's builds take it at every launch.
        if set_up_valve_build(proton)? {
            bind(&mut out, "--bind", &proton.join("dist.lock"))?;
        }
    }
    out.push("--".into());
    out.push(umu.display().to_string());
    out.extend(args);
    Ok((bwrap, out))
}

/// The `.ref` lock files pressure-vessel takes in the platform folders of `runtime`, made when
/// missing. In its real folders alone: `var`, which an isolated game can write, could hold a link
/// at `files/.ref`, and this runs outside any sandbox; a lock that is a link is not shared either.
fn ref_locks(runtime: &Path) -> Result<Vec<PathBuf>> {
    let mut locks = Vec::new();
    for platform in std::fs::read_dir(runtime).into_iter().flatten().flatten() {
        if platform.file_name() == "var" || !platform.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let files = platform.path().join("files");
        if !std::fs::symlink_metadata(&files).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        let lock = files.join(".ref");
        match std::fs::symlink_metadata(&lock) {
            Ok(m) if m.is_file() => {}
            Ok(_) => continue,
            Err(_) => {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock)
                    .map_err(|e| Error::io(format!("create {}", lock.display()), e))?;
            }
        }
        locks.push(lock);
    }
    Ok(locks)
}

/// Refuses an isolated launch in a runtime umu does not have yet: umu would install it during the
/// launch, inside the sandbox, where nothing binds it read-only, and the game could change it
/// (`refresh_runtimes` installs it before, outside).
fn runtime_installed(umu: &Path, runtime: &str) -> Result<()> {
    if umu.join(runtime).is_dir() {
        return Ok(());
    }
    Err(Error::Refused(format!(
        "umu's runtime {runtime} is not installed yet: it is installed as an isolated game starts, \
         which needs the network (umu-runtimes.log in the logs folder says what failed)"
    )))
}

/// The runtime a native game runs in when isolated: umu's Steam Linux Runtime 3.0 (sniper).
const NATIVE_RUNTIME: &str = "steamrt3";

/// The folder of the runtime an isolated launch of `install` runs in; `None` when not isolated or
/// when it cannot be known, which the launch then refuses.
pub fn container(install: &Install) -> Option<&'static str> {
    if !install.isolated {
        return None;
    }
    match &install.runner {
        Runner::Native => Some(NATIVE_RUNTIME),
        Runner::Umu { proton, .. } => container_of(proton).ok(),
        Runner::Wine { .. } => None,
    }
}

/// Steam's app ids of the runtimes umu keeps, by folder (umu 1.4's `RUNTIME_VERSIONS`).
const RUNTIME_APPIDS: [(&str, &str); 4] = [
    ("steamrt2", "1391110"),
    ("steamrt3", "1628350"),
    ("steamrt4", "4183110"),
    ("steamrt4-arm64", "4185400"),
];

/// The folder of the runtime whose container a Proton build runs in: the one its
/// `toolmanifest.vdf` names by `require_tool_appid`. umu starts a build naming none on the host,
/// without pressure-vessel (umu 1.4's `CompatLayer.required_runtime`, "host"): nothing an isolated
/// launch sets would hold, so such a build is refused for an isolated game.
fn container_of(proton: &Path) -> Result<&'static str> {
    let appid = crate::fsutil::read_game_file(&proton.join("toolmanifest.vdf"))
        .ok()
        .flatten()
        .and_then(|bytes| crate::steam::vdf::parse(&String::from_utf8_lossy(&bytes)).ok())
        .and_then(|vdf| {
            vdf.get("manifest")?
                .text("require_tool_appid")
                .map(str::to_string)
        });
    appid
        .and_then(|appid| RUNTIME_APPIDS.iter().find(|(_, a)| *a == appid))
        .map(|(name, _)| *name)
        .ok_or_else(|| {
            Error::Refused(format!(
                "{} would run outside umu's container (its toolmanifest.vdf names no Steam Linux \
                 Runtime umu knows), seeing all your files: choose another Proton build for this \
                 game, or turn its isolation off",
                proton.display()
            ))
        })
}

/// A compatibility tool that runs its command as it is in the runtime of folder `name`, made as
/// umu makes its own `umu-sniper`: umu brings the runtime up to date before using it. umu's own
/// tool is reached through `UMU_NO_PROTON`, with which umu first asks GitHub for its Proton
/// releases. `None` for a runtime umu 1.4 does not know.
fn runtime_tool(dirs: &Dirs, name: &str) -> Option<Result<PathBuf>> {
    use std::os::unix::fs::PermissionsExt;
    let (_, appid) = RUNTIME_APPIDS.iter().find(|(n, _)| *n == name)?;
    let tool = dirs.data.join("umu-runtimes/tools").join(name);
    let manifest = format!(
        "\"manifest\"\n{{\n\t\"version\"\t\"2\"\n\t\"commandline\"\t\"/entry-point\"\n\t\
         \"use_sessions\"\t\"1\"\n\t\"compatmanager_layer_name\"\t\"umu-passthrough\"\n\t\
         \"require_tool_appid\"\t\"{appid}\"\n}}\n"
    );
    let entry = tool.join("entry-point");
    let made = crate::fsutil::write_atomic(&tool.join("toolmanifest.vdf"), manifest.as_bytes())
        .and_then(|()| crate::fsutil::write_atomic(&entry, b"#!/bin/sh\nexec \"$@\"\n"))
        .and_then(|()| {
            std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| Error::io(format!("make {} executable", entry.display()), e))
        });
    Some(made.map(|()| tool))
}

/// Brings umu's runtimes up to date, at most once a day, before an isolated launch: such
/// launches use them read-only and leave them as they are (`read_only_tools`). `needed`, the
/// runtime that launch runs in (`container`), is installed at once when umu does not have it. umu
/// runs nothing of a game's for it (`true` in each runtime, through `runtime_tool`), not isolated.
/// A failure leaves the runtimes as they are: the game starts with them, or not at all without the
/// one it needs.
pub async fn refresh_runtimes(db: &crate::db::Db, dirs: &Dirs, needed: Option<&str>) {
    const DAY: i64 = 24 * 60 * 60;
    let now = chrono::Utc::now().timestamp();
    let checked = crate::settings::umu_runtimes_checked_at(db).ok().flatten();
    let due = !checked.is_some_and(|at| (0..DAY).contains(&(now - at)));
    let missing = needed.filter(|name| !umu_data().join(name).is_dir());
    if !due && missing.is_none() {
        return;
    }
    let Some(umu) = find_in_path("umu-run") else {
        return;
    };
    let _ = crate::paths::ensure_dir(&dirs.logs());
    let Ok(log) = std::fs::File::create(dirs.logs().join("umu-runtimes.log")) else {
        return;
    };
    let present = if due { runtimes() } else { Vec::new() };
    let names = present
        .iter()
        .filter_map(|runtime| runtime.file_name()?.to_str())
        .chain(missing);
    for name in names {
        if name.ends_with("-arm64") != cfg!(target_arch = "aarch64") {
            continue;
        }
        let Some(Ok(tool)) = runtime_tool(dirs, name) else {
            continue;
        };
        if crate::paths::ensure_dir(&dirs.data.join("umu-runtimes")).is_err() {
            return;
        }
        let output =
            |log: &std::fs::File| log.try_clone().map_or_else(|_| Stdio::null(), Into::into);
        let _ = refresh_command(&umu, &tool, dirs)
            .stdout(output(&log))
            .stderr(output(&log))
            .status()
            .await;
    }
    if due {
        let _ = crate::settings::set_umu_runtimes_checked_at(db, now);
    }
}

/// What brings a runtime up to date: umu running `true` through `tool`, from a folder of its own.
/// umu takes its first argument for a program in the working directory when one of that name is
/// there: from a game's folder (the command line started there), a `true` the game wrote would
/// run, not isolated.
fn refresh_command(umu: &Path, tool: &Path, dirs: &Dirs) -> tokio::process::Command {
    let own = dirs.data.join("umu-runtimes");
    let mut command = tokio::process::Command::new(umu);
    command
        .arg("true")
        .env("PROTONPATH", tool)
        .env("GAMEID", "umu-0")
        // Else umu makes `~/Games/umu/umu-0` for a prefix.
        .env("WINEPREFIX", &own)
        .current_dir(own)
        .stdin(Stdio::null());
    command
}

/// umu's runtimes (Steam Linux Runtime 2, 3, 4…), in umu's data folder.
fn runtimes() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(umu_data())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("steamrt"))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    found.sort();
    found
}

/// umu's own files (its runtimes and scripts), which it runs from inside the container.
fn umu_data() -> PathBuf {
    data_home().join("umu")
}

fn data_home() -> PathBuf {
    xdg_home("XDG_DATA_HOME", ".local/share")
}

fn cache_home() -> PathBuf {
    xdg_home("XDG_CACHE_HOME", ".cache")
}

fn xdg_home(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(default)))
        .unwrap_or_default()
}

/// Paths for pressure-vessel's lists, `:`-separated. It knows no escape for `:` (it splits on
/// every one), so such a path cannot be shared: the launch is refused rather than started
/// without the folder.
fn path_list(paths: &[PathBuf]) -> Result<String> {
    let mut out = Vec::new();
    for path in paths {
        let path = path.display().to_string();
        if path.contains(':') {
            return Err(Error::Refused(format!(
                "{path} cannot be shared with the container: its path contains `:`"
            )));
        }
        out.push(path);
    }
    Ok(out.join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::Platform;

    /// umu takes `true` for a program in its working directory when one of that name is there:
    /// started from a game's folder, the refresh would run a `true` the game wrote there, not
    /// isolated. It runs in a folder of SlattyLauncher's own.
    #[test]
    fn the_runtime_refresh_runs_in_a_folder_of_its_own() {
        let dirs = Dirs::under(Path::new("/[FAKE]/slatty"));
        let command = refresh_command(Path::new("/umu-run"), Path::new("/tool"), &dirs);
        assert_eq!(
            command.as_std().get_current_dir(),
            Some(dirs.data.join("umu-runtimes").as_path())
        );
    }

    /// Proton compares what it wrote with Python's `str(os.path.getmtime(…))`; anything else and it
    /// would apply the fixups again inside the sandbox, where the build is read-only. These are
    /// what Proton wrote for three builds Steam installed, with their files' times.
    #[test]
    fn modification_times_are_written_as_python_does() {
        assert_eq!(python_time(1785465617, 157678679), "1785465617.1576786");
        assert_eq!(python_time(1790982754, 235707053), "1790982754.235707");
        assert_eq!(python_time(1790982750, 502144544), "1790982750.5021446");
        assert_eq!(python_time(1790982750, 0), "1790982750.0");
    }

    /// Valve's Proton builds apply SteamPipe's fixups to their own folder at their first launch
    /// after a Steam update, under `dist.lock`: done before an isolated launch, outside the
    /// sandbox, where the build is read-only. A build Proton never set up is refused.
    #[test]
    fn valve_builds_are_set_up_before_an_isolated_launch() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("slatty-valve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files/share/default_pfx")).unwrap();
        std::fs::write(dir.join("files/lib.so"), b"").unwrap();
        std::fs::write(
            dir.join("steampipe_fixups.json"),
            r#"{"empty_dirs": ["files/lib/empty", "../outside"], "no_write_paths": ["files/lib.so"]}"#,
        )
        .unwrap();
        let fixed = |p: &str| dir.join(p);

        assert!(set_up_valve_build(&dir).unwrap(), "a build Steam ships");
        assert!(fixed("files/lib/empty").is_dir());
        assert!(!dir.parent().unwrap().join("outside").exists());
        let mode = std::fs::metadata(fixed("files/lib.so"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o222, 0);
        assert!(fixed("dist.lock").is_file());
        let json = std::fs::metadata(fixed("steampipe_fixups.json")).unwrap();
        assert_eq!(
            std::fs::read_to_string(fixed("files/steampipe_fixups_mtime")).unwrap(),
            format!("{}\n", python_time(json.mtime(), json.mtime_nsec() as u32))
        );
        // Done: left as it is until Steam updates the build.
        std::fs::remove_dir(fixed("files/lib/empty")).unwrap();
        assert!(set_up_valve_build(&dir).unwrap());
        assert!(!fixed("files/lib/empty").exists());

        std::fs::remove_dir_all(fixed("files/share/default_pfx")).unwrap();
        assert!(matches!(set_up_valve_build(&dir), Err(Error::Refused(_))));
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        assert!(!set_up_valve_build(&other).unwrap(), "not one of Valve's");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A game set to a `<project>-latest` build closed at once when isolated: bubblewrap cannot
    /// mount over a link ("Can't mount on symlink destination"). The build it leads to is mounted.
    #[test]
    fn a_latest_link_is_mounted_where_it_leads() {
        let dir = std::env::temp_dir().join(format!("slatty-bind-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("GE-Proton11-7")).unwrap();
        std::os::unix::fs::symlink("GE-Proton11-7", dir.join("GE-Proton-latest")).unwrap();
        let mut out = Vec::new();
        bind(&mut out, "--ro-bind", &dir.join("GE-Proton-latest")).unwrap();
        let real = dir.join("GE-Proton11-7").canonicalize().unwrap();
        let real = real.to_str().unwrap();
        assert_eq!(out, ["--ro-bind", real, real]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An isolated launch where bubblewrap cannot hold the game is refused with the reason, never
    /// run without isolation; `false` stands in for a bubblewrap AppArmor stops.
    #[test]
    fn isolation_refuses_a_bubblewrap_that_cannot_hold_the_game() {
        assert!(matches!(sandbox(None), Err(Error::NotFound(_))));
        let refused = sandbox(find_in_path("false")).unwrap_err();
        assert!(matches!(refused, Error::Refused(_)));
        assert!(refused.to_string().contains("Isolation can be turned off"));
        let ok = find_in_path("true").unwrap();
        assert_eq!(sandbox(Some(ok.clone())).unwrap(), ok);
    }

    #[test]
    fn umu_gets_the_games_id_or_the_unknown_one() {
        let dirs = Dirs::under(Path::new("/nonexistent"));
        let mut install = Install {
            game_id: "1000000002".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: false,
        };
        let game_id = |install: &Install| {
            umu_env(&dirs, install, Path::new("/proton"), Path::new("/prefix"))
                .unwrap()
                .into_iter()
                .find(|(k, _)| k == "GAMEID")
                .map(|(_, v)| v)
        };
        assert_eq!(game_id(&install).as_deref(), Some("umu-0"));
        install.umu_id = Some("umu-1091500".into());
        assert_eq!(game_id(&install).as_deref(), Some("umu-1091500"));
    }

    fn value<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn an_isolated_game_gets_a_home_of_its_own_and_only_its_folder() {
        let root = std::env::temp_dir().join(format!("slatty-isolated-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let mut install = Install {
            game_id: "1000000002".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game Remastered".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let env = umu_env(&dirs, &install, Path::new("/proton"), Path::new("/prefix")).unwrap();
        let home = dirs.data.join("homes/1000000002");
        let home_str = home.to_str().unwrap();
        assert_eq!(value(&env, "PRESSURE_VESSEL_HOME"), Some(home_str));
        assert!(home.is_dir(), "made ready for the container");
        assert_eq!(
            value(&env, "PRESSURE_VESSEL_FILESYSTEMS_RW"),
            Some("/games/Game Remastered")
        );
        let read_only = value(&env, "PRESSURE_VESSEL_FILESYSTEMS_RO").unwrap();
        for shared in [
            umu_data(),
            crate::setup::redist_dir(&dirs),
            crate::installer::support_dir(&dirs, "1000000002"),
        ] {
            assert!(read_only.contains(shared.to_str().unwrap()), "{read_only}");
        }
        // umu would share the whole filesystem under it, and the container `$TMPDIR`.
        assert_eq!(value(&env, "STEAM_COMPAT_INSTALL_PATH"), Some(""));
        for temp in ["TMPDIR", "TMP", "TEMP", "TEMPDIR"] {
            assert_eq!(value(&env, temp), Some("/tmp"), "{temp}");
        }
        assert_eq!(value(&env, "DBUS_SESSION_BUS_ADDRESS"), Some("disabled:"));
        assert_eq!(value(&env, "DBUS_SYSTEM_BUS_ADDRESS"), Some("disabled:"));
        // The user's environment could share the home folder or more folders through these.
        assert_eq!(value(&env, "PRESSURE_VESSEL_SHARE_HOME"), Some("0"));
        for shared in [
            "STEAM_COMPAT_MOUNT_PATHS",
            "STEAM_COMPAT_TOOL_PATH",
            "STEAM_COMPAT_APP_LIBRARY_PATH",
            "STEAM_COMPAT_APP_LIBRARY_PATHS",
            "STEAM_EXTRA_COMPAT_TOOLS_PATHS",
            "STEAM_RUNTIME_SCOUT",
            "STEAM_COMPAT_LIBRARY_PATHS",
            "STEAM_COMPAT_CLIENT_INSTALL_PATH",
        ] {
            assert_eq!(value(&env, shared), Some(""), "{shared}");
        }
        assert_eq!(value(&env, "PROTON_LOG_DIR"), Some(home_str));
        // The container shares `~/.steam` from `$HOME`; umu keeps its data where it is.
        assert_eq!(value(&env, "HOME"), Some(home_str));
        assert_eq!(
            value(&env, "XDG_DATA_HOME").map(Path::new),
            umu_data().parent()
        );
        assert_eq!(
            value(&env, "XDG_CACHE_HOME").map(Path::new),
            Some(cache_home().as_path())
        );

        install.isolated = false;
        let env = umu_env(&dirs, &install, Path::new("/proton"), Path::new("/prefix")).unwrap();
        assert_eq!(
            value(&env, "STEAM_COMPAT_INSTALL_PATH"),
            Some("/games/Game Remastered")
        );
        assert!(env.iter().all(|(k, _)| !k.starts_with("PRESSURE_VESSEL")));
        assert!(env.iter().all(|(k, _)| k != "HOME"));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The daily refresh hands umu a runtime tool of its own, which needs nothing from GitHub:
    /// umu reached through `UMU_NO_PROTON` asks GitHub for its Proton releases first.
    #[test]
    fn runtimes_are_refreshed_through_a_tool_naming_them() {
        let root = std::env::temp_dir().join(format!("slatty-rt-tool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        assert!(runtime_tool(&dirs, "steamrt9").is_none(), "unknown to umu");
        let tool = runtime_tool(&dirs, "steamrt3").unwrap().unwrap();
        let manifest = std::fs::read_to_string(tool.join("toolmanifest.vdf")).unwrap();
        assert!(manifest.contains("\"require_tool_appid\"\t\"1628350\""));
        assert!(manifest.contains("\"compatmanager_layer_name\"\t\"umu-passthrough\""));
        let entry = tool.join("entry-point");
        assert_eq!(
            std::fs::read_to_string(&entry).unwrap(),
            "#!/bin/sh\nexec \"$@\"\n"
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            entry.metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        // Made again, as found.
        assert_eq!(runtime_tool(&dirs, "steamrt3").unwrap().unwrap(), tool);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Without isolation the game has the user's own home, where only Steam installed natively
    /// makes `~/.steam/sdk64`: a Valve build from Steam's Flatpak would stop at steam.exe's
    /// assertion. It is refused with what to do instead.
    #[test]
    fn a_valve_build_without_steams_client_library_is_refused_with_a_reason() {
        let root = std::env::temp_dir().join(format!("slatty-no-sdk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let valve = root.join("lib/steamapps/common/Proton - Experimental");
        assert!(matches!(
            check_steam_sdk_in(&home, &valve),
            Err(Error::Refused(_))
        ));
        assert!(check_steam_sdk_in(&home, &root.join("protons/GE-Proton11-7")).is_ok());
        std::fs::create_dir_all(home.join(".steam/sdk64")).unwrap();
        std::fs::write(home.join(".steam/sdk64/steamclient.so"), b"").unwrap();
        assert!(check_steam_sdk_in(&home, &valve).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn valve_builds_get_steam_client_library_links_in_the_game_home() {
        assert!(valve_proton(Path::new(
            "/mnt/games/SteamLibrary/steamapps/common/Proton - Experimental"
        )));
        assert!(!valve_proton(Path::new(
            "/home/u/.local/share/Steam/compatibilitytools.d/GE-Proton11-7"
        )));
        assert!(!valve_proton(Path::new(
            "/home/u/.local/share/slatty/protons/GE-Proton11-7"
        )));

        let root = std::env::temp_dir().join(format!("slatty-sdk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let libs = [root.join("steam/linux64"), root.join("steam/linux32")];
        for lib in &libs {
            std::fs::create_dir_all(lib).unwrap();
            std::fs::write(lib.join("steamclient.so"), b"").unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        steam_sdk_links(&home, &libs).unwrap();
        steam_sdk_links(&home, &libs).unwrap();
        for (sdk, lib) in ["sdk64", "sdk32"].into_iter().zip(&libs) {
            let link = home.join(".steam").join(sdk).join("steamclient.so");
            assert_eq!(
                std::fs::read_link(&link).unwrap(),
                lib.join("steamclient.so")
            );
        }
        // Replaced by the game with a link out of its home: nothing is written through it.
        std::fs::remove_dir_all(home.join(".steam/sdk64")).unwrap();
        std::os::unix::fs::symlink(root.join("steam"), home.join(".steam/sdk64")).unwrap();
        assert!(matches!(
            steam_sdk_links(&home, &libs),
            Err(Error::Refused(_))
        ));
        assert!(!root.join("steam/steamclient.so").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_folder_the_container_cannot_be_told_is_refused() {
        // pressure-vessel splits its lists on every `:`, escaped or not (checked: the folder
        // was not shared and the game did not start).
        assert!(matches!(
            path_list(&["/games/Game: Remastered".into()]),
            Err(Error::Refused(_))
        ));
        assert_eq!(
            path_list(&["/games/Game\\Remastered".into(), "/b".into()]).unwrap(),
            "/games/Game\\Remastered:/b"
        );
    }

    #[test]
    fn a_program_umu_could_find_on_disk_or_a_linked_working_directory_is_refused() {
        let dir = std::env::temp_dir().join(format!("slatty-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("game/sub")).unwrap();
        std::fs::create_dir_all(dir.join("outside")).unwrap();
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.join("game"),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let check = |program: &str, cwd: &Path| check_shared(&install, &[program.into()], cwd);
        let game = install.path.clone();
        assert!(check("Z:\\games\\game.exe", &game).is_ok());
        assert!(check("sh", &game.join("sub")).is_ok());
        // Planted by the game in its folder: umu would take the folder for the game's and share
        // the whole filesystem under it.
        for planted in ["sh", "Z:\\games\\game.exe"] {
            std::fs::write(game.join(planted), b"").unwrap();
            assert!(
                matches!(check(planted, &game), Err(Error::Refused(_))),
                "{planted}"
            );
        }
        assert!(matches!(check("~", &game), Err(Error::Refused(_))));
        assert!(matches!(check("..", &game), Err(Error::Refused(_))));
        // A working directory that is a link out of the game folder: the container shares where
        // it leads.
        std::os::unix::fs::symlink(dir.join("outside"), game.join("wd")).unwrap();
        assert!(matches!(
            check("wineboot", &game.join("wd")),
            Err(Error::Refused(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn programs_are_named_so_that_umu_shares_nothing_more() {
        assert_eq!(
            windows_path(Path::new("/mnt/games/Game/bin/game.exe")),
            "Z:\\mnt\\games\\Game\\bin\\game.exe"
        );
        let dir = std::env::temp_dir().join(format!("slatty-interp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("start.sh");
        for (first, name) in [
            ("#!/bin/bash", "bash"),
            ("#! /usr/bin/env bash -e", "bash"),
            ("#!/bin/sh", "sh"),
            ("echo no line for an interpreter", "sh"),
        ] {
            std::fs::write(&script, format!("{first}\necho hi\n")).unwrap();
            assert_eq!(interpreter_name(&script).unwrap(), name, "{first}");
        }
        // Rewritten by the game: umu would take the script for its configuration (where the
        // prefix it shares is), run winetricks, or a file it shares the folder of.
        for first in [
            "#!/usr/bin/env --config",
            "#!--config",
            "#!/usr/bin/winetricks",
            "#!/bin/run",
            "#!/usr/bin/python3",
        ] {
            std::fs::write(&script, format!("{first}\n[umu]\nprefix = \"/\"\n")).unwrap();
            assert!(
                matches!(interpreter_name(&script), Err(Error::Refused(_))),
                "{first}"
            );
        }
        // A pipe in its place is not waited on.
        std::fs::remove_file(&script).unwrap();
        nix::unistd::mkfifo(&script, nix::sys::stat::Mode::S_IRWXU).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let pipe = script.clone();
        std::thread::spawn(move || tx.send(interpreter_name(&pipe).ok()));
        let name = rx.recv_timeout(std::time::Duration::from_secs(5));
        assert_eq!(name.expect("not blocked").as_deref(), Some("sh"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_isolated_home_is_never_outside_the_data_folder() {
        let dirs = Dirs::under(Path::new("/data"));
        assert!(isolated_home(&dirs, "../1").is_err());
        assert_eq!(
            isolated_home(&dirs, "1").unwrap(),
            dirs.data.join("homes/1")
        );
    }

    /// The `.ref` locks pressure-vessel takes in a runtime are made in its real platform folders
    /// alone: never in `var`, which an isolated game writes, through a link it put there (the file
    /// would be made where the link leads, outside any sandbox), nor shared when one is a link.
    #[test]
    fn the_runtimes_locks_are_made_in_its_own_folders_only() {
        let root = std::env::temp_dir().join(format!("slatty-ref-locks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let runtime = root.join("steamrt3");
        std::fs::create_dir_all(runtime.join("sniper_platform_0.1/files")).unwrap();
        std::fs::create_dir_all(runtime.join("var/files")).unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::os::unix::fs::symlink(root.join("outside/x"), runtime.join("var/files/.ref")).unwrap();
        std::fs::create_dir_all(runtime.join("linked/files")).unwrap();
        std::os::unix::fs::symlink(root.join("outside/y"), runtime.join("linked/files/.ref"))
            .unwrap();
        let locks = ref_locks(&runtime).unwrap();
        assert_eq!(locks, [runtime.join("sniper_platform_0.1/files/.ref")]);
        assert!(locks[0].is_file());
        assert!(!root.join("outside/x").exists() && !root.join("outside/y").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// pressure-vessel looks at `$HOME/.steam`, here the game's own home, which the game writes,
    /// from outside the container: a link the game put there to the user's Steam installation would
    /// have it shared as the user's own (the client's settings and tokens). Such links go before
    /// each isolated launch; SlattyLauncher's own `sdk64` and `sdk32` folders stay.
    #[test]
    fn links_a_game_put_in_its_steam_folder_go_before_it_starts() {
        let root = std::env::temp_dir().join(format!("slatty-home-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root.join("slatty"));
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: root.join("game"),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let home = isolated_home(&dirs, "1").unwrap();
        let steam = root.join("user/.steam");
        std::fs::create_dir_all(&steam).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(&steam, home.join(".steam")).unwrap();
        isolation_env(&dirs, &install, None).unwrap();
        assert!(
            std::fs::symlink_metadata(home.join(".steam")).map_or(true, |m| !m.is_symlink()),
            "the link went"
        );
        assert!(steam.is_dir(), "not what it led to");
        std::fs::create_dir_all(home.join(".steam/sdk64")).unwrap();
        std::os::unix::fs::symlink(&steam, home.join(".steam/root")).unwrap();
        isolation_env(&dirs, &install, None).unwrap();
        assert!(std::fs::symlink_metadata(home.join(".steam/root")).is_err());
        assert!(home.join(".steam/sdk64").is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// An isolated game following `<project>-latest` gets the build the link leads to as the
    /// launch is made, for umu as for the read-only mount: a newer build linked in between (a
    /// download finishing while the cloud syncs) would otherwise run writable in the container.
    #[test]
    fn an_isolated_game_runs_the_build_its_link_led_to() {
        let dir = std::env::temp_dir().join(format!("slatty-latest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let build = dir.join("GE-Proton1-1");
        std::fs::create_dir_all(&build).unwrap();
        std::os::unix::fs::symlink(&build, dir.join("GE-Proton-latest")).unwrap();
        let mut install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.join("game"),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let dirs = Dirs::under(&dir.join("slatty"));
        let latest = dir.join("GE-Proton-latest");
        let env = umu_env(&dirs, &install, &latest, Path::new("/prefix")).unwrap();
        let real = build.canonicalize().unwrap();
        assert_eq!(value(&env, "PROTONPATH"), real.to_str());
        install.isolated = false;
        let env = umu_env(&dirs, &install, &latest, Path::new("/prefix")).unwrap();
        assert_eq!(
            value(&env, "PROTONPATH"),
            latest.to_str(),
            "followed as it moves"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// umu starts a Proton build whose `toolmanifest.vdf` names no runtime (Proton-tkg and other
    /// builds made to run on the host) without its container: the game would see the whole disk
    /// through Z: while shown isolated. Such a build is refused for an isolated game, before
    /// anything starts; one naming the sniper runtime is not.
    #[test]
    fn a_proton_build_umu_runs_outside_its_container_is_refused_isolated() {
        let dir = std::env::temp_dir().join(format!("slatty-host-proton-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let proton = dir.join("proton-tkg");
        std::fs::create_dir_all(&proton).unwrap();
        std::fs::write(proton.join("proton"), b"").unwrap();
        let manifest = |extra: &str| {
            let text = format!(
                "\"manifest\"\n{{\n\t\"version\"\t\"2\"\n\t\"commandline\"\t\"/proton %verb%\"\n{extra}}}\n"
            );
            std::fs::write(proton.join("toolmanifest.vdf"), text).unwrap();
        };
        let mut install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.join("game"),
            client_id: None,
            runner: Runner::Umu {
                proton: proton.clone(),
                prefix: dir.join("prefix"),
            },
            umu_id: None,
            isolated: true,
        };
        let dirs = Dirs::under(&dir.join("slatty"));
        let outside = |install: &Install| {
            matches!(windows_command(&dirs, install, vec![], dir.clone()),
                Err(Error::Refused(m)) if m.contains("outside umu's container"))
        };
        manifest("");
        assert!(outside(&install));
        std::fs::remove_file(proton.join("toolmanifest.vdf")).unwrap();
        assert!(outside(&install), "none at all");
        manifest("\t\"require_tool_appid\"\t\"1628350\"\n");
        assert!(!outside(&install), "the sniper runtime");
        manifest("");
        install.isolated = false;
        assert!(!outside(&install), "not isolated, as the user");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// umu installs a runtime it does not have during the launch, inside the sandbox, where nothing
    /// binds it read-only: the game could change it and have its code run outside its container at
    /// the next launch. Such a launch is refused (`refresh_runtimes` installs the runtime before,
    /// outside), and the runtime each isolated launch needs is known before it starts.
    #[test]
    fn an_isolated_launch_needs_its_runtime_installed_before() {
        let dir = std::env::temp_dir().join(format!("slatty-runtime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("umu/steamrt3")).unwrap();
        assert!(runtime_installed(&dir.join("umu"), "steamrt3").is_ok());
        assert!(runtime_installed(&dir.join("umu"), "steamrt4").is_err());

        let proton = dir.join("proton");
        std::fs::create_dir_all(&proton).unwrap();
        std::fs::write(proton.join("proton"), b"").unwrap();
        let manifest = |appid: &str| {
            let text = format!("\"manifest\"\n{{\n\t\"require_tool_appid\"\t\"{appid}\"\n}}\n");
            std::fs::write(proton.join("toolmanifest.vdf"), text).unwrap();
        };
        let mut install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.join("game"),
            client_id: None,
            runner: Runner::Umu {
                proton: proton.clone(),
                prefix: dir.join("prefix"),
            },
            umu_id: None,
            isolated: true,
        };
        manifest("4183110");
        assert_eq!(container(&install), Some("steamrt4"));
        // umu's own folder, this computer's: a runtime it lacks, if any.
        if let Some((runtime, appid)) = RUNTIME_APPIDS
            .iter()
            .find(|(name, _)| !umu_data().join(name).exists())
        {
            manifest(appid);
            let dirs = Dirs::under(&dir.join("slatty"));
            let refused = windows_command(&dirs, &install, vec![], dir.clone());
            assert!(
                matches!(&refused, Err(Error::Refused(m)) if m.contains(runtime)),
                "{refused:?}"
            );
        }
        install.isolated = false;
        assert_eq!(container(&install), None);
        install.isolated = true;
        install.platform = Platform::Linux;
        install.runner = Runner::Native;
        assert_eq!(container(&install), Some("steamrt3"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn wine_alone_refuses_to_run_a_game_meant_to_be_isolated() {
        let dirs = Dirs::under(Path::new("/nonexistent"));
        let mut install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Wine {
                wine: "/wine".into(),
                prefix: "/prefix".into(),
            },
            umu_id: None,
            isolated: true,
        };
        let run = |install: &Install| windows_command(&dirs, install, vec![], "/".into());
        assert!(matches!(run(&install), Err(Error::Refused(_))));
        install.isolated = false;
        assert!(run(&install).is_ok());
    }
    #[test]
    fn a_native_script_starts_through_an_interpreter_that_exists() {
        let dir = std::env::temp_dir().join(format!("slatty-native-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("start.sh");
        let arg = vec![script.display().to_string()];
        // GOG's scripts name /bin/bash, which NixOS does not have.
        std::fs::write(&script, "#!/nonexistent/bin/sh \necho hi\n").unwrap();
        let (program, args, env) = native_command(&script, None);
        assert_eq!(program, find_in_path("sh").unwrap());
        assert_eq!(args, arg);
        assert!(env.is_empty());
        // An interpreter that exists: the script runs as it is.
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        if Path::new("/bin/sh").exists() {
            assert_eq!(
                native_command(&script, None),
                (script.clone(), Vec::new(), Vec::new())
            );
        }
        // steam-run runs it in a usual Linux layout.
        let run = PathBuf::from("/bin/steam-run");
        assert_eq!(
            native_command(&script, Some(Wrapper::SteamRun(run.clone()))),
            (run, arg.clone(), Vec::new())
        );
        // umu without Proton, in the Steam Linux Runtime 3.0, through SlattyLauncher's tool.
        let umu = PathBuf::from("/bin/umu-run");
        let tool = PathBuf::from("/data/umu-runtimes/tools/steamrt3");
        let (program, args, env) =
            native_command(&script, Some(Wrapper::Umu(umu.clone(), tool.clone())));
        assert_eq!((program, args), (umu, arg));
        assert!(env.contains(&("PROTONPATH".into(), tool.display().to_string())));
        assert!(env.iter().all(|(k, _)| k != "UMU_NO_PROTON"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Proton's fixes for a game target the game: DOOM's adds `+r_renderAPI 1` to every program
    /// run with its id, which made DirectX's setup and `reg add` refuse their command line.
    #[test]
    fn protons_fixes_apply_to_the_game_only() {
        let dir = std::env::temp_dir().join(format!("slatty-fixes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("proton")).unwrap();
        std::fs::write(dir.join("proton/proton"), b"").unwrap();
        std::fs::write(
            dir.join("goggame-1.info"),
            r#"{"gameId":"1","name":"Game","playTasks":[
              {"category":"game","isPrimary":true,"name":"Game","path":"game.exe","type":"FileTask"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("game.exe"), b"").unwrap();
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.clone(),
            client_id: None,
            runner: Runner::Umu {
                proton: dir.join("proton"),
                prefix: dir.join("prefix"),
            },
            umu_id: Some("umu-379720".into()),
            isolated: false,
        };
        let dirs = Dirs::under(&dir.join("slatty"));
        assert!(find_in_path("umu-run").is_some(), "run in `nix develop`");
        let game = launch_spec(&dirs, &install, None).unwrap();
        assert_eq!(value(&game.env, "PROTONFIXES_DISABLE"), None);
        assert_eq!(value(&game.env, "GAMEID"), Some("umu-379720"));
        let tool = windows_command(&dirs, &install, vec!["reg".into()], dir.clone()).unwrap();
        assert_eq!(value(&tool.env, "PROTONFIXES_DISABLE"), Some("1"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_game_can_be_started_as_one_of_its_visible_tasks() {
        let dir = std::env::temp_dir().join(format!("slatty-tasks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // As in The Legend of Heroes: Trails in the Sky.
        std::fs::write(
            dir.join("goggame-1.info"),
            r#"{"gameId":"1","name":"Game","playTasks":[
              {"category":"game","isPrimary":true,"name":"Game","path":"game.exe","type":"FileTask"},
              {"category":"launcher","name":"Configuration Tool","path":"Config.exe","type":"FileTask"},
              {"category":"game","isHidden":true,"name":"Configuration Tool - launcher process","path":"game.exe","type":"FileTask"},
              {"category":"document","name":"Support","link":"https://example.invalid","type":"URLTask"}]}"#,
        )
        .unwrap();
        for exe in ["game.exe", "Config.exe"] {
            std::fs::write(dir.join(exe), b"").unwrap();
        }
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: dir.clone(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: false,
        };
        assert_eq!(launch_options(&install), ["Game", "Configuration Tool"]);
        let exe = |choice| windows_task(&install, choice).unwrap().0;
        assert_eq!(exe(None), dir.join("game.exe"));
        assert_eq!(exe(Some("Configuration Tool")), dir.join("Config.exe"));
        assert_eq!(
            exe(Some("No longer offered")),
            dir.join("game.exe"),
            "the main task"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
