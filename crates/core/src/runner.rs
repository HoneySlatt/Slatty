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
    match (&install.platform, &install.runner) {
        (Platform::Linux, Runner::Native) => {
            let script = install.path.join("start.sh");
            if !script.is_file() {
                return Err(Error::NotFound(format!("{} is missing", script.display())));
            }
            // Only umu's container isolates. Elsewhere on NixOS, which keeps neither `/bin/bash`
            // (which GOG's scripts ask for) nor the libraries games load (OpenGL, sound) where
            // they look, games run in a usual Linux layout.
            let wrapper = if install.isolated {
                let umu = find_in_path("umu-run").ok_or_else(|| {
                    Error::NotFound("umu-run is needed to run a game isolated".into())
                })?;
                Some(Wrapper::Umu(umu))
            } else if Path::new("/etc/NIXOS").exists() {
                find_in_path("steam-run")
                    .map(Wrapper::SteamRun)
                    .or_else(|| find_in_path("umu-run").map(Wrapper::Umu))
            } else {
                None
            };
            let (mut program, mut args, mut env) = native_command(&script, wrapper);
            if install.isolated {
                env.extend(isolation_env(dirs, install, None)?);
                // Without one, umu takes `~/Games/umu/<id>` or the user's `$WINEPREFIX` as the
                // prefix, and the container shares it: the game's own home takes that place.
                env.push((
                    "WINEPREFIX".into(),
                    isolated_home(dirs, &install.game_id)?.display().to_string(),
                ));
                // Given a file, umu would share the whole filesystem it is on: the script goes to
                // its interpreter, which umu looks for inside the container.
                args.insert(0, interpreter_name(&script)?);
                check_shared(install, &args, &install.path)?;
                env.push(("UMU_RUNTIME_UPDATE".into(), "0".into()));
                (program, args) = read_only_tools(program, args, None)?;
            }
            Ok(LaunchSpec {
                program,
                args,
                cwd: install.path.clone(),
                env,
            })
        }
        (Platform::Windows, Runner::Umu { .. } | Runner::Wine { .. }) => {
            let (exe, args, cwd) = windows_task(install, choice)?;
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
    /// downloads for Windows games.
    Umu(PathBuf),
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
        Some(Wrapper::Umu(umu)) => {
            let env = [
                ("UMU_NO_PROTON", "1"),
                ("RUNTIMEPATH", "steamrt3"),
                ("GAMEID", "umu-0"),
            ];
            let env = env.map(|(k, v)| (k.to_string(), v.to_string())).to_vec();
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
            let umu = find_in_path("umu-run")
                .ok_or_else(|| Error::NotFound("umu-run is not in PATH".into()))?;
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
    let mut env = vec![
        ("WINEPREFIX".into(), prefix.display().to_string()),
        ("PROTONPATH".into(), proton.display().to_string()),
        ("GAMEID".into(), game_id),
        ("STORE".into(), "gog".into()),
    ];
    // Given this, umu would share the whole filesystem the game is on (`/home`, a library disk),
    // writable: an isolated game's folder is shared on its own instead. Proton's fixes then find
    // the game from its working directory.
    if install.isolated {
        env.extend(isolation_env(dirs, install, Some(proton))?);
    } else {
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

/// Where an isolated game finds a home folder of its own, in place of the user's. It holds what
/// the game writes there, Linux games' saves among them.
pub fn isolated_home(dirs: &Dirs, game_id: &str) -> Result<PathBuf> {
    crate::paths::check_game_id(game_id)?;
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
fn read_only_tools(
    umu: PathBuf,
    args: Vec<String>,
    proton: Option<&Path>,
) -> Result<(PathBuf, Vec<String>)> {
    let bwrap = find_in_path("bwrap").ok_or_else(|| {
        Error::NotFound("bubblewrap (bwrap) is needed to run a game isolated".into())
    })?;
    let mut out: Vec<String> = ["--dev-bind", "/", "/"].map(String::from).to_vec();
    let mut bind = |how: &str, path: &Path| {
        let path = path.display().to_string();
        out.extend([how.to_string(), path.clone(), path]);
    };
    for runtime in runtimes() {
        bind("--ro-bind", &runtime);
        let var = runtime.join("var");
        crate::paths::ensure_dir(&var)?;
        bind("--bind", &var);
        for platform in std::fs::read_dir(&runtime).into_iter().flatten().flatten() {
            let files = platform.path().join("files");
            if files.is_dir() {
                let lock = files.join(".ref");
                if !lock.exists() {
                    std::fs::File::create(&lock)
                        .map_err(|e| Error::io(format!("create {}", lock.display()), e))?;
                }
                bind("--bind", &lock);
            }
        }
    }
    if let Some(proton) = proton {
        bind("--ro-bind", proton);
    }
    out.push("--".into());
    out.push(umu.display().to_string());
    out.extend(args);
    Ok((bwrap, out))
}

/// Brings umu's runtimes up to date, at most once a day, before an isolated launch: such
/// launches use them read-only and leave them as they are (`read_only_tools`). umu runs nothing of
/// a game's for it (`true` in each runtime), not isolated. A failure leaves the runtimes as they
/// are, and the game starts with them.
pub async fn refresh_runtimes(db: &crate::db::Db, dirs: &Dirs) {
    const DAY: i64 = 24 * 60 * 60;
    let now = chrono::Utc::now().timestamp();
    let checked = crate::settings::umu_runtimes_checked_at(db).ok().flatten();
    if checked.is_some_and(|at| (0..DAY).contains(&(now - at))) {
        return;
    }
    let Some(umu) = find_in_path("umu-run") else {
        return;
    };
    let _ = crate::paths::ensure_dir(&dirs.logs());
    let Ok(log) = std::fs::File::create(dirs.logs().join("umu-runtimes.log")) else {
        return;
    };
    for runtime in runtimes() {
        let Some(name) = runtime.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.ends_with("-arm64") != cfg!(target_arch = "aarch64") {
            continue;
        }
        let output =
            |log: &std::fs::File| log.try_clone().map_or_else(|_| Stdio::null(), Into::into);
        let _ = tokio::process::Command::new(&umu)
            .arg("true")
            .env("UMU_NO_PROTON", "1")
            .env("RUNTIMEPATH", name)
            .env("GAMEID", "umu-0")
            // Else umu makes `~/Games/umu/umu-0` for a prefix.
            .env("WINEPREFIX", dirs.data.join("umu-runtimes"))
            .stdin(Stdio::null())
            .stdout(output(&log))
            .stderr(output(&log))
            .status()
            .await;
    }
    let _ = crate::settings::set_umu_runtimes_checked_at(db, now);
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

    #[test]
    fn umu_gets_the_games_id_or_the_unknown_one() {
        let dirs = Dirs::under(Path::new("/nonexistent"));
        let mut install = Install {
            game_id: "1423049311".into(),
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
            game_id: "1423049311".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game Remastered".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let env = umu_env(&dirs, &install, Path::new("/proton"), Path::new("/prefix")).unwrap();
        let home = dirs.data.join("homes/1423049311");
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
            crate::installer::support_dir(&dirs, "1423049311"),
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

    #[test]
    fn valve_builds_get_steam_client_library_links_in_the_game_home() {
        assert!(valve_proton(Path::new(
            "/mnt/share/SteamLibrary/steamapps/common/Proton - Experimental"
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
            windows_path(Path::new("/mnt/share/Games/Game/bin/game.exe")),
            "Z:\\NAS\\Games\\Game\\bin\\game.exe"
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
        // umu without Proton, in the Steam Linux Runtime 3.0.
        let umu = PathBuf::from("/bin/umu-run");
        let (program, args, env) = native_command(&script, Some(Wrapper::Umu(umu.clone())));
        assert_eq!((program, args), (umu, arg));
        assert!(env.contains(&("UMU_NO_PROTON".into(), "1".into())));
        assert!(env.contains(&("RUNTIMEPATH".into(), "steamrt3".into())));
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
