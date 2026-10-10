//! Which store a game comes from, read from its id.

use crate::error::{Error, Result};

/// Where a game comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Gog,
    Steam,
}

/// Steam games are known as `steam-<app id>`. GOG's product ids are kept as they are, so ids saved
/// before Steam support still name the same game.
const STEAM_PREFIX: &str = "steam-";

/// The store of a game id, once the id is known to be safe in a file name: letters and digits for
/// GOG, `steam-` and digits for Steam.
pub fn of(game_id: &str) -> Result<Store> {
    let store = match game_id.strip_prefix(STEAM_PREFIX) {
        Some(app) if !app.is_empty() && app.bytes().all(|b| b.is_ascii_digit()) => {
            Some(Store::Steam)
        }
        None if !game_id.is_empty() && game_id.bytes().all(|b| b.is_ascii_alphanumeric()) => {
            Some(Store::Gog)
        }
        _ => None,
    };
    store.ok_or_else(|| Error::Refused(format!("unexpected game id `{game_id}`")))
}

/// Refuses anything but a GOG game: SlattyLauncher installs only those, Steam installs its own.
pub fn require_gog(game_id: &str) -> Result<()> {
    match of(game_id)? {
        Store::Gog => Ok(()),
        Store::Steam => Err(Error::Refused(format!(
            "{game_id} is a Steam game, installed and changed by Steam only"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_name_their_store() {
        assert_eq!(of("1456487183").unwrap(), Store::Gog);
        assert_eq!(of("g").unwrap(), Store::Gog);
        assert_eq!(of("steam-440").unwrap(), Store::Steam);
    }

    #[test]
    fn ids_unsafe_in_a_file_name_are_refused() {
        for id in [
            "",
            "..",
            "../1",
            "1/2",
            "steam-",
            "steam-../1",
            "steam-44a",
            "steam-4 4",
            "1.2",
            "steam:440",
            "-1",
            "é",
        ] {
            assert!(of(id).is_err(), "{id:?}");
        }
    }

    #[test]
    fn only_gog_games_are_installed_by_slattylauncher() {
        assert!(require_gog("1456487183").is_ok());
        assert!(matches!(require_gog("steam-440"), Err(Error::Refused(_))));
        assert!(require_gog("../1").is_err());
    }
}
