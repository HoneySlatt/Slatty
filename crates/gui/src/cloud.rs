//! Cloud saves of an installed game: check, sync, and the choice that settles a conflict.

use iced::Task;
use slatty_core::cloud::{
    self,
    plan::Action,
    sync::{Prefer, SyncOptions},
};
use slatty_core::install::Install;

use crate::work::tokens_for;
use crate::{App, Core, Message, err, ui};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudStatus {
    UpToDate,
    Pending(usize),
    Conflict,
    NoCloud,
    Problem,
}

#[derive(Default)]
pub struct CloudView {
    /// Notes about the game as a whole (no cloud saves, an error).
    pub lines: Vec<String>,
    pub locations: Vec<SaveLocation>,
    pub conflicts: bool,
    pub busy: bool,
    pub status: Option<CloudStatus>,
    /// What the last sync did, kept across the checks that follow it.
    pub last_sync: Option<String>,
}

/// One save folder of the game, as the last check or sync left it.
#[derive(Debug, Clone)]
pub struct SaveLocation {
    pub name: String,
    pub folder: String,
    /// What a sync would do; known after a check.
    pub counts: Option<Counts>,
    /// Warnings, conflicts and problems.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub upload: usize,
    pub download: usize,
    pub compare: usize,
    pub unchanged: usize,
    /// Deleted on one side, kept on the other until deletions are allowed.
    pub deleted: usize,
}

#[derive(Debug, Clone)]
pub struct CloudResult {
    pub lines: Vec<String>,
    pub locations: Vec<SaveLocation>,
    pub conflicts: bool,
    pub status: CloudStatus,
    /// For a sync: what it did.
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudRequest {
    Check,
    Sync,
    Keep(Prefer),
}

impl App {
    pub fn request_cloud(&mut self, game_id: String, request: CloudRequest) -> Task<Message> {
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            return Task::none();
        };
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
        self.cloud.entry(game_id.clone()).or_default().busy = true;
        self.account_task(Task::perform(
            cloud_task(core, user_id, install, request),
            move |r| Message::CloudDone(game_id.clone(), request, r),
        ))
    }

    /// A sync is followed by a check, so the drawer shows where things stand afterwards.
    pub fn cloud_done(
        &mut self,
        game_id: String,
        request: CloudRequest,
        result: Result<CloudResult, String>,
    ) -> Task<Message> {
        let last_sync = self.cloud.get(&game_id).and_then(|c| c.last_sync.clone());
        let synced = request != CloudRequest::Check && result.is_ok();
        let view = match result {
            Ok(r) => CloudView {
                lines: r.lines,
                locations: r.locations,
                conflicts: r.conflicts,
                busy: false,
                status: Some(r.status),
                last_sync: r.summary.or(last_sync),
            },
            Err(e) => CloudView {
                lines: vec![format!("Error: {e}")],
                status: Some(CloudStatus::Problem),
                last_sync,
                ..Default::default()
            },
        };
        let conflicts = view.conflicts;
        self.cloud.insert(game_id.clone(), view);
        if synced && !conflicts {
            return self.request_cloud(game_id, CloudRequest::Check);
        }
        Task::none()
    }
}

async fn cloud_task(
    core: Core,
    user_id: String,
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
    let tokens = tokens_for(&core, &user_id).await?;
    let Some(outcomes) =
        cloud::sync_game(&core.db, &core.dirs, &core.http, &tokens, &install, opts)
            .await
            .map_err(err)?
    else {
        return Ok(CloudResult {
            lines: vec!["GOG has no cloud saves for this game.".into()],
            locations: Vec::new(),
            conflicts: false,
            status: CloudStatus::NoCloud,
            summary: None,
        });
    };
    let mut locations = Vec::new();
    let mut conflicts = false;
    let mut pending = 0;
    let mut problem = false;
    let (mut uploaded, mut downloaded) = (0, 0);
    for o in &outcomes {
        let folder = o
            .root
            .as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_else(|| o.template.clone());
        let mut location = SaveLocation {
            name: o.name.clone(),
            folder,
            counts: None,
            notes: Vec::new(),
        };
        match &o.result {
            Err(e) => {
                problem = true;
                location.notes.push(format!("Error: {e}"));
            }
            Ok(r) => {
                location.notes.extend(
                    r.plan
                        .warnings
                        .iter()
                        .map(|w| ui::format::describe_warning(*w).to_string()),
                );
                if opts.dry_run {
                    let p = &r.plan;
                    let counts = Counts {
                        upload: p.count(Action::Upload),
                        download: p.count(Action::Download),
                        compare: p.count(Action::Compare),
                        unchanged: p.count(Action::Keep),
                        deleted: p.count(Action::DeleteRemote) + p.count(Action::DeleteLocal),
                    };
                    pending += counts.upload + counts.download + counts.compare;
                    location.counts = Some(counts);
                    for (path, _) in p.conflicts() {
                        location.notes.push(format!("Conflict: {path}"));
                        conflicts = true;
                    }
                } else {
                    uploaded += r.uploaded.len();
                    downloaded += r.downloaded.len();
                    for (path, _) in &r.conflicts {
                        location.notes.push(format!("Conflict: {path}"));
                        conflicts = true;
                    }
                    problem |= !r.refused.is_empty() || !r.errors.is_empty();
                    location.notes.extend(
                        r.refused
                            .iter()
                            .chain(&r.errors)
                            .map(|(p, e)| format!("Problem with {p}: {e}")),
                    );
                    location.notes.extend(
                        r.pending_deletions
                            .iter()
                            .map(|p| format!("Deletion not applied: {p}")),
                    );
                    if let Some(dir) = &r.backup_dir {
                        location
                            .notes
                            .push(format!("Previous versions: {}", dir.display()));
                    }
                }
            }
        }
        locations.push(location);
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
        lines: Vec::new(),
        locations,
        conflicts,
        status,
        summary: (!opts.dry_run)
            .then(|| format!("Last sync: {uploaded} file(s) uploaded, {downloaded} downloaded.")),
    })
}
