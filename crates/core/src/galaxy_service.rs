//! Comet's dummy `GalaxyCommunication` Windows service. Some Galaxy SDK versions refuse to talk
//! to Comet unless this service is registered in the prefix, as GOG Galaxy would register it.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::install::Install;
use crate::paths::Dirs;
use crate::runner;
use crate::session::SessionHandle;

pub const ENV: &str = "SLATTY_GALAXY_COMMUNICATION";
const SERVICE: &str = "GalaxyCommunication";
const WINDOWS_PATH: &str = r"C:\ProgramData\GOG.com\Galaxy\redists\GalaxyCommunication.exe";
const PREFIX_PATH: &str = "drive_c/ProgramData/GOG.com/Galaxy/redists/GalaxyCommunication.exe";

/// The service executable: `$SLATTY_GALAXY_COMMUNICATION`, else `<data dir>/GalaxyCommunication.exe`.
pub fn find(dirs: &Dirs) -> Option<PathBuf> {
    std::env::var_os(ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .into_iter()
        .chain([dirs.data.join("GalaxyCommunication.exe")])
        .find(|p| p.is_file())
}

/// Whether the service is installed in `prefix`. The executable is copied last, so its presence
/// means every step succeeded.
pub fn installed(prefix: &Path) -> bool {
    prefix.join(PREFIX_PATH).is_file()
}

/// Windows commands registering the service, as Heroic does.
pub fn commands() -> Vec<Vec<String>> {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
    vec![
        args(&["sc", "create", SERVICE, &format!("binpath={WINDOWS_PATH}")]),
        args(&[
            "reg",
            "add",
            r"HKLM\SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths",
            "/v",
            "client",
            "/t",
            "REG_SZ",
            "/d",
            r"C:\Program Files\GOG Galaxy",
            "/f",
        ]),
    ]
}

/// Registers the service in the game's prefix unless it is already there.
/// Returns false when nothing had to be done.
pub async fn ensure(dirs: &Dirs, install: &Install, exe: &Path, supervisor: &Path) -> Result<bool> {
    let prefix = install
        .runner
        .prefix()
        .ok_or_else(|| Error::Unsupported("the Galaxy service needs a Wine prefix".into()))?;
    if installed(prefix) {
        return Ok(false);
    }
    let log = dirs
        .logs()
        .join(format!("galaxy-service-{}.log", install.game_id));
    for args in commands() {
        let label = args[..2].join(" ");
        let spec = runner::windows_command(install, args, install.path.clone())?;
        let busy = crate::lock::session(dirs, &install.game_id);
        let outcome = SessionHandle::start(supervisor, &spec, &log, Some(&busy))
            .await?
            .wait()
            .await?;
        if outcome.main_code != Some(0) && !already_registered(&label, outcome.main_code) {
            return Err(Error::Refused(format!(
                "`{label}` exited with code {:?}; see {}",
                outcome.main_code,
                log.display()
            )));
        }
    }
    let dest = prefix.join(PREFIX_PATH);
    let dir = dest.parent().expect("constant path has a parent");
    crate::paths::ensure_dir(dir)?;
    let partial = dir.join("GalaxyCommunication.exe.slatty-partial");
    // The source may be read-only (Nix store) and fs::copy carries the mode over.
    let _ = std::fs::remove_file(&partial);
    std::fs::copy(exe, &partial)
        .and_then(|_| std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o644)))
        .and_then(|_| std::fs::rename(&partial, &dest))
        .map_err(|e| Error::io(format!("copy {} to {}", exe.display(), dest.display()), e))?;
    Ok(true)
}

/// `sc create` fails with ERROR_SERVICE_EXISTS (1073) when an earlier attempt stopped after it;
/// the exit status only keeps its low byte.
fn already_registered(label: &str, code: Option<i32>) -> bool {
    label == "sc create" && code == Some(1073 & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_the_path_the_executable_is_copied_to() {
        let commands = commands();
        assert_eq!(
            commands[0],
            [
                "sc",
                "create",
                "GalaxyCommunication",
                r"binpath=C:\ProgramData\GOG.com\Galaxy\redists\GalaxyCommunication.exe"
            ]
        );
        assert_eq!(
            format!("drive_c/{}", WINDOWS_PATH[3..].replace('\\', "/")),
            PREFIX_PATH
        );
    }

    #[test]
    fn only_an_existing_service_is_tolerated() {
        assert!(already_registered("sc create", Some(49)));
        assert!(!already_registered("sc create", Some(1)));
        assert!(!already_registered("reg add", Some(49)));
    }
}
