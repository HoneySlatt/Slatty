mod achievements;
mod cloud;
mod edit;
mod icons;
mod install;
mod library;
mod login;
mod maintenance;
mod play;
mod settings;
#[cfg(test)]
mod tests;
mod theme;
mod ui;
mod work;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use achievements::{AchievementChange, PendingChange, achievement_icons};
use cloud::{CloudRequest, CloudResult, CloudStatus, CloudView};
use install::{InstallMsg, InstallView};
pub use library::{Filters, Shelf, Sort};
use maintenance::{MaintenanceMsg, MaintenanceView};
use play::{PlayMsg, PlayState};
use settings::SettingsMsg;

use iced::animation::Easing;
use iced::time::Instant;
use iced::widget::{image, operation};
use iced::{Animation, Size, Subscription, Task, keyboard};
use slatty_core::account::{Account, AccountInfo};
use slatty_core::achievements::Achievement;
use slatty_core::db::Db;
use slatty_core::http::HttpClient;
use slatty_core::install::Install;
use slatty_core::installer::{InstallJob, InstallRecord};
use slatty_core::library::{LibraryCache, LibraryGame};
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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("SLATTY_LOG"))
        .with_writer(std::io::stderr)
        .init();
    iced::application(App::boot, App::update, App::view)
        .title("SlattyLauncher")
        .subscription(App::subscription)
        .theme(App::theme)
        .window(iced::window::Settings {
            size: Size::new(1440.0, 900.0),
            min_size: Some(Size::new(800.0, 560.0)),
            ..Default::default()
        })
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
    /// Cut off (window closed, crash, power loss): resumed at the next start.
    Download,
    /// Paused on request: resumed only when asked.
    Paused,
    /// An update, language or DLC change cut off: the game cannot start until it is finished, which
    /// happens by itself at the next start.
    Update,
    /// The same, paused on request: finished only when asked.
    UpdatePaused,
}

impl Interrupted {
    fn of(job: &InstallJob) -> Self {
        if job.is_update() && job.is_paused() {
            Interrupted::UpdatePaused
        } else if job.is_update() {
            Interrupted::Update
        } else if job.is_paused() {
            Interrupted::Paused
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
    /// How the user renamed or illustrated games, and GOG's own titles to go back to.
    pub customs: HashMap<String, slatty_core::custom::Custom>,
    pub gog_titles: HashMap<String, String>,
    pub edit: Option<edit::EditDraft>,
    pub menu_for: Option<String>,
    pub context_menu: Option<edit::ContextMenu>,
    /// A download being planned again to resume on its own at start-up.
    pub auto_resume: Option<String>,
    /// Size of the window, for layouts that change with it.
    pub window: Size,
    /// Fade in a new page, and the key art of a game once downloaded; `now` is the time of the
    /// frame drawn.
    pub page_shown: Animation<bool>,
    pub art_shown: Animation<bool>,
    pub now: Instant,
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
            customs: HashMap::new(),
            gog_titles: HashMap::new(),
            edit: None,
            menu_for: None,
            context_menu: None,
            auto_resume: None,
            window: Size::new(1440.0, 900.0),
            page_shown: Animation::new(true),
            art_shown: Animation::new(true),
            now: Instant::now(),
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
    Edit(edit::EditMsg),
    WindowResized(Size),
    Frame(Instant),
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
    proton_choices: Vec<PathBuf>,
    favorites: Vec<String>,
    playtime: HashMap<String, Playtime>,
    overview: HashMap<String, GameOverview>,
    jobs: Vec<(String, Interrupted)>,
    customs: HashMap<String, slatty_core::custom::Custom>,
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
        let frames = if self.animating() {
            iced::window::frames().map(Message::Frame)
        } else {
            Subscription::none()
        };
        Subscription::batch([
            frames,
            keyboard::listen().map(Message::Key),
            iced::window::close_requests().map(|_| Message::CloseRequested),
            iced::window::resize_events().map(|(_, size)| Message::WindowResized(size)),
        ])
    }

    /// Plays the page transition when the update moved to another page.
    fn update(&mut self, message: Message) -> Task<Message> {
        let shown = self.location();
        let task = self.handle(message);
        if self.location() != shown {
            self.now = Instant::now();
            self.page_shown = Self::fade_in(self.now);
        }
        task
    }

    fn fade_in(now: Instant) -> Animation<bool> {
        Animation::new(false)
            .duration(theme::tokens().transition)
            .easing(Easing::EaseOutCubic)
            .go(true, now)
    }

    /// A page or a picture is fading in: frames are drawn until it is done.
    pub fn animating(&self) -> bool {
        self.page_shown.is_animating(self.now) || self.art_shown.is_animating(self.now)
    }

    /// The page shown, apart from panels and dialogs over it.
    fn location(&self) -> (Page, Option<String>, Option<String>) {
        (
            self.page,
            self.selected.clone(),
            self.achievements_game.clone(),
        )
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Booted(Ok(boot)) => {
                self.core = Some(boot.core);
                self.account = boot.account;
                self.library_root = boot.library_root.display().to_string();
                self.proton = boot.proton;
                self.proton_choices = boot.proton_choices;
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
                self.customs = boot.customs;
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
                let resume = self.resume_interrupted();
                if let Some(cache) = boot.library {
                    return Task::batch([avatar, resume, self.set_library(cache)]);
                }
                return Task::batch([avatar, resume]);
            }
            Message::Booted(Err(e)) => self.fatal = Some(e),
            Message::OpenLoginPage => self.open_login_page(),
            Message::LoginInput(v) => self.login_input = v,
            Message::PasteLogin => return iced::clipboard::read().map(Message::Pasted),
            Message::Pasted(Some(v)) => self.login_input = v.trim().to_string(),
            Message::Pasted(None) => self.notify_error("The clipboard is empty.".into()),
            Message::SubmitLogin => return self.submit_login(),
            Message::LoggedIn(result) => return self.logged_in(result),
            Message::Logout => return self.logout(),
            Message::LoggedOut(Ok(())) => self.logged_out(),
            Message::LoggedOut(Err(e)) => self.notify_error(e),
            Message::SyncLibrary => return self.sync_library(),
            Message::LibrarySynced(result) => return self.library_synced(result),
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
            Message::SetFilters(f) => return self.set_filters(f),
            Message::ToggleFavorite(id) => self.toggle_favorite(id),
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
                // The key art of the open game fades in once downloaded.
                if self.selected_game().and_then(|g| g.background.as_ref()) == Some(&url) {
                    self.now = Instant::now();
                    self.art_shown = Self::fade_in(self.now);
                }
                self.images.insert(url, image::Handle::from_bytes(bytes));
            }
            Message::Image(url, None) => {
                self.images_requested.remove(&url);
            }
            Message::ScanOverview => return self.scan_overview(true),
            Message::OverviewFetched(id, result) => self.overview_fetched(id, result),
            Message::OverviewDone => self.overview_done(),
            Message::PlaytimesFetched(times) => self.playtimes_fetched(times),
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
            Message::CloudDone(game_id, request, result) => {
                return self.cloud_done(game_id, request, result);
            }
            Message::Install(msg) => return self.update_install(msg),
            Message::Maintenance(msg) => return self.update_maintenance(msg),
            Message::Settings(msg) => return self.update_settings(msg),
            Message::Edit(msg) => return self.update_edit(msg),
            Message::WindowResized(size) => self.window = size,
            Message::Frame(now) => self.now = now,
            Message::DismissNotice => self.notice = None,
            Message::CloseRequested => {
                let running = self.running_work();
                if running.is_empty() {
                    return iced::exit();
                }
                self.quit_confirm = Some(running);
            }
            Message::CancelQuit => self.quit_confirm = None,
            // Work is left as it stands: downloads and updates resume at the next start, as after a
            // crash.
            Message::ConfirmQuit => return iced::exit(),
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
        if self.context_menu.take().is_some() {
            return;
        }
        if self.edit.take().is_some() {
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
                "Downloading {title} ({:.0} %). It stops here and resumes where it stopped when SlattyLauncher starts again.",
                ui::format::fraction(p) * 100.0
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
            let art: Vec<String> = game.background.iter().cloned().collect();
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
        // A running download shows on the game page itself.
        if panel == Panel::Install
            && matches!(
                self.install_views.get(&id),
                Some(InstallView::Running { .. })
            )
        {
            return Task::none();
        }
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
            slatty_core::library::load_cache(&dirs, &a.user_id).map_err(err)?,
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
    // Steam libraries can sit on slow or network drives: listed here, off the interface thread.
    let proton_choices = slatty_core::settings::proton_candidates();
    let favorites = slatty_core::settings::favorites(&db).map_err(err)?;
    let customs = slatty_core::custom::all(&db).map_err(err)?;
    let playtime = session::playtime(&db).map_err(err)?;
    InstallJob::forget_finished(&db).map_err(err)?;
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
        proton_choices,
        favorites,
        playtime,
        overview,
        jobs,
        customs,
    })
}
