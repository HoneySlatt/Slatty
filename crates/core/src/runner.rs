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

pub fn launch_spec(install: &Install) -> Result<LaunchSpec> {
    match (&install.platform, &install.runner) {
        (Platform::Linux, Runner::Native) => {
            let script = install.path.join("start.sh");
            if !script.is_file() {
                return Err(Error::NotFound(format!("{} is missing", script.display())));
            }
            let steam_run = Path::new("/etc/NIXOS")
                .exists()
                .then(|| find_in_path("steam-run"))
                .flatten();
            let (program, args) = native_command(&script, steam_run);
            Ok(LaunchSpec {
                program,
                args,
                cwd: install.path.clone(),
                env: Vec::new(),
            })
        }
        (Platform::Windows, Runner::Umu { .. } | Runner::Wine { .. }) => {
            let (exe, args, cwd) = windows_task(install)?;
            windows_command(install, prepend(exe, args), cwd)
        }
        (platform, runner) => Err(Error::Unsupported(format!(
            "{platform:?} game with runner {runner:?}"
        ))),
    }
}

/// How to start a native game's script. On NixOS, through `steam-run` when given: it lends the
/// game the usual Linux layout and libraries (`/bin/bash`, OpenGL, sound) that NixOS lacks.
/// Elsewhere, through the interpreter its first line names, found in PATH by name when that path
/// does not exist (GOG's scripts ask for `/bin/bash`).
fn native_command(script: &Path, steam_run: Option<PathBuf>) -> (PathBuf, Vec<String>) {
    let script_arg = vec![script.display().to_string()];
    if let Some(run) = steam_run {
        return (run, script_arg);
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
            Some(found) => (found, script_arg),
            None => (script.to_path_buf(), Vec::new()),
        },
        _ => (script.to_path_buf(), Vec::new()),
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

fn windows_task(install: &Install) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    let info: GameInfo = gameinfo::read(&install.path, Some(&install.game_id))?;
    let task = info.primary_task().ok_or_else(|| {
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
        let (program, args) = native_command(&script, None);
        assert_eq!(program, find_in_path("sh").unwrap());
        assert_eq!(args, arg);
        // An interpreter that exists: the script runs as it is.
        std::fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        if Path::new("/bin/sh").exists() {
            assert_eq!(native_command(&script, None), (script.clone(), Vec::new()));
        }
        // steam-run, when given, runs it in its usual Linux layout.
        let run = PathBuf::from("/run/current-system/sw/bin/steam-run");
        assert_eq!(native_command(&script, Some(run.clone())), (run, arg));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
