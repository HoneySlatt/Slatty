//! Which store a game comes from, read from its id. GOG's product ids are kept as they are, so ids
//! saved before Steam support still name the same game; Steam games are `steam-<app id>`, and
//! games added by hand, from no store, `local-<number>`, and games run in an emulator,
//! `emu-<number>`.
//! `paths::check_game_id` accepts GOG's alone and guards every request to GOG, so GOG never
//! learns of another store's game. `paths::check_owned_id` accepts all but Steam's:
//! what installs, records or deletes a game's files uses it, so nothing Slatty writes or
//! deletes can be a Steam game's, which Steam owns.

use crate::error::{Error, Result};

/// Where a game comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    Gog,
    Steam,
    /// Added by hand: a game from no store, or one Slatty cannot install.
    Local,
    /// Run in an emulator, from a disc image, a file or a folder the user has.
    Emulated,
}

const STEAM_PREFIX: &str = "steam-";
const LOCAL_PREFIX: &str = "local-";
const EMULATED_PREFIX: &str = "emu-";

/// The store of a game id, once the id is known to be safe in a file name: letters and digits for
/// GOG, `steam-` and digits for Steam, `local-` and digits for a game added by hand, `emu-` and
/// digits for an emulated one.
pub fn of(game_id: &str) -> Result<Store> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let store = if let Some(app) = game_id.strip_prefix(STEAM_PREFIX) {
        digits(app).then_some(Store::Steam)
    } else if let Some(n) = game_id.strip_prefix(LOCAL_PREFIX) {
        digits(n).then_some(Store::Local)
    } else if let Some(n) = game_id.strip_prefix(EMULATED_PREFIX) {
        digits(n).then_some(Store::Emulated)
    } else {
        crate::paths::check_game_id(game_id)
            .is_ok()
            .then_some(Store::Gog)
    };
    store.ok_or_else(|| Error::Refused(format!("unexpected game id `{game_id}`")))
}

/// The id of the game added by hand `n`th.
pub fn local_id(n: u64) -> String {
    format!("{LOCAL_PREFIX}{n}")
}

/// Whether a game id is that of a game added by hand.
pub fn is_local(game_id: &str) -> bool {
    of(game_id).is_ok_and(|s| s == Store::Local)
}

/// The id of the game emulated `n`th.
pub fn emulated_id(n: u64) -> String {
    format!("{EMULATED_PREFIX}{n}")
}

/// Whether a game id is that of a game run in an emulator.
pub fn is_emulated(game_id: &str) -> bool {
    of(game_id).is_ok_and(|s| s == Store::Emulated)
}

/// Whether a game is from no store, added by hand or emulated: no store is told of it.
pub fn from_no_store(game_id: &str) -> bool {
    of(game_id).is_ok_and(|s| matches!(s, Store::Local | Store::Emulated))
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
            "local-",
            "local-../1",
            "local-1a",
            "local:1",
            "emu-",
            "emu-../1",
            "emu-1a",
        ] {
            assert!(of(id).is_err(), "{id:?}");
        }
    }

    #[test]
    fn games_added_by_hand_have_ids_of_their_own() {
        assert_eq!(local_id(7), "local-7");
        assert_eq!(of("local-7").unwrap(), Store::Local);
        assert!(is_local("local-7"));
        assert!(!is_local("1456487183") && !is_local("steam-440"));
    }

    #[test]
    fn emulated_games_have_ids_of_their_own() {
        assert_eq!(emulated_id(3), "emu-3");
        assert_eq!(of("emu-3").unwrap(), Store::Emulated);
        assert!(is_emulated("emu-3") && !is_local("emu-3"));
        assert!(from_no_store("emu-3") && from_no_store("local-7"));
        assert!(!from_no_store("1456487183") && !from_no_store("steam-440"));
    }

    /// What installs, records or deletes a game's files accepts GOG's ids and those of games added
    /// by hand, never a Steam game's; what asks GOG accepts GOG's alone.
    #[test]
    fn steam_games_never_enter_what_slatty_writes() {
        use crate::paths::{check_game_id, check_owned_id};
        assert!(check_owned_id("1456487183").is_ok());
        assert!(check_owned_id("local-7").is_ok());
        assert!(check_owned_id("emu-3").is_ok());
        assert!(check_owned_id("steam-440").is_err());
        assert!(check_owned_id("local-../1").is_err());
        assert!(check_game_id("1456487183").is_ok());
        assert!(check_game_id("steam-440").is_err());
        assert!(check_game_id("local-7").is_err(), "never sent to GOG");
        assert!(check_game_id("emu-3").is_err(), "never sent to GOG");
    }
}
