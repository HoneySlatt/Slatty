//! The game library: GOG sync, covers and images, favorites, what GOG reads for each game
//! (achievements, cloud saves, play time), and the shelf, filters and order shown.

use std::fmt;

use iced::Task;
use iced::futures::{SinkExt, Stream, StreamExt};
use slatty_core::library::{self, LibraryCache, LibraryGame};
use slatty_core::overview::{self, GameOverview};

use crate::work::tokens;
use crate::{App, Core, Message, err};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shelf {
    #[default]
    All,
    Installed,
    Favorites,
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
        self.library_busy = true;
        Task::perform(
            async move {
                let tokens = tokens(&core).await?;
                let games = library::fetch(&core.http, &tokens).await.map_err(err)?;
                library::save_cache(&core.dirs, &tokens.user_id, games).map_err(err)
            },
            Message::LibrarySynced,
        )
    }

    pub fn library_synced(&mut self, result: Result<LibraryCache, String>) -> Task<Message> {
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
        if ids.is_empty() || self.account.is_none() {
            return Task::none();
        }
        Task::perform(
            async move {
                let Ok(tokens) = tokens(&core).await else {
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
        )
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
        let Some(core) = self.core.clone() else {
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
        Task::run(overview_stream(core, ids), |m| m)
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
        Task::batch(wanted.into_iter().map(|url| {
            let (core, user_id, key) = (core.clone(), user_id.clone(), url.clone());
            Task::perform(
                async move {
                    let _permit = core.downloads.acquire().await.ok()?;
                    library::image(&core.http, &core.dirs, &user_id, &url)
                        .await
                        .ok()
                },
                move |bytes| Message::Image(key.clone(), bytes),
            )
        }))
    }

    pub fn set_library(&mut self, cache: LibraryCache) -> Task<Message> {
        self.fetched_at = Some(cache.fetched_at);
        self.library = cache.games;
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
                move |bytes| Message::Cover(id.clone(), bytes),
            )
        }));
        let all = self.library.iter().map(|g| g.id.clone()).collect();
        Task::batch([
            covers,
            self.scan_overview(false),
            self.refresh_playtime(all),
        ])
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
            Sort::NameAsc => games.sort_by_cached_key(|g| g.title.to_lowercase()),
            Sort::NameDesc => {
                games.sort_by_cached_key(|g| std::cmp::Reverse(g.title.to_lowercase()));
            }
            Sort::RecentlyPlayed => {
                games.sort_by_cached_key(|g| {
                    (
                        std::cmp::Reverse(played(g).last_played),
                        g.title.to_lowercase(),
                    )
                });
            }
            Sort::MostPlayed => {
                games.sort_by_cached_key(|g| {
                    (
                        std::cmp::Reverse(self.played_seconds(&g.id)),
                        g.title.to_lowercase(),
                    )
                });
            }
        }
        games
    }
}

fn overview_stream(core: Core, ids: Vec<String>) -> impl Stream<Item = Message> {
    iced::stream::channel(16, async move |mut output| {
        let tokens = tokens(&core).await;
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
