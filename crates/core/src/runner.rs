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
            Ok(LaunchSpec {
                program: script,
                args: Vec::new(),
                cwd: install.path.clone(),
                env: Vec::new(),
            })
        }
        (Platform::Windows, Runner::Umu { proton, prefix }) => {
            if !proton.join("proton").is_file() {
                return Err(Error::NotFound(format!(
                    "no `proton` script in {}",
                    proton.display()
                )));
            }
            let umu = find_in_path("umu-run")
                .ok_or_else(|| Error::NotFound("umu-run is not in PATH".into()))?;
            let (exe, args, cwd) = windows_task(install)?;
            let env = vec![
                ("WINEPREFIX".into(), prefix.display().to_string()),
                ("PROTONPATH".into(), proton.display().to_string()),
                ("GAMEID".into(), "umu-0".into()),
                ("STORE".into(), "gog".into()),
                (
                    "STEAM_COMPAT_INSTALL_PATH".into(),
                    install.path.display().to_string(),
                ),
            ];
            Ok(LaunchSpec {
                program: umu,
                args: prepend(exe, args),
                cwd,
                env,
            })
        }
        (Platform::Windows, Runner::Wine { wine, prefix }) => {
            let (exe, args, cwd) = windows_task(install)?;
            let env = vec![("WINEPREFIX".into(), prefix.display().to_string())];
            Ok(LaunchSpec {
                program: wine.clone(),
                args: prepend(exe, args),
                cwd,
                env,
            })
        }
        (platform, runner) => Err(Error::Unsupported(format!(
            "{platform:?} game with runner {runner:?}"
        ))),
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
