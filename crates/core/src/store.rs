//! Which store a game comes from, read from its id. GOG's product ids are kept as they are, so ids
//! saved before Steam support still name the same game; Steam games are `steam-<app id>`.
//! `paths::check_game_id` accepts GOG's alone: what installs, records or deletes a game's files
//! uses it, so nothing SlattyLauncher writes or deletes can be a Steam game's, which Steam owns.

use crate::error::{Error, Result};

/// Where a game comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Gog,
    Steam,
}

const STEAM_PREFIX: &str = "steam-";

/// The store of a game id, once the id is known to be safe in a file name: letters and digits for
/// GOG, `steam-` and digits for Steam.
pub fn of(game_id: &str) -> Result<Store> {
    let store = match game_id.strip_prefix(STEAM_PREFIX) {
        Some(app) if !app.is_empty() && app.bytes().all(|b| b.is_ascii_digit()) => {
            Some(Store::Steam)
        }
        None if crate::paths::check_game_id(game_id).is_ok() => Some(Store::Gog),
        _ => None,
    };
    store.ok_or_else(|| Error::Refused(format!("unexpected game id `{game_id}`")))
}

/// The id of a Steam app.
pub fn steam_id(app_id: u32) -> String {
    format!("{STEAM_PREFIX}{app_id}")
}

/// The Steam app id of a game id, for a Steam game.
pub fn steam_app(game_id: &str) -> Option<u32> {
    game_id.strip_prefix(STEAM_PREFIX)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_name_their_store() {
        assert_eq!(of("1456487183").unwrap(), Store::Gog);
        assert_eq!(of("steam-440").unwrap(), Store::Steam);
        assert_eq!(steam_id(440), "steam-440");
        assert_eq!(steam_app("steam-440"), Some(440));
        assert_eq!(steam_app("1456487183"), None);
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

    /// What installs, records or deletes a game's files accepts GOG's ids alone.
    #[test]
    fn steam_games_never_enter_what_slattylauncher_writes() {
        assert!(crate::paths::check_game_id("1456487183").is_ok());
        assert!(crate::paths::check_game_id("steam-440").is_err());
    }
}
