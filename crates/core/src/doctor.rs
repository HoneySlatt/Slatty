use std::net::TcpListener;
use std::path::PathBuf;

use crate::paths::Dirs;

#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    find_in(&std::env::var_os("PATH")?, program)
}

/// `program` in the folders `path` lists. Only absolute folders count: an empty or relative entry
/// names the current folder, which is a game's own when the session supervisor starts it.
fn find_in(path: &std::ffi::OsStr, program: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// Whether bubblewrap, which every isolated launch runs in, is there and can start a sandbox.
fn bubblewrap(bwrap: Option<PathBuf>) -> Check {
    let name = "bubblewrap";
    let Some(bwrap) = bwrap else {
        return Check {
            name,
            ok: false,
            detail: "`bwrap` not found in PATH: isolated games cannot start".into(),
        };
    };
    let started = std::process::Command::new(&bwrap)
        .args(["--dev-bind", "/", "/", "true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    Check {
        name,
        ok: started,
        detail: if started {
            bwrap.display().to_string()
        } else {
            format!(
                "{} cannot start a sandbox (user namespaces not allowed?): isolated games cannot \
                 start",
                bwrap.display()
            )
        },
    }
}

pub fn comet_port_free() -> bool {
    TcpListener::bind(("127.0.0.1", crate::comet::PORT)).is_ok()
}

pub fn run(dirs: &Dirs) -> Vec<Check> {
    let tool = |name: &'static str, program: &str| match find_in_path(program) {
        Some(p) => Check {
            name,
            ok: true,
            detail: p.display().to_string(),
        },
        None => Check {
            name,
            ok: false,
            detail: format!("`{program}` not found in PATH"),
        },
    };
    let keyring = match keyring::Entry::store_status() {
        Ok(()) => Check {
            name: "secret storage",
            ok: true,
            detail: "system keyring available".into(),
        },
        Err(e) => Check {
            name: "secret storage",
            ok: false,
            detail: e.to_string(),
        },
    };
    let port = Check {
        name: "comet port",
        ok: comet_port_free(),
        detail: format!(
            "127.0.0.1:{} must be free (another Comet, e.g. Heroic's, blocks it)",
            crate::comet::PORT
        ),
    };
    let runtime = Check {
        name: "runtime dir",
        ok: dirs.runtime.is_some(),
        detail: dirs
            .runtime
            .as_ref()
            .map_or("XDG_RUNTIME_DIR is not set".into(), |p| {
                p.display().to_string()
            }),
    };
    let service = match crate::galaxy_service::find(dirs) {
        Some(p) => Check {
            name: "galaxy service",
            ok: true,
            detail: p.display().to_string(),
        },
        None => Check {
            name: "galaxy service",
            ok: false,
            detail: format!(
                "GalaxyCommunication.exe not found: set {} or copy it to {}",
                crate::galaxy_service::ENV,
                dirs.data.display()
            ),
        },
    };
    vec![
        tool("umu-launcher", "umu-run"),
        bubblewrap(find_in_path("bwrap")),
        tool("comet", "comet"),
        service,
        keyring,
        port,
        runtime,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests run from the crate's folder, which holds Cargo.toml: found through an empty or `.`
    /// entry, it would be found in a game's folder the same way.
    #[test]
    fn programs_are_found_in_absolute_folders_only() {
        for relative in ["", ".", ":.", "./"] {
            assert_eq!(
                find_in(relative.as_ref(), "Cargo.toml"),
                None,
                "{relative:?}"
            );
        }
        assert!(find_in("/nonexistent:/bin".as_ref(), "sh").is_some_and(|p| p.is_absolute()));
    }

    /// `true` and `false` stand in for a bubblewrap that starts a sandbox and one that cannot (no
    /// user namespaces allowed, as some systems keep them).
    #[test]
    fn bubblewrap_is_found_and_tried() {
        assert!(!bubblewrap(None).ok);
        assert!(bubblewrap(find_in_path("true")).ok);
        assert!(!bubblewrap(find_in_path("false")).ok);
    }
}
