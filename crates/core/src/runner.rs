use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::doctor::find_in_path;
use crate::error::{Error, Result};
use crate::gameinfo::{self, GameInfo};
use crate::install::{Install, Platform};

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
pub fn launch_spec(install: &Install, choice: Option<&str>) -> Result<LaunchSpec> {
    match (&install.platform, &install.runner) {
        (Platform::Linux, Runner::Native) => {
            let script = install.path.join("start.sh");
            if !script.is_file() {
                return Err(Error::NotFound(format!("{} is missing", script.display())));
            }
            // NixOS keeps neither `/bin/bash` (which GOG's scripts ask for) nor the libraries games
            // load (OpenGL, sound) where they look: they run in a usual Linux layout there.
            let wrapper = if Path::new("/etc/NIXOS").exists() {
                find_in_path("steam-run")
                    .map(Wrapper::SteamRun)
                    .or_else(|| find_in_path("umu-run").map(Wrapper::Umu))
            } else {
                None
            };
            let (program, args, env) = native_command(&script, wrapper);
            Ok(LaunchSpec {
                program,
                args,
                cwd: install.path.clone(),
                env,
            })
        }
        (Platform::Windows, Runner::Umu { .. } | Runner::Wine { .. }) => {
            let (exe, args, cwd) = windows_task(install, choice)?;
            windows_command(install, prepend(exe, args), cwd)
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

/// Whether the game's Wine prefix has been created (always true for native games). Nothing may be
/// written into a prefix before, or Proton would take it for created and leave it half set up.
pub fn prefix_ready(install: &Install) -> bool {
    install
        .runner
        .prefix()
        .is_none_or(|p| p.join("drive_c/users").is_dir())
}

/// `wineboot` for a prefix that has never been created, so save folders exist before the first launch.
pub fn prefix_init_spec(install: &Install) -> Result<Option<LaunchSpec>> {
    let Some(prefix) = install.runner.prefix().filter(|_| !prefix_ready(install)) else {
        return Ok(None);
    };
    crate::paths::ensure_dir(prefix)?;
    windows_command(
        install,
        vec!["wineboot".into(), "-u".into()],
        install.path.clone(),
    )
    .map(Some)
}

pub fn windows_command(install: &Install, args: Vec<String>, cwd: PathBuf) -> Result<LaunchSpec> {
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
            Ok(LaunchSpec {
                program: umu,
                args,
                cwd,
                env: umu_env(install, proton, prefix),
            })
        }
        Runner::Wine { wine, prefix } => Ok(LaunchSpec {
            program: wine.clone(),
            args,
            cwd,
            env: vec![("WINEPREFIX".into(), prefix.display().to_string())],
        }),
        Runner::Native => Err(Error::Unsupported("Windows command without Wine".into())),
    }
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
fn umu_env(install: &Install, proton: &Path, prefix: &Path) -> Vec<(String, String)> {
    let game_id = install
        .umu_id
        .clone()
        .unwrap_or_else(|| crate::umu::UNKNOWN.into());
    vec![
        ("WINEPREFIX".into(), prefix.display().to_string()),
        ("PROTONPATH".into(), proton.display().to_string()),
        ("GAMEID".into(), game_id),
        ("STORE".into(), "gog".into()),
        (
            "STEAM_COMPAT_INSTALL_PATH".into(),
            install.path.display().to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::Platform;

    #[test]
    fn umu_gets_the_games_id_or_the_unknown_one() {
        let mut install = Install {
            game_id: "1423049311".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
        };
        let game_id = |install: &Install| {
            umu_env(install, Path::new("/proton"), Path::new("/prefix"))
                .into_iter()
                .find(|(k, _)| k == "GAMEID")
                .map(|(_, v)| v)
        };
        assert_eq!(game_id(&install).as_deref(), Some("umu-0"));
        install.umu_id = Some("umu-1091500".into());
        assert_eq!(game_id(&install).as_deref(), Some("umu-1091500"));
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
