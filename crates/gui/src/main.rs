mod game;
mod icons;
mod installs;
#[cfg(test)]
mod tests;
mod theme;
mod view;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use installs::{InstallMsg, InstallView, MaintenanceMsg, MaintenanceView, SettingsMsg};

use iced::futures::{SinkExt, Stream, StreamExt};
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
use slatty_core::installer::InstallRecord;
use slatty_core::library::{self, LibraryCache, LibraryGame};
use slatty_core::overview::{self, GameOverview};
use slatty_core::paths::Dirs;
use slatty_core::play::{self, PlayEvent, PlayRequest};
use slatty_core::session::{self, Playtime};
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
        .window_size((1440.0, 900.0))
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudStatus {
    UpToDate,
    Pending(usize),
    Conflict,
    NoCloud,
    Problem,
}

pub struct CloudView {
    pub lines: Vec<String>,
    pub conflicts: bool,
    pub busy: bool,
    pub status: Option<CloudStatus>,
}

pub struct PlayState {
    pub game_id: String,
    pub log: Vec<String>,
    pub stop: UnboundedSender<()>,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Library,
    Achievements,
    Settings,
}

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

/// Tools of the game page, opened over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Install,
    GameSettings,
    Manage,
    Cloud,
    Achievements,
    Session,
}

/// Version and size of a game installed by slatty.
#[derive(Debug, Clone)]
pub struct InstallSummary {
    pub version: String,
    pub size: u64,
}

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
    pub page: Page,
    pub shelf: Shelf,
    pub sort: Sort,
    pub card_width: f32,
    pub filters: Filters,
    pub filters_open: bool,
    pub selected: Option<String>,
    pub panel: Option<Panel>,
    pub installs: HashMap<String, Install>,
    pub records: HashMap<String, InstallSummary>,
    pub favorites: HashSet<String>,
    pub playtime: HashMap<String, Playtime>,
    pub overview: HashMap<String, GameOverview>,
    pub overview_busy: bool,
    pub covers: HashMap<String, image::Handle>,
    pub images: HashMap<String, image::Handle>,
    pub images_requested: HashSet<String>,
    pub notice: Option<Notice>,
    pub play: Option<PlayState>,
    pub achievements: HashMap<String, Loadable<Vec<Achievement>>>,
    pub pending_change: Option<PendingChange>,
    pub cloud: HashMap<String, CloudView>,
    pub install_views: HashMap<String, InstallView>,
    pub maintenance: HashMap<String, MaintenanceView>,
    pub library_root: String,
    pub proton: Option<PathBuf>,
    pub proton_choices: Vec<PathBuf>,
}

impl Default for App {
    fn default() -> Self {
        App {
            core: None,
            fatal: None,
            account: None,
            login_input: String::new(),
            login_busy: false,
            library: Vec::new(),
            fetched_at: None,
            library_busy: false,
            search: String::new(),
            page: Page::default(),
            shelf: Shelf::default(),
            sort: Sort::default(),
            card_width: 150.0,
            filters: Filters::default(),
            filters_open: false,
            selected: None,
            panel: None,
            installs: HashMap::new(),
            records: HashMap::new(),
            favorites: HashSet::new(),
            playtime: HashMap::new(),
            overview: HashMap::new(),
            overview_busy: false,
            covers: HashMap::new(),
            images: HashMap::new(),
            images_requested: HashSet::new(),
            notice: None,
            play: None,
            achievements: HashMap::new(),
            pending_change: None,
            cloud: HashMap::new(),
            install_views: HashMap::new(),
            maintenance: HashMap::new(),
            library_root: String::new(),
            proton: None,
            proton_choices: Vec::new(),
        }
    }
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
pub struct CloudResult {
    pub lines: Vec<String>,
    pub conflicts: bool,
    pub status: CloudStatus,
}

#[derive(Debug, Clone)]
pub enum Message {
    Booted(Result<Box<Boot>, String>),
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
    ShowPage(Page),
    ShowShelf(Shelf),
    SortBy(Sort),
    CardWidth(f32),
    ToggleFilters,
    SetFilters(Filters),
    ToggleFavorite(String),
    Select(String),
    SelectWith(String, Panel),
    CloseDetail,
    OpenPanel(Panel),
    ClosePanel,
    Cover(String, Option<Vec<u8>>),
    Image(String, Option<Vec<u8>>),
    ScanOverview,
    OverviewFetched(String, Result<GameOverview, String>),
    OverviewDone,
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
    CloudDone(String, CloudRequest, Result<CloudResult, String>),
    Install(InstallMsg),
    Maintenance(MaintenanceMsg),
    Settings(SettingsMsg),
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
    records: HashMap<String, InstallSummary>,
    interrupted: Vec<String>,
    library_root: PathBuf,
    proton: Option<PathBuf>,
    favorites: Vec<String>,
    playtime: HashMap<String, Playtime>,
    overview: HashMap<String, GameOverview>,
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
        (
            App::default(),
            Task::perform(async { boot().await.map(Box::new) }, Message::Booted),
        )
    }

    fn theme(&self) -> iced::Theme {
        theme::theme()
    }

    fn subscription(&self) -> Subscription<Message> {
        keyboard::listen().map(Message::Key)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Booted(Ok(boot)) => {
                self.core = Some(boot.core);
                self.account = boot.account;
                self.library_root = boot.library_root.display().to_string();
                self.proton = boot.proton;
                self.proton_choices = slatty_core::settings::proton_candidates();
                self.installs = boot
                    .installs
                    .into_iter()
                    .map(|i| (i.game_id.clone(), i))
                    .collect();
                self.records = boot.records;
                self.favorites = boot.favorites.into_iter().collect();
                self.playtime = boot.playtime;
                self.overview = boot.overview;
                if !boot.interrupted.is_empty() {
                    self.notice = Some(Notice {
                        error: true,
                        text: format!(
                            "Session(s) without a recorded end: {}. Check cloud saves before playing again.",
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
                    self.notify_error(format!("Could not open a browser. Open: {url}"));
                }
            }
            Message::LoginInput(v) => self.login_input = v,
            Message::PasteLogin => return iced::clipboard::read().map(Message::Pasted),
            Message::Pasted(Some(v)) => self.login_input = v.trim().to_string(),
            Message::Pasted(None) => self.notify_error("The clipboard is empty.".into()),
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
                        if let Some(core) = &self.core {
                            self.overview =
                                overview::load(&core.dirs, &info.user_id).unwrap_or_default();
                        }
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
                self.images.clear();
                self.images_requested.clear();
                self.overview.clear();
                self.selected = None;
                self.panel = None;
                self.page = Page::Library;
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
                        "Library not refreshed: {e}. Showing the cached version."
                    )),
                }
            }
            Message::Search(s) => {
                self.search = s;
                self.page = Page::Library;
                self.selected = None;
                self.panel = None;
            }
            Message::ShowPage(page) => {
                self.page = page;
                self.selected = None;
                self.panel = None;
            }
            Message::ShowShelf(shelf) => self.shelf = shelf,
            Message::SortBy(sort) => self.sort = sort,
            Message::CardWidth(w) => self.card_width = w,
            Message::ToggleFilters => self.filters_open = !self.filters_open,
            Message::SetFilters(f) => {
                self.filters = f;
                if (f.achievements || f.cloud_saves) && !self.overview_complete() {
                    return self.scan_overview(false);
                }
            }
            Message::ToggleFavorite(id) => {
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
            Message::Select(id) => return self.open_game(id, None),
            Message::SelectWith(id, panel) => return self.open_game(id, Some(panel)),
            Message::CloseDetail => {
                self.selected = None;
                self.panel = None;
            }
            Message::OpenPanel(panel) => return self.open_panel(panel),
            Message::ClosePanel => self.panel = None,
            Message::Cover(id, Some(bytes)) => {
                self.covers.insert(id, image::Handle::from_bytes(bytes));
            }
            Message::Cover(_, None) => {}
            Message::Image(url, Some(bytes)) => {
                self.images.insert(url, image::Handle::from_bytes(bytes));
            }
            Message::Image(_, None) => {}
            Message::ScanOverview => return self.scan_overview(true),
            Message::OverviewFetched(id, Ok(o)) => {
                self.overview.insert(id, o);
                self.save_overview();
            }
            Message::OverviewFetched(_, Err(_)) => {}
            Message::OverviewDone => self.overview_busy = false,
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
                    log: vec!["Preparing…".into()],
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
                let Some(p) = &mut self.play else {
                    return Task::none();
                };
                p.running = false;
                match result {
                    Ok(()) => p.log.push("Done.".into()),
                    Err(e) => p.log.push(format!("Failed: {e}")),
                }
                let game = p.game_id.clone();
                self.cloud.remove(&game);
                self.achievements.remove(&game);
                if let Some(core) = &self.core {
                    self.playtime = session::playtime(&core.db).unwrap_or_default();
                }
                if self.selected.as_deref() == Some(game.as_str()) {
                    return Task::batch([
                        Task::done(Message::LoadAchievements(game.clone())),
                        Task::done(Message::Cloud(game, CloudRequest::Check)),
                    ]);
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
                    self.achievements_loaded(&game_id, &list);
                    self.achievements.insert(game_id, Loadable::Ready(list));
                    if !failures.is_empty() {
                        self.notify_error(format!("Failed: {}", failures.join(" ; ")));
                    }
                }
                Err(e) => {
                    self.achievements.insert(game_id, Loadable::Failed(e));
                }
            },
            Message::Achievements(id, result) => {
                let task = match &result {
                    Ok(list) => {
                        self.achievements_loaded(&id, list);
                        self.request_images(achievement_icons(list, self.panel_shows_all(&id)))
                    }
                    Err(_) => Task::none(),
                };
                self.achievements.insert(
                    id,
                    match result {
                        Ok(a) => Loadable::Ready(a),
                        Err(e) => Loadable::Failed(e),
                    },
                );
                return task;
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
                        "The game is running; sync once the session has ended.".into(),
                    );
                    return Task::none();
                }
                self.cloud
                    .entry(game_id.clone())
                    .or_insert(CloudView {
                        lines: Vec::new(),
                        conflicts: false,
                        busy: true,
                        status: None,
                    })
                    .busy = true;
                return Task::perform(cloud_task(core, install, request), move |r| {
                    Message::CloudDone(game_id.clone(), request, r)
                });
            }
            Message::CloudDone(game_id, _, result) => {
                let view = match result {
                    Ok(r) => CloudView {
                        lines: r.lines,
                        conflicts: r.conflicts,
                        busy: false,
                        status: Some(r.status),
                    },
                    Err(e) => CloudView {
                        lines: vec![format!("Error: {e}")],
                        conflicts: false,
                        busy: false,
                        status: Some(CloudStatus::Problem),
                    },
                };
                self.cloud.insert(game_id, view);
            }
            Message::Install(msg) => return self.update_install(msg),
            Message::Maintenance(msg) => return self.update_maintenance(msg),
            Message::Settings(msg) => return self.update_settings(msg),
            Message::DismissNotice => self.notice = None,
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use keyboard::key::Named;
                match key {
                    keyboard::Key::Named(Named::Tab) if modifiers.shift() => {
                        return operation::focus_previous();
                    }
                    keyboard::Key::Named(Named::Tab) => return operation::focus_next(),
                    keyboard::Key::Named(Named::Escape) => match self.panel.take() {
                        Some(_) => {}
                        None => self.selected = None,
                    },
                    _ => {}
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }

    fn open_game(&mut self, id: String, panel: Option<Panel>) -> Task<Message> {
        self.page = Page::Library;
        self.selected = Some(id.clone());
        self.panel = None;
        let mut tasks = Vec::new();
        if let Some(game) = self.library.iter().find(|g| g.id == id) {
            let art: Vec<String> = [&game.background, &game.logo]
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            tasks.push(self.request_images(art));
        }
        if !self.achievements.contains_key(&id) {
            tasks.push(Task::done(Message::LoadAchievements(id.clone())));
        }
        if self.installs.contains_key(&id) && !self.cloud.contains_key(&id) {
            tasks.push(Task::done(Message::Cloud(id, CloudRequest::Check)));
        }
        if let Some(p) = panel {
            tasks.push(self.open_panel(p));
        }
        Task::batch(tasks)
    }

    fn open_panel(&mut self, panel: Panel) -> Task<Message> {
        let Some(id) = self.selected.clone() else {
            return Task::none();
        };
        self.panel = Some(panel);
        match panel {
            Panel::Install if !self.install_views.contains_key(&id) => {
                Task::done(Message::Install(InstallMsg::Prepare(id, None)))
            }
            Panel::GameSettings
                if self
                    .maintenance
                    .get(&id)
                    .is_none_or(|m| m.content.is_none() && !m.busy) =>
            {
                Task::done(Message::Maintenance(MaintenanceMsg::LoadContent(id)))
            }
            Panel::Achievements => match self.achievements.get(&id) {
                Some(Loadable::Ready(list)) => self.request_images(achievement_icons(list, true)),
                _ => Task::none(),
            },
            _ => Task::none(),
        }
    }

    fn panel_shows_all(&self, game_id: &str) -> bool {
        self.panel == Some(Panel::Achievements) && self.selected.as_deref() == Some(game_id)
    }

    /// Keeps the cached overview in step with a freshly read achievement list.
    fn achievements_loaded(&mut self, game_id: &str, list: &[Achievement]) {
        let cloud_saves = self.overview.get(game_id).is_some_and(|o| o.cloud_saves);
        let fresh = GameOverview::from_achievements(list, cloud_saves);
        if self.overview.get(game_id) != Some(&fresh) {
            self.overview.insert(game_id.to_string(), fresh);
            self.save_overview();
        }
    }

    fn save_overview(&mut self) {
        let (Some(core), Some(account)) = (&self.core, &self.account) else {
            return;
        };
        if let Err(e) = overview::save(&core.dirs, &account.user_id, &self.overview) {
            self.notify_error(e.to_string());
        }
    }

    pub fn overview_complete(&self) -> bool {
        self.library
            .iter()
            .all(|g| self.overview.contains_key(&g.id))
    }

    /// Reads achievements and cloud support of every game (or only of those not known yet).
    fn scan_overview(&mut self, all: bool) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        if self.overview_busy {
            return Task::none();
        }
        let ids: Vec<String> = self
            .library
            .iter()
            .filter(|g| all || !self.overview.contains_key(&g.id))
            .map(|g| g.id.clone())
            .collect();
        if ids.is_empty() {
            return Task::none();
        }
        self.overview_busy = true;
        Task::run(overview_stream(core, ids), |m| m)
    }

    fn request_images(&mut self, urls: Vec<String>) -> Task<Message> {
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
        Task::batch([covers, self.scan_overview(false)])
    }

    pub fn refresh_record(&mut self, game_id: &str) {
        let Some(core) = &self.core else {
            return;
        };
        match InstallRecord::load(&core.dirs, game_id) {
            Ok(Some(r)) => {
                self.records.insert(game_id.to_string(), summary(&r));
            }
            _ => {
                self.records.remove(game_id);
            }
        }
    }

    fn notify_error(&mut self, text: String) {
        self.notice = Some(Notice { error: true, text });
    }
}

fn summary(r: &InstallRecord) -> InstallSummary {
    InstallSummary {
        version: r.version.clone(),
        size: r.files.iter().map(|f| f.size).sum(),
    }
}

/// Icons to show: the three latest unlocks on the game page, or all of them in the panel.
fn achievement_icons(list: &[Achievement], all: bool) -> Vec<String> {
    if all {
        list.iter()
            .map(|a| {
                if a.date_unlocked.is_some() {
                    a.image_url_unlocked.clone()
                } else {
                    a.image_url_locked.clone()
                }
            })
            .collect()
    } else {
        view::latest_unlocked(list)
            .iter()
            .map(|a| a.image_url_unlocked.clone())
            .collect()
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
    let (library, overview) = match &account {
        Some(a) => (
            library::load_cache(&dirs, &a.user_id).map_err(err)?,
            overview::load(&dirs, &a.user_id).unwrap_or_default(),
        ),
        None => (None, HashMap::new()),
    };
    let installs = Install::list(&db).map_err(err)?;
    let records = installs
        .iter()
        .filter_map(|i| {
            InstallRecord::load(&dirs, &i.game_id)
                .ok()
                .flatten()
                .map(|r| (i.game_id.clone(), summary(&r)))
        })
        .collect();
    let library_root = slatty_core::settings::library_root(&db).map_err(err)?;
    let proton = slatty_core::settings::default_proton(&db).map_err(err)?;
    let favorites = slatty_core::settings::favorites(&db).map_err(err)?;
    let playtime = session::playtime(&db).map_err(err)?;
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
        records,
        interrupted,
        library_root,
        proton,
        favorites,
        playtime,
        overview,
    })
}

fn overview_stream(core: Core, ids: Vec<String>) -> impl Stream<Item = Message> {
    iced::stream::channel(16, async move |mut output| {
        let tokens = async {
            let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
            account.tokens(&core.http).await.map_err(err).cloned()
        }
        .await;
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
) -> Result<CloudResult, String> {
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
        return Ok(CloudResult {
            lines: vec!["GOG has no cloud saves for this game.".into()],
            conflicts: false,
            status: CloudStatus::NoCloud,
        });
    };
    let mut lines = Vec::new();
    let mut conflicts = false;
    let mut pending = 0;
    let mut problem = false;
    for o in &outcomes {
        let root = o
            .root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_else(|| o.template.clone());
        lines.push(format!("[{}] {root}", o.name));
        match &o.result {
            Err(e) => {
                problem = true;
                lines.push(format!("  error: {e}"));
            }
            Ok(r) => {
                lines.extend(
                    r.plan
                        .warnings
                        .iter()
                        .map(|w| format!("  warning: {}", view::describe_warning(*w))),
                );
                if opts.dry_run {
                    let p = &r.plan;
                    pending += p.count(Action::Upload) + p.count(Action::Download);
                    lines.push(format!(
                        "  to upload {} · to download {} · to compare {} · unchanged {}",
                        p.count(Action::Upload),
                        p.count(Action::Download),
                        p.count(Action::Compare),
                        p.count(Action::Keep)
                    ));
                    for (path, _) in p.conflicts() {
                        lines.push(format!("  conflict: {path}"));
                        conflicts = true;
                    }
                } else {
                    lines.push(format!(
                        "  uploaded {} · downloaded {}",
                        r.uploaded.len(),
                        r.downloaded.len()
                    ));
                    for (path, _) in &r.conflicts {
                        lines.push(format!("  conflict: {path}"));
                        conflicts = true;
                    }
                    problem |= !r.refused.is_empty() || !r.errors.is_empty();
                    lines.extend(
                        r.refused
                            .iter()
                            .chain(&r.errors)
                            .map(|(p, e)| format!("  problem {p}: {e}")),
                    );
                    lines.extend(
                        r.pending_deletions
                            .iter()
                            .map(|p| format!("  deletion not applied: {p}")),
                    );
                    if let Some(dir) = &r.backup_dir {
                        lines.push(format!("  previous versions: {}", dir.display()));
                    }
                }
            }
        }
    }
    let status = if conflicts {
        CloudStatus::Conflict
    } else if problem {
        CloudStatus::Problem
    } else if pending > 0 {
        CloudStatus::Pending(pending)
    } else {
        CloudStatus::UpToDate
    };
    Ok(CloudResult {
        lines,
        conflicts,
        status,
    })
}
