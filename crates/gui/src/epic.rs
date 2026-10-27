//! Epic Games Store games in the library, beside GOG's: the account signed in from Settings →
//! Account, its library read from Epic's services at start and on demand, kept in a cache, with
//! how many achievements of each game the account unlocked. They install through the same drawer
//! and queue as GOG's (`install`) and play as GOG's do.

use std::collections::HashMap;
use std::path::PathBuf;

use iced::Task;
use slatty_core::epic::auth;
use slatty_core::epic::library::{self, EpicGame, EpicLibrary};
use slatty_core::store;

use crate::{App, Message, err};

#[derive(Debug, Default)]
pub struct EpicView {
    /// The id of the account signed in.
    pub account: Option<String>,
    /// Its name and its games, as last read: from the cache at start, then from Epic.
    pub name: Option<String>,
    pub games: Vec<EpicGame>,
    pub fetched_at: Option<i64>,
    /// Their covers, by game id, once in the cache.
    pub covers: HashMap<String, PathBuf>,
    /// What is pasted from the page Epic shows after signing in.
    pub input: String,
    pub signing_in: bool,
    pub reading: bool,
    /// Unlocked and total achievements of its games, from the cache then from Epic; none until
    /// the cache was read.
    pub achievements: Option<slatty_core::epic::achievements::Counts>,
    /// Their achievements are being read from Epic.
    pub counting: bool,
}

#[derive(Debug, Clone)]
pub enum EpicMsg {
    OpenLoginPage,
    Input(String),
    SignIn,
    /// The account signed in: its id and its name.
    SignedIn(Result<(String, String), String>),
    SignOut,
    SignedOut(Result<(), String>),
    /// Reads the library again from Epic.
    Refresh,
    /// The library read for the account of that id.
    Read(String, Result<EpicLibrary, String>),
    /// A cover in the cache.
    Cover(String, Option<PathBuf>),
    /// The achievement counts kept for the account of that id.
    CountsRead(String, slatty_core::epic::achievements::Counts),
    /// Reads every game's achievements again from Epic (Refresh in the Achievements tab).
    Recount,
    /// A game's counts read from Epic for that account.
    Counted(String, String, Result<Option<(usize, usize)>, String>),
    CountingDone(String),
}

impl App {
    pub fn update_epic(&mut self, msg: EpicMsg) -> Task<Message> {
        match msg {
            EpicMsg::OpenLoginPage => {
                let url = auth::login_url();
                if slatty_core::auth::open_in_browser(&url).is_err() {
                    self.notify_error(format!("Could not open a browser. Open: {url}"));
                }
            }
            EpicMsg::Input(v) => self.epic.input = v,
            EpicMsg::SignIn => {
                let Some(core) = self.core.clone() else {
                    return Task::none();
                };
                if self.epic.signing_in {
                    return Task::none();
                }
                let code = match auth::extract_code(&self.epic.input) {
                    Ok(code) => code,
                    Err(e) => {
                        self.notify_error(e.to_string());
                        return Task::none();
                    }
                };
                self.epic.signing_in = true;
                return Task::perform(
                    async move {
                        let tokens = auth::sign_in(&core.http, &core.db, &code)
                            .await
                            .map_err(err)?;
                        Ok((tokens.account_id, tokens.display_name))
                    },
                    |r| Message::Epic(EpicMsg::SignedIn(r)),
                );
            }
            EpicMsg::SignedIn(Ok((account, name))) => {
                self.epic.signing_in = false;
                self.epic.input.clear();
                if self.epic.account.as_ref() != Some(&account) {
                    self.epic = EpicView {
                        account: Some(account),
                        ..Default::default()
                    };
                }
                self.epic.name = Some(name);
                // Its installed games' updates are looked for, as at sign-in to GOG.
                let updates = self.update_updates(crate::updates::UpdatesMsg::Check);
                return Task::batch([self.update_epic(EpicMsg::Refresh), updates]);
            }
            EpicMsg::SignedIn(Err(e)) => {
                self.epic.signing_in = false;
                self.notify_error(e);
            }
            EpicMsg::SignOut => {
                let Some(core) = self.core.clone() else {
                    return Task::none();
                };
                self.epic = EpicView::default();
                self.merge_steam();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || auth::sign_out(&core.db).map(drop))
                            .await
                            .expect("sign-out task panicked")
                            .map_err(err)
                    },
                    |r| Message::Epic(EpicMsg::SignedOut(r)),
                );
            }
            EpicMsg::SignedOut(Ok(())) => {}
            EpicMsg::SignedOut(Err(e)) => self.notify_error(e),
            EpicMsg::Refresh => {
                let (Some(core), Some(account)) = (self.core.clone(), self.epic.account.clone())
                else {
                    return Task::none();
                };
                if std::mem::replace(&mut self.epic.reading, true) {
                    return Task::none();
                }
                return Task::perform(
                    async move {
                        let read = async {
                            let tokens = auth::fresh(&core.http, &core.db, &core.dirs).await?;
                            library::refresh(&core.http, &core.dirs, &tokens).await
                        };
                        (account, read.await.map_err(err))
                    },
                    |(account, read)| Message::Epic(EpicMsg::Read(account, read)),
                );
            }
            // Signed out, or into another account, while it was read.
            EpicMsg::Read(account, _) if self.epic.account.as_ref() != Some(&account) => {}
            EpicMsg::Read(_, Ok(read)) => {
                self.epic.reading = false;
                return self.epic_read(read);
            }
            EpicMsg::Read(_, Err(e)) => {
                self.epic.reading = false;
                self.notify_error(format!("Epic Games: {e}"));
            }
            EpicMsg::Cover(id, Some(path))
                if self
                    .epic
                    .games
                    .iter()
                    .any(|g| store::epic_id(&g.app_name) == id) =>
            {
                self.covers
                    .insert(id.clone(), iced::widget::image::Handle::from_path(&path));
                self.epic.covers.insert(id.clone(), path);
                return self.scale_covers(Some(vec![id]));
            }
            EpicMsg::Cover(..) => {}
            EpicMsg::CountsRead(account, _) | EpicMsg::CountingDone(account)
                if self.epic.account.as_ref() != Some(&account) => {}
            EpicMsg::CountsRead(_, kept) => {
                // Read from Epic meanwhile: newer than the cache.
                let mut counts = kept;
                counts.extend(self.epic.achievements.take().unwrap_or_default());
                self.epic.achievements = Some(counts);
                return self.count_epic_achievements(false);
            }
            EpicMsg::Recount => return self.count_epic_achievements(true),
            EpicMsg::Counted(account, id, Ok(counts))
                if self.epic.account.as_ref() == Some(&account) =>
            {
                self.epic
                    .achievements
                    .get_or_insert_default()
                    .insert(id, counts);
            }
            // Left to read again at the next start or Refresh.
            EpicMsg::Counted(..) => {}
            EpicMsg::CountingDone(_) => {
                self.epic.counting = false;
                self.save_epic_counts();
            }
        }
        Task::none()
    }

    /// Reads from Epic the achievement counts of its games not known yet, or, with `all`, of
    /// every one, four at a time.
    fn count_epic_achievements(&mut self, all: bool) -> Task<Message> {
        let (Some(core), Some(account), Some(known)) = (
            self.core.clone(),
            self.epic.account.clone(),
            self.epic.achievements.as_ref(),
        ) else {
            return Task::none();
        };
        let games: Vec<(String, String)> = self
            .epic
            .games
            .iter()
            .map(|g| (store::epic_id(&g.app_name), g.namespace.clone()))
            .filter(|(id, _)| all || !known.contains_key(id))
            .collect();
        if games.is_empty() || std::mem::replace(&mut self.epic.counting, true) {
            return Task::none();
        }
        let done = account.clone();
        Task::run(
            iced::stream::channel(16, async move |mut output| {
                use iced::futures::{SinkExt, StreamExt};
                use slatty_core::epic::achievements::{EpicGraphql, fetch};
                if let Ok(tokens) = auth::fresh(&core.http, &core.db, &core.dirs).await {
                    let graphql = EpicGraphql {
                        http: &core.http,
                        token: &tokens.access_token,
                    };
                    let locale = slatty_core::epic::launch::system_locale();
                    let (graphql, tokens, locale) = (&graphql, &tokens, &locale);
                    let mut counted = iced::futures::stream::iter(games)
                        .map(|(id, namespace)| async move {
                            let list = fetch(graphql, &tokens.account_id, &namespace, locale).await;
                            (
                                id,
                                list.map(|l| slatty_core::overview::counts(&l)).map_err(err),
                            )
                        })
                        .buffer_unordered(4);
                    while let Some((id, counts)) = counted.next().await {
                        let read = EpicMsg::Counted(account.clone(), id, counts);
                        let _ = output.send(Message::Epic(read)).await;
                    }
                }
                let _ = output
                    .send(Message::Epic(EpicMsg::CountingDone(done)))
                    .await;
            }),
            |m| m,
        )
    }

    /// Keeps the counts read in the account's cache.
    pub fn save_epic_counts(&mut self) {
        let (Some(core), Some(account), Some(counts)) = (
            &self.core,
            &self.epic.account,
            self.epic.achievements.as_ref(),
        ) else {
            return;
        };
        if let Err(e) = slatty_core::epic::achievements::save_counts(&core.dirs, account, counts) {
            self.notify_error(e.to_string());
        }
    }

    /// The counts kept for the account, read once off the interface thread; those not known
    /// are then read from Epic.
    fn epic_counts(&mut self) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.epic.account.clone()) else {
            return Task::none();
        };
        if self.epic.achievements.is_some() {
            return self.count_epic_achievements(false);
        }
        Task::perform(
            async move {
                let read = account.clone();
                let kept = tokio::task::spawn_blocking(move || {
                    slatty_core::epic::achievements::load_counts(&core.dirs, &read)
                        .unwrap_or_default()
                })
                .await
                .unwrap_or_default();
                (account, kept)
            },
            |(account, kept)| Message::Epic(EpicMsg::CountsRead(account, kept)),
        )
    }

    /// Shows the library read, from the cache or from Epic, and the covers it names.
    pub fn epic_read(&mut self, read: EpicLibrary) -> Task<Message> {
        self.epic.name = Some(read.display_name);
        self.epic.games = read.games;
        self.epic.fetched_at = Some(read.fetched_at);
        let ids: std::collections::HashSet<String> = self
            .epic
            .games
            .iter()
            .map(|g| store::epic_id(&g.app_name))
            .collect();
        self.epic.covers.retain(|id, _| ids.contains(id));
        self.merge_steam();
        Task::batch([self.epic_covers(), self.epic_counts()])
    }

    /// The covers not shown yet: from the cache, else downloaded from Epic's image servers.
    fn epic_covers(&self) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.epic.account.clone()) else {
            return Task::none();
        };
        Task::batch(
            self.epic
                .games
                .iter()
                .filter(|g| g.cover.is_some())
                .filter(|g| !self.epic.covers.contains_key(&store::epic_id(&g.app_name)))
                .cloned()
                .map(|game| {
                    let (core, account) = (core.clone(), account.clone());
                    let id = store::epic_id(&game.app_name);
                    Task::perform(
                        async move {
                            let _permit = core.downloads.acquire().await.ok()?;
                            library::cover(&core.http, &core.dirs, &account, &game)
                                .await
                                .ok()
                                .flatten()
                        },
                        move |path| Message::Epic(EpicMsg::Cover(id, path)),
                    )
                }),
        )
    }

    /// The Epic game of that id, as the account's library describes it.
    pub fn epic_game(&self, game_id: &str) -> Option<EpicGame> {
        let app = store::epic_app(game_id)?;
        self.epic.games.iter().find(|g| g.app_name == app).cloned()
    }

    /// The Epic games the library shows.
    pub fn epic_library_games(&self) -> impl Iterator<Item = slatty_core::library::LibraryGame> {
        self.epic.games.iter().map(library::library_game)
    }
}
