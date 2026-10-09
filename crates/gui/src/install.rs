//! Installing a game: plan, download with progress and pause, discard.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::Stream;
use slatty_core::install::Install;
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest, Progress,
};
use slatty_core::maintenance::Change;
use tokio_util::sync::CancellationToken;

use crate::maintenance::MaintenanceMsg;
use crate::settings::ProtonChoice;
use crate::work::{paused_or, progress_stream, tokens};
use crate::{App, Core, Message, err};

#[derive(Debug, Clone)]
pub struct PlanInfo {
    pub title: String,
    pub version: String,
    /// The build to install, among the versions GOG offers.
    pub build_id: String,
    pub versions: Vec<Version>,
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
    Running {
        title: String,
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
    /// Another build to install.
    Version(String, String),
    /// The install dialog over the library, for this game.
    Open(String),
    CloseDialog,
    Planned(String, Result<PlanInfo, String>),
    Start(String),
    Progress(String, Progress),
    /// `Err(None)` means paused by the user.
    Done(String, Result<Install, Option<String>>),
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
                return self.prepare(core, game_id, language, build);
            }
            InstallMsg::Open(game_id) => {
                self.install_dialog = Some(game_id.clone());
                if !self.install_views.contains_key(&game_id) {
                    return self.update_install(InstallMsg::Prepare(game_id, None));
                }
            }
            InstallMsg::CloseDialog => self.install_dialog = None,
            InstallMsg::Version(game_id, build) => {
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                let language = Some(info.language.clone());
                return self.prepare(core, game_id, language, Some(build));
            }
            InstallMsg::Planned(game_id, result) => {
                let auto = self.auto_resume.take_if(|id| *id == game_id).is_some();
                let view = match result {
                    Ok(info) => InstallView::Ready(info),
                    Err(e) => {
                        if auto {
                            self.notify_error(format!(
                                "The download of {} could not resume: {e}",
                                self.title_of(&game_id)
                            ));
                        }
                        InstallView::Failed(e)
                    }
                };
                let ready = matches!(view, InstallView::Ready(_));
                self.install_views.insert(game_id.clone(), view);
                if auto && ready {
                    return self.update_install(InstallMsg::Start(game_id));
                }
            }
            InstallMsg::Start(game_id) => {
                if self.installing().is_some() {
                    self.notify_error("Another install is already running.".into());
                    return Task::none();
                }
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                let Some(proton) = info.proton.clone() else {
                    self.notify_error("Choose a Proton version first.".into());
                    return Task::none();
                };
                let root = PathBuf::from(info.root.trim());
                if !root.is_absolute() {
                    self.notify_error("The install folder must be an absolute path.".into());
                    return Task::none();
                }
                let req = InstallRequest {
                    game_id: game_id.clone(),
                    language: Some(info.language.clone()),
                    build: Some(info.build_id.clone()),
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
                self.forget_interrupted(&game_id);
                self.install_views.insert(
                    game_id.clone(),
                    InstallView::Running {
                        title,
                        progress: Progress::default(),
                        cancel: cancel.clone(),
                        cancelling: Cancelling::No,
                        rate: Rate::default(),
                    },
                );
                // The game page and the library banner show the download from here.
                self.install_dialog.take_if(|id| *id == game_id);
                if self.selected.as_deref() == Some(game_id.as_str())
                    && self.panel == Some(crate::Panel::Install)
                {
                    self.panel = None;
                }
                return Task::run(install_stream(core, req, cancel), |m| m);
            }
            InstallMsg::Progress(game_id, p) => {
                if let Some(InstallView::Running { progress, rate, .. }) =
                    self.install_views.get_mut(&game_id)
                {
                    *progress = p;
                    rate.record(Instant::now(), p.bytes_done);
                }
            }
            InstallMsg::Discard(game_id) => {
                if matches!(
                    self.install_views.get(&game_id),
                    Some(InstallView::Running { .. })
                ) {
                    return Task::none();
                }
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
                self.install_views.remove(&game_id);
                // Nothing is left to show in its Install panel.
                self.install_dialog.take_if(|id| *id == game_id);
                if self.selected.as_deref() == Some(game_id.as_str())
                    && self.panel == Some(crate::Panel::Install)
                {
                    self.panel = None;
                }
                match result {
                    Ok(()) => self.forget_interrupted(&game_id),
                    Err(e) => self.notify_error(e),
                }
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
                self.install_views.remove(&game_id);
                return self.update_install(InstallMsg::Discard(game_id));
            }
            InstallMsg::Done(game_id, Ok(install)) => {
                self.install_views.remove(&game_id);
                self.installs.insert(game_id.clone(), install);
                self.refresh_record(&game_id);
                if self.selected.as_deref() == Some(game_id.as_str()) {
                    self.panel = None;
                }
            }
            InstallMsg::Done(game_id, Err(None)) => {
                self.sync_interrupted(&game_id);
                return self.update_install(InstallMsg::Prepare(game_id, None));
            }
            InstallMsg::Done(game_id, Err(Some(e))) => {
                self.sync_interrupted(&game_id);
                self.install_views.insert(game_id, InstallView::Failed(e));
            }
        }
        Task::none()
    }
}

impl App {
    /// Plans the install again, keeping the folder and Proton already chosen.
    fn prepare(
        &mut self,
        core: Core,
        game_id: String,
        language: Option<String>,
        build: Option<String>,
    ) -> Task<Message> {
        let (root, proton) = match self.install_views.get(&game_id) {
            Some(InstallView::Ready(info)) => {
                (PathBuf::from(info.root.trim()), info.proton.clone())
            }
            _ => (PathBuf::from(&self.library_root), self.proton.clone()),
        };
        self.install_views
            .insert(game_id.clone(), InstallView::Planning);
        let id = game_id.clone();
        Task::perform(plan(core, id, language, build, root, proton), move |r| {
            Message::Install(InstallMsg::Planned(game_id, r))
        })
    }
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

async fn plan(
    core: Core,
    game_id: String,
    language: Option<String>,
    build: Option<String>,
    root: PathBuf,
    proton: Option<PathBuf>,
) -> Result<PlanInfo, String> {
    let tokens = tokens(&core).await?;
    let job = InstallJob::load(&core.db, &game_id).map_err(err)?;
    let (language, build, root, dlcs) = match &job {
        Some(j) => (
            Some(j.language.clone()),
            Some(j.build_id.clone()),
            j.root.clone(),
            DlcSelection::Only(j.dlcs.clone()),
        ),
        None => (language, build, root, DlcSelection::AllOwned),
    };
    let plan = installer::plan_for(
        &core.http,
        &tokens,
        &game_id,
        language.as_deref(),
        build.as_deref(),
        &dlcs,
    )
    .await
    .map_err(err)?;
    let free = free_space(root.display().to_string()).await;
    Ok(PlanInfo {
        root: root.display().to_string(),
        free,
        proton,
        directory: plan.directory_name().map_err(err)?,
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

fn install_stream(
    core: Core,
    req: InstallRequest,
    cancel: CancellationToken,
) -> impl Stream<Item = Message> {
    let game_id = req.game_id.clone();
    let id = game_id.clone();
    progress_stream(
        async move |throttle| {
            let result = async {
                let tokens = tokens(&core).await.map_err(Some)?;
                let emit = move |e| {
                    if let InstallEvent::Progress(p) = e {
                        throttle.report(p);
                    }
                };
                installer::install(&core.db, &core.dirs, &core.http, &tokens, req, emit, cancel)
                    .await
                    .map_err(paused_or)
            }
            .await;
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

/// Download speed over the last few seconds of progress.
#[derive(Debug, Default)]
pub struct Rate(VecDeque<(Instant, u64)>);

impl Rate {
    const WINDOW: Duration = Duration::from_secs(3);

    pub fn record(&mut self, at: Instant, bytes: u64) {
        self.0.push_back((at, bytes));
        while self
            .0
            .get(1)
            .is_some_and(|(t, _)| at.duration_since(*t) >= Self::WINDOW)
        {
            self.0.pop_front();
        }
    }

    /// Bytes per second, once a second of progress has been seen.
    pub fn per_second(&self) -> Option<f64> {
        let (&(t0, b0), &(t1, b1)) = (self.0.front()?, self.0.back()?);
        let seconds = t1.duration_since(t0).as_secs_f64();
        (seconds >= 1.0).then(|| b1.saturating_sub(b0) as f64 / seconds)
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
    /// the first download. Work paused on request waits for Resume or Finish update.
    pub fn resume_interrupted(&mut self) -> Task<Message> {
        if self.account.is_none() {
            return Task::none();
        }
        let updates: Vec<String> = self
            .interrupted
            .iter()
            .filter(|(_, kind)| *kind == crate::Interrupted::Update)
            .map(|(id, _)| id.clone())
            .collect();
        let mut tasks: Vec<Task<Message>> = updates
            .into_iter()
            .map(|id| self.update_maintenance(MaintenanceMsg::Apply(id, Change::Update)))
            .collect();
        if self.installing().is_some() {
            return Task::batch(tasks);
        }
        let Some(game_id) = self
            .interrupted
            .iter()
            .find(|(_, kind)| *kind == crate::Interrupted::Download)
            .map(|(id, _)| id.clone())
        else {
            return Task::batch(tasks);
        };
        self.auto_resume = Some(game_id.clone());
        tasks.push(self.update_install(InstallMsg::Prepare(game_id, None)));
        Task::batch(tasks)
    }
}
