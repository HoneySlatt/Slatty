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

/// Hands `action` on `app_id` to the Steam client, through the desktop (native or Flatpak Steam,
/// whichever registered `steam://`). The client asks itself for what it needs: a sign-in, a
/// confirmation.
pub fn ask(action: Action, app_id: u32) -> Result<()> {
    crate::auth::open_with_desktop(url(action, app_id).as_ref())
        .map_err(|e| Error::io("open the Steam client (xdg-open)", e))
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
}
