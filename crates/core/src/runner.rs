use std::path::{Path, PathBuf};

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
            let (program, mut args, mut env) = native_command(&script, wrapper);
            if install.isolated {
                env.extend(isolation_env(dirs, install)?);
                // Given a file, umu would share the whole filesystem it is on: the script goes to
                // its interpreter, which umu looks for inside the container.
                args.insert(0, interpreter_name(&script));
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
            windows_command(dirs, install, prepend(exe, args), cwd)
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
    let first = std::fs::read(script)
        .ok()
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
/// `/usr/bin/env`), else `sh`.
fn interpreter_name(script: &Path) -> String {
    let bytes = std::fs::read(script).unwrap_or_default();
    let first = bytes.split(|c| *c == b'\n').next().unwrap_or_default();
    let line = String::from_utf8_lossy(first);
    let mut words = line
        .strip_prefix("#!")
        .unwrap_or_default()
        .split_whitespace()
        .map(|w| w.rsplit('/').next().unwrap_or(w));
    match words.next() {
        Some("env") => words.next(),
        other => other,
    }
    .filter(|w| !w.is_empty())
    .unwrap_or("sh")
    .to_string()
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

pub fn windows_command(
    dirs: &Dirs,
    install: &Install,
    args: Vec<String>,
    cwd: PathBuf,
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
            Ok(LaunchSpec {
                program: umu,
                args,
                cwd,
                env: umu_env(dirs, install, proton, prefix)?,
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
        env.extend(isolation_env(dirs, install)?);
    } else {
        env.push((
            "STEAM_COMPAT_INSTALL_PATH".into(),
            install.path.display().to_string(),
        ));
    }
    Ok(env)
}

/// Where an isolated game finds a home folder of its own, in place of the user's. It holds what
/// the game writes there, Linux games' saves among them.
pub fn isolated_home(dirs: &Dirs, game_id: &str) -> Result<PathBuf> {
    crate::paths::check_game_id(game_id)?;
    Ok(dirs.data.join("homes").join(game_id))
}

/// What the container umu runs games in (pressure-vessel, from the Steam Runtime) is told for an
/// isolated game: a home folder of its own, its folder writable, and read-only what umu and the
/// post-install setup need. The rest of the user's files, `/tmp` included, stays out of it.
fn isolation_env(dirs: &Dirs, install: &Install) -> Result<Vec<(String, String)>> {
    let home = isolated_home(dirs, &install.game_id)?;
    crate::paths::ensure_dir(&home)?;
    let read_only = [
        umu_data(),
        crate::setup::redist_dir(dirs),
        crate::installer::support_dir(dirs, &install.game_id),
    ];
    Ok(vec![
        ("PRESSURE_VESSEL_HOME".into(), home.display().to_string()),
        (
            "PRESSURE_VESSEL_FILESYSTEMS_RW".into(),
            path_list(std::slice::from_ref(&install.path)),
        ),
        (
            "PRESSURE_VESSEL_FILESYSTEMS_RO".into(),
            path_list(&read_only),
        ),
        // The container shares the folders these name with the game (checked: any of them in the
        // home folder showed all of it); its own `/tmp` is private. It would also share the last
        // two, writable.
        ("TMPDIR".into(), "/tmp".into()),
        ("TMP".into(), "/tmp".into()),
        ("TEMP".into(), "/tmp".into()),
        ("TEMPDIR".into(), "/tmp".into()),
        ("STEAM_COMPAT_LIBRARY_PATHS".into(), String::new()),
        ("STEAM_COMPAT_CLIENT_INSTALL_PATH".into(), String::new()),
        // Through the D-Bus buses a program could have other services act for it, outside the
        // container (systemd starts commands; the system's rules may let the session suspend or
        // mount drives). Wine, Proton and umu do not need them (checked: Wine saw the same network
        // adapters and drives without the system bus); D-Bus's own "no bus" address keeps them
        // out (a missing socket would stop the container).
        ("DBUS_SESSION_BUS_ADDRESS".into(), "disabled:".into()),
        ("DBUS_SYSTEM_BUS_ADDRESS".into(), "disabled:".into()),
    ])
}

/// umu's own files (its runtimes and scripts), which it runs from inside the container.
fn umu_data() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_default()
        .join("umu")
}

/// Paths for pressure-vessel's lists: `:`-separated, with `:` and `\` escaped.
fn path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| {
            p.display()
                .to_string()
                .replace('\\', "\\\\")
                .replace(':', "\\:")
        })
        .collect::<Vec<_>>()
        .join(":")
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
            path: "/games/Game: Remastered".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
            isolated: true,
        };
        let env = umu_env(&dirs, &install, Path::new("/proton"), Path::new("/prefix")).unwrap();
        let home = dirs.data.join("homes/1423049311");
        assert_eq!(
            value(&env, "PRESSURE_VESSEL_HOME"),
            Some(home.to_str().unwrap())
        );
        assert!(home.is_dir(), "made ready for the container");
        assert_eq!(
            value(&env, "PRESSURE_VESSEL_FILESYSTEMS_RW"),
            Some("/games/Game\\: Remastered")
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
        assert_eq!(value(&env, "STEAM_COMPAT_INSTALL_PATH"), None);
        for temp in ["TMPDIR", "TMP", "TEMP", "TEMPDIR"] {
            assert_eq!(value(&env, temp), Some("/tmp"), "{temp}");
        }
        assert_eq!(value(&env, "DBUS_SESSION_BUS_ADDRESS"), Some("disabled:"));
        assert_eq!(value(&env, "DBUS_SYSTEM_BUS_ADDRESS"), Some("disabled:"));

        install.isolated = false;
        let env = umu_env(&dirs, &install, Path::new("/proton"), Path::new("/prefix")).unwrap();
        assert_eq!(
            value(&env, "STEAM_COMPAT_INSTALL_PATH"),
            Some("/games/Game: Remastered")
        );
        assert!(env.iter().all(|(k, _)| !k.starts_with("PRESSURE_VESSEL")));
        std::fs::remove_dir_all(root).unwrap();
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
            assert_eq!(interpreter_name(&script), name, "{first}");
        }
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
