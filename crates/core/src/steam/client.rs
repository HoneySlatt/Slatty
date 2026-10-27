//! What is asked of the Steam client: installing, launching, verifying and uninstalling a game,
//! through its `steam://` URLs. The client does the work; Slatty never touches the files.

use crate::error::{Error, Result};

/// What the client is asked to do with a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Play,
    Install,
    Verify,
    Uninstall,
}

/// The `steam://` URL asking the client for `action` on `app_id`.
pub fn url(action: Action, app_id: u32) -> String {
    let verb = match action {
        Action::Play => "rungameid",
        Action::Install => "install",
        Action::Verify => "validate",
        Action::Uninstall => "uninstall",
    };
    format!("steam://{verb}/{app_id}")
}

/// Hands `action` on `app_id` to the Steam client. The client asks itself for what it needs: a
/// sign-in, a confirmation.
pub fn ask(action: Action, app_id: u32) -> Result<()> {
    let url = url(action, app_id);
    match crate::doctor::find_in_path("steam") {
        Some(steam) => crate::auth::spawn_reaped(&mut command(&steam, &url))
            .map(drop)
            .map_err(|e| Error::io("start the Steam client", e)),
        // Flatpak Steam, or another one only the desktop knows: it opens the client's window.
        None => crate::auth::open_with_desktop(url.as_ref())
            .map_err(|e| Error::io("open the Steam client (xdg-open)", e)),
    }
}

/// `steam` asked for `url`, starting in the tray when the client is not running yet (`-silent`)
/// rather than with its library window, as the desktop starts it; a running client takes the
/// address as it is.
fn command(steam: &std::path::Path, url: &str) -> std::process::Command {
    let mut command = std::process::Command::new(steam);
    command
        .args(["-silent", url])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_is_the_clients_own_url() {
        assert_eq!(url(Action::Play, 440), "steam://rungameid/440");
        assert_eq!(url(Action::Install, 440), "steam://install/440");
        assert_eq!(url(Action::Verify, 440), "steam://validate/440");
        assert_eq!(url(Action::Uninstall, 440), "steam://uninstall/440");
    }

    #[test]
    fn steam_starts_in_the_tray() {
        let command = command("/fake/bin/steam".as_ref(), "steam://rungameid/440");
        assert_eq!(command.get_program(), "/fake/bin/steam");
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(args, ["-silent", "steam://rungameid/440"]);
    }
}
