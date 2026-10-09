//! Verify, repair, update, uninstall, and language or DLC changes of an installed game.

use std::path::PathBuf;

use iced::Task;
use slatty_core::installer::{DlcChoice, Progress};
use slatty_core::maintenance::Change;
use tokio_util::sync::CancellationToken;

use crate::ui::format::human_size;
use crate::work::{paused_or, progress_stream, tokens};
use crate::{App, Message, err};

#[derive(Default)]
pub struct MaintenanceView {
    pub busy: bool,
    pub lines: Vec<String>,
    pub confirm_uninstall: bool,
    pub update_available: bool,
    pub content: Option<ContentInfo>,
    /// Progress of a verify, repair or update, which `cancel` stops.
    pub progress: Option<Progress>,
    pub cancel: Option<CancellationToken>,
}

impl MaintenanceView {
    fn start(&mut self, line: &str) -> CancellationToken {
        let cancel = CancellationToken::new();
        self.busy = true;
        self.lines = vec![line.into()];
        self.progress = Some(Progress::default());
        self.cancel = Some(cancel.clone());
        cancel
    }

    fn finish(&mut self, lines: Vec<String>) {
        self.busy = false;
        self.lines = lines;
        self.progress = None;
        self.cancel = None;
    }
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
    /// `Err(None)` means stopped by the user.
    Checked(String, bool, Result<Vec<PathBuf>, Option<String>>),
    Progress(String, Progress),
    Pause(String),
    AskUninstall(String),
    CancelUninstall(String),
    Uninstall(String, bool),
    Uninstalled(String, Result<Vec<String>, String>),
    CheckUpdate(String),
    UpdateChecked(String, Result<Option<String>, String>),
    Apply(String, Change),
    /// `Err(None)` means paused by the user.
    Updated(String, Result<String, Option<String>>),
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
                let cancel =
                    self.maintenance
                        .entry(game_id.clone())
                        .or_default()
                        .start(if repair {
                            "Repairing…"
                        } else {
                            "Checking…"
                        });
                let id = game_id.clone();
                return Task::run(
                    progress_stream(
                        async move |throttle| {
                            let result = async {
                                let tokens = tokens(&core).await.map_err(Some)?;
                                slatty_core::maintenance::check(
                                    &core.db,
                                    &core.dirs,
                                    &core.http,
                                    &tokens,
                                    &game_id,
                                    repair,
                                    &|p| throttle.report(p),
                                    cancel,
                                )
                                .await
                                .map(|c| c.bad)
                                .map_err(paused_or)
                            }
                            .await;
                            Message::Maintenance(MaintenanceMsg::Checked(game_id, repair, result))
                        },
                        move |p| Message::Maintenance(MaintenanceMsg::Progress(id.clone(), p)),
                    ),
                    |m| m,
                );
            }
            MaintenanceMsg::Progress(game_id, p) => {
                if let Some(v) = self.maintenance.get_mut(&game_id)
                    && v.busy
                {
                    v.progress = Some(p);
                }
            }
            MaintenanceMsg::Pause(game_id) => {
                if let Some(cancel) = self
                    .maintenance
                    .get(&game_id)
                    .and_then(|v| v.cancel.as_ref())
                {
                    cancel.cancel();
                }
            }
            MaintenanceMsg::Checked(game_id, repair, result) => {
                let view = self.maintenance.entry(game_id).or_default();
                view.finish(match result {
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
                    Err(None) => vec!["Stopped.".into()],
                    Err(Some(e)) => vec![format!("Error: {e}")],
                });
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
                        let tokens = tokens(&core).await?;
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
                view.update_available = false;
                view.content = None;
                let cancel =
                    view.start("Applying… the game cannot be launched until this finishes.");
                let id = game_id.clone();
                return Task::run(
                    progress_stream(
                        async move |throttle| {
                            let result = async {
                                let tokens = tokens(&core).await.map_err(Some)?;
                                slatty_core::maintenance::reconfigure(
                                    &core.db,
                                    &core.dirs,
                                    &core.http,
                                    &tokens,
                                    &game_id,
                                    change,
                                    &|p| throttle.report(p),
                                    cancel,
                                )
                                .await
                                .map(|r| update_summary(&r))
                                .map_err(paused_or)
                            }
                            .await;
                            Message::Maintenance(MaintenanceMsg::Updated(game_id, result))
                        },
                        move |p| Message::Maintenance(MaintenanceMsg::Progress(id.clone(), p)),
                    ),
                    |m| m,
                );
            }
            MaintenanceMsg::LoadContent(game_id) => {
                let view = self.maintenance.entry(game_id.clone()).or_default();
                view.busy = true;
                view.lines = vec!["Reading languages and DLC…".into()];
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let tokens = tokens(&core).await?;
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
                    Err(_) => self.sync_interrupted(&game_id),
                }
                self.refresh_record(&game_id);
                let view = self.maintenance.entry(game_id).or_default();
                view.finish(vec![match result {
                    Ok(summary) => summary,
                    Err(None) => "Paused. Apply it again to resume.".into(),
                    Err(Some(e)) => format!("Update failed: {e}. Run it again to resume."),
                }]);
            }
        }
        Task::none()
    }
}

fn update_summary(r: &slatty_core::maintenance::UpdateReport) -> String {
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
}
