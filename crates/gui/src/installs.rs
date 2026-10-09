use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::Task;
use iced::futures::{SinkExt, Stream};
use slatty_core::account::Account;
use slatty_core::install::Install;
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest, Progress,
};
use slatty_core::maintenance::Change;
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
    /// Folder the game goes into, as typed; the game gets its own subfolder there.
    pub root: String,
    pub directory: String,
    pub dependencies: Vec<String>,
    pub resumable: bool,
    pub dlcs: Vec<DlcChoice>,
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
    Event(String, InstallEvent),
    /// `Err(None)` means paused by the user.
    Done(String, Result<Install, Option<String>>),
    Pause(String),
    RootInput(String, String),
    Browse(String),
    Browsed(String, Option<PathBuf>),
    ToggleDlc(String, String),
    Discard(String),
    Discarded(String, Result<(), String>),
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
                let root = match self.install_views.get(&game_id) {
                    Some(InstallView::Ready(info)) => PathBuf::from(info.root.trim()),
                    _ => PathBuf::from(&self.library_root),
                };
                self.install_views
                    .insert(game_id.clone(), InstallView::Planning);
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
            InstallMsg::Event(game_id, InstallEvent::Progress(p)) => {
                if let Some(InstallView::Running { progress, .. }) =
                    self.install_views.get_mut(&game_id)
                {
                    *progress = p;
                }
            }
            InstallMsg::Event(_, _) => {}
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
                            installer::discard(&core.db, &id).map(drop)
                        })
                        .await
                        .map_err(err)?
                        .map_err(err)
                    },
                    move |r| Message::Install(InstallMsg::Discarded(game_id.clone(), r)),
                );
            }
            InstallMsg::Discarded(game_id, result) => {
                self.install_views.remove(&game_id);
                match result {
                    Ok(()) => self.forget_interrupted(&game_id),
                    Err(e) => self.notify_error(e),
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
                    move |picked| Message::Install(InstallMsg::Browsed(game_id.clone(), picked)),
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
                self.note_interrupted(&game_id, crate::Interrupted::Download);
                return self.update_install(InstallMsg::Prepare(game_id, None));
            }
            InstallMsg::Done(game_id, Err(Some(e))) => {
                self.note_interrupted(&game_id, crate::Interrupted::Download);
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
        b if b >= 1 << 30 => format!("{:.2} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KiB", b >> 10),
    }
}

#[derive(Default)]
pub struct MaintenanceView {
    pub busy: bool,
    pub lines: Vec<String>,
    pub confirm_uninstall: bool,
    pub update_available: bool,
    pub content: Option<ContentInfo>,
}

/// Language and DLC choices of an installed game, being edited.
#[derive(Debug, Clone)]
pub struct ContentInfo {
    pub language: String,
    pub languages: Vec<String>,
    pub chosen_language: String,
    pub dlcs: Vec<DlcChoice>,
    pub chosen_dlcs: Vec<String>,
}

impl ContentInfo {
    pub fn installed_dlcs(&self) -> Vec<String> {
        self.dlcs
            .iter()
            .filter(|d| d.selected)
            .map(|d| d.id.clone())
            .collect()
    }

    pub fn dlcs_changed(&self) -> bool {
        let mut a = self.installed_dlcs();
        let mut b = self.chosen_dlcs.clone();
        a.sort();
        b.sort();
        a != b
    }
}

#[derive(Debug, Clone)]
pub enum MaintenanceMsg {
    Check(String, bool),
    Checked(String, bool, Result<Vec<PathBuf>, String>),
    AskUninstall(String),
    CancelUninstall(String),
    Uninstall(String, bool),
    Uninstalled(String, Result<Vec<String>, String>),
    CheckUpdate(String),
    UpdateChecked(String, Result<Option<String>, String>),
    Apply(String, Change),
    Updated(String, Result<String, String>),
    LoadContent(String),
    ContentLoaded(String, Result<ContentInfo, String>),
    ChooseLanguage(String, String),
    ToggleContentDlc(String, String),
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
                        .map(|c| c.bad)
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
                self.records.remove(&game_id);
                self.panel = None;
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
            MaintenanceMsg::CheckUpdate(game_id) => {
                let view = self.maintenance.entry(game_id.clone()).or_default();
                view.busy = true;
                view.lines = vec!["Checking for updates…".into()];
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        if slatty_core::maintenance::update_pending(&core.db, &id)
                            .map_err(err)?
                            .is_some()
                        {
                            return Ok(Some("an unfinished update".to_string()));
                        }
                        slatty_core::maintenance::check_update(
                            &core.db, &core.dirs, &core.http, &tokens, &id,
                        )
                        .await
                        .map(|u| {
                            u.map(|u| format!("{} → {}", u.installed_version, u.available_version))
                        })
                        .map_err(err)
                    },
                    move |r| {
                        Message::Maintenance(MaintenanceMsg::UpdateChecked(game_id.clone(), r))
                    },
                );
            }
            MaintenanceMsg::UpdateChecked(game_id, result) => {
                let view = self.maintenance.entry(game_id).or_default();
                view.busy = false;
                view.update_available = matches!(result, Ok(Some(_)));
                view.lines = vec![match result {
                    Ok(Some(what)) => format!("Update available: {what}"),
                    Ok(None) => "Up to date.".into(),
                    Err(e) => format!("Error: {e}"),
                }];
            }
            MaintenanceMsg::Apply(game_id, change) => {
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
                view.update_available = false;
                view.content = None;
                view.lines =
                    vec!["Applying… the game cannot be launched until this finishes.".into()];
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        slatty_core::maintenance::reconfigure(
                            &core.db,
                            &core.dirs,
                            &core.http,
                            &tokens,
                            &id,
                            change,
                            &|_| {},
                            CancellationToken::new(),
                        )
                        .await
                        .map(|r| {
                            let mut summary = format!(
                                "{}Now at {}: {} file(s) downloaded, {} removed.",
                                if r.resumed {
                                    "An unfinished change was completed first. "
                                } else {
                                    ""
                                },
                                r.to_version,
                                r.downloaded.len(),
                                r.removed.len(),
                            );
                            if !r.patched.is_empty() {
                                summary += &format!(
                                    " {} file(s) rebuilt from GOG patches ({}).",
                                    r.patched.len(),
                                    human_size(r.patch_bytes)
                                );
                            }
                            if r.reused_bytes > 0 {
                                summary += &format!(
                                    " {} reused from installed files.",
                                    human_size(r.reused_bytes)
                                );
                            }
                            summary
                        })
                        .map_err(err)
                    },
                    move |r| Message::Maintenance(MaintenanceMsg::Updated(game_id.clone(), r)),
                );
            }
            MaintenanceMsg::LoadContent(game_id) => {
                let view = self.maintenance.entry(game_id.clone()).or_default();
                view.busy = true;
                view.lines = vec!["Reading languages and DLC…".into()];
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        let plan = slatty_core::maintenance::content_options(
                            &core.db, &core.dirs, &core.http, &tokens, &id,
                        )
                        .await
                        .map_err(err)?;
                        let chosen_dlcs = plan.selected_dlcs();
                        Ok(ContentInfo {
                            chosen_language: plan.language.clone(),
                            language: plan.language,
                            languages: plan.languages,
                            dlcs: plan.dlcs,
                            chosen_dlcs,
                        })
                    },
                    move |r| {
                        Message::Maintenance(MaintenanceMsg::ContentLoaded(game_id.clone(), r))
                    },
                );
            }
            MaintenanceMsg::ContentLoaded(game_id, result) => {
                let view = self.maintenance.entry(game_id).or_default();
                view.busy = false;
                match result {
                    Ok(info) => {
                        view.lines.clear();
                        view.content = Some(info);
                    }
                    Err(e) => view.lines = vec![format!("Error: {e}")],
                }
            }
            MaintenanceMsg::ChooseLanguage(game_id, language) => {
                if let Some(c) = self
                    .maintenance
                    .get_mut(&game_id)
                    .and_then(|v| v.content.as_mut())
                {
                    c.chosen_language = language;
                }
            }
            MaintenanceMsg::ToggleContentDlc(game_id, dlc) => {
                if let Some(c) = self
                    .maintenance
                    .get_mut(&game_id)
                    .and_then(|v| v.content.as_mut())
                    && c.dlcs.iter().any(|d| d.id == dlc && d.owned)
                {
                    match c.chosen_dlcs.iter().position(|d| *d == dlc) {
                        Some(i) => {
                            c.chosen_dlcs.remove(i);
                        }
                        None => c.chosen_dlcs.push(dlc),
                    }
                }
            }
            MaintenanceMsg::Updated(game_id, result) => {
                match &result {
                    Ok(_) => self.forget_interrupted(&game_id),
                    Err(_) => self.note_interrupted(&game_id, crate::Interrupted::Update),
                }
                self.refresh_record(&game_id);
                let view = self.maintenance.entry(game_id).or_default();
                view.busy = false;
                view.lines = vec![match result {
                    Ok(summary) => summary,
                    Err(e) => format!("Update failed: {e}. Run it again to resume."),
                }];
            }
        }
        Task::none()
    }
}
