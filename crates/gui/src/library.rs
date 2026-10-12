//! The game library: GOG sync, covers and images, favorites, what GOG reads for each game
//! (achievements, cloud saves, play time), and the shelf, filters and order shown.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;

use iced::Task;
use iced::futures::{SinkExt, Stream, StreamExt};
use slatty_core::library::{self, LibraryCache, LibraryGame};
use slatty_core::overview::{self, GameOverview};
use slatty_core::settings::TwinPlaytime;
use slatty_core::store::{self, Store};

use crate::work::tokens_for;
use crate::{App, Core, Message, err};

/// The cover grid of the Library tab, scrolled back to where it was after a game page.
pub const GRID: &str = "library-grid";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shelf {
    #[default]
    All,
    Installed,
    Favorites,
    /// Games the user hid, left out of the other shelves.
    Hidden,
}

impl Shelf {
    pub const ALL: [Shelf; 3] = [Shelf::All, Shelf::Installed, Shelf::Favorites];
}

impl fmt::Display for Shelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Shelf::All => "All",
            Shelf::Installed => "Installed",
            Shelf::Favorites => "Favorites",
            Shelf::Hidden => "Hidden games",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    NameAsc,
    NameDesc,
    RecentlyPlayed,
    MostPlayed,
}

impl Sort {
    pub const ALL: [Sort; 4] = [
        Sort::NameAsc,
        Sort::NameDesc,
        Sort::RecentlyPlayed,
        Sort::MostPlayed,
    ];

    /// How it is stored in the settings.
    pub fn key(self) -> &'static str {
        match self {
            Sort::NameAsc => "name",
            Sort::NameDesc => "name-desc",
            Sort::RecentlyPlayed => "recently-played",
            Sort::MostPlayed => "most-played",
        }
    }

    pub fn from_key(key: &str) -> Option<Sort> {
        Sort::ALL.into_iter().find(|s| s.key() == key)
    }
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::NameAsc => "Name A–Z",
            Sort::NameDesc => "Name Z–A",
            Sort::RecentlyPlayed => "Recently played",
            Sort::MostPlayed => "Most played",
        })
    }
}

/// Library filters; a game must match every enabled one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filters {
    pub windows: bool,
    pub linux: bool,
    pub achievements: bool,
    pub cloud_saves: bool,
}

impl Filters {
    pub fn any(&self) -> bool {
        *self != Filters::default()
    }
}

impl App {
    pub fn sync_library(&mut self) -> Task<Message> {
        let (Some(core), false) = (self.core.clone(), self.library_busy) else {
            return Task::none();
        };
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            return Task::none();
        };
        self.library_busy = true;
        self.account_task(Task::perform(
            async move {
                let tokens = tokens_for(&core, &user_id).await?;
                let games = library::fetch(&core.http, &tokens).await.map_err(err)?;
                library::save_cache(&core.dirs, &tokens.user_id, games).map_err(err)
            },
            Message::LibrarySynced,
        ))
    }

    pub fn library_synced(&mut self, result: Result<LibraryCache, String>) -> Task<Message> {
        if self.account.is_none() {
            return Task::none();
        }
        self.library_busy = false;
        match result {
            Ok(cache) => self.set_library(cache),
            Err(e) => {
                let cached = if self.fetched_at.is_some() {
                    " Showing the cached version."
                } else {
                    ""
                };
                self.notify_error(format!("Library not refreshed: {e}.{cached}"));
                Task::none()
            }
        }
    }

    pub fn toggle_favorite(&mut self, id: String) {
        if !self.favorites.remove(&id) {
            self.favorites.insert(id);
        }
        if let Some(core) = &self.core {
            let mut ids: Vec<String> = self.favorites.iter().cloned().collect();
            ids.sort();
            if let Err(e) = slatty_core::settings::set_favorites(&core.db, &ids) {
                self.notify_error(e.to_string());
            }
        }
    }

    pub fn set_filters(&mut self, f: Filters) -> Task<Message> {
        self.filters = f;
        if (f.achievements || f.cloud_saves) && !self.overview_complete() {
            return self.scan_overview(false);
        }
        Task::none()
    }

    /// Saved once the scan is done: writing the cache after each game stalls the interface.
    pub fn overview_fetched(&mut self, id: String, result: Result<GameOverview, String>) {
        if let Ok(mut o) = result {
            // GOG did not give the play time this time: the one known stays.
            if o.playtime_minutes.is_none() {
                o.playtime_minutes = self.overview.get(&id).and_then(|k| k.playtime_minutes);
            }
            // What the lists go by (filters, order, the Achievements tab): a scan reads every game
            // again, mostly as it was (`Message::keeps_lists`).
            let listed = |o: &GameOverview| (o.achievements, o.cloud_saves, o.playtime_minutes);
            if listed(&self.overview.get(&id).cloned().unwrap_or_default()) != listed(&o) {
                self.lists.clear();
            }
            self.overview.insert(id, o);
        }
        // How many games were read, shown while the scan runs.
        self.lists.overview_read.take();
    }

    pub fn overview_done(&mut self) {
        self.overview_busy = false;
        self.save_overview();
    }

    /// Saved only when a play time changed: at 10,000 games, writing the cache takes a few
    /// milliseconds on the interface thread, and opening a game page reads its play time again.
    /// The library's list is made again only when sorted by play time (`Message::keeps_lists`).
    pub fn playtimes_fetched(&mut self, times: Vec<(String, u64)>) {
        let mut changed = false;
        for (id, minutes) in times {
            let known = &mut self.overview.entry(id).or_default().playtime_minutes;
            changed |= *known != Some(minutes);
            *known = Some(minutes);
        }
        if changed {
            self.save_overview();
            if self.sort == Sort::MostPlayed {
                self.lists.clear();
            }
        }
    }

    /// Reads play time recorded by GOG for these games, GOG's alone.
    pub fn refresh_playtime(&self, ids: Vec<String>) -> Task<Message> {
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| store::of(id).is_ok_and(|s| s == Store::Gog))
            .collect();
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            return Task::none();
        };
        if ids.is_empty() {
            return Task::none();
        }
        self.account_task(Task::perform(
            async move {
                let Ok(tokens) = tokens_for(&core, &user_id).await else {
                    return Vec::new();
                };
                let (http, tokens) = (&core.http, &tokens);
                iced::futures::stream::iter(ids)
                    .map(|id| async move {
                        let minutes = slatty_core::playtime::total_minutes(http, tokens, &id).await;
                        minutes.ok().map(|m| (id, m))
                    })
                    .buffer_unordered(6)
                    .filter_map(|r| async move { r })
                    .collect()
                    .await
            },
            Message::PlaytimesFetched,
        ))
    }

    /// Seconds played, as GOG records them, or Steam for a Steam game; for a game owned on both,
    /// both added up (their sessions are not the same ones) or the store preferred in Settings.
    pub fn played_seconds(&self, game_id: &str) -> i64 {
        let on_steam = |id: &str| {
            self.steam
                .played
                .get(id)
                .map_or(0, |p| p.minutes as i64 * 60)
        };
        if store::steam_app(game_id).is_some() {
            return on_steam(game_id);
        }
        // GOG is not told of a game added by hand: the sessions played here.
        if store::is_local(game_id) {
            return self.playtime.get(game_id).map_or(0, |p| p.seconds);
        }
        let on_gog = self
            .overview
            .get(game_id)
            .and_then(|o| o.playtime_minutes)
            .map_or(0, |m| m as i64 * 60);
        let Some(twin) = self.steam_twin(game_id) else {
            return on_gog;
        };
        let on_steam = on_steam(twin);
        match self.twin_playtime {
            TwinPlaytime::Both => on_gog + on_steam,
            TwinPlaytime::PreferSteam if on_steam > 0 => on_steam,
            TwinPlaytime::PreferGog if on_gog > 0 => on_gog,
            // The preferred store records none.
            _ => on_gog.max(on_steam),
        }
    }

    pub fn save_overview(&mut self) {
        let (Some(core), Some(account)) = (&self.core, &self.account) else {
            return;
        };
        if let Err(e) = overview::save(&core.dirs, &account.user_id, &self.overview) {
            self.notify_error(e.to_string());
        }
    }

    fn overview_checked(&self, game_id: &str) -> bool {
        self.overview.get(game_id).is_some_and(|o| o.checked)
    }

    /// GOG's games of the library: GOG is asked about these alone, never about another store's.
    fn gog_games(&self) -> impl Iterator<Item = &LibraryGame> {
        self.library
            .iter()
            .filter(|g| store::of(&g.id).is_ok_and(|s| s == Store::Gog))
    }

    pub fn overview_complete(&self) -> bool {
        self.gog_games().all(|g| self.overview_checked(&g.id))
    }

    /// Progress of reading achievements and cloud saves from GOG, while anything is missing.
    pub fn overview_status(&self) -> Option<String> {
        let (read, total) = *self
            .lists
            .overview_read
            .borrow_mut()
            .get_or_insert_with(|| {
                let read = self
                    .gog_games()
                    .filter(|g| self.overview_checked(&g.id))
                    .count();
                (read, self.gog_games().count())
            });
        if self.overview_busy {
            Some(format!("Reading GOG data… {read}/{total} games"))
        } else if read < total {
            Some(match total - read {
                1 => "1 game could not be read from GOG".into(),
                n => format!("{n} games could not be read from GOG"),
            })
        } else {
            None
        }
    }

    /// Reads achievements and cloud support of every game (or only of those not known yet).
    pub fn scan_overview(&mut self, all: bool) -> Task<Message> {
        let (Some(core), Some(user_id)) = (
            self.core.clone(),
            self.account.as_ref().map(|a| a.user_id.clone()),
        ) else {
            return Task::none();
        };
        if self.overview_busy {
            return Task::none();
        }
        let ids: Vec<String> = self
            .gog_games()
            .filter(|g| all || !self.overview_checked(&g.id))
            .map(|g| g.id.clone())
            .collect();
        if ids.is_empty() {
            return Task::none();
        }
        self.overview_busy = true;
        self.account_task(Task::run(overview_stream(core, user_id, ids), |m| m))
    }

    pub fn request_images(&mut self, urls: Vec<String>) -> Task<Message> {
        // Steam's games name the files its client cached: shown as they are, nothing downloaded.
        let (local, urls): (Vec<String>, Vec<String>) =
            urls.into_iter().partition(|u| u.starts_with('/'));
        for path in local {
            if std::path::Path::new(&path).is_file() {
                self.images
                    .insert(path.clone(), iced::widget::image::Handle::from_path(&path));
            }
        }
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        // Those the Steam client has not saved come from Steam's image servers once turned on,
        // whatever the GOG account; the others from GOG, for the account.
        let steam_images = self.steam_images_allowed();
        let wanted: Vec<String> = urls
            .into_iter()
            .filter(|u| steam_images || !slatty_core::steam::library::is_server_image(u))
            .filter(|u| !u.is_empty() && self.images_requested.insert(u.clone()))
            .collect();
        let (steam, gog): (Vec<String>, Vec<String>) = wanted
            .into_iter()
            .partition(|u| slatty_core::steam::library::is_server_image(u));
        let allowed = self.steam.images_allowed.clone();
        let fetch = |core: Core, url: String, user_id: Option<String>| {
            let key = url.clone();
            let allowed = allowed.clone();
            Task::perform(
                async move {
                    let _permit = core.downloads.acquire().await.ok()?;
                    let path = match user_id {
                        Some(user_id) => {
                            library::image(&core.http, &core.dirs, &user_id, &url).await
                        }
                        // Turned off while waiting its turn: not asked.
                        None if !allowed.load(std::sync::atomic::Ordering::Relaxed) => {
                            return None;
                        }
                        None => {
                            slatty_core::steam::library::image(&core.http, &core.dirs, &url).await
                        }
                    };
                    let path = path.ok()?;
                    tokio::task::spawn_blocking(move || fit_image(path))
                        .await
                        .ok()
                },
                move |path| Message::Image(key, path),
            )
        };
        let steam = Task::batch(steam.into_iter().map(|url| fetch(core.clone(), url, None)));
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            // Asked again once signed in.
            for url in &gog {
                self.images_requested.remove(url);
            }
            return steam;
        };
        let gog = Task::batch(
            gog.into_iter()
                .map(|url| fetch(core.clone(), url, Some(user_id.clone()))),
        );
        Task::batch([steam, self.account_task(gog)])
    }

    pub fn set_library(&mut self, cache: LibraryCache) -> Task<Message> {
        if self
            .account
            .as_ref()
            .is_none_or(|a| a.user_id != cache.user_id)
        {
            return Task::none();
        }
        self.fetched_at = Some(cache.fetched_at);
        self.library = cache.games;
        self.gog_titles = self
            .library
            .iter()
            .map(|g| (g.id.clone(), g.title.clone()))
            .collect();
        self.merge_steam();
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        // The covers already cached, all in one message; then each missing one on its own.
        let dirs = core.dirs.clone();
        let covers = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || library::cached_covers(&dirs, &cache.user_id))
                    .await
                    .unwrap_or_default()
            },
            Message::CachedCovers,
        );
        let all = self.library.iter().map(|g| g.id.clone()).collect();
        Task::batch([
            self.account_task(covers),
            self.scan_overview(false),
            self.refresh_playtime(all),
        ])
    }

    /// Shows the covers found in the cache, and downloads those still missing.
    pub fn covers_cached(&mut self, found: HashMap<String, PathBuf>) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.account.as_ref()) else {
            return Task::none();
        };
        let user_id = account.user_id.clone();
        let mut missing = Vec::new();
        // Steam's covers are the client's, or come from Steam's servers (`App::download_covers`).
        let gog = self
            .library
            .iter()
            .filter(|g| store::of(&g.id).is_ok_and(|s| s == Store::Gog));
        for game in gog {
            if self.covers.contains_key(&game.id) {
                continue;
            }
            match found.get(&game.id) {
                Some(path) => {
                    self.covers.insert(
                        game.id.clone(),
                        iced::widget::image::Handle::from_path(path),
                    );
                }
                None if game.cover.is_some() => missing.push(game.clone()),
                None => {}
            }
        }
        let covers = Task::batch(missing.into_iter().map(|game| {
            let (core, user_id) = (core.clone(), user_id.clone());
            let id = game.id.clone();
            Task::perform(
                async move {
                    let _permit = core.downloads.acquire().await.ok()?;
                    library::cover(&core.http, &core.dirs, &user_id, &game)
                        .await
                        .ok()
                        .flatten()
                },
                move |path| Message::Cover(id, path),
            )
        }));
        self.account_task(covers)
    }

    pub fn is_hidden(&self, game_id: &str) -> bool {
        self.customs.get(game_id).is_some_and(|c| c.hidden)
    }

    /// The shelves offered: Hidden games only while some game is hidden.
    pub fn shelves(&self) -> Vec<Shelf> {
        let mut shelves = Shelf::ALL.to_vec();
        if self.customs.values().any(|c| c.hidden) {
            shelves.push(Shelf::Hidden);
        }
        shelves
    }

    /// Games of the current shelf matching search and filters, in the chosen order.
    pub fn visible_games(&self) -> Vec<&LibraryGame> {
        let key = (
            self.search.clone(),
            self.shelf,
            self.filters,
            self.sort,
            self.library.len(),
        );
        let mut kept = self.lists.library.borrow_mut();
        let shown = match &*kept {
            Some((k, shown)) if *k == key => shown.clone(),
            _ => {
                let shown: Rc<[usize]> = self.find_visible_games().into();
                *kept = Some((key, shown.clone()));
                shown
            }
        };
        shown.iter().map(|&i| &self.library[i]).collect()
    }

    /// `visible_games`, as places in `library`.
    fn find_visible_games(&self) -> Vec<usize> {
        let needle = self.search.to_lowercase();
        let f = self.filters;
        let mut games: Vec<(usize, &LibraryGame)> = self
            .library
            .iter()
            .enumerate()
            .filter(|(_, g)| needle.is_empty() || g.title.to_lowercase().contains(&needle))
            .filter(|(_, g)| match self.shelf {
                Shelf::Hidden => self.is_hidden(&g.id),
                _ if self.is_hidden(&g.id) => false,
                Shelf::All => true,
                Shelf::Installed => self.is_installed(&g.id),
                Shelf::Favorites => self.favorites.contains(&g.id),
            })
            .filter(|(_, g)| !f.windows || g.os.iter().any(|o| o == "windows"))
            .filter(|(_, g)| !f.linux || g.os.iter().any(|o| o == "linux"))
            .filter(|(_, g)| {
                let o = self.overview.get(&g.id);
                (!f.achievements || self.achievement_counts(&g.id).is_some())
                    && (!f.cloud_saves || o.is_some_and(|o| o.cloud_saves))
            })
            .collect();
        match self.sort {
            Sort::NameAsc => games.sort_by_cached_key(|(_, g)| self.sort_name(g)),
            Sort::NameDesc => {
                games.sort_by_cached_key(|(_, g)| std::cmp::Reverse(self.sort_name(g)));
            }
            Sort::RecentlyPlayed => {
                games.sort_by_cached_key(|(_, g)| {
                    (
                        std::cmp::Reverse(self.last_played(&g.id).unwrap_or_default()),
                        self.sort_name(g),
                    )
                });
            }
            Sort::MostPlayed => {
                games.sort_by_cached_key(|(_, g)| {
                    (
                        std::cmp::Reverse(self.played_seconds(&g.id)),
                        self.sort_name(g),
                    )
                });
            }
        }
        games.into_iter().map(|(i, _)| i).collect()
    }
}

fn overview_stream(core: Core, user_id: String, ids: Vec<String>) -> impl Stream<Item = Message> {
    iced::stream::channel(16, async move |mut output| {
        let tokens = tokens_for(&core, &user_id).await;
        if let Ok(tokens) = tokens {
            let (http, tokens) = (&core.http, &tokens);
            let mut results = iced::futures::stream::iter(ids)
                .map(|id| async move {
                    let r = overview::fetch(http, tokens, &id).await.map_err(err);
                    (id, r)
                })
                .buffer_unordered(4);
            while let Some((id, r)) = results.next().await {
                let _ = output.send(Message::OverviewFetched(id, r)).await;
            }
        }
        let _ = output.send(Message::OverviewDone).await;
    })
}

/// Widest image kept: key art is drawn at most window-wide.
const IMAGE_MAX_WIDTH: u32 = 2560;

/// Scales an image wider than `IMAGE_MAX_WIDTH` down to it, in the cache, once, so it is decoded
/// quickly and fills less graphics memory: some of GOG's key art is 7184 pixels wide, about 100 MB
/// once decoded, for a page at most a screen wide. Returns the file to show.
fn fit_image(path: PathBuf) -> PathBuf {
    let reader = || {
        image::ImageReader::open(&path)
            .ok()?
            .with_guessed_format()
            .ok()
    };
    let Some((width, _)) = reader().and_then(|r| r.into_dimensions().ok()) else {
        return path;
    };
    if width <= IMAGE_MAX_WIDTH {
        return path;
    }
    let Some(full) = reader().and_then(|r| r.decode().ok()) else {
        return path;
    };
    // Area averaging: quick, and smooth when shrinking.
    let fitted = full.thumbnail(IMAGE_MAX_WIDTH, u32::MAX);
    let mut out = Vec::new();
    // Key art is JPEG; anything with transparency stays lossless.
    let (written, ext) = if fitted.color().has_alpha() {
        let png = fitted.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png);
        (png, "png")
    } else {
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90);
        (fitted.to_rgb8().write_with_encoder(encoder), "jpg")
    };
    // The original is downloaded again only if this copy goes away.
    let small = path.with_extension(ext);
    if written.is_err() || slatty_core::fsutil::write_atomic(&small, &out).is_err() {
        return path;
    }
    if small != path {
        let _ = std::fs::remove_file(&path);
    }
    small
}

/// The `NaturalKey` of each title, made once: making them took 4 ms of every view at 10,000 games.
/// Kept by title, which alone decides the key, so a renamed game never finds a stale one.
#[derive(Default)]
pub struct SortKeys(RefCell<HashMap<String, Rc<NaturalKey>>>);

/// The games the Library and Achievements tabs list, as places in `library`, kept from one view
/// to the next: at 10,000 games, filtering and sorting take about 2 ms, nearly all of a view, and
/// `view` runs after every message. Scrolling, animation frames, covers and progress change none
/// of it; every other message forgets them (`Message::keeps_lists`). The library's list is also
/// kept by what it was made for (search, shelf, filters, order).
#[derive(Default)]
pub struct Lists {
    library: RefCell<Option<(ListKey, Rc<[usize]>)>>,
    /// For the library's length: each game's place, and its unlocked and total achievements.
    pub achievements: RefCell<Option<(usize, AchievementsList)>>,
    /// How many of GOG's games their data was read for, and how many there are, shown while some
    /// are missing.
    overview_read: RefCell<Option<(usize, usize)>>,
}

type ListKey = (String, Shelf, Filters, Sort, usize);
pub type AchievementsList = Rc<[(usize, usize, usize)]>;

impl Lists {
    pub fn clear(&self) {
        self.library.take();
        self.achievements.take();
        self.overview_read.take();
    }
}

impl SortKeys {
    pub fn of(&self, title: &str) -> Rc<NaturalKey> {
        let mut keys = self.0.borrow_mut();
        if let Some(key) = keys.get(title) {
            return key.clone();
        }
        let key = Rc::new(NaturalKey::of(title));
        keys.insert(title.to_string(), key.clone());
        key
    }

    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.borrow().len()
    }

    /// Keys made ahead (at start, off the interface thread).
    pub fn fill(&self, made: Vec<(String, NaturalKey)>) {
        let mut keys = self.0.borrow_mut();
        for (title, key) in made {
            keys.insert(title, Rc::new(key));
        }
    }
}

/// The name each game of `games` sorts by (its own sort title, else its title, the user's if they
/// changed it), with its key.
pub fn sort_names(
    games: &[LibraryGame],
    customs: &HashMap<String, slatty_core::custom::Custom>,
) -> Vec<(String, NaturalKey)> {
    games
        .iter()
        .map(|g| {
            let custom = customs.get(&g.id);
            let name = custom
                .and_then(|c| c.sort_title.as_deref().or(c.title.as_deref()))
                .unwrap_or(&g.title);
            (name.to_string(), NaturalKey::of(name))
        })
        .collect()
}

/// A title sorted the way people read it: case aside, and numbers by their value, so "Game 2"
/// comes before "Game 10".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NaturalKey(Vec<Part>);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Number(u128),
    Text(String),
}

impl NaturalKey {
    pub fn of(title: &str) -> NaturalKey {
        let mut parts = Vec::new();
        let mut chars = title.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_digit() {
                let mut digits = String::new();
                while let Some(d) = chars.next_if(char::is_ascii_digit) {
                    digits.push(d);
                }
                // Too long to be a number people read: kept as text.
                match digits.parse() {
                    Ok(n) => parts.push(Part::Number(n)),
                    Err(_) => parts.push(Part::Text(digits)),
                }
            } else {
                let mut text = String::new();
                while let Some(t) = chars.next_if(|c| !c.is_ascii_digit()) {
                    text.extend(t.to_lowercase());
                }
                parts.push(Part::Text(text));
            }
        }
        NaturalKey(parts)
    }
}

/// The rows of a cover grid to build: a library of thousands of games shows a few dozen covers at
/// once, and building or laying out the others would cost every frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GridWindow {
    pub columns: usize,
    /// Every row of the grid.
    pub rows: usize,
    /// The rows to build, from the first in view to the last, a row more on each side so a short
    /// scroll never shows a gap.
    pub shown: std::ops::Range<usize>,
    /// Height of a row, without the space below it.
    pub row_height: f32,
}

impl GridWindow {
    /// For `count` cards at most `max_width` wide, `spacing` apart, across `width`, scrolled to
    /// `offset` and showing `height`. Cells are `cell_height` high, or 3:4 when `None`. The columns
    /// follow Iced's grid.
    pub fn new(
        count: usize,
        width: f32,
        max_width: f32,
        spacing: f32,
        cell_height: Option<f32>,
        offset: f32,
        height: f32,
    ) -> Self {
        let columns = (((width + spacing) / (max_width + spacing)).ceil() as usize).max(1);
        let cell = (width - spacing * (columns - 1) as f32) / columns as f32;
        let row_height = cell_height.unwrap_or(cell * 4.0 / 3.0);
        let pitch = row_height + spacing;
        let rows = count.div_ceil(columns);
        let total = (rows as f32 * pitch - spacing).max(0.0);
        let offset = offset.clamp(0.0, (total - height).max(0.0));
        let first = ((offset / pitch).floor() as usize)
            .saturating_sub(1)
            .min(rows);
        let end = (((offset + height) / pitch).ceil() as usize + 1).min(rows);
        GridWindow {
            columns,
            rows,
            shown: first..end.max(first),
            row_height,
        }
    }

    /// Indices of the cards of `row`, out of `count`.
    pub fn cards(&self, row: usize, count: usize) -> std::ops::Range<usize> {
        (row * self.columns).min(count)..((row + 1) * self.columns).min(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_rows_in_view_are_built() {
        // 10,000 covers, 4 columns of 100 wide: 2,500 rows of 133.3 plus 12 between.
        let w = GridWindow::new(10_000, 436.0, 100.0, 12.0, None, 0.0, 600.0);
        assert_eq!((w.columns, w.rows), (4, 2_500));
        assert!((w.row_height - 100.0 * 4.0 / 3.0).abs() < 0.01);
        let pitch = w.row_height + 12.0;
        assert_eq!(w.shown, 0..(600.0_f32 / pitch).ceil() as usize + 1);
        assert_eq!(w.cards(0, 10_000), 0..4);

        // Halfway down: a row of margin above.
        let w = GridWindow::new(10_000, 436.0, 100.0, 12.0, None, 1_000.0 * pitch, 600.0);
        assert_eq!(w.shown.start, 999);
        // Past the end (the library just got shorter): the last rows, the last one short.
        let w = GridWindow::new(10, 436.0, 100.0, 12.0, None, 1e9, 600.0);
        assert_eq!((w.rows, w.shown.clone()), (3, 0..3));
        assert_eq!(w.cards(2, 10), 8..10);
        assert!(
            GridWindow::new(0, 436.0, 100.0, 12.0, None, 0.0, 600.0)
                .shown
                .is_empty()
        );
    }

    #[test]
    fn titles_sort_with_their_numbers_read_as_numbers() {
        let mut titles = vec![
            "Game 10",
            "game 2",
            "Game 1",
            "The Witcher 3: Wild Hunt",
            "The Witcher 2",
            "Alan Wake",
        ];
        titles.sort_by_key(|t| NaturalKey::of(t));
        assert_eq!(
            titles,
            [
                "Alan Wake",
                "Game 1",
                "game 2",
                "Game 10",
                "The Witcher 2",
                "The Witcher 3: Wild Hunt"
            ]
        );
    }

    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        image::RgbImage::new(width, height)
            .write_to(
                &mut std::io::Cursor::new(&mut out),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        out
    }

    #[test]
    fn oversized_images_are_scaled_down_once_in_the_cache() {
        let dir = std::env::temp_dir().join(format!("slatty-fit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let small = jpeg(1920, 1080);
        std::fs::write(dir.join("small.jpg"), &small).unwrap();
        assert_eq!(fit_image(dir.join("small.jpg")), dir.join("small.jpg"));
        assert_eq!(
            std::fs::read(dir.join("small.jpg")).unwrap(),
            small,
            "kept as it is"
        );

        std::fs::write(dir.join("art.jpg"), jpeg(5120, 1340)).unwrap();
        let fitted = fit_image(dir.join("art.jpg"));
        assert_eq!(
            fitted,
            dir.join("art.jpg"),
            "the cache holds the small copy"
        );
        let size = image::open(&fitted).unwrap();
        assert_eq!((size.width(), size.height()), (2560, 670));
        assert_eq!(fit_image(fitted.clone()), fitted, "scaled once");

        // An opaque PNG goes back to the cache as a JPEG, under a name that says so.
        let mut png = Vec::new();
        image::RgbImage::new(4000, 1000)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        std::fs::write(dir.join("logo.png"), png).unwrap();
        assert_eq!(fit_image(dir.join("logo.png")), dir.join("logo.jpg"));
        assert!(!dir.join("logo.png").exists());
        assert_eq!(image::open(dir.join("logo.jpg")).unwrap().width(), 2560);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
