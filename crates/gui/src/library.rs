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
                self.notify_error(format!(
                    "Library not refreshed: {e}. Showing the cached version."
                ));
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
        if let Ok(o) = result {
            self.overview.insert(id, o);
        }
    }

    pub fn overview_done(&mut self) {
        self.overview_busy = false;
        self.save_overview();
    }

    pub fn playtimes_fetched(&mut self, times: Vec<(String, u64)>) {
        for (id, minutes) in times {
            self.overview.entry(id).or_default().playtime_minutes = Some(minutes);
        }
        self.save_overview();
    }

    /// Reads play time recorded by GOG for these games.
    pub fn refresh_playtime(&self, ids: Vec<String>) -> Task<Message> {
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

    /// Seconds played, as GOG records them.
    pub fn played_seconds(&self, game_id: &str) -> i64 {
        self.overview
            .get(game_id)
            .and_then(|o| o.playtime_minutes)
            .map_or(0, |m| m as i64 * 60)
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

    pub fn overview_complete(&self) -> bool {
        self.library.iter().all(|g| self.overview_checked(&g.id))
    }

    /// Progress of reading achievements and cloud saves from GOG, while anything is missing.
    pub fn overview_status(&self) -> Option<String> {
        let total = self.library.len();
        let read = self
            .library
            .iter()
            .filter(|g| self.overview_checked(&g.id))
            .count();
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
            .library
            .iter()
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
        let (Some(core), Some(account)) = (self.core.clone(), self.account.as_ref()) else {
            return Task::none();
        };
        let user_id = account.user_id.clone();
        let wanted: Vec<String> = urls
            .into_iter()
            .filter(|u| !u.is_empty() && self.images_requested.insert(u.clone()))
            .collect();
        self.account_task(Task::batch(wanted.into_iter().map(|url| {
            let (core, user_id, key) = (core.clone(), user_id.clone(), url.clone());
            Task::perform(
                async move {
                    let _permit = core.downloads.acquire().await.ok()?;
                    let path = library::image(&core.http, &core.dirs, &user_id, &url)
                        .await
                        .ok()?;
                    tokio::task::spawn_blocking(move || fit_image(path))
                        .await
                        .ok()
                },
                move |path| Message::Image(key, path),
            )
        })))
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
        self.apply_customs();
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let user_id = cache.user_id;
        let missing: Vec<LibraryGame> = self
            .library
            .iter()
            .filter(|g| !self.covers.contains_key(&g.id))
            .cloned()
            .collect();
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
        let all = self.library.iter().map(|g| g.id.clone()).collect();
        Task::batch([
            self.account_task(covers),
            self.scan_overview(false),
            self.refresh_playtime(all),
        ])
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
        let needle = self.search.to_lowercase();
        let f = self.filters;
        let mut games: Vec<&LibraryGame> = self
            .library
            .iter()
            .filter(|g| needle.is_empty() || g.title.to_lowercase().contains(&needle))
            .filter(|g| match self.shelf {
                Shelf::Hidden => self.is_hidden(&g.id),
                _ if self.is_hidden(&g.id) => false,
                Shelf::All => true,
                Shelf::Installed => self.installs.contains_key(&g.id),
                Shelf::Favorites => self.favorites.contains(&g.id),
            })
            .filter(|g| !f.windows || g.os.iter().any(|o| o == "windows"))
            .filter(|g| !f.linux || g.os.iter().any(|o| o == "linux"))
            .filter(|g| {
                let o = self.overview.get(&g.id);
                (!f.achievements || o.is_some_and(|o| o.achievements.is_some()))
                    && (!f.cloud_saves || o.is_some_and(|o| o.cloud_saves))
            })
            .collect();
        let played = |g: &LibraryGame| self.playtime.get(&g.id).copied().unwrap_or_default();
        match self.sort {
            Sort::NameAsc => games.sort_by_cached_key(|g| self.sort_name(g)),
            Sort::NameDesc => {
                games.sort_by_cached_key(|g| std::cmp::Reverse(self.sort_name(g)));
            }
            Sort::RecentlyPlayed => {
                games.sort_by_cached_key(|g| {
                    (std::cmp::Reverse(played(g).last_played), self.sort_name(g))
                });
            }
            Sort::MostPlayed => {
                games.sort_by_cached_key(|g| {
                    (
                        std::cmp::Reverse(self.played_seconds(&g.id)),
                        self.sort_name(g),
                    )
                });
            }
        }
        games
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
