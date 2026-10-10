//! Settings → Runners: Proton builds downloaded from GitHub.

use std::path::PathBuf;

use iced::Task;
use slatty_core::protons::{self, Release, Source, Stage};
use tokio_util::sync::CancellationToken;

use crate::work::{paused_or, progress_stream};
use crate::{App, Message, err};

#[derive(Debug, Default)]
pub struct RunnersView {
    /// Whether GitHub may be asked, off until turned on.
    pub downloads: bool,
    /// Whether the builds games follow as `-latest` are updated at start, off until turned on.
    pub updates: bool,
    pub checking: bool,
    pub updating: bool,
    /// The newest build of each project, once asked for.
    pub releases: Vec<(Source, Result<Release, String>)>,
    /// The build being downloaded, how far it is, and what stops it.
    pub installing: Option<(String, Option<Stage>, CancellationToken)>,
    /// The builds SlattyLauncher downloaded.
    pub downloaded: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum RunnersMsg {
    Downloads(bool),
    Check,
    Checked(Vec<(Source, Result<Release, String>)>),
    Install(Release),
    /// How far the download of the build named is.
    Progress(String, Stage),
    /// `Err(None)` when the download was stopped.
    Installed(Result<PathBuf, Option<String>>),
    Updates(bool),
    /// Downloads the newest build of each project games follow as `-latest`.
    Update,
    /// The builds an update installed; `Err(None)` when it was stopped.
    Updated(Result<Vec<Release>, Option<String>>),
    Cancel,
    Remove(PathBuf),
    Removed(Result<(), String>),
    /// Every Proton build found, and those downloaded, once one was added or deleted.
    Listed(Vec<PathBuf>, Vec<PathBuf>),
}

impl App {
    pub fn update_runners(&mut self, msg: RunnersMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let view = &mut self.runners;
        match msg {
            RunnersMsg::Downloads(on) => {
                view.downloads = on;
                if !on {
                    view.releases.clear();
                }
                if let Err(e) = slatty_core::settings::set_proton_downloads(&core.db, on) {
                    self.notify_error(e.to_string());
                }
            }
            RunnersMsg::Check => {
                if view.checking {
                    return Task::none();
                }
                if let Err(e) = protons::check_allowed(&core.db) {
                    self.notify_error(e.to_string());
                    return Task::none();
                }
                view.checking = true;
                return Task::perform(
                    async move {
                        let mut found = Vec::new();
                        for source in Source::ALL {
                            let release = protons::latest(&core.http, protons::API, source).await;
                            found.push((source, release.map_err(err)));
                        }
                        found
                    },
                    |found| Message::Runners(RunnersMsg::Checked(found)),
                );
            }
            RunnersMsg::Checked(found) => {
                view.checking = false;
                view.releases = found;
            }
            RunnersMsg::Install(release) => {
                if view.installing.is_some() {
                    return Task::none();
                }
                let cancel = CancellationToken::new();
                view.installing = Some((release.name.clone(), None, cancel.clone()));
                let name = release.name.clone();
                return Task::run(
                    progress_stream(
                        async move |throttle| {
                            let report = move |stage| match stage {
                                Stage::Verifying => throttle.report_now(stage),
                                _ => throttle.report(stage),
                            };
                            let result =
                                protons::install(&core.http, &core.dirs, &release, report, &cancel)
                                    .await
                                    .map_err(paused_or);
                            Message::Runners(RunnersMsg::Installed(result))
                        },
                        move |stage| Message::Runners(RunnersMsg::Progress(name.clone(), stage)),
                    ),
                    |m| m,
                );
            }
            RunnersMsg::Progress(name, stage) => {
                if let Some((current, at, _)) = &mut view.installing {
                    *current = name;
                    *at = Some(stage);
                }
            }
            RunnersMsg::Updates(on) => {
                view.updates = on;
                if let Err(e) = slatty_core::settings::set_proton_updates(&core.db, on) {
                    self.notify_error(e.to_string());
                }
            }
            RunnersMsg::Update => {
                if view.installing.is_some() || view.updating {
                    return Task::none();
                }
                let cancel = CancellationToken::new();
                view.updating = true;
                view.installing = Some((String::new(), None, cancel.clone()));
                return Task::run(
                    progress_stream(
                        async move |throttle| {
                            let report = move |name: &str, stage| {
                                let progress = (name.to_string(), stage);
                                match stage {
                                    Stage::Verifying => throttle.report_now(progress),
                                    _ => throttle.report(progress),
                                }
                            };
                            let result = protons::update(
                                &core.db,
                                &core.http,
                                &core.dirs,
                                protons::API,
                                report,
                                &cancel,
                            )
                            .await
                            .map_err(paused_or);
                            Message::Runners(RunnersMsg::Updated(result))
                        },
                        |(name, stage)| Message::Runners(RunnersMsg::Progress(name, stage)),
                    ),
                    |m| m,
                );
            }
            RunnersMsg::Updated(result) => {
                view.updating = false;
                view.installing = None;
                match result {
                    Ok(updated) if updated.is_empty() => {}
                    Ok(updated) => {
                        let names: Vec<&str> = updated.iter().map(|r| r.name.as_str()).collect();
                        self.notice = Some(crate::Notice {
                            error: false,
                            text: format!("Proton updated: {}.", names.join(", ")),
                        });
                        return list(&core);
                    }
                    Err(Some(e)) => self.notify_error(format!("Proton update: {e}")),
                    Err(None) => {}
                }
            }
            RunnersMsg::Installed(result) => {
                view.installing = None;
                match result {
                    Ok(_) => return list(&core),
                    Err(Some(e)) => self.notify_error(e),
                    Err(None) => {}
                }
            }
            RunnersMsg::Cancel => {
                if let Some((_, _, cancel)) = &view.installing {
                    cancel.cancel();
                }
            }
            RunnersMsg::Remove(path) => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            protons::remove(&core.db, &core.dirs, &path)
                        })
                        .await
                        .map_err(err)?
                        .map_err(err)
                    },
                    |r| Message::Runners(RunnersMsg::Removed(r)),
                );
            }
            RunnersMsg::Removed(Ok(())) => return list(&core),
            RunnersMsg::Removed(Err(e)) => self.notify_error(e),
            RunnersMsg::Listed(all, downloaded) => {
                self.proton_choices = all;
                view.downloaded = downloaded;
            }
        }
        Task::none()
    }
}

/// Lists the Proton builds again, off the interface thread: Steam libraries can be on slow drives.
fn list(core: &crate::Core) -> Task<Message> {
    let dirs = core.dirs.clone();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                (
                    slatty_core::settings::proton_candidates(&dirs),
                    protons::installed(&dirs),
                )
            })
            .await
            .unwrap_or_default()
        },
        |(all, downloaded)| Message::Runners(RunnersMsg::Listed(all, downloaded)),
    )
}
