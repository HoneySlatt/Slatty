use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::{SinkExt, Stream};
use slatty_core::account::Account;
use slatty_core::install::Install;
use slatty_core::installer::{self, InstallEvent, InstallJob, InstallRequest, Progress};
use slatty_core::settings;
use tokio_util::sync::CancellationToken;

use crate::{App, Core, Message, err};

#[derive(Debug, Clone)]
pub struct PlanInfo {
    pub title: String,
    pub version: String,
    pub language: String,
    pub languages: Vec<String>,
    pub download_size: u64,
    pub disk_size: u64,
    pub folder: PathBuf,
    pub dependencies: Vec<String>,
    pub resumable: bool,
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
    Event(String, InstallEvent),
    /// `Err(None)` means paused by the user.
    Done(String, Result<Install, Option<String>>),
    Pause(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtonChoice(pub PathBuf);

impl fmt::Display for ProtonChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = self
            .0
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        f.write_str(&name)
    }
}

#[derive(Debug, Clone)]
pub enum SettingsMsg {
    Toggle,
    RootInput(String),
    SaveRoot,
    Proton(ProtonChoice),
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
                self.install_views
                    .insert(game_id.clone(), InstallView::Planning);
                let root = PathBuf::from(&self.library_root);
                let id = game_id.clone();
                return Task::perform(plan(core, id, language, root), move |r| {
                    Message::Install(InstallMsg::Planned(game_id.clone(), r))
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
                let Some(proton) = self.proton.clone() else {
                    self.notify_error("Choose a Proton version in Settings first.".into());
                    return Task::none();
                };
                let Some(InstallView::Ready(info)) = self.install_views.get(&game_id) else {
                    return Task::none();
                };
                let req = InstallRequest {
                    game_id: game_id.clone(),
                    language: Some(info.language.clone()),
                    root: PathBuf::from(&self.library_root),
                    proton,
                    restart: false,
                };
                let cancel = CancellationToken::new();
                let title = info.title.clone();
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
            InstallMsg::Event(game_id, InstallEvent::Progress(p)) => {
                if let Some(InstallView::Running { progress, .. }) =
                    self.install_views.get_mut(&game_id)
                {
                    *progress = p;
                }
            }
            InstallMsg::Event(_, _) => {}
            InstallMsg::Pause(game_id) => {
                if let Some(InstallView::Running { cancel, .. }) = self.install_views.get(&game_id)
                {
                    cancel.cancel();
                }
            }
            InstallMsg::Done(game_id, Ok(install)) => {
                self.install_views.remove(&game_id);
                self.installs.insert(game_id, install);
            }
            InstallMsg::Done(game_id, Err(None)) => {
                return self.update_install(InstallMsg::Prepare(game_id, None));
            }
            InstallMsg::Done(game_id, Err(Some(e))) => {
                self.install_views.insert(game_id, InstallView::Failed(e));
            }
        }
        Task::none()
    }

    pub fn update_settings(&mut self, msg: SettingsMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            SettingsMsg::Toggle => self.settings_open = !self.settings_open,
            SettingsMsg::RootInput(v) => self.library_root = v,
            SettingsMsg::SaveRoot => {
                let path = PathBuf::from(self.library_root.trim());
                if !path.is_absolute() {
                    self.notify_error("The games folder must be an absolute path.".into());
                } else if let Err(e) = settings::set_library_root(&core.db, &path) {
                    self.notify_error(e.to_string());
                }
            }
            SettingsMsg::Proton(choice) => {
                match settings::set_default_proton(&core.db, &choice.0) {
                    Ok(()) => self.proton = Some(choice.0),
                    Err(e) => self.notify_error(e.to_string()),
                }
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
) -> Result<PlanInfo, String> {
    let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
    let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
    let job = InstallJob::load(&core.db, &game_id).map_err(err)?;
    let (language, build, root) = match &job {
        Some(j) => (
            Some(j.language.clone()),
            Some(j.build_id.clone()),
            j.root.clone(),
        ),
        None => (language, None, root),
    };
    let plan = installer::plan_for(
        &core.http,
        &tokens,
        &game_id,
        language.as_deref(),
        build.as_deref(),
    )
    .await
    .map_err(err)?;
    Ok(PlanInfo {
        folder: root.join(plan.directory_name().map_err(err)?),
        title: plan.title,
        version: plan.build.version_name,
        language: plan.language,
        languages: plan.languages,
        download_size: plan.download_size,
        disk_size: plan.disk_size,
        dependencies: plan.meta.dependencies,
        resumable: job.is_some(),
    })
}

fn install_stream(
    core: Core,
    req: InstallRequest,
    cancel: CancellationToken,
) -> impl Stream<Item = Message> {
    iced::stream::channel(64, async move |mut output| {
        let game_id = req.game_id.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run = async {
            let mut account = Account::load(&core.db, &core.dirs)
                .await
                .map_err(|e| Some(e.to_string()))?;
            let tokens = account
                .tokens(&core.http)
                .await
                .map_err(|e| Some(e.to_string()))?
                .clone();
            match installer::install(
                &core.db,
                &core.dirs,
                &core.http,
                &tokens,
                req,
                move |e| drop(tx.send(e)),
                cancel.clone(),
            )
            .await
            {
                Ok(i) => Ok(i),
                Err(slatty_core::Error::Cancelled) => Err(None),
                Err(e) => Err(Some(e.to_string())),
            }
        };
        let mut forward_out = output.clone();
        let id = game_id.clone();
        let forward = async move {
            let mut last = Instant::now() - Duration::from_secs(1);
            while let Some(e) = rx.recv().await {
                if matches!(e, InstallEvent::Progress(_))
                    && last.elapsed() < Duration::from_millis(250)
                {
                    continue;
                }
                last = Instant::now();
                let _ = forward_out
                    .send(Message::Install(InstallMsg::Event(id.clone(), e)))
                    .await;
            }
        };
        let (result, ()) = tokio::join!(run, forward);
        let _ = output
            .send(Message::Install(InstallMsg::Done(game_id, result)))
            .await;
    })
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} Gio", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} Mio", b as f64 / (1u64 << 20) as f64),
        b => format!("{} Kio", b >> 10),
    }
}

#[derive(Default)]
pub struct MaintenanceView {
    pub busy: bool,
    pub lines: Vec<String>,
    pub confirm_uninstall: bool,
}

#[derive(Debug, Clone)]
pub enum MaintenanceMsg {
    Check(String, bool),
    Checked(String, bool, Result<Vec<PathBuf>, String>),
    AskUninstall(String),
    CancelUninstall(String),
    Uninstall(String, bool),
    Uninstalled(String, Result<Vec<String>, String>),
}

impl App {
    pub fn update_maintenance(&mut self, msg: MaintenanceMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            MaintenanceMsg::Check(game_id, repair) => {
                if self
                    .play
                    .as_ref()
                    .is_some_and(|p| p.running && p.game_id == game_id)
                {
                    self.notify_error("The game is running.".into());
                    return Task::none();
                }
                let view = self.maintenance.entry(game_id.clone()).or_default();
                view.busy = true;
                view.lines = vec![
                    if repair {
                        "Repairing…"
                    } else {
                        "Checking…"
                    }
                    .into(),
                ];
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        slatty_core::maintenance::check(
                            &core.db,
                            &core.dirs,
                            &core.http,
                            &tokens,
                            &id,
                            repair,
                            &|_| {},
                            CancellationToken::new(),
                        )
                        .await
                        .map_err(err)
                    },
                    move |r| {
                        Message::Maintenance(MaintenanceMsg::Checked(game_id.clone(), repair, r))
                    },
                );
            }
            MaintenanceMsg::Checked(game_id, repair, result) => {
                let view = self.maintenance.entry(game_id).or_default();
                view.busy = false;
                view.lines = match result {
                    Ok(bad) if bad.is_empty() => vec!["All files are intact.".into()],
                    Ok(bad) => std::iter::once(format!(
                        "{} file(s) {}:",
                        bad.len(),
                        if repair {
                            "repaired"
                        } else {
                            "missing or damaged"
                        }
                    ))
                    .chain(bad.iter().take(20).map(|b| format!("  {}", b.display())))
                    .collect(),
                    Err(e) => vec![format!("Error: {e}")],
                };
            }
            MaintenanceMsg::AskUninstall(game_id) => {
                self.maintenance
                    .entry(game_id)
                    .or_default()
                    .confirm_uninstall = true;
            }
            MaintenanceMsg::CancelUninstall(game_id) => {
                self.maintenance
                    .entry(game_id)
                    .or_default()
                    .confirm_uninstall = false;
            }
            MaintenanceMsg::Uninstall(game_id, delete_prefix) => {
                if self
                    .play
                    .as_ref()
                    .is_some_and(|p| p.running && p.game_id == game_id)
                {
                    self.notify_error("The game is running.".into());
                    return Task::none();
                }
                let view = self.maintenance.entry(game_id.clone()).or_default();
                view.busy = true;
                view.confirm_uninstall = false;
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            slatty_core::maintenance::uninstall(
                                &core.db,
                                &core.dirs,
                                &id,
                                delete_prefix,
                            )
                        })
                        .await
                        .map_err(err)?
                        .map_err(err)
                        .map(|r| {
                            let mut lines = vec![format!("{} file(s) deleted.", r.removed_files)];
                            if !r.kept.is_empty() {
                                lines.push(format!(
                                    "{} file(s) not installed by slatty kept in the game folder.",
                                    r.kept.len()
                                ));
                            }
                            if let Some(b) = r.prefix_backup {
                                lines.push(format!(
                                    "Prefix user folder backed up to {}",
                                    b.display()
                                ));
                            }
                            lines
                        })
                    },
                    move |r| Message::Maintenance(MaintenanceMsg::Uninstalled(game_id.clone(), r)),
                );
            }
            MaintenanceMsg::Uninstalled(game_id, Ok(lines)) => {
                self.installs.remove(&game_id);
                self.cloud.remove(&game_id);
                self.maintenance.remove(&game_id);
                self.notice = Some(crate::Notice {
                    error: false,
                    text: format!("Uninstalled. {}", lines.join(" ")),
                });
            }
            MaintenanceMsg::Uninstalled(game_id, Err(e)) => {
                let view = self.maintenance.entry(game_id).or_default();
                view.busy = false;
                view.lines = vec![format!("Uninstall refused: {e}")];
            }
        }
        Task::none()
    }
}
