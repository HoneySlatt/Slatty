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
    /// Whether the images the client has not saved come from Steam's image servers.
    pub images: bool,
    pub games: Vec<LibraryGame>,
    /// Their covers, as the client cached them or as downloaded, by game id.
    pub covers: HashMap<String, std::path::PathBuf>,
    /// The games Steam installed, by game id, once its library folders are read.
    pub installed: HashMap<String, Installed>,
}

#[derive(Debug, Clone)]
pub enum SteamMsg {
    Switch(bool),
    /// Whether images the client has not saved come from Steam's image servers.
    Images(bool),
    Loaded(Option<SteamLibrary>),
    /// A cover downloaded from Steam's image servers.
    Cover(String, Option<std::path::PathBuf>),
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
                    return load(self.steam.images);
                }
                self.steam.games.clear();
                self.steam.covers.clear();
                self.steam.installed.clear();
                self.merge_steam();
            }
            SteamMsg::Images(on) => {
                self.steam.images = on;
                if let Some(core) = &self.core
                    && let Err(e) = slatty_core::settings::set_steam_images(&core.db, on)
                {
                    self.notify_error(e.to_string());
                }
                // Read again: the images to download, or none.
                if self.steam.on {
                    return load(on);
                }
            }
            SteamMsg::Loaded(Some(library)) if self.steam.on => {
                self.steam.found = Some(library.user.name.clone());
                self.steam.games = library.games;
                self.steam.covers = library.covers;
                self.merge_steam();
                return Task::batch([read_installed(), self.download_covers()]);
            }
            SteamMsg::Loaded(_) => {}
            SteamMsg::Cover(id, Some(path)) if self.steam.on => {
                self.covers
                    .insert(id.clone(), iced::widget::image::Handle::from_path(&path));
                self.steam.covers.insert(id, path);
            }
            SteamMsg::Cover(..) => {}
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

    /// Downloads from Steam's image servers the covers the client has not saved: only named once
    /// that is turned on (`settings::steam_images`).
    fn download_covers(&self) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        Task::batch(
            self.steam
                .games
                .iter()
                .filter(|g| !self.steam.covers.contains_key(&g.id))
                .filter_map(|g| Some((g.id.clone(), g.cover.clone()?)))
                .map(|(id, url)| {
                    let core = core.clone();
                    Task::perform(
                        async move {
                            let _permit = core.downloads.acquire().await.ok()?;
                            slatty_core::steam::library::image(&core.http, &core.dirs, &url)
                                .await
                                .ok()
                        },
                        move |path| Message::Steam(SteamMsg::Cover(id, path)),
                    )
                }),
        )
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

/// Reads the library of the account signed in to Steam, off the interface thread; with
/// `from_servers`, the images its client has not saved are named on Steam's image servers.
pub fn load(from_servers: bool) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                slatty_core::steam::library::read(&local::find(&home)?, from_servers)
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
