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
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
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
        tool("comet", "comet"),
        service,
        keyring,
        port,
        runtime,
    ]
}
