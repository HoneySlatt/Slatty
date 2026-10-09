//! Installing a game: plan, download with progress and pause, discard.

use std::path::{Path, PathBuf};

use iced::Task;
use iced::futures::Stream;
use slatty_core::install::Install;
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest, Progress,
};
use tokio_util::sync::CancellationToken;

use crate::settings::ProtonChoice;
use crate::work::{paused_or, progress_stream, tokens};
use crate::{App, Core, Message, err};

#[derive(Debug, Clone)]
pub struct PlanInfo {
    pub title: String,
    pub version: String,
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

pub enum InstallView {
    Planning,
    Ready(PlanInfo),
    Running {
        title: String,
        progress: Progress,
        cancel: CancellationToken,
    },
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum InstallMsg {
    Prepare(String, Option<String>),
    Planned(String, Result<PlanInfo, String>),
    Start(String),
    Progress(String, Progress),
    /// `Err(None)` means paused by the user.
    Done(String, Result<Install, Option<String>>),
    Pause(String),
    RootInput(String, String),
    Proton(String, ProtonChoice),
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
                let (root, proton) = match self.install_views.get(&game_id) {
                    Some(InstallView::Ready(info)) => {
                        (PathBuf::from(info.root.trim()), info.proton.clone())
                    }
                    _ => (PathBuf::from(&self.library_root), self.proton.clone()),
                };
                self.install_views
                    .insert(game_id.clone(), InstallView::Planning);
                let id = game_id.clone();
                return Task::perform(plan(core, id, language, root, proton), move |r| {
                    Message::Install(InstallMsg::Planned(game_id, r))
                });
            }
            InstallMsg::Planned(game_id, result) => {
                let view = match result {
                    Ok(info) => InstallView::Ready(info),
                    Err(e) => InstallView::Failed(e),
                };
                self.install_views.insert(game_id, view);
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
                    },
                );
                return Task::run(install_stream(core, req, cancel), |m| m);
            }
            InstallMsg::Progress(game_id, p) => {
                if let Some(InstallView::Running { progress, .. }) =
                    self.install_views.get_mut(&game_id)
                {
                    *progress = p;
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
                    info.root = root;
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

async fn plan(
    core: Core,
    game_id: String,
    language: Option<String>,
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
        None => (language, None, root, DlcSelection::AllOwned),
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
    Ok(PlanInfo {
        root: root.display().to_string(),
        proton,
        directory: plan.directory_name().map_err(err)?,
        title: plan.title,
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
