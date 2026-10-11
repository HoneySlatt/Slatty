//! Steam games in the library, beside GOG's: read from the Steam client's own files, and played,
//! installed, verified and uninstalled by the client itself (`steam://`). Off until turned on.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

use iced::Task;
use slatty_core::library::LibraryGame;
use slatty_core::steam::client::{self, Action};
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{self, Installed, Played};
use slatty_core::steam::web::{self, OwnedGame};
use slatty_core::store::{self, Store};

use crate::notify::Kind;
use crate::{App, Message};

/// How often the games Steam runs are looked for, while one is asked for or runs (about 2 ms).
pub const LOOK_EVERY: Duration = Duration::from_secs(2);
/// How long a game asked of Steam is looked for: Steam may update it, or ask something, first.
const START_WAIT: Duration = Duration::from_secs(300);

/// How often the manifest of a game Steam downloads is read, while one does.
pub const DOWNLOAD_EVERY: Duration = Duration::from_secs(5);
/// Downloads whose manifests stayed the same this long are read less often (Steam closed, or
/// waiting for another one), and no longer while the window is in the background.
const STILL_FOR: Duration = Duration::from_secs(120);
const STALLED_EVERY: Duration = Duration::from_secs(30);
/// A download told done is not told again this soon: a slow read of every library folder,
/// begun before it ended, can land after one that saw it end.
const TOLD_FOR: Duration = Duration::from_secs(600);

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
    /// Whether they may be downloaded now (Steam's games and their images on), read by downloads
    /// waiting their turn: turned off meanwhile, they give up. The same for the Web API.
    pub images_allowed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub web_allowed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Whether Steam's Web API fills in what the client has not saved, whether its key is in the
    /// keyring (once asked), the key being typed, and the games it added.
    pub web: bool,
    pub web_key: Option<bool>,
    pub web_key_input: String,
    pub from_web: std::collections::HashSet<String>,
    /// The Steam id of the account whose games show.
    pub account: Option<u64>,
    /// Why the client's list of apps could not be read, which leaves no game to show.
    pub unreadable: Option<String>,
    pub games: Vec<LibraryGame>,
    /// Their covers, as the client cached them or as downloaded, by game id.
    pub covers: HashMap<String, std::path::PathBuf>,
    /// The games Steam installed, by game id, once its library folders are read.
    pub installed: HashMap<String, Installed>,
    /// The Steam game each GOG game of the library also is, by GOG id: it shows as GOG's alone.
    pub twins: HashMap<String, String>,
    /// When what Steam downloads last changed: its manifests are read until it stays the same.
    pub download_moved: Option<Instant>,
    /// When each download Steam made was told done.
    pub told: HashMap<String, Instant>,
    /// Whether the library folders are being read.
    pub reading_installed: bool,
    /// How long each game was played and when last, as Steam records it, by game id.
    pub played: HashMap<String, Played>,
    pub achievements: Counts,
    /// The games Steam runs, by game id.
    pub running: BTreeSet<String>,
    /// A game Steam was asked to play, until it runs or `START_WAIT` passed.
    pub starting: Option<(String, Instant)>,
    /// What Steam wrote so far in the `downloading` folder of each game it downloads, as last read;
    /// and whether those are being read again.
    pub written: HashMap<String, local::Written>,
    pub reading_downloads: bool,
    /// A game Steam was asked to install, until its manifest shows or `START_WAIT` passed: Steam
    /// writes it once its own window was answered, maybe after the launcher's had the focus back.
    pub expecting: Option<(u32, Instant)>,
    /// Whether the library folders are to be read again once the read running ends: asked for
    /// meanwhile (the window's focus back), it would have missed what changed since it began.
    pub read_installed_again: bool,
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
    /// Time to read again the manifests of the games Steam downloads.
    ReadDownloads,
    /// Those manifests, by game id (none for a game Steam removed), and what Steam wrote in the
    /// `downloading` folders it has.
    Downloads(
        HashMap<String, Option<Installed>>,
        HashMap<String, local::Written>,
    ),
    /// Play times read again, with the Steam id of the account they are of.
    Played(Option<(u64, HashMap<String, Played>)>),
    /// Unlocked and total achievements of the games that have any, with the account's Steam id.
    Counted(Option<(u64, Counts)>),
    /// A game's achievements, read for the account of that Steam id.
    Achieved(
        u64,
        String,
        Result<Vec<slatty_core::achievements::Achievement>, String>,
    ),
    /// Time to look at which games Steam runs.
    Look,
    /// The games Steam runs, by app id.
    Running(BTreeSet<u32>),
    /// Whether Steam's Web API fills in what the client has not saved.
    WebApi(bool),
    WebKeyInput(String),
    SaveWebKey,
    RemoveWebKey,
    /// Whether a Web API key is in the keyring, after reading, saving or removing it.
    WebKey(Result<bool, String>),
    OpenWebKeyPage,
    /// The account's games as the Web API lists them, with the account's Steam id.
    Owned(u64, Result<Vec<OwnedGame>, String>),
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
                self.allow_steam_requests();
                self.login_open = false;
                if let Some(core) = &self.core
                    && let Err(e) = slatty_core::settings::set_steam_games(&core.db, on)
                {
                    self.notify_error(e.to_string());
                }
                if on {
                    return load(self.steam.images);
                }
                // What is forgotten is what was read; the switches stay as they are saved.
                let found = self.steam.found.take();
                self.steam = crate::steam::SteamView {
                    found,
                    images: self.steam.images,
                    images_allowed: self.steam.images_allowed.clone(),
                    web: self.steam.web,
                    web_key: self.steam.web_key,
                    web_allowed: self.steam.web_allowed.clone(),
                    ..Default::default()
                };
                self.allow_steam_requests();
                self.merge_steam();
            }
            SteamMsg::Images(on) => {
                self.steam.images = on;
                self.allow_steam_requests();
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
                // Another account signed in to Steam: the achievements read were the other's.
                if self.steam.account.is_some_and(|a| a != library.user.id64) {
                    self.achievements
                        .retain(|id, _| store::steam_app(id).is_none());
                    self.steam.achievements.clear();
                }
                self.steam.found = Some(library.user.name.clone());
                self.steam.account = Some(library.user.id64);
                self.steam.games = library.games;
                // Read while images from Steam's servers were on, and turned off since.
                if !self.steam.images {
                    let local = |url: &mut Option<String>| {
                        url.take_if(|u| slatty_core::steam::library::is_server_image(u));
                    };
                    for g in &mut self.steam.games {
                        local(&mut g.cover);
                        local(&mut g.background);
                        local(&mut g.logo);
                    }
                }
                self.steam.covers = library.covers;
                self.steam.played = library.played;
                self.steam.unreadable = library.unreadable;
                self.steam.from_web.clear();
                let apps = self
                    .steam
                    .games
                    .iter()
                    .filter_map(|g| store::steam_app(&g.id))
                    .collect();
                self.merge_steam();
                return Task::batch([
                    self.read_steam_installed(),
                    self.download_covers(),
                    count_achievements(apps),
                    self.fetch_owned(),
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
                self.steam.reading_installed = false;
                let mut read: HashMap<String, Option<Installed>> = self
                    .steam
                    .installed
                    .keys()
                    .map(|id| (id.clone(), None))
                    .collect();
                read.extend(
                    games
                        .into_iter()
                        .map(|g| (store::steam_id(g.app_id), Some(g))),
                );
                let told = self.steam_installed(read);
                if std::mem::take(&mut self.steam.read_installed_again) {
                    return Task::batch([told, self.read_steam_installed()]);
                }
                return told;
            }
            SteamMsg::Installed(_) => {
                self.steam.reading_installed = false;
                self.steam.read_installed_again = false;
            }
            SteamMsg::ReadDownloads if self.steam.on => {
                // One read at a time: a network drive not answering would have them pile up.
                if std::mem::replace(&mut self.steam.reading_downloads, true) {
                    return Task::none();
                }
                let downloading: Vec<Installed> = self
                    .steam
                    .installed
                    .values()
                    .filter(|g| g.update.is_some_and(|u| u.started && !u.paused))
                    .cloned()
                    .collect();
                // An install asked of Steam whose manifest is not seen yet: that file alone.
                let expected = self
                    .steam
                    .expecting
                    .filter(|(app, _)| !self.steam.installed.contains_key(&store::steam_id(*app)))
                    .map(|(app, _)| app);
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let (mut read, mut written) = (HashMap::new(), HashMap::new());
                            if let Some(app) = expected
                                && let Some(game) = std::env::var_os("HOME")
                                    .and_then(|home| local::find(std::path::Path::new(&home)))
                                    .and_then(|steam| local::find_manifest(&steam, app))
                            {
                                read.insert(store::steam_id(app), Some(game));
                            }
                            for g in downloading {
                                let id = store::steam_id(g.app_id);
                                if let Some(w) = local::written(&g) {
                                    written.insert(id.clone(), w);
                                }
                                read.insert(id, local::read_again(&g));
                            }
                            (read, written)
                        })
                        .await
                        .unwrap_or_default()
                    },
                    |(read, written)| Message::Steam(SteamMsg::Downloads(read, written)),
                );
            }
            SteamMsg::ReadDownloads => {}
            SteamMsg::Downloads(read, written) if self.steam.on => {
                self.steam.reading_downloads = false;
                // Steam's manifest is said to stay as it was while it downloads: what it writes
                // in its `downloading` folder tells that the download goes on.
                if written.iter().any(|(id, now)| {
                    self.steam.written.get(id).is_none_or(|was| {
                        now.apparent > was.apparent || now.allocated > was.allocated
                    })
                }) {
                    self.steam.download_moved = Some(Instant::now());
                }
                self.steam.written = written;
                if self.steam.expecting.is_some_and(|(app, asked)| {
                    read.contains_key(&store::steam_id(app)) || asked.elapsed() > START_WAIT
                }) {
                    self.steam.expecting = None;
                }
                return self.steam_installed(read);
            }
            SteamMsg::Downloads(..) => self.steam.reading_downloads = false,
            // Another account signed in to Steam meanwhile: its own games show.
            SteamMsg::Played(Some((account, _)))
                if self.steam.on && self.steam.account != Some(account) =>
            {
                return load(self.steam.images);
            }
            SteamMsg::Played(Some((_, mut played))) if self.steam.on => {
                // What the Web API gave for games never played here stays.
                for (id, before) in std::mem::take(&mut self.steam.played) {
                    played.entry(id).or_insert(before);
                }
                self.steam.played = played;
            }
            SteamMsg::Played(_) => {}
            SteamMsg::Counted(Some((account, counts))) if self.steam.account == Some(account) => {
                self.steam.achievements = counts;
            }
            SteamMsg::Counted(_) => {}
            SteamMsg::Achieved(account, game_id, list)
                if self.steam.on && self.steam.account == Some(account) =>
            {
                return self.achievements_received(game_id, list);
            }
            // Read for an account no longer shown: read again when asked.
            SteamMsg::Achieved(_, game_id, _) => {
                self.achievements.remove(&game_id);
            }
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
            // window has the focus again (`NotifyMsg::Focus`), and a game it was asked to install
            // as soon as Steam writes its manifest (`SteamView::expecting`).
            SteamMsg::Ask(game_id, action) => {
                if let Some(app) = store::steam_app(&game_id) {
                    if action == Action::Play {
                        self.steam.starting = Some((game_id, Instant::now()));
                    }
                    if action == Action::Install {
                        self.steam.expecting = Some((app, Instant::now()));
                    }
                    return Task::perform(
                        async move { client::ask(action, app).map_err(|e| e.to_string()) },
                        |r| Message::Steam(SteamMsg::Asked(r)),
                    );
                }
            }
            SteamMsg::Asked(Ok(())) => {}
            // Steam was not reached: nothing is starting.
            SteamMsg::Asked(Err(e)) => {
                self.steam.starting = None;
                self.notify_error(e);
            }
            SteamMsg::WebApi(on) => {
                self.steam.web = on;
                self.allow_steam_requests();
                if let Some(core) = &self.core
                    && let Err(e) = slatty_core::settings::set_steam_web_api(&core.db, on)
                {
                    self.notify_error(e.to_string());
                }
                if !on {
                    self.drop_web_games();
                } else if self.steam.web_key.is_none() {
                    return Task::perform(web_key_saved(), |r| Message::Steam(SteamMsg::WebKey(r)));
                } else {
                    return self.fetch_owned();
                }
            }
            SteamMsg::WebKeyInput(v) => self.steam.web_key_input = v,
            SteamMsg::SaveWebKey => {
                let key = std::mem::take(&mut self.steam.web_key_input);
                let key = key.trim();
                if key.is_empty() {
                    return Task::none();
                }
                let key = slatty_core::secret::Secret::new(key);
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            slatty_core::credentials::save_key(web::KEY, &key)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map(|()| true)
                        .map_err(|e| e.to_string())
                    },
                    |r| Message::Steam(SteamMsg::WebKey(r)),
                );
            }
            SteamMsg::RemoveWebKey => {
                return Task::perform(
                    async {
                        tokio::task::spawn_blocking(|| {
                            slatty_core::credentials::delete_key(web::KEY)
                        })
                        .await
                        .map_err(|e| e.to_string())?
                        .map(|()| false)
                        .map_err(|e| e.to_string())
                    },
                    |r| Message::Steam(SteamMsg::WebKey(r)),
                );
            }
            SteamMsg::WebKey(Ok(saved)) => {
                self.steam.web_key = Some(saved);
                self.allow_steam_requests();
                if !saved {
                    self.drop_web_games();
                }
                return self.fetch_owned();
            }
            SteamMsg::WebKey(Err(e)) => self.notify_error(e),
            SteamMsg::OpenWebKeyPage => {
                if slatty_core::auth::open_in_browser(&web::key_page()).is_err() {
                    self.notify_error(format!("Open {} in your browser.", web::KEY_PAGE));
                }
            }
            SteamMsg::Owned(account, Ok(games))
                if self.asks_steam_web() && self.steam.account == Some(account) =>
            {
                return self.add_web_games(games);
            }
            SteamMsg::Owned(_, Err(e)) if self.asks_steam_web() => {
                self.notify_error(format!("Steam's Web API did not answer: {e}"));
            }
            SteamMsg::Owned(..) => {}
        }
        Task::none()
    }

    /// Puts Steam's games in the library beside GOG's, with their covers, or takes them out once
    /// turned off; and those added by hand, from their installs.
    pub fn merge_steam(&mut self) {
        self.library
            .retain(|g| store::of(&g.id).is_ok_and(|s| s == Store::Gog));
        let mut added: Vec<LibraryGame> = self
            .installs
            .values()
            .filter(|i| store::is_local(&i.game_id))
            .map(slatty_core::added::library_game)
            .collect();
        added.sort_by(|a, b| a.id.cmp(&b.id));
        self.library.extend(added);
        self.covers.retain(|id, _| store::steam_app(id).is_none());
        self.steam.twins.clear();
        if self.steam.on {
            // A game owned on both shows once, as GOG's: matched with GOG's own titles, not the
            // ones the user gave.
            let gog = self.library.iter().map(|g| {
                let title = self.gog_titles.get(&g.id).unwrap_or(&g.title);
                (g.id.as_str(), title.as_str(), g.steam.as_deref())
            });
            let twins = slatty_core::steam::library::twins(gog, &self.steam.games);
            let twinned: std::collections::HashSet<&String> = twins.values().collect();
            self.library.extend(
                self.steam
                    .games
                    .iter()
                    .filter(|g| !twinned.contains(&g.id))
                    .cloned(),
            );
            // Those of games shown as their GOG twin too: Steam's downloads of them show it.
            for (id, cover) in &self.steam.covers {
                self.covers
                    .insert(id.clone(), iced::widget::image::Handle::from_path(cover));
            }
            self.steam.twins = twins;
        }
        self.apply_customs();
    }

    /// Whether the Steam games switch can be used: to turn them on where Steam is found, and to
    /// turn them off always (Steam may have been uninstalled since).
    pub fn steam_switch_usable(&self) -> bool {
        self.steam.found.is_some() || self.steam.on
    }

    /// Whether Steam's Web API is to be asked: turned on, with a key saved, for Steam's games.
    pub fn asks_steam_web(&self) -> bool {
        self.steam.on && self.steam.web && self.steam.web_key == Some(true)
    }

    /// Asks the Web API for the account's games, once it is to be asked.
    fn fetch_owned(&self) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.steam.account) else {
            return Task::none();
        };
        if !self.asks_steam_web() {
            return Task::none();
        }
        let allowed = self.steam.web_allowed.clone();
        Task::perform(
            async move {
                let key = web_key().await?;
                if !allowed.load(std::sync::atomic::Ordering::Relaxed) {
                    return Ok(Vec::new());
                }
                web::owned_games(&core.http, &key, account)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |games| Message::Steam(SteamMsg::Owned(account, games)),
        )
    }

    /// Adds the account's games the client has not described yet, and the play time of those
    /// never played on this computer.
    fn add_web_games(&mut self, games: Vec<OwnedGame>) -> Task<Message> {
        let known: std::collections::HashSet<String> =
            self.steam.games.iter().map(|g| g.id.clone()).collect();
        for game in games {
            let shown = slatty_core::steam::library::web_game(&game, self.steam.images);
            if game.minutes > 0 || game.last_played > 0 {
                self.steam.played.entry(shown.id.clone()).or_insert(Played {
                    minutes: game.minutes,
                    last_played: game.last_played,
                });
            }
            if !known.contains(&shown.id) {
                self.steam.from_web.insert(shown.id.clone());
                self.steam.games.push(shown);
            }
        }
        self.merge_steam();
        self.download_covers()
    }

    /// Takes out the games the Web API added.
    fn drop_web_games(&mut self) {
        let from_web = std::mem::take(&mut self.steam.from_web);
        self.steam.games.retain(|g| !from_web.contains(&g.id));
        self.merge_steam();
    }

    /// The Steam game a GOG game of the library also is, owned on both.
    pub fn steam_twin(&self, game_id: &str) -> Option<&str> {
        self.steam.twins.get(game_id).map(String::as_str)
    }

    /// Downloads from Steam's image servers the covers the client has not saved: only named once
    /// that is turned on (`settings::steam_images`).
    fn download_covers(&self) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        if !self.steam_images_allowed() {
            return Task::none();
        }
        Task::batch(
            self.steam
                .games
                .iter()
                .filter(|g| !self.steam.covers.contains_key(&g.id))
                .filter_map(|g| Some((g.id.clone(), g.cover.clone()?)))
                .map(|(id, url)| {
                    let (core, allowed) = (core.clone(), self.steam.images_allowed.clone());
                    Task::perform(
                        async move {
                            let _permit = core.downloads.acquire().await.ok()?;
                            if !allowed.load(std::sync::atomic::Ordering::Relaxed) {
                                return None;
                            }
                            slatty_core::steam::library::image(&core.http, &core.dirs, &url)
                                .await
                                .ok()
                        },
                        move |path| Message::Steam(SteamMsg::Cover(id, path)),
                    )
                }),
        )
    }

    /// Whether images may be downloaded from Steam's image servers now.
    pub fn steam_images_allowed(&self) -> bool {
        self.steam
            .images_allowed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Tells requests waiting their turn, or yet to start, whether Steam's image servers and Web
    /// API may be asked: Steam's games on, and their switches.
    pub fn allow_steam_requests(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        let images = self.steam.on && self.steam.images;
        self.steam.images_allowed.store(images, Relaxed);
        self.steam.web_allowed.store(self.asks_steam_web(), Relaxed);
    }

    /// The Steam app id of a game of the library, for a Steam game.
    pub fn steam_game(&self, game_id: &str) -> Option<u32> {
        store::steam_app(game_id)
    }

    /// Whether a game is installed, by SlattyLauncher or by Steam.
    pub fn is_installed(&self, game_id: &str) -> bool {
        // Steam's whole once installed, updates included; a first install still downloads.
        let on_steam = |id: &str| self.steam.installed.get(id).is_some_and(|g| g.ready);
        self.installs.contains_key(game_id)
            || on_steam(game_id)
            || self.steam_twin(game_id).is_some_and(on_steam)
    }

    /// When a game was last played: as Steam records it for a Steam game, else from the sessions
    /// SlattyLauncher followed (GOG does not tell).
    pub fn last_played(&self, game_id: &str) -> Option<i64> {
        let on_steam = |id: &str| {
            self.steam
                .played
                .get(id)
                .map(|p| p.last_played)
                .filter(|t| *t > 0)
        };
        if store::steam_app(game_id).is_some() {
            return on_steam(game_id);
        }
        // Owned on both: the latest of either.
        let here = self.playtime.get(game_id).map(|p| p.last_played);
        here.max(self.steam_twin(game_id).and_then(on_steam))
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

    /// Puts in what Steam's manifests now say of these games (none: removed), and tells of the
    /// downloads they show done.
    fn steam_installed(&mut self, read: HashMap<String, Option<Installed>>) -> Task<Message> {
        // Steam's own tools (its runtimes, Proton, redistributables) and apps of no game of the
        // account have manifests too: left out, as the library leaves them out, rather than shown
        // and told by their numbers.
        let games: std::collections::HashSet<String> =
            self.steam.games.iter().map(|g| g.id.clone()).collect();
        let mut told = Vec::new();
        for (id, now) in read {
            if !games.contains(&id) {
                self.steam.installed.remove(&id);
                continue;
            }
            let before = self.steam.installed.get(&id).and_then(|g| g.update);
            let after = now.as_ref().and_then(|g| g.update);
            if before != after {
                self.steam.download_moved = Some(Instant::now());
            }
            match now {
                Some(game) => {
                    let done = before.is_some_and(|u| u.started) && after.is_none() && game.ready;
                    let told_lately = self
                        .steam
                        .told
                        .get(&id)
                        .is_some_and(|t| t.elapsed() < TOLD_FOR);
                    if done && !told_lately {
                        self.steam.told.insert(id.clone(), Instant::now());
                        told.push(id.clone());
                    }
                    self.steam.installed.insert(id, game);
                }
                None => {
                    self.steam.installed.remove(&id);
                }
            }
        }
        Task::batch(told.into_iter().map(|id| {
            let body = format!(
                "Steam installed {}: it is ready to play.",
                self.title_of(&id)
            );
            self.notify(Kind::Downloads, Some(&id), "Download complete", body)
        }))
    }

    /// Whether a download Steam began waits for another to end: Steam downloads one game at a time,
    /// in a `downloading` folder of its own, which this one has not while another has. Their
    /// manifests tell them apart in nothing (both "started").
    fn steam_waiting(&self, id: &str) -> bool {
        !self.steam.written.contains_key(id) && !self.steam.written.is_empty()
    }

    /// How far an install or update Steam makes went (done, of), from what it wrote when it can
    /// be told.
    pub fn steam_progress(&self, id: &str, u: &local::Update) -> Option<(u64, u64)> {
        u.progress(self.steam.written.get(id).copied())
    }

    /// Where an install or update Steam makes is, in words.
    pub fn steam_status(&self, id: &str, u: &local::Update) -> String {
        crate::ui::format::steam_download(
            u,
            self.steam_progress(id, u),
            self.steam.written.contains_key(id),
            self.steam_waiting(id),
        )
    }

    /// How often the manifests of the games Steam downloads are read: every few seconds while one
    /// downloads and changed lately; less often once it stays the same while the window is in
    /// front, someone may be watching it; not at all otherwise (Steam closed, or the window in
    /// the background: its focus reads them again).
    pub fn steam_downloads_every(&self) -> Option<Duration> {
        let downloading = self.steam.on
            && self
                .steam
                .installed
                .values()
                .any(|g| g.update.is_some_and(|u| u.started && !u.paused));
        let moved = self
            .steam
            .download_moved
            .is_some_and(|t| t.elapsed() < STILL_FOR);
        // Asked to install a game, Steam writes its manifest once its own window was answered.
        let expecting = self.steam.on
            && self
                .steam
                .expecting
                .is_some_and(|(_, asked)| asked.elapsed() < START_WAIT);
        match (downloading, moved) {
            _ if expecting => Some(DOWNLOAD_EVERY),
            (false, _) => None,
            (true, true) => Some(DOWNLOAD_EVERY),
            (true, false) => (!self.unfocused).then_some(STALLED_EVERY),
        }
    }

    /// Whether the games Steam runs are to be looked at: one was asked for, or runs.
    pub fn watching_steam(&self) -> bool {
        self.steam.on && (self.steam.starting.is_some() || !self.steam.running.is_empty())
    }

    /// What may have changed while the window was in the background: the games installed, play
    /// times and the account (Steam's own window, or a game played from it), the games running.
    pub fn refresh_steam(&mut self) -> Task<Message> {
        Task::batch([self.read_steam_installed(), read_played(), look()])
    }

    /// Reads the games installed in Steam's library folders, or once more after the read still
    /// running, not beside it: a network drive waking up holds it for seconds, and each focus of
    /// the window would add another.
    fn read_steam_installed(&mut self) -> Task<Message> {
        if std::mem::replace(&mut self.steam.reading_installed, true) {
            self.steam.read_installed_again = true;
            return Task::none();
        }
        read_installed()
    }
}

/// Whether a Steam Web API key is in the keyring, read off the interface thread.
pub async fn web_key_saved() -> Result<bool, String> {
    tokio::task::spawn_blocking(|| slatty_core::credentials::load_key(web::KEY))
        .await
        .map_err(|e| e.to_string())?
        .map(|key| key.is_some())
        .map_err(|e| e.to_string())
}

/// The Steam Web API key, from the keyring.
async fn web_key() -> Result<slatty_core::secret::Secret, String> {
    tokio::task::spawn_blocking(|| slatty_core::credentials::load_key(web::KEY))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Add your Steam Web API key in Settings → Steam.".to_string())
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

/// The achievements of a Steam game, read from the Steam client's files, else, with `web` (the
/// HTTP client and the account's Steam id, once the Web API is to be asked), from Steam's Web
/// API; with `icons`, named on Steam's image servers.
pub fn load_achievements(
    game_id: String,
    icons: bool,
    account: u64,
    web: Option<(
        slatty_core::http::HttpClient,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    )>,
) -> Task<Message> {
    let id = game_id.clone();
    Task::perform(
        async move {
            let app = store::steam_app(&id).ok_or("not a Steam game")?;
            let here = tokio::task::spawn_blocking(move || {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                let steam = local::find(&home)?;
                // Another account signed in to Steam meanwhile: not its achievements.
                let user = local::signed_in_user(&steam).filter(|u| u.id64 == account)?;
                slatty_core::steam::stats::achievements(&steam, &user, app, icons)
            })
            .await
            .ok()
            .flatten();
            match (here, web) {
                (Some(list), _) => Ok(list),
                // Turned off while the client's files were read: not asked.
                (None, Some((_, allowed)))
                    if !allowed.load(std::sync::atomic::Ordering::Relaxed) =>
                {
                    Err("Steam's Web API is turned off.".to_string())
                }
                (None, Some((http, _))) => {
                    let key = web_key().await?;
                    web::achievements(&http, &key, account, app, icons)
                        .await
                        .map_err(|e| e.to_string())
                }
                (None, None) => Err(
                    "Steam keeps no achievements of this game on this computer: it saves them \
                     once the game's page was shown in Steam, or the game played. Steam's Web API \
                     (Settings → Steam) can tell them."
                        .to_string(),
                ),
            }
        },
        move |list| Message::Steam(SteamMsg::Achieved(account, game_id, list)),
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
fn read_installed() -> Task<Message> {
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
