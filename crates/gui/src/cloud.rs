//! Cloud saves of an installed game: check, sync, and the choice that settles a conflict.

use iced::Task;
use slatty_core::cloud::{
    self,
    plan::Action,
    sync::{Prefer, SyncOptions},
};
use slatty_core::install::Install;

use crate::work::tokens;
use crate::{App, Core, Message, err, view};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudStatus {
    UpToDate,
    Pending(usize),
    Conflict,
    NoCloud,
    Problem,
}

pub struct CloudView {
    pub lines: Vec<String>,
    pub conflicts: bool,
    pub busy: bool,
    pub status: Option<CloudStatus>,
}

#[derive(Debug, Clone)]
pub struct CloudResult {
    pub lines: Vec<String>,
    pub conflicts: bool,
    pub status: CloudStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudRequest {
    Check,
    Sync,
    Keep(Prefer),
}

impl App {
    pub fn request_cloud(&mut self, game_id: String, request: CloudRequest) -> Task<Message> {
        let (Some(core), Some(install)) = (self.core.clone(), self.installs.get(&game_id).cloned())
        else {
            return Task::none();
        };
        if self
            .play
            .as_ref()
            .is_some_and(|p| p.running && p.game_id == game_id)
        {
            self.notify_error("The game is running; sync once the session has ended.".into());
            return Task::none();
        }
        self.cloud
            .entry(game_id.clone())
            .or_insert(CloudView {
                lines: Vec::new(),
                conflicts: false,
                busy: true,
                status: None,
            })
            .busy = true;
        Task::perform(cloud_task(core, install, request), move |r| {
            Message::CloudDone(game_id.clone(), request, r)
        })
    }

    pub fn cloud_done(&mut self, game_id: String, result: Result<CloudResult, String>) {
        let view = match result {
            Ok(r) => CloudView {
                lines: r.lines,
                conflicts: r.conflicts,
                busy: false,
                status: Some(r.status),
            },
            Err(e) => CloudView {
                lines: vec![format!("Error: {e}")],
                conflicts: false,
                busy: false,
                status: Some(CloudStatus::Problem),
            },
        };
        self.cloud.insert(game_id, view);
    }
}

async fn cloud_task(
    core: Core,
    install: Install,
    request: CloudRequest,
) -> Result<CloudResult, String> {
    let opts = match request {
        CloudRequest::Check => SyncOptions {
            dry_run: true,
            ..Default::default()
        },
        CloudRequest::Sync => SyncOptions::default(),
        CloudRequest::Keep(side) => SyncOptions {
            prefer: Some(side),
            ..Default::default()
        },
    };
    let tokens = tokens(&core).await?;
    let Some(outcomes) =
        cloud::sync_game(&core.db, &core.dirs, &core.http, &tokens, &install, opts)
            .await
            .map_err(err)?
    else {
        return Ok(CloudResult {
            lines: vec!["GOG has no cloud saves for this game.".into()],
            conflicts: false,
            status: CloudStatus::NoCloud,
        });
    };
    let mut lines = Vec::new();
    let mut conflicts = false;
    let mut pending = 0;
    let mut problem = false;
    for o in &outcomes {
        let root = o
            .root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_else(|| o.template.clone());
        lines.push(format!("[{}] {root}", o.name));
        match &o.result {
            Err(e) => {
                problem = true;
                lines.push(format!("  error: {e}"));
            }
            Ok(r) => {
                lines.extend(
                    r.plan
                        .warnings
                        .iter()
                        .map(|w| format!("  warning: {}", view::describe_warning(*w))),
                );
                if opts.dry_run {
                    let p = &r.plan;
                    pending += p.count(Action::Upload) + p.count(Action::Download);
                    lines.push(format!(
                        "  to upload {} · to download {} · to compare {} · unchanged {}",
                        p.count(Action::Upload),
                        p.count(Action::Download),
                        p.count(Action::Compare),
                        p.count(Action::Keep)
                    ));
                    for (path, _) in p.conflicts() {
                        lines.push(format!("  conflict: {path}"));
                        conflicts = true;
                    }
                } else {
                    lines.push(format!(
                        "  uploaded {} · downloaded {}",
                        r.uploaded.len(),
                        r.downloaded.len()
                    ));
                    for (path, _) in &r.conflicts {
                        lines.push(format!("  conflict: {path}"));
                        conflicts = true;
                    }
                    problem |= !r.refused.is_empty() || !r.errors.is_empty();
                    lines.extend(
                        r.refused
                            .iter()
                            .chain(&r.errors)
                            .map(|(p, e)| format!("  problem {p}: {e}")),
                    );
                    lines.extend(
                        r.pending_deletions
                            .iter()
                            .map(|p| format!("  deletion not applied: {p}")),
                    );
                    if let Some(dir) = &r.backup_dir {
                        lines.push(format!("  previous versions: {}", dir.display()));
                    }
                }
            }
        }
    }
    let status = if conflicts {
        CloudStatus::Conflict
    } else if problem {
        CloudStatus::Problem
    } else if pending > 0 {
        CloudStatus::Pending(pending)
    } else {
        CloudStatus::UpToDate
    };
    Ok(CloudResult {
        lines,
        conflicts,
        status,
    })
}
