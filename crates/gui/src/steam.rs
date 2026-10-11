//! Steam games in the library, beside GOG's: read from the Steam client's own files, and played,
//! installed, verified and uninstalled by the client itself (`steam://`). Off until turned on.

use std::collections::HashMap;

use iced::Task;
use slatty_core::library::LibraryGame;
use slatty_core::steam::client::{self, Action};
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{self, Installed};
use slatty_core::store::{self, Store};

use crate::{App, Message};

#[derive(Debug, Default)]
pub struct SteamView {
    /// The name of the account signed in to Steam on this computer, when Steam is installed.
    pub found: Option<String>,
    /// Whether Steam's games show (Settings → Steam).
    pub on: bool,
    pub games: Vec<LibraryGame>,
    /// Their covers, as the client cached them, by game id.
    pub covers: HashMap<String, std::path::PathBuf>,
    /// The games Steam installed, by game id, once its library folders are read.
    pub installed: HashMap<String, Installed>,
}

#[derive(Debug, Clone)]
pub enum SteamMsg {
    Switch(bool),
    Loaded(Option<SteamLibrary>),
    /// The games installed in Steam's library folders.
    Installed(Vec<Installed>),
    /// Asks the Steam client to act on a game.
    Ask(String, Action),
    /// The desktop could not hand the request to Steam.
    Asked(Result<(), String>),
}

impl App {
    pub fn update_steam(&mut self, msg: SteamMsg) -> Task<Message> {
        match msg {
            SteamMsg::Switch(on) => {
                self.steam.on = on;
                self.login_open = false;
                if let Some(core) = &self.core
                    && let Err(e) = slatty_core::settings::set_steam_games(&core.db, on)
                {
                    self.notify_error(e.to_string());
                }
                if on {
                    return load();
                }
                self.steam.games.clear();
                self.steam.covers.clear();
                self.steam.installed.clear();
                self.merge_steam();
            }
            SteamMsg::Loaded(Some(library)) if self.steam.on => {
                self.steam.found = Some(library.user.name.clone());
                self.steam.games = library.games;
                self.steam.covers = library.covers;
                self.merge_steam();
                return read_installed();
            }
            SteamMsg::Loaded(_) => {}
            SteamMsg::Installed(games) => {
                self.steam.installed = games
                    .into_iter()
                    .map(|g| (store::steam_id(g.app_id), g))
                    .collect();
            }
            // Steam's own window takes over; what it installs or removes shows once the launcher's
            // window has the focus again (`NotifyMsg::Focus`).
            SteamMsg::Ask(game_id, action) => {
                if let Some(app) = store::steam_app(&game_id) {
                    return Task::perform(
                        async move { client::ask(action, app).map_err(|e| e.to_string()) },
                        |r| Message::Steam(SteamMsg::Asked(r)),
                    );
                }
            }
            SteamMsg::Asked(Ok(())) => {}
            SteamMsg::Asked(Err(e)) => self.notify_error(e),
        }
        Task::none()
    }

    /// Puts Steam's games in the library beside GOG's, with their covers, or takes them out once
    /// turned off.
    pub fn merge_steam(&mut self) {
        self.library
            .retain(|g| store::of(&g.id).is_ok_and(|s| s == Store::Gog));
        self.covers.retain(|id, _| store::steam_app(id).is_none());
        if self.steam.on {
            self.library.extend(self.steam.games.iter().cloned());
            for (id, cover) in &self.steam.covers {
                self.covers
                    .insert(id.clone(), iced::widget::image::Handle::from_path(cover));
            }
        }
        self.apply_customs();
    }

    /// The Steam app id of a game of the library, for a Steam game.
    pub fn steam_game(&self, game_id: &str) -> Option<u32> {
        store::steam_app(game_id)
    }

    /// Whether a game is installed, by SlattyLauncher or by Steam.
    pub fn is_installed(&self, game_id: &str) -> bool {
        self.installs.contains_key(game_id) || self.steam.installed.contains_key(game_id)
    }
}

/// Reads the library of the account signed in to Steam, off the interface thread.
pub fn load() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                slatty_core::steam::library::read(&local::find(&home)?)
            })
            .await
            .ok()
            .flatten()
        },
        |library| Message::Steam(SteamMsg::Loaded(library)),
    )
}

/// Reads the games installed in Steam's library folders, which can be network drives.
pub fn read_installed() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                Some(local::installed(&local::find(&home)?))
            })
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
        },
        |games| Message::Steam(SteamMsg::Installed(games)),
    )
}
