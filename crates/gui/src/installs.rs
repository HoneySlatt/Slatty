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
                    self.notify_error("Une installation est déjà en cours.".into());
                    return Task::none();
                }
                let Some(proton) = self.proton.clone() else {
                    self.notify_error(
                        "Choisissez d'abord une version de Proton dans les paramètres.".into(),
                    );
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
                    self.notify_error("Le dossier des jeux doit être un chemin absolu.".into());
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
