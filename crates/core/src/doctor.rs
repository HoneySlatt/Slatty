use std::net::TcpListener;
use std::path::{Path, PathBuf};

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

/// Whether bubblewrap, which every isolated launch runs in, is there and can hold one.
fn bubblewrap(bwrap: Option<PathBuf>) -> Check {
    let name = "bubblewrap";
    let Some(bwrap) = bwrap else {
        return Check {
            name,
            ok: false,
            detail: "`bwrap` not found in PATH: isolated games cannot start".into(),
        };
    };
    match sandbox_problem(&bwrap) {
        None => Check {
            name,
            ok: true,
            detail: bwrap.display().to_string(),
        },
        Some(problem) => Check {
            name,
            ok: false,
            detail: format!("{}: {problem}", bwrap.display()),
        },
    }
}

/// Why `bwrap` cannot hold an isolated game here, if it cannot: it must start a sandbox, and
/// another inside it, as umu's own container starts inside SlattyLauncher's. About 5 ms.
pub fn sandbox_problem(bwrap: &Path) -> Option<String> {
    let restricted =
        std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
            .is_ok_and(|v| v.trim() == "1");
    sandbox_problem_with(bwrap, restricted)
}

fn sandbox_problem_with(bwrap: &Path, apparmor_restricted: bool) -> Option<String> {
    let starts = |inner: &[&std::ffi::OsStr]| {
        std::process::Command::new(bwrap)
            .args(["--dev-bind", "/", "/"])
            .args(inner)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let see = "see \"Ubuntu and AppArmor\" in the user guide's Troubleshooting";
    if !starts(&["true".as_ref()]) {
        return Some(if apparmor_restricted {
            format!(
                "bubblewrap cannot start a sandbox: this system (Ubuntu 23.10 and later) lets only \
                 programs with an AppArmor profile start one, and bubblewrap has none; umu cannot \
                 start Windows games without it either, isolated or not ({see})"
            )
        } else {
            "bubblewrap cannot start a sandbox (are user namespaces turned off?)".into()
        });
    }
    let inner = ["--dev-bind", "/", "/", "true"].map(std::ffi::OsStr::new);
    if !starts(&[&[bwrap.as_os_str()], &inner[..]].concat()) {
        return Some(format!(
            "bubblewrap cannot start a sandbox inside another, which an isolated game needs (umu's \
             container starts inside SlattyLauncher's); an AppArmor profile, such as Ubuntu's \
             bwrap-userns-restrict, can forbid it ({see})"
        ));
    }
    None
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

    /// Ubuntu 23.10 and later let only programs with an AppArmor profile start a sandbox; Ubuntu's
    /// profile for bubblewrap may still forbid one inside another, which an isolated game needs.
    /// Each case says why, rather than the game failing as it starts.
    #[test]
    fn a_sandbox_that_cannot_start_says_why() {
        let dir = std::env::temp_dir().join(format!("slatty-bwrap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Starts what follows `--dev-bind / /`, unless already inside itself.
        let nested_refused = dir.join("bwrap");
        std::fs::write(
            &nested_refused,
            "#!/bin/sh\n[ -n \"$FAKE_BWRAP_IN\" ] && exit 1\nshift 3\nFAKE_BWRAP_IN=1 exec \"$@\"\n",
        )
        .unwrap();
        std::fs::set_permissions(
            &nested_refused,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let ok = find_in_path("true").unwrap();
        let refused = find_in_path("false").unwrap();

        assert_eq!(sandbox_problem_with(&ok, true), None);
        assert!(
            sandbox_problem_with(&refused, true)
                .unwrap()
                .contains("AppArmor profile")
        );
        assert!(
            !sandbox_problem_with(&refused, false)
                .unwrap()
                .contains("AppArmor")
        );
        assert!(
            sandbox_problem_with(&nested_refused, false)
                .unwrap()
                .contains("inside another")
        );
        assert!(!bubblewrap(Some(nested_refused)).ok);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
