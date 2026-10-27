//! Installing a game: plan, download with progress and pause, discard.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::Stream;
use slatty_core::account::Account;
use slatty_core::epic::library::EpicGame;
use slatty_core::install::{Install, Platform};
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest, Progress,
};
use slatty_core::maintenance::Change;
use tokio_util::sync::CancellationToken;

use crate::maintenance::MaintenanceMsg;
use crate::notify::Kind;
use crate::settings::ProtonChoice;
use crate::work::progress_stream;
use crate::{App, CloudRequest, Core, Message, err};

#[derive(Debug, Clone)]
pub struct PlanInfo {
    pub title: String,
    /// The build being planned, and those the game has.
    pub platform: Platform,
    pub platforms: Vec<Platform>,
    pub version: String,
    /// The build to install, among the versions GOG offers.
    pub build_id: String,
    pub versions: Vec<Version>,
    /// Whether `build_id` is the newest public build, as GOG listed them when planned.
    pub newest: bool,
    pub language: String,
    pub languages: Vec<String>,
    pub download_size: u64,
    pub disk_size: u64,
    /// Folder the game goes into, as typed; the game gets its own subfolder there.
    pub root: String,
    pub directory: String,
    pub dependencies: Vec<String>,
    pub resumable: bool,
    pub dlcs: Vec<DlcChoice>,
    /// Proton build the game will run with: the default from Settings unless changed here.
    pub proton: Option<PathBuf>,
    /// Free space where the game would go, once known.
    pub free: Option<u64>,
}

impl PlanInfo {
    pub fn folder(&self) -> PathBuf {
        Path::new(self.root.trim()).join(&self.directory)
    }

    pub fn total_download(&self) -> u64 {
        self.download_size
            + self
                .dlcs
                .iter()
                .filter(|d| d.selected)
                .map(|d| d.download_size)
                .sum::<u64>()
    }

    pub fn total_disk(&self) -> u64 {
        self.disk_size
            + self
                .dlcs
                .iter()
                .filter(|d| d.selected)
                .map(|d| d.disk_size)
                .sum::<u64>()
    }
}

// One per game at most: the size of a plan does not matter.
#[allow(clippy::large_enum_variant)]
pub enum InstallView {
    Planning,
    Ready(PlanInfo),
    /// Chosen while another install downloads: waits in the download queue.
    Queued(PlanInfo),
    Running {
        title: String,
        /// Where the game goes.
        folder: PathBuf,
        progress: Progress,
        cancel: CancellationToken,
        cancelling: Cancelling,
        rate: Rate,
    },
    Failed(String),
}

/// Where Cancel stands on a running download: it deletes what was downloaded, so it asks first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cancelling {
    #[default]
    No,
    Asked,
    /// Stopping; the partial download is deleted once stopped.
    Confirmed,
}

#[derive(Debug, Clone)]
pub enum InstallMsg {
    Prepare(String, Option<String>),
    /// The Windows or the Linux build.
    Platform(String, Platform),
    /// Another build to install.
    Version(String, String),
    Planned(String, Result<PlanInfo, String>),
    /// Not planned: GOG could not be reached for now.
    Unreachable(String, String),
    /// GOG is asked again for the downloads waiting for it.
    Retry,
    Start(String),
    Progress(String, Progress),
    /// `Err(None)` means paused by the user.
    Done(String, Result<Install, Option<Failure>>),
    Pause(String),
    AskCancel(String),
    KeepDownloading(String),
    ConfirmCancel(String),
    RootInput(String, String),
    Proton(String, ProtonChoice),
    /// Free space under the install folder typed then.
    FreeSpace(String, String, Option<u64>),
    Browse(String),
    Browsed(String, Option<PathBuf>),
    ToggleDlc(String, String),
    Discard(String),
    Discarded(String, Result<(), String>),
}

impl App {
    /// The downloads wait for GOG, or Epic for one of its games, said once as they start waiting.
    pub(crate) fn wait_for_gog(&mut self, game_id: &str, e: String) -> Task<Message> {
        let store = if slatty_core::store::epic_app(game_id).is_some() {
            "Epic"
        } else {
            "GOG"
        };
        self.waiting_for = store;
        let told = if !std::mem::replace(&mut self.waiting_told, true) {
            self.notify(
                Kind::Downloads,
                None,
                format!("Downloads waiting for {store}"),
                format!(
                    "{store} cannot be reached ({e}). They go on by themselves once it answers."
                ),
            )
        } else {
            Task::none()
        };
        self.downloads_waiting = Some(e);
        told
    }

    pub fn installing(&self) -> Option<(&str, &str, Progress)> {
        self.install_views.iter().find_map(|(id, v)| match v {
            InstallView::Running {
                title, progress, ..
            } => Some((id.as_str(), title.as_str(), *progress)),
            _ => None,
        })
    }

    pub fn update_install(&mut self, msg: InstallMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            InstallMsg::Prepare(game_id, language) => {
                // Another language keeps the version chosen, if any.
                let build = match self.install_views.get(&game_id) {
                    Some(InstallView::Ready(info)) if language.is_some() => {
                        Some(info.build_id.clone())
                    }
                    _ => None,
                };
                return self.prepare(core, game_id, language, build, None);
            }
            InstallMsg::Platform(game_id, platform) => {
                return self.prepare(core, game_id, None, None, Some(platform));
            }
            InstallMsg::Version(game_id, build) => {
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                let language = Some(info.language.clone());
                return self.prepare(core, game_id, language, Some(build), None);
            }
            // A plan asked for before the game started downloading changes nothing once it does.
            InstallMsg::Planned(game_id, _) | InstallMsg::Unreachable(game_id, _)
                if matches!(
                    self.install_views.get(&game_id),
                    Some(InstallView::Running { .. })
                ) =>
            {
                self.start_planned.remove(&game_id);
            }
            InstallMsg::Planned(game_id, result) => {
                let auto = self.auto_resume.take_if(|id| *id == game_id).is_some();
                // Asked from the couch mode: with the default choices, as soon as planned.
                let start = self.start_planned.remove(&game_id);
                if let (true, Err(e)) = (start, &result) {
                    self.notify_error(format!(
                        "{} cannot be installed: {e}",
                        self.title_of(&game_id)
                    ));
                }
                // GOG answered: nothing waits for it any longer.
                if result.is_ok() {
                    self.downloads_waiting = None;
                    self.waiting_told = false;
                }
                let view = match result {
                    // Planned again after a restart: it keeps its place in the queue.
                    Ok(info) if !auto && self.queue.contains(&game_id) => InstallView::Queued(info),
                    Ok(info) => InstallView::Ready(info),
                    Err(e) => {
                        self.starting_now.take_if(|id| *id == game_id);
                        if auto {
                            self.notify_error(format!(
                                "The download of {} could not resume: {e}",
                                self.title_of(&game_id)
                            ));
                            // It waits for Resume or Discard, rather than being tried again and
                            // again, without holding the queue; at the next start too.
                            for (id, kind) in &mut self.interrupted {
                                if *id == game_id && *kind == crate::Interrupted::Download {
                                    *kind = crate::Interrupted::Failed;
                                }
                            }
                            if let Ok(Some(job)) = InstallJob::load(&core.db, &game_id)
                                && !job.is_update()
                            {
                                let failed = InstallJob {
                                    state: installer::FAILED.into(),
                                    ..job
                                };
                                if let Err(e) = failed.save(&core.db) {
                                    self.notify_error(e.to_string());
                                }
                            }
                            // One the launcher stopped waited in the queue, not among those
                            // stopped: listed there now, as at the next start, its pass gone.
                            if !self.interrupted.iter().any(|(id, _)| *id == game_id) {
                                self.sync_interrupted(&game_id);
                            }
                            self.goes_on.retain(|id| *id != game_id);
                        }
                        InstallView::Failed(e)
                    }
                };
                let ready = matches!(view, InstallView::Ready(_));
                self.install_views.insert(game_id.clone(), view);
                if (auto || start) && ready {
                    return self.update_install(InstallMsg::Start(game_id));
                }
                // One that cannot start does not stop the queue.
                if auto {
                    return self.start_next();
                }
            }
            InstallMsg::Unreachable(game_id, e) => {
                if self.auto_resume.take_if(|id| *id == game_id).is_none() {
                    // A queued install planned for its size keeps its place, without one.
                    if self.queue.contains(&game_id) {
                        self.install_views.remove(&game_id);
                        return Task::none();
                    }
                    return self.update_install(InstallMsg::Planned(game_id, Err(e)));
                }
                // Starting it, from the queue or cut off: the others would fail the same way. It
                // keeps its place, and the queue waits for GOG.
                self.install_views.remove(&game_id);
                let cut_off = self
                    .interrupted
                    .iter()
                    .any(|(id, k)| *id == game_id && *k == crate::Interrupted::Download);
                let waiting = self.wait_for_gog(&game_id, e);
                if !cut_off {
                    self.drag = None;
                    self.queue.insert(0, game_id);
                    self.save_queue();
                }
                return waiting;
            }
            InstallMsg::Retry => {
                self.downloads_waiting = None;
                // What Download now asked for goes before the download GOG cut off.
                return self.start_asked().unwrap_or_else(|| self.start_next());
            }
            InstallMsg::Start(game_id) => {
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                // Out of the queue: queued again, it gets its pass back if it had started.
                self.goes_on.retain(|e| *e != game_id);
                // Asked for by Download now: it starts even while a game runs.
                let now = self.starting_now.take_if(|id| *id == game_id).is_some();
                let proton = match (info.platform, info.proton.clone()) {
                    (Platform::Linux, _) => PathBuf::new(),
                    (Platform::Windows, Some(proton)) => proton,
                    (Platform::Windows, None) => {
                        let title = self.title_of(&game_id);
                        self.notify_error(format!("Choose a Proton version for {title} first."));
                        // Started from the queue, it does not hold up the installs after it.
                        return self.start_next();
                    }
                };
                let root = PathBuf::from(info.root.trim());
                if !root.is_absolute() {
                    self.notify_error("The install folder must be an absolute path.".into());
                    return Task::none();
                }
                if self.downloading() || (self.held_for_play() && !now) {
                    return self.enqueue(game_id);
                }
                let req = InstallRequest {
                    game_id: game_id.clone(),
                    language: Some(info.language.clone()),
                    build: Some(info.build_id.clone()),
                    platform: info.platform,
                    root,
                    proton,
                    dlcs: DlcSelection::Only(
                        info.dlcs
                            .iter()
                            .filter(|d| d.selected)
                            .map(|d| d.id.clone())
                            .collect(),
                    ),
                    restart: false,
                };
                let cancel = CancellationToken::new();
                let title = info.title.clone();
                let folder = info.folder();
                let started = self.notify(
                    Kind::Downloads,
                    Some(&game_id),
                    "Download started",
                    title.clone(),
                );
                self.forget_interrupted(&game_id);
                self.install_views.insert(
                    game_id.clone(),
                    InstallView::Running {
                        title,
                        folder,
                        progress: Progress::default(),
                        cancel: cancel.clone(),
                        cancelling: Cancelling::No,
                        rate: Rate::default(),
                    },
                );
                // The game page and the library banner show the download from here.
                self.dialog.take_if(|(id, _)| *id == game_id);
                if self.selected.as_deref() == Some(game_id.as_str())
                    && self.panel == Some(crate::Panel::Install)
                {
                    self.panel = None;
                }
                let epic = self.epic_game(&game_id);
                return Task::batch([
                    Task::run(install_stream(core, req, epic, cancel), |m| m),
                    started,
                ]);
            }
            InstallMsg::Progress(game_id, p) => {
                if let Some(InstallView::Running { progress, rate, .. }) =
                    self.install_views.get_mut(&game_id)
                {
                    *progress = p;
                    rate.record(Instant::now(), p.bytes_done, p.network_bytes);
                }
            }
            InstallMsg::Discard(game_id) => {
                if matches!(
                    self.install_views.get(&game_id),
                    Some(InstallView::Running { .. })
                ) {
                    return Task::none();
                }
                // Nor is it started by a plan for its resume answering before the discard ends.
                self.auto_resume.take_if(|id| *id == game_id);
                self.start_planned.remove(&game_id);
                self.goes_on.retain(|id| *id != game_id);
                self.install_views
                    .insert(game_id.clone(), InstallView::Planning);
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            installer::discard(&core.db, &core.dirs, &id).map(drop)
                        })
                        .await
                        .map_err(err)?
                        .map_err(err)
                    },
                    move |r| Message::Install(InstallMsg::Discarded(game_id, r)),
                );
            }
            InstallMsg::Discarded(game_id, result) => {
                // One started meanwhile (Install pressed on a plan that came in between) keeps its
                // view and its panel, its Pause and Cancel with them.
                let started = matches!(
                    self.install_views.get(&game_id),
                    Some(InstallView::Running { .. })
                );
                // Nor is it started once planned, which the queue may be waiting for.
                self.auto_resume.take_if(|id| *id == game_id);
                if !started {
                    self.install_views.remove(&game_id);
                    // Nothing is left to show in its Install panel.
                    self.dialog.take_if(|(id, _)| *id == game_id);
                    if self.selected.as_deref() == Some(game_id.as_str())
                        && self.panel == Some(crate::Panel::Install)
                    {
                        self.panel = None;
                    }
                }
                match result {
                    Ok(()) => {
                        self.forget_interrupted(&game_id);
                        self.unset_aside(&game_id);
                    }
                    Err(e) => self.notify_error(e),
                }
                // A discarded download no longer holds the queue.
                return self.start_next();
            }
            InstallMsg::Proton(game_id, choice) => {
                if let Some(InstallView::Ready(info)) = self.install_views.get_mut(&game_id) {
                    info.proton = Some(choice.0);
                }
            }
            InstallMsg::RootInput(game_id, root) => {
                if let Some(InstallView::Ready(info)) = self.install_views.get_mut(&game_id)
                    && !info.resumable
                {
                    info.root = root.clone();
                    info.free = None;
                    return Task::perform(free_space(root.clone()), move |free| {
                        Message::Install(InstallMsg::FreeSpace(game_id, root, free))
                    });
                }
            }
            InstallMsg::FreeSpace(game_id, root, free) => {
                if let Some(InstallView::Ready(info)) = self.install_views.get_mut(&game_id)
                    && info.root == root
                {
                    info.free = free;
                }
            }
            InstallMsg::Browse(game_id) => {
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                let start = PathBuf::from(info.root.trim());
                return Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new().set_title("Install in");
                        if start.is_dir() {
                            dialog = dialog.set_directory(&start);
                        }
                        dialog.pick_folder().await.map(|f| f.path().to_path_buf())
                    },
                    move |picked| Message::Install(InstallMsg::Browsed(game_id, picked)),
                );
            }
            InstallMsg::Browsed(game_id, Some(folder)) => {
                return self
                    .update_install(InstallMsg::RootInput(game_id, folder.display().to_string()));
            }
            InstallMsg::Browsed(_, None) => {}
            InstallMsg::ToggleDlc(game_id, dlc) => {
                if let Some(InstallView::Ready(info)) = self.install_views.get_mut(&game_id)
                    && !info.resumable
                    && let Some(d) = info.dlcs.iter_mut().find(|d| d.id == dlc && d.owned)
                {
                    d.selected = !d.selected;
                }
            }
            InstallMsg::Pause(game_id) => {
                if let Some(InstallView::Running { cancel, .. }) = self.install_views.get(&game_id)
                {
                    cancel.cancel();
                }
            }
            InstallMsg::AskCancel(game_id) => self.set_cancelling(&game_id, Cancelling::Asked),
            InstallMsg::KeepDownloading(game_id) => self.set_cancelling(&game_id, Cancelling::No),
            InstallMsg::ConfirmCancel(game_id) => {
                if let Some(InstallView::Running {
                    cancel, cancelling, ..
                }) = self.install_views.get_mut(&game_id)
                {
                    *cancelling = Cancelling::Confirmed;
                    cancel.cancel();
                }
            }
            InstallMsg::Done(game_id, Err(_))
                if matches!(
                    self.install_views.get(&game_id),
                    Some(InstallView::Running {
                        cancelling: Cancelling::Confirmed,
                        ..
                    })
                ) =>
            {
                self.preempted.take_if(|(e, _)| *e == game_id);
                self.install_views.remove(&game_id);
                let discard = self.update_install(InstallMsg::Discard(game_id));
                return Task::batch([discard, self.start_next()]);
            }
            InstallMsg::Done(game_id, Ok(install)) => {
                self.preempted.take_if(|(e, _)| *e == game_id);
                self.unset_aside(&game_id);
                self.downloads_waiting = None;
                self.waiting_told = false;
                if let Some(InstallView::Running { progress, .. }) =
                    self.install_views.remove(&game_id)
                {
                    self.completed
                        .insert(0, (game_id.clone(), progress.bytes_total));
                }
                let done = self.notify(
                    Kind::Downloads,
                    Some(&game_id),
                    "Download complete",
                    format!("{} is ready to play.", self.title_of(&game_id)),
                );
                self.installs.insert(game_id.clone(), install);
                let read = self.refresh_record(&game_id);
                // Checked as when the page of an installed game opens.
                let cloud = if self.selected.as_deref() == Some(game_id.as_str()) {
                    self.panel = None;
                    self.request_cloud(game_id.clone(), CloudRequest::Check)
                } else {
                    Task::none()
                };
                // What Download now asked for while it ended goes first, game or not.
                self.made_way = None;
                let next = self.start_asked().unwrap_or_else(|| self.start_next());
                return Task::batch([read, done, cloud, next]);
            }
            // Paused: the queue waits too, unless the launcher paused it.
            InstallMsg::Done(game_id, Err(None)) => {
                if let Some(next) = self.requeue_preempted(&game_id) {
                    return next;
                }
                // Paused on request, it holds the queue, whether it was set aside before or not.
                self.unset_aside(&game_id);
                self.sync_interrupted(&game_id);
                let plan = self.update_install(InstallMsg::Prepare(game_id, None));
                // What Download now asked for while it was pausing starts, now that it is paused;
                // else the download Download now stopped for it goes on.
                let next = self
                    .start_asked()
                    .or_else(|| self.start_made_way())
                    .unwrap_or_else(Task::none);
                return Task::batch([plan, next]);
            }
            InstallMsg::Done(game_id, Err(Some(Failure(e, temporary)))) => {
                self.made_way = None;
                self.preempted.take_if(|(e, _)| *e == game_id);
                self.sync_interrupted(&game_id);
                // Left cut off because GOG could not be reached: it resumes first once GOG answers,
                // and the queue waits for it rather than fail the same way.
                let cut_off = self
                    .interrupted
                    .iter()
                    .any(|(id, k)| *id == game_id && *k == crate::Interrupted::Download);
                if cut_off && temporary {
                    self.install_views.remove(&game_id);
                    return self.wait_for_gog(&game_id, e);
                }
                // Stopped otherwise (the game busy elsewhere), it is not started again by itself.
                for (id, kind) in &mut self.interrupted {
                    if *id == game_id && *kind == crate::Interrupted::Download {
                        *kind = crate::Interrupted::Failed;
                    }
                }
                let failed = self.notify(
                    Kind::Downloads,
                    Some(&game_id),
                    "Download failed",
                    format!("{}: {e}", self.title_of(&game_id)),
                );
                self.install_views.insert(game_id, InstallView::Failed(e));
                // What Download now asked for while it stopped starts, game or not.
                let next = self.start_asked().unwrap_or_else(|| self.start_next());
                return Task::batch([failed, next]);
            }
        }
        Task::none()
    }
}

impl App {
    /// Plans the install again, keeping the folder, Proton and platform already chosen.
    fn prepare(
        &mut self,
        core: Core,
        game_id: String,
        language: Option<String>,
        build: Option<String>,
        platform: Option<Platform>,
    ) -> Task<Message> {
        let platforms = self.platforms(&game_id);
        let (root, proton, current) = match self.install_views.get(&game_id) {
            Some(InstallView::Ready(info)) => (
                PathBuf::from(info.root.trim()),
                info.proton.clone(),
                Some(info.platform),
            ),
            _ => (PathBuf::from(&self.library_root), self.proton.clone(), None),
        };
        let platform = platform.or(current).unwrap_or_else(|| {
            if platforms.contains(&self.default_platform) {
                self.default_platform
            } else {
                platforms[0]
            }
        });
        self.install_views
            .insert(game_id.clone(), InstallView::Planning);
        let id = game_id.clone();
        let choice = Choice {
            language,
            build,
            platform,
            platforms,
            root,
            proton,
            epic: self.epic_game(&game_id),
        };
        Task::perform(plan(core, id, choice), move |r| {
            Message::Install(match r {
                Err(Failure(e, true)) => InstallMsg::Unreachable(game_id, e),
                r => InstallMsg::Planned(game_id, r.map_err(|f| f.0)),
            })
        })
    }

    /// The builds GOG lists for a game: Windows, Linux or both. Windows when it lists neither.
    pub fn platforms(&self, game_id: &str) -> Vec<Platform> {
        let os = self
            .library
            .iter()
            .find(|g| g.id == game_id)
            .map(|g| g.os.as_slice())
            .unwrap_or_default();
        let mut platforms = Vec::new();
        if os.iter().any(|o| o == "windows") || !os.iter().any(|o| o == "linux") {
            platforms.push(Platform::Windows);
        }
        if os.iter().any(|o| o == "linux") {
            platforms.push(Platform::Linux);
        }
        platforms
    }
}

/// What the install dialog asks GOG to plan.
struct Choice {
    language: Option<String>,
    build: Option<String>,
    platform: Platform,
    platforms: Vec<Platform>,
    root: PathBuf,
    proton: Option<PathBuf>,
    /// For an Epic game, the game as the account's library describes it.
    epic: Option<EpicGame>,
}

/// A build of a game, named for a pick list: its version, date and branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub build_id: String,
    pub label: String,
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

/// The builds GOG offers, newest first, the newest public one marked as the latest.
pub fn versions(builds: &[slatty_core::galaxy::Build]) -> Vec<Version> {
    let latest = builds
        .iter()
        .find(|b| b.branch.is_none())
        .map(|b| &b.build_id);
    builds
        .iter()
        .map(|b| {
            let mut label = b.version_name.clone();
            if let Some(date) = b
                .date_published
                .as_deref()
                .and_then(|d| chrono::DateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S%z").ok())
            {
                label += &format!(" · {}", date.format("%-d %b %Y"));
            }
            if let Some(branch) = &b.branch {
                label += &format!(" · {branch} branch");
            }
            if Some(&b.build_id) == latest {
                label += " (latest)";
            }
            Version {
                build_id: b.build_id.clone(),
                label,
            }
        })
        .collect()
}

/// Why a plan or a download failed, and whether only because GOG could not be reached for now.
#[derive(Debug, Clone)]
pub struct Failure(pub String, pub bool);

impl From<slatty_core::Error> for Failure {
    fn from(e: slatty_core::Error) -> Self {
        Failure(e.to_string(), e.is_temporary())
    }
}

async fn plan(core: Core, game_id: String, choice: Choice) -> Result<PlanInfo, Failure> {
    if slatty_core::store::epic_app(&game_id).is_some() {
        return epic_plan(core, game_id, choice).await;
    }
    let mut account = Account::load(&core.db, &core.dirs).await?;
    let tokens = account.tokens(&core.http).await?.clone();
    let job = InstallJob::load(&core.db, &game_id)?;
    // An interrupted install resumes as it started.
    let (platform, language, build, root, dlcs) = match &job {
        Some(j) => (
            if installer::linux::is_linux_build(&j.build_id) {
                Platform::Linux
            } else {
                Platform::Windows
            },
            Some(j.language.clone()),
            Some(j.build_id.clone()),
            j.root.clone(),
            DlcSelection::Only(j.dlcs.clone()),
        ),
        None => (
            choice.platform,
            choice.language,
            choice.build,
            choice.root,
            DlcSelection::AllOwned,
        ),
    };
    let plan = installer::plan_install(
        &core.http,
        &tokens,
        &game_id,
        platform,
        language.as_deref(),
        build.as_deref(),
        &dlcs,
    )
    .await?;
    let free = free_space(root.display().to_string()).await;
    Ok(PlanInfo {
        newest: plan.is_newest(),
        platform: plan.platform,
        platforms: choice.platforms,
        proton: job
            .as_ref()
            .and_then(|j| j.proton.clone())
            .or(choice.proton),
        root: root.display().to_string(),
        free,
        directory: plan.directory_name()?,
        title: plan.title,
        versions: versions(&plan.builds),
        build_id: plan.build.build_id,
        version: plan.build.version_name,
        language: plan.language,
        languages: plan.languages,
        download_size: plan.download_size
            - plan
                .dlcs
                .iter()
                .filter(|d| d.selected)
                .map(|d| d.download_size)
                .sum::<u64>(),
        disk_size: plan.disk_size
            - plan
                .dlcs
                .iter()
                .filter(|d| d.selected)
                .map(|d| d.disk_size)
                .sum::<u64>(),
        dependencies: plan.meta.dependencies,
        resumable: job.is_some(),
        dlcs: plan.dlcs,
    })
}

/// An Epic game's plan: the newest Windows build Epic offers, every file of it.
async fn epic_plan(core: Core, game_id: String, choice: Choice) -> Result<PlanInfo, Failure> {
    let game = choice.epic.ok_or_else(|| not_in_epic_library(&game_id))?;
    let tokens = slatty_core::epic::auth::fresh(&core.http, &core.db, &core.dirs).await?;
    let job = InstallJob::load(&core.db, &game_id)?;
    // An interrupted install resumes into its own folder.
    let root = job.as_ref().map_or(choice.root, |j| j.root.clone());
    let plan = slatty_core::epic::install::plan(&core.http, &tokens, &game).await?;
    let free = free_space(root.display().to_string()).await;
    let build_id = plan.build_id();
    Ok(PlanInfo {
        title: plan.title.clone(),
        platform: Platform::Windows,
        platforms: vec![Platform::Windows],
        versions: vec![Version {
            build_id: build_id.clone(),
            label: format!("{} (latest)", plan.build.build_version),
        }],
        version: plan.build.build_version.clone(),
        build_id,
        newest: true,
        // Every file is installed, language packs included.
        language: "*".into(),
        languages: Vec::new(),
        download_size: plan.download_size,
        disk_size: plan.disk_size,
        root: root.display().to_string(),
        directory: job
            .as_ref()
            .map_or_else(|| plan.folder.clone(), |j| j.directory.clone()),
        dependencies: Vec::new(),
        resumable: job.is_some(),
        dlcs: Vec::new(),
        proton: job
            .as_ref()
            .and_then(|j| j.proton.clone())
            .or(choice.proton),
        free,
    })
}

fn not_in_epic_library(game_id: &str) -> Failure {
    Failure(
        format!("{game_id} is not in the Epic library as last read: sign in to Epic Games again"),
        false,
    )
}

fn install_stream(
    core: Core,
    req: InstallRequest,
    epic: Option<EpicGame>,
    cancel: CancellationToken,
) -> impl Stream<Item = Message> {
    let game_id = req.game_id.clone();
    let id = game_id.clone();
    progress_stream(
        async move |throttle| {
            let result = async {
                let emit = move |e| {
                    if let InstallEvent::Progress(p) = e {
                        throttle.report(p);
                    }
                };
                if slatty_core::store::epic_app(&req.game_id).is_some() {
                    use slatty_core::epic;
                    let game = epic.ok_or_else(|| {
                        slatty_core::Error::NotFound(not_in_epic_library(&req.game_id).0)
                    })?;
                    let tokens = epic::auth::fresh(&core.http, &core.db, &core.dirs).await?;
                    let req = epic::install::EpicInstallRequest {
                        game,
                        root: req.root,
                        proton: req.proton,
                        restart: false,
                    };
                    return epic::install::install(
                        &core.db, &core.dirs, &core.http, &tokens, req, emit, cancel,
                    )
                    .await;
                }
                let mut account = Account::load(&core.db, &core.dirs).await?;
                let tokens = account.tokens(&core.http).await?.clone();
                installer::install(&core.db, &core.dirs, &core.http, &tokens, req, emit, cancel)
                    .await
            }
            .await
            .map_err(|e| match e {
                slatty_core::Error::Cancelled => None,
                e => Some(Failure::from(e)),
            });
            Message::Install(InstallMsg::Done(game_id, result))
        },
        move |p| Message::Install(InstallMsg::Progress(id.clone(), p)),
    )
}

impl App {
    fn set_cancelling(&mut self, game_id: &str, to: Cancelling) {
        if let Some(InstallView::Running { cancelling, .. }) = self.install_views.get_mut(game_id)
            && *cancelling != Cancelling::Confirmed
        {
            *cancelling = to;
        }
    }
}

/// Download speed over the last few seconds of progress: bytes written, which tell the time left,
/// and bytes received, the speed shown.
#[derive(Debug, Default)]
pub struct Rate(VecDeque<(Instant, u64, u64)>);

impl Rate {
    const WINDOW: Duration = Duration::from_secs(3);

    pub fn record(&mut self, at: Instant, written: u64, received: u64) {
        self.0.push_back((at, written, received));
        while self
            .0
            .get(1)
            .is_some_and(|(t, ..)| at.duration_since(*t) >= Self::WINDOW)
        {
            self.0.pop_front();
        }
    }

    /// Bytes written per second, once a second of progress has been seen.
    pub fn per_second(&self) -> Option<f64> {
        self.over(|&(_, written, _)| written)
    }

    /// Bytes received from the network per second, once a second of progress has been seen.
    pub fn received_per_second(&self) -> Option<f64> {
        self.over(|&(.., received)| received)
    }

    fn over(&self, bytes: impl Fn(&(Instant, u64, u64)) -> u64) -> Option<f64> {
        let (first, last) = (self.0.front()?, self.0.back()?);
        let seconds = last.0.duration_since(first.0).as_secs_f64();
        (seconds >= 1.0).then(|| bytes(last).saturating_sub(bytes(first)) as f64 / seconds)
    }
}

/// Free space on the drive that would hold a game installed under `root`. Off the interface
/// thread: the folder may sit on a slow or network drive.
async fn free_space(root: String) -> Option<u64> {
    let root = PathBuf::from(root.trim());
    if !root.is_absolute() {
        return None;
    }
    tokio::task::spawn_blocking(move || installer::free_space(&root).ok())
        .await
        .ok()
        .flatten()
}

impl App {
    /// Resumes what a closed window, a crash or a power loss cut off: every unfinished update and
    /// the first download, else the queue. Work paused on request waits for Resume or Finish update.
    pub fn resume_interrupted(&mut self) -> Task<Message> {
        let signed_in = |id: &str| match slatty_core::store::epic_app(id) {
            Some(_) => self.epic.account.is_some(),
            None => self.account.is_some(),
        };
        // An Epic game's update finishes signed in to Epic alone: until then it cannot be played.
        let updates: Vec<String> = self
            .interrupted
            .iter()
            .filter(|(id, kind)| *kind == crate::Interrupted::Update && signed_in(id))
            .map(|(id, _)| id.clone())
            .collect();
        let mut tasks: Vec<Task<Message>> = updates
            .into_iter()
            .map(|id| self.update_maintenance(MaintenanceMsg::Apply(id, Change::Update)))
            .collect();
        // The queue, of either store, waits for GOG's account, as it always did.
        if self.account.is_none() {
            return Task::batch(tasks);
        }
        tasks.push(self.start_next());
        // The installs still waiting are planned, for their size.
        for game_id in self.queue.clone() {
            if crate::offline::of_entry(&game_id).is_none() {
                tasks.push(self.update_install(InstallMsg::Prepare(game_id, None)));
            }
        }
        Task::batch(tasks)
    }
}
