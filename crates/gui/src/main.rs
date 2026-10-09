mod achievements;
mod cloud;
mod game;
mod icons;
mod install;
mod maintenance;
mod play;
mod settings;
#[cfg(test)]
mod tests;
mod theme;
mod view;
mod work;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use achievements::{AchievementChange, PendingChange, achievement_icons};
use cloud::{CloudRequest, CloudResult, CloudStatus, CloudView};
use install::{InstallMsg, InstallView};
use maintenance::{MaintenanceMsg, MaintenanceView};
use play::{PlayMsg, PlayState};
use settings::SettingsMsg;
use work::tokens;

use iced::futures::{SinkExt, Stream, StreamExt};
use iced::widget::{image, operation};
use iced::{Subscription, Task, keyboard};
use slatty_core::account::{Account, AccountInfo};
use slatty_core::achievements::Achievement;
use slatty_core::db::Db;
use slatty_core::http::HttpClient;
use slatty_core::install::Install;
use slatty_core::installer::{InstallJob, InstallRecord};
use slatty_core::library::{self, LibraryCache, LibraryGame};
use slatty_core::overview::{self, GameOverview};
use slatty_core::paths::Dirs;
use slatty_core::session::{self, Playtime};
use tokio::sync::Semaphore;

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
        .exit_on_close_request(false)
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

/// Work left unfinished by an earlier run (or paused in this one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupted {
    Download,
    /// An update, language or DLC change: the game cannot start until it is finished.
    Update,
}

impl Interrupted {
    fn of(job: &InstallJob) -> Self {
        if job.is_update() {
            Interrupted::Update
        } else {
            Interrupted::Download
        }
    }
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
    pub avatar: Option<String>,
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
    /// Game whose achievements fill the Achievements tab.
    pub achievements_game: Option<String>,
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
    /// Work that closing the window would interrupt, waiting for the user's choice.
    pub quit_confirm: Option<Vec<String>>,
    pub interrupted: Vec<(String, Interrupted)>,
}

impl Default for App {
    fn default() -> Self {
        App {
            core: None,
            fatal: None,
            account: None,
            avatar: None,
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
            achievements_game: None,
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
            quit_confirm: None,
            interrupted: Vec::new(),
        }
    }
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
    OpenAchievements(String),
    Avatar(Option<String>),
    CloseDetail,
    OpenPanel(Panel),
    ClosePanel,
    Cover(String, Option<Vec<u8>>),
    Image(String, Option<Vec<u8>>),
    ScanOverview,
    OverviewFetched(String, Result<GameOverview, String>),
    OverviewDone,
    PlaytimesFetched(Vec<(String, u64)>),
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
    CloseRequested,
    ConfirmQuit,
    CancelQuit,
    Key(keyboard::Event),
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
    jobs: Vec<(String, Interrupted)>,
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
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            iced::window::close_requests().map(|_| Message::CloseRequested),
        ])
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
                self.interrupted = boot.jobs;
                if !boot.interrupted.is_empty() {
                    self.notice = Some(Notice {
                        error: true,
                        text: format!(
                            "Session(s) without a recorded end: {}. Check cloud saves before playing again.",
                            boot.interrupted.join(", ")
                        ),
                    });
                }
                let avatar = self.fetch_avatar();
                if let Some(cache) = boot.library {
                    return Task::batch([avatar, self.set_library(cache)]);
                }
                return avatar;
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
                        let avatar = self.fetch_avatar();
                        return Task::batch([
                            avatar,
                            match cache {
                                Some(c) => self.set_library(c),
                                None => Task::done(Message::SyncLibrary),
                            },
                        ]);
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
                self.avatar = None;
                self.achievements_game = None;
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
                        let tokens = tokens(&core).await?;
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
                self.achievements_game = None;
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
            Message::OpenAchievements(id) => return self.open_achievements(id),
            Message::Avatar(Some(url)) => {
                self.avatar = Some(url.clone());
                return self.request_images(vec![url]);
            }
            Message::Avatar(None) => {}
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
            Message::Image(url, None) => {
                self.images_requested.remove(&url);
            }
            Message::ScanOverview => return self.scan_overview(true),
            Message::OverviewFetched(id, Ok(o)) => {
                self.overview.insert(id, o);
                self.save_overview();
            }
            Message::OverviewFetched(_, Err(_)) => {}
            Message::OverviewDone => self.overview_busy = false,
            Message::PlaytimesFetched(times) => {
                for (id, minutes) in times {
                    self.overview.entry(id).or_default().playtime_minutes = Some(minutes);
                }
                self.save_overview();
            }
            Message::Play(game_id) => return self.start_game(game_id),
            Message::Playing(msg) => return self.on_play(msg),
            Message::StopGame => self.stop_game(),
            Message::LoadAchievements(game_id) => return self.load_achievements(game_id),
            Message::AskAchievementChange(game_id, changes) => {
                self.ask_achievement_change(game_id, changes);
            }
            Message::CancelAchievementChange => self.pending_change = None,
            Message::ConfirmAchievementChange => return self.confirm_achievement_change(),
            Message::AchievementsChanged(game_id, result) => {
                self.achievements_changed(game_id, result);
            }
            Message::Achievements(id, result) => return self.achievements_received(id, result),
            Message::Cloud(game_id, request) => return self.request_cloud(game_id, request),
            Message::CloudDone(game_id, _, result) => self.cloud_done(game_id, result),
            Message::Install(msg) => return self.update_install(msg),
            Message::Maintenance(msg) => return self.update_maintenance(msg),
            Message::Settings(msg) => return self.update_settings(msg),
            Message::DismissNotice => self.notice = None,
            Message::CloseRequested => {
                let running = self.running_work();
                if running.is_empty() {
                    return iced::exit();
                }
                self.quit_confirm = Some(running);
            }
            Message::CancelQuit => self.quit_confirm = None,
            Message::ConfirmQuit => {
                for view in self.install_views.values() {
                    if let InstallView::Running { cancel, .. } = view {
                        cancel.cancel();
                    }
                }
                for cancel in self.maintenance.values().filter_map(|v| v.cancel.as_ref()) {
                    cancel.cancel();
                }
                return iced::exit();
            }
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                use keyboard::key::Named;
                match key {
                    keyboard::Key::Named(Named::Tab) if modifiers.shift() => {
                        return operation::focus_previous();
                    }
                    keyboard::Key::Named(Named::Tab) => return operation::focus_next(),
                    keyboard::Key::Named(Named::Escape) => self.go_back(),
                    _ => {}
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }

    /// Escape: closes the panel, else the game page, else the per-game achievements page.
    fn go_back(&mut self) {
        if self.quit_confirm.take().is_some() {
            return;
        }
        if self.panel.take().is_none() && self.selected.take().is_none() {
            self.achievements_game = None;
        }
    }

    pub fn forget_interrupted(&mut self, game_id: &str) {
        self.interrupted.retain(|(id, _)| id != game_id);
    }

    /// Lists the game again if, and only if, it has unfinished work recorded.
    pub fn sync_interrupted(&mut self, game_id: &str) {
        self.forget_interrupted(game_id);
        let Some(core) = &self.core else {
            return;
        };
        if let Ok(Some(job)) = InstallJob::load(&core.db, game_id) {
            self.interrupted
                .push((game_id.to_string(), Interrupted::of(&job)));
        }
    }

    pub fn title_of(&self, game_id: &str) -> String {
        self.library
            .iter()
            .find(|g| g.id == game_id)
            .map(|g| g.title.clone())
            .or_else(|| self.installs.get(game_id).map(|i| i.title.clone()))
            .unwrap_or_else(|| game_id.to_string())
    }

    /// What closing the window would interrupt, described for the user.
    pub fn running_work(&self) -> Vec<String> {
        let mut work = Vec::new();
        if let Some((_, title, p)) = self.installing() {
            work.push(format!(
                "Downloading {title} ({:.0} %). Quitting pauses it; it resumes where it stopped.",
                view::fraction(p) * 100.0
            ));
        }
        for (id, m) in &self.maintenance {
            if m.busy {
                work.push(format!(
                    "An operation on {} is running. An interrupted update must be finished before playing.",
                    self.title_of(id)
                ));
            }
        }
        for (id, c) in &self.cloud {
            if c.busy {
                work.push(format!("Cloud saves of {} are syncing.", self.title_of(id)));
            }
        }
        if let Some(p) = self.play.as_ref().filter(|p| p.running) {
            work.push(format!(
                "{} is running. It keeps running, but its cloud saves will not be uploaded and its play time will not be sent to GOG.",
                self.title_of(&p.game_id)
            ));
        }
        work
    }

    fn open_game(&mut self, id: String, panel: Option<Panel>) -> Task<Message> {
        self.page = Page::Library;
        self.achievements_game = None;
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
            tasks.push(self.refresh_playtime(vec![id.clone()]));
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

    fn fetch_avatar(&self) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.account.as_ref()) else {
            return Task::none();
        };
        let user_id = account.user_id.clone();
        Task::perform(
            async move {
                slatty_core::account::avatar_url(&core.http, &user_id)
                    .await
                    .ok()
                    .flatten()
            },
            Message::Avatar,
        )
    }

    /// Reads play time recorded by GOG for these games.
    fn refresh_playtime(&self, ids: Vec<String>) -> Task<Message> {
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

    fn save_overview(&mut self) {
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
            .filter(|g| all || !self.overview_checked(&g.id))
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
        let all = self.library.iter().map(|g| g.id.clone()).collect();
        Task::batch([
            covers,
            self.scan_overview(false),
            self.refresh_playtime(all),
        ])
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

async fn boot() -> Result<Boot, String> {
    let dirs = Dirs::from_system().map_err(err)?;
    let db = Db::open(&dirs.db_file()).map_err(err)?;
    let http = slatty_core::http::client().map_err(err)?;
    let interrupted = slatty_core::play::recover_unfinished(&db)
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
    let jobs = InstallJob::list(&db)
        .map_err(err)?
        .into_iter()
        .map(|j| (j.game_id.clone(), Interrupted::of(&j)))
        .collect();
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
        jobs,
    })
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
