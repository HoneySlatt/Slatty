#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::sync::Arc;

use iced::futures::{SinkExt, Stream};
use iced::widget::{image, operation};
use iced::{Subscription, Task, keyboard};
use slatty_core::account::{Account, AccountInfo};
use slatty_core::achievements::{self, Achievement};
use slatty_core::cloud::{
    self,
    plan::Action,
    sync::{Prefer, SyncOptions},
};
use slatty_core::db::Db;
use slatty_core::http::HttpClient;
use slatty_core::install::Install;
use slatty_core::library::{self, LibraryCache, LibraryGame};
use slatty_core::paths::Dirs;
use slatty_core::play::{self, PlayEvent, PlayRequest};
use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedSender;

fn main() -> iced::Result {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == slatty_core::session::SUPERVISE_ARG)
    {
        std::process::exit(slatty_core::session::supervise_main());
    }
    iced::application(App::boot, App::update, App::view)
        .title("SlattyLauncher")
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size((1280.0, 820.0))
        .run()
}

#[derive(Clone)]
pub struct Core {
    dirs: Dirs,
    db: Arc<Db>,
    http: HttpClient,
    downloads: Arc<Semaphore>,
}

pub struct Notice {
    pub error: bool,
    pub text: String,
}

pub enum Loadable<T> {
    Loading,
    Ready(T),
    Failed(String),
}

pub struct CloudView {
    pub lines: Vec<String>,
    pub conflicts: bool,
    pub busy: bool,
}

pub struct PlayState {
    pub game_id: String,
    pub log: Vec<String>,
    pub stop: UnboundedSender<()>,
    pub running: bool,
}

#[derive(Default)]
pub struct App {
    pub core: Option<Core>,
    pub fatal: Option<String>,
    pub account: Option<AccountInfo>,
    pub login_input: String,
    pub login_busy: bool,
    pub library: Vec<LibraryGame>,
    pub fetched_at: Option<i64>,
    pub library_busy: bool,
    pub search: String,
    pub selected: Option<String>,
    pub installs: HashMap<String, Install>,
    pub covers: HashMap<String, image::Handle>,
    pub notice: Option<Notice>,
    pub play: Option<PlayState>,
    pub achievements: HashMap<String, Loadable<Vec<Achievement>>>,
    pub pending_change: Option<PendingChange>,
    pub cloud: HashMap<String, CloudView>,
}

/// Manual achievement changes waiting for the user's confirmation.
#[derive(Debug, Clone)]
pub struct PendingChange {
    pub game_id: String,
    pub changes: Vec<AchievementChange>,
}

#[derive(Debug, Clone)]
pub struct AchievementChange {
    pub achievement_id: String,
    pub name: String,
    pub unlock: bool,
}

#[derive(Debug, Clone)]
pub enum PlayMsg {
    Event(PlayEvent),
    Done(Result<(), String>),
}

#[derive(Debug, Clone)]
pub enum Message {
    Booted(Result<Boot, String>),
    OpenLoginPage,
    LoginInput(String),
    PasteLogin,
    Pasted(Option<String>),
    SubmitLogin,
    LoggedIn(Result<(AccountInfo, Option<LibraryCache>), String>),
    Logout,
    LoggedOut(Result<(), String>),
    SyncLibrary,
    LibrarySynced(Result<LibraryCache, String>),
    Search(String),
    Select(String),
    CloseDetail,
    Cover(String, Option<Vec<u8>>),
    Play(String),
    Playing(PlayMsg),
    StopGame,
    LoadAchievements(String),
    Achievements(String, Result<Vec<Achievement>, String>),
    AskAchievementChange(String, Vec<AchievementChange>),
    ConfirmAchievementChange,
    CancelAchievementChange,
    AchievementsChanged(String, Result<(Vec<Achievement>, Vec<String>), String>),
    Cloud(String, CloudRequest),
    CloudDone(String, CloudRequest, Result<(Vec<String>, bool), String>),
    DismissNotice,
    Key(keyboard::Event),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudRequest {
    Check,
    Sync,
    Keep(Prefer),
}

#[derive(Debug, Clone)]
pub struct Boot {
    core: Core,
    account: Option<AccountInfo>,
    library: Option<LibraryCache>,
    installs: Vec<Install>,
    interrupted: Vec<String>,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Core")
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        (App::default(), Task::perform(boot(), Message::Booted))
    }

    fn theme(&self) -> iced::Theme {
        iced::Theme::TokyoNight
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Key)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Booted(Ok(boot)) => {
                self.core = Some(boot.core);
                self.account = boot.account;
                self.installs = boot
                    .installs
                    .into_iter()
                    .map(|i| (i.game_id.clone(), i))
                    .collect();
                if !boot.interrupted.is_empty() {
                    self.notice = Some(Notice {
                        error: true,
                        text: format!(
                            "Session(s) interrompue(s) sans fin enregistrée : {}. Vérifiez l'état cloud avant de rejouer.",
                            boot.interrupted.join(", ")
                        ),
                    });
                }
                if let Some(cache) = boot.library {
                    return self.set_library(cache);
                }
            }
            Message::Booted(Err(e)) => self.fatal = Some(e),
            Message::OpenLoginPage => {
                let url = slatty_core::auth::login_url();
                if slatty_core::auth::open_in_browser(&url).is_err() {
                    self.notify_error(format!("Impossible d'ouvrir le navigateur. Ouvrez : {url}"));
                }
            }
            Message::LoginInput(v) => self.login_input = v,
            Message::PasteLogin => return iced::clipboard::read().map(Message::Pasted),
            Message::Pasted(Some(v)) => self.login_input = v.trim().to_string(),
            Message::Pasted(None) => self.notify_error("Le presse-papiers est vide.".into()),
            Message::SubmitLogin => {
                let (Some(core), false) = (self.core.clone(), self.login_busy) else {
                    return Task::none();
                };
                let code = match slatty_core::auth::extract_code(&self.login_input) {
                    Ok(c) => c,
                    Err(e) => {
                        self.notify_error(e.to_string());
                        return Task::none();
                    }
                };
                self.login_busy = true;
                return Task::perform(
                    async move {
                        let account = Account::login(&core.http, &core.db, &core.dirs, &code)
                            .await
                            .map_err(err)?;
                        let cache =
                            library::load_cache(&core.dirs, &account.info.user_id).map_err(err)?;
                        Ok((account.info, cache))
                    },
                    Message::LoggedIn,
                );
            }
            Message::LoggedIn(result) => {
                self.login_busy = false;
                self.login_input.clear();
                match result {
                    Ok((info, cache)) => {
                        self.account = Some(info);
                        return match cache {
                            Some(c) => self.set_library(c),
                            None => Task::done(Message::SyncLibrary),
                        };
                    }
                    Err(e) => self.notify_error(e),
                }
            }
            Message::Logout => {
                let Some(core) = self.core.clone() else {
                    return Task::none();
                };
                return Task::perform(
                    async move {
                        let account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        account.logout(&core.db).await.map_err(err)
                    },
                    Message::LoggedOut,
                );
            }
            Message::LoggedOut(Ok(())) => {
                self.account = None;
                self.library.clear();
                self.covers.clear();
                self.selected = None;
                self.achievements.clear();
                self.cloud.clear();
            }
            Message::LoggedOut(Err(e)) => self.notify_error(e),
            Message::SyncLibrary => {
                let (Some(core), false) = (self.core.clone(), self.library_busy) else {
                    return Task::none();
                };
                self.library_busy = true;
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        let games = library::fetch(&core.http, &tokens).await.map_err(err)?;
                        library::save_cache(&core.dirs, &tokens.user_id, games).map_err(err)
                    },
                    Message::LibrarySynced,
                );
            }
            Message::LibrarySynced(result) => {
                self.library_busy = false;
                match result {
                    Ok(cache) => return self.set_library(cache),
                    Err(e) => self.notify_error(format!(
                        "Bibliothèque non actualisée : {e}. La version en cache reste affichée."
                    )),
                }
            }
            Message::Search(s) => self.search = s,
            Message::Select(id) => self.selected = Some(id),
            Message::CloseDetail => self.selected = None,
            Message::Cover(id, Some(bytes)) => {
                self.covers.insert(id, image::Handle::from_bytes(bytes));
            }
            Message::Cover(_, None) => {}
            Message::Play(game_id) => {
                let Some(core) = self.core.clone() else {
                    return Task::none();
                };
                if self.play.as_ref().is_some_and(|p| p.running) {
                    return Task::none();
                }
                let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
                self.play = Some(PlayState {
                    game_id: game_id.clone(),
                    log: vec!["Préparation…".into()],
                    stop: stop_tx,
                    running: true,
                });
                return Task::run(play_stream(core, game_id, stop_rx), Message::Playing);
            }
            Message::Playing(PlayMsg::Event(event)) => {
                if let Some(p) = &mut self.play {
                    p.log.push(view::describe_play_event(&event));
                }
            }
            Message::Playing(PlayMsg::Done(result)) => {
                if let Some(p) = &mut self.play {
                    p.running = false;
                    match result {
                        Ok(()) => p.log.push("Terminé.".into()),
                        Err(e) => p.log.push(format!("Échec : {e}")),
                    }
                    let game = p.game_id.clone();
                    self.cloud.remove(&game);
                    self.achievements.remove(&game);
                }
            }
            Message::StopGame => {
                if let Some(p) = &self.play {
                    let _ = p.stop.send(());
                }
            }
            Message::LoadAchievements(game_id) => {
                let Some(core) = self.core.clone() else {
                    return Task::none();
                };
                let install = self.installs.get(&game_id).cloned();
                self.achievements.insert(game_id.clone(), Loadable::Loading);
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let (user_id, client_id, token) =
                            achievement_access(&core, &id, install).await?;
                        achievements::fetch(&core.http, &user_id, &client_id, &token)
                            .await
                            .map_err(err)
                    },
                    move |r| Message::Achievements(game_id.clone(), r),
                );
            }
            Message::AskAchievementChange(game_id, changes) => {
                if !changes.is_empty() {
                    self.pending_change = Some(PendingChange { game_id, changes });
                }
            }
            Message::CancelAchievementChange => self.pending_change = None,
            Message::ConfirmAchievementChange => {
                let (Some(core), Some(pending)) = (self.core.clone(), self.pending_change.take())
                else {
                    return Task::none();
                };
                let install = self.installs.get(&pending.game_id).cloned();
                let game_id = pending.game_id.clone();
                self.achievements.insert(game_id.clone(), Loadable::Loading);
                return Task::perform(
                    async move {
                        let (user_id, client_id, token) =
                            achievement_access(&core, &pending.game_id, install).await?;
                        let mut failures = Vec::new();
                        for change in &pending.changes {
                            if let Err(e) = achievements::set_unlocked(
                                &core.http,
                                &user_id,
                                &client_id,
                                &token,
                                &change.achievement_id,
                                change.unlock,
                            )
                            .await
                            {
                                failures.push(format!("{} : {e}", change.name));
                            }
                        }
                        let list = achievements::fetch(&core.http, &user_id, &client_id, &token)
                            .await
                            .map_err(err)?;
                        Ok((list, failures))
                    },
                    move |r| Message::AchievementsChanged(game_id.clone(), r),
                );
            }
            Message::AchievementsChanged(game_id, result) => match result {
                Ok((list, failures)) => {
                    self.achievements.insert(game_id, Loadable::Ready(list));
                    if !failures.is_empty() {
                        self.notify_error(format!("Échec pour : {}", failures.join(" ; ")));
                    }
                }
                Err(e) => {
                    self.achievements.insert(game_id, Loadable::Failed(e));
                }
            },
            Message::Achievements(id, result) => {
                self.achievements.insert(
                    id,
                    match result {
                        Ok(a) => Loadable::Ready(a),
                        Err(e) => Loadable::Failed(e),
                    },
                );
            }
            Message::Cloud(game_id, request) => {
                let (Some(core), Some(install)) =
                    (self.core.clone(), self.installs.get(&game_id).cloned())
                else {
                    return Task::none();
                };
                if self
                    .play
                    .as_ref()
                    .is_some_and(|p| p.running && p.game_id == game_id)
                {
                    self.notify_error(
                        "Le jeu est en cours : la synchronisation attendra la fin de la session."
                            .into(),
                    );
                    return Task::none();
                }
                self.cloud
                    .entry(game_id.clone())
                    .or_insert(CloudView {
                        lines: Vec::new(),
                        conflicts: false,
                        busy: true,
                    })
                    .busy = true;
                return Task::perform(cloud_task(core, install, request), move |r| {
                    Message::CloudDone(game_id.clone(), request, r)
                });
            }
            Message::CloudDone(game_id, _, result) => {
                let view = match result {
                    Ok((lines, conflicts)) => CloudView {
                        lines,
                        conflicts,
                        busy: false,
                    },
                    Err(e) => CloudView {
                        lines: vec![format!("Erreur : {e}")],
                        conflicts: false,
                        busy: false,
                    },
                };
                self.cloud.insert(game_id, view);
            }
            Message::DismissNotice => self.notice = None,
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use keyboard::key::Named;
                match key {
                    keyboard::Key::Named(Named::Tab) if modifiers.shift() => {
                        return operation::focus_previous();
                    }
                    keyboard::Key::Named(Named::Tab) => return operation::focus_next(),
                    keyboard::Key::Named(Named::Escape) => self.selected = None,
                    _ => {}
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }

    fn set_library(&mut self, cache: LibraryCache) -> Task<Message> {
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
        Task::batch(missing.into_iter().map(|game| {
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
        }))
    }

    fn notify_error(&mut self, text: String) {
        self.notice = Some(Notice { error: true, text });
    }
}

/// User id, Galaxy client id and game token for any owned game, installed or not.
async fn achievement_access(
    core: &Core,
    game_id: &str,
    install: Option<Install>,
) -> Result<(String, String, slatty_core::secret::Secret), String> {
    let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
    let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
    let (client_id, token) = match &install {
        Some(i) => achievements::game_token(&core.http, &tokens, i).await,
        None => achievements::product_token(&core.http, &tokens, game_id).await,
    }
    .map_err(err)?;
    Ok((tokens.user_id, client_id, token))
}

async fn boot() -> Result<Boot, String> {
    let dirs = Dirs::from_system().map_err(err)?;
    let db = Db::open(&dirs.db_file()).map_err(err)?;
    let http = slatty_core::http::client().map_err(err)?;
    let interrupted = play::recover_unfinished(&db)
        .map_err(err)?
        .into_iter()
        .map(|s| s.game_id)
        .collect();
    let account = Account::active(&db).map_err(err)?;
    let library = match &account {
        Some(a) => library::load_cache(&dirs, &a.user_id).map_err(err)?,
        None => None,
    };
    let installs = Install::list(&db).map_err(err)?;
    let core = Core {
        dirs,
        db: Arc::new(db),
        http,
        downloads: Arc::new(Semaphore::new(6)),
    };
    Ok(Boot {
        core,
        account,
        library,
        installs,
        interrupted,
    })
}

fn play_stream(
    core: Core,
    game_id: String,
    stop: tokio::sync::mpsc::UnboundedReceiver<()>,
) -> impl Stream<Item = PlayMsg> {
    iced::stream::channel(64, async move |mut output| {
        let supervisor = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                let _ = output.send(PlayMsg::Done(Err(e.to_string()))).await;
                return;
            }
        };
        let req = PlayRequest {
            game_id,
            cloud: true,
            comet: true,
            supervisor,
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run = async {
            play::play(
                &core.db,
                &core.dirs,
                &core.http,
                req,
                move |e| drop(tx.send(e)),
                stop,
            )
            .await
            .map_err(err)
        };
        let mut forward_out = output.clone();
        let forward = async move {
            while let Some(e) = rx.recv().await {
                let _ = forward_out.send(PlayMsg::Event(e)).await;
            }
        };
        let (result, ()) = tokio::join!(run, forward);
        let _ = output.send(PlayMsg::Done(result)).await;
    })
}

async fn cloud_task(
    core: Core,
    install: Install,
    request: CloudRequest,
) -> Result<(Vec<String>, bool), String> {
    let opts = match request {
        CloudRequest::Check => SyncOptions {
            dry_run: true,
            ..Default::default()
        },
        CloudRequest::Sync => SyncOptions::default(),
        CloudRequest::Keep(side) => SyncOptions {
            prefer: Some(side),
            ..Default::default()
        },
    };
    let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
    let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
    let Some(outcomes) =
        cloud::sync_game(&core.db, &core.dirs, &core.http, &tokens, &install, opts)
            .await
            .map_err(err)?
    else {
        return Ok((
            vec!["GOG ne propose pas de sauvegardes cloud pour ce jeu.".into()],
            false,
        ));
    };
    let mut lines = Vec::new();
    let mut conflicts = false;
    for o in &outcomes {
        let root = o
            .root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_else(|| o.template.clone());
        lines.push(format!("[{}] {root}", o.name));
        match &o.result {
            Err(e) => lines.push(format!("  erreur : {e}")),
            Ok(r) => {
                lines.extend(
                    r.plan
                        .warnings
                        .iter()
                        .map(|w| format!("  attention : {}", view::describe_warning(*w))),
                );
                if opts.dry_run {
                    let p = &r.plan;
                    lines.push(format!(
                        "  à envoyer {} · à télécharger {} · à comparer {} · inchangés {}",
                        p.count(Action::Upload),
                        p.count(Action::Download),
                        p.count(Action::Compare),
                        p.count(Action::Keep)
                    ));
                    for (path, _) in p.conflicts() {
                        lines.push(format!("  conflit : {path}"));
                        conflicts = true;
                    }
                } else {
                    lines.push(format!(
                        "  envoyés {} · téléchargés {}",
                        r.uploaded.len(),
                        r.downloaded.len()
                    ));
                    for (path, _) in &r.conflicts {
                        lines.push(format!("  conflit : {path}"));
                        conflicts = true;
                    }
                    lines.extend(
                        r.refused
                            .iter()
                            .chain(&r.errors)
                            .map(|(p, e)| format!("  problème {p} : {e}")),
                    );
                    lines.extend(
                        r.pending_deletions
                            .iter()
                            .map(|p| format!("  suppression non appliquée : {p}")),
                    );
                    if let Some(dir) = &r.backup_dir {
                        lines.push(format!("  versions précédentes : {}", dir.display()));
                    }
                }
            }
        }
    }
    Ok((lines, conflicts))
}
