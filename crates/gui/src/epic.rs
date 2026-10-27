//! Epic Games Store games in the library, beside GOG's: the account signed in from Settings →
//! Account, its library read from Epic's services at start and on demand, kept in a cache. They
//! install through the same drawer and queue as GOG's (`install`); they cannot be played from
//! Slatty yet.

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
        }
        Task::none()
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
        self.epic_covers()
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
