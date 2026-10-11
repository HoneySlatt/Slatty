//! Steam games in the library, beside GOG's: read from the Steam client's own files, and played,
//! installed, verified and uninstalled by the client itself (`steam://`). Off until turned on.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use iced::Task;
use slatty_core::library::LibraryGame;
use slatty_core::steam::client::{self, Action};
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{self, Installed, Played};
use slatty_core::store::{self, Store};

use crate::{App, Message};

/// How often the games Steam runs are looked for, while one is asked for or runs (about 2 ms).
pub const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How long a game asked of Steam is looked for: Steam may update it, or ask something, first.
const START_WAIT: Duration = Duration::from_secs(300);

/// Unlocked and total achievements of each game that has any, by game id.
pub type Counts = HashMap<String, (usize, usize)>;

#[derive(Debug, Default)]
pub struct SteamView {
    /// The name of the account signed in to Steam on this computer, when Steam is installed.
    pub found: Option<String>,
    /// Whether Steam's games show (Settings → Steam).
    pub on: bool,
    /// Whether the images the client has not saved come from Steam's image servers.
    pub images: bool,
    /// The Steam id of the account whose games show.
    pub account: Option<u64>,
    pub games: Vec<LibraryGame>,
    /// Their covers, as the client cached them or as downloaded, by game id.
    pub covers: HashMap<String, std::path::PathBuf>,
    /// The games Steam installed, by game id, once its library folders are read.
    pub installed: HashMap<String, Installed>,
    /// How long each game was played and when last, as Steam records it, by game id.
    pub played: HashMap<String, Played>,
    pub achievements: Counts,
    /// The games Steam runs, by game id.
    pub running: BTreeSet<String>,
    /// A game Steam was asked to play, until it runs or `START_WAIT` passed.
    pub starting: Option<(String, Instant)>,
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
    /// Play times read again, with the Steam id of the account they are of.
    Played(Option<(u64, HashMap<String, Played>)>),
    /// Unlocked and total achievements of the games that have any, with the account's Steam id.
    Counted(Option<(u64, Counts)>),
    /// Time to look at which games Steam runs.
    Look,
    /// The games Steam runs, by app id.
    Running(BTreeSet<u32>),
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
                let found = self.steam.found.take();
                self.steam = crate::steam::SteamView {
                    found,
                    images: self.steam.images,
                    ..Default::default()
                };
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
                self.steam.account = Some(library.user.id64);
                self.steam.games = library.games;
                self.steam.covers = library.covers;
                self.steam.played = library.played;
                let apps = self
                    .steam
                    .games
                    .iter()
                    .filter_map(|g| store::steam_app(&g.id))
                    .collect();
                self.merge_steam();
                return Task::batch([
                    read_installed(),
                    self.download_covers(),
                    count_achievements(apps),
                ]);
            }
            SteamMsg::Loaded(_) => {}
            SteamMsg::Cover(id, Some(path)) if self.steam.on => {
                self.covers
                    .insert(id.clone(), iced::widget::image::Handle::from_path(&path));
                self.steam.covers.insert(id, path);
            }
            SteamMsg::Cover(..) => {}
            SteamMsg::Installed(games) if self.steam.on => {
                self.steam.installed = games
                    .into_iter()
                    .map(|g| (store::steam_id(g.app_id), g))
                    .collect();
            }
            SteamMsg::Installed(_) => {}
            // Another account signed in to Steam meanwhile: its own games show.
            SteamMsg::Played(Some((account, _)))
                if self.steam.on && self.steam.account != Some(account) =>
            {
                return load(self.steam.images);
            }
            SteamMsg::Played(Some((_, played))) if self.steam.on => self.steam.played = played,
            SteamMsg::Played(_) => {}
            SteamMsg::Counted(Some((account, counts))) if self.steam.account == Some(account) => {
                self.steam.achievements = counts;
            }
            SteamMsg::Counted(_) => {}
            SteamMsg::Look if self.steam.on => return look(),
            SteamMsg::Look => {}
            SteamMsg::Running(apps) => {
                let now: BTreeSet<String> = apps.into_iter().map(store::steam_id).collect();
                let ended = self.steam.running.iter().any(|id| !now.contains(id));
                self.steam.running = now;
                if self.steam.starting.as_ref().is_some_and(|(id, asked)| {
                    self.steam.running.contains(id) || asked.elapsed() > START_WAIT
                }) {
                    self.steam.starting = None;
                }
                // Steam records the session as the game ends; the window may have the focus back
                // later, which reads it again too.
                if ended {
                    return read_played();
                }
            }
            // Steam's own window takes over; what it installs or removes shows once the launcher's
            // window has the focus again (`NotifyMsg::Focus`).
            SteamMsg::Ask(game_id, action) => {
                if let Some(app) = store::steam_app(&game_id) {
                    if action == Action::Play {
                        self.steam.starting = Some((game_id, Instant::now()));
                    }
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

    /// When a game was last played: as Steam records it for a Steam game, else from the sessions
    /// SlattyLauncher followed (GOG does not tell).
    pub fn last_played(&self, game_id: &str) -> Option<i64> {
        if store::steam_app(game_id).is_some() {
            return self
                .steam
                .played
                .get(game_id)
                .map(|p| p.last_played)
                .filter(|t| *t > 0);
        }
        self.playtime.get(game_id).map(|p| p.last_played)
    }

    /// Unlocked and total achievements of a game: GOG's, or Steam's for a Steam game.
    pub fn achievement_counts(&self, game_id: &str) -> Option<(usize, usize)> {
        if store::steam_app(game_id).is_some() {
            return self.steam.achievements.get(game_id).copied();
        }
        self.overview.get(game_id)?.achievements
    }

    /// Whether a game's achievements can be unlocked or cleared by hand: once turned on, and never
    /// a Steam game's.
    pub fn manual_changes(&self, game_id: &str) -> bool {
        self.manual_achievements && store::steam_app(game_id).is_none()
    }

    /// Whether the games Steam runs are to be looked at: one was asked for, or runs.
    pub fn watching_steam(&self) -> bool {
        self.steam.on && (self.steam.starting.is_some() || !self.steam.running.is_empty())
    }
}

/// What may have changed while the window was in the background: the games installed, play
/// times and the account (Steam's own window, or a game played from it), the games running.
pub fn refresh() -> Task<Message> {
    Task::batch([read_installed(), read_played(), look()])
}

/// Reads the play times of the account signed in to Steam.
fn read_played() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                slatty_core::steam::library::played(&local::find(&home)?)
            })
            .await
            .ok()
            .flatten()
        },
        |played| Message::Steam(SteamMsg::Played(played)),
    )
}

/// Counts the achievements of `apps`, read from the Steam client's files (some tens of ms for a
/// few hundred games): after the library shows.
fn count_achievements(apps: Vec<u32>) -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                let steam = local::find(&home)?;
                let user = local::signed_in_user(&steam)?;
                let counts = slatty_core::steam::stats::counts(&steam, &user, &apps)
                    .into_iter()
                    .map(|(app, counted)| (store::steam_id(app), counted))
                    .collect();
                Some((user.id64, counts))
            })
            .await
            .ok()
            .flatten()
        },
        |counted| Message::Steam(SteamMsg::Counted(counted)),
    )
}

/// The achievements of a Steam game, read from the Steam client's files; with `icons`, named on
/// Steam's image servers.
pub fn load_achievements(game_id: String, icons: bool) -> Task<Message> {
    let id = game_id.clone();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let app = store::steam_app(&id)?;
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                let steam = local::find(&home)?;
                let user = local::signed_in_user(&steam)?;
                slatty_core::steam::stats::achievements(&steam, &user, app, icons)
            })
            .await
            .ok()
            .flatten()
            .ok_or_else(|| {
                "Steam keeps no achievements of this game on this computer: it saves them once \
                 the game's page was shown in Steam, or the game played."
                    .to_string()
            })
        },
        move |list| Message::Achievements(game_id, list),
    )
}

/// Looks at which games Steam runs.
fn look() -> Task<Message> {
    Task::perform(
        async {
            tokio::task::spawn_blocking(|| {
                slatty_core::steam::running::running(std::path::Path::new("/proc"))
            })
            .await
            .unwrap_or_default()
        },
        |apps| Message::Steam(SteamMsg::Running(apps)),
    )
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
