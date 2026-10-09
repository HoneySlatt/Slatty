use std::path::PathBuf;
use std::time::Instant;

use reqwest::Client;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::account::Account;
use crate::achievements::{self, Achievement};
use crate::auth::Tokens;
use crate::cloud::{self, LocationOutcome, sync::SyncOptions};
use crate::comet::Comet;
use crate::db::Db;
use crate::doctor::find_in_path;
use crate::error::{Error, Result};
use crate::galaxy_service;
use crate::install::Install;
use crate::paths::Dirs;
use crate::runner;
use crate::session::{self, SessionHandle, SessionOutcome, SupervisorEvent};
use crate::setup::SetupEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudSummary {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: Vec<String>,
    pub problems: Vec<String>,
    pub pending_deletions: Vec<String>,
    pub backup_dir: Option<PathBuf>,
}

impl CloudSummary {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.problems.is_empty()
    }

    fn from_outcomes(outcomes: &[LocationOutcome]) -> Self {
        let mut s = CloudSummary::default();
        for o in outcomes {
            match &o.result {
                Ok(r) => {
                    s.uploaded += r.uploaded.len();
                    s.downloaded += r.downloaded.len();
                    s.conflicts.extend(
                        r.conflicts
                            .iter()
                            .map(|(p, k)| format!("{}/{p} ({k:?})", o.name)),
                    );
                    s.problems.extend(
                        r.refused
                            .iter()
                            .chain(&r.errors)
                            .map(|(p, e)| format!("{}/{p}: {e}", o.name)),
                    );
                    s.pending_deletions.extend(
                        r.pending_deletions
                            .iter()
                            .map(|p| format!("{}/{p}", o.name)),
                    );
                    s.backup_dir = s.backup_dir.take().or_else(|| r.backup_dir.clone());
                }
                Err(e) => s.problems.push(format!("{}: {e}", o.name)),
            }
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayEvent {
    /// First launch of a fresh install: the Wine prefix is being created.
    PreparingPrefix,
    /// Post-install setup (GOG scripts, redistributables), once per installed build.
    SetupStep(String),
    SetupWarning(String),
    /// Setup could not run now (offline, error); it is retried at the next launch.
    SetupSkipped(String),
    CloudChecked(CloudSummary),
    /// Cloud not checked; the reason is shown and local saves are kept.
    CloudSkipped(String),
    /// Conflicts or errors before launch: the game is not started.
    Blocked(CloudSummary),
    CometReady,
    CometUnavailable(String),
    Started {
        pid: u32,
    },
    LauncherExited {
        code: Option<i32>,
    },
    StopRequested,
    Ended {
        seconds: u64,
        code: Option<i32>,
        clean: bool,
    },
    CloudUploaded(CloudSummary),
    CloudUploadSkipped(String),
    /// Minutes of play sent to GOG (this session and any earlier one not sent yet).
    PlaytimeReported(i64),
    /// Play time not sent now; it is retried after the next session.
    PlaytimeNotReported(String),
    Unlocked(Vec<String>),
    NoNewAchievement,
    AchievementsUnknown,
}

pub struct PlayRequest {
    pub game_id: String,
    pub cloud: bool,
    pub comet: bool,
    /// Executable that understands `session::SUPERVISE_ARG`.
    pub supervisor: PathBuf,
}

/// Full play flow: cloud check, Comet, supervised session, cloud upload, achievement diff.
/// Each message on `stop` asks the game to quit; the second one forces it.
pub async fn play(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    req: PlayRequest,
    emit: impl Fn(PlayEvent) + Send + Sync,
    mut stop: UnboundedReceiver<()>,
) -> Result<()> {
    let _busy = crate::lock::game(dirs, &req.game_id)?;
    let mut install = Install::get(db, &req.game_id)?
        .ok_or_else(|| Error::NotFound(format!("{} is not imported", req.game_id)))?;
    crate::umu::resolve(db, http, &mut install).await;
    if crate::maintenance::update_pending(db, &req.game_id)?.is_some() {
        return Err(Error::Refused(
            "an update of this game is unfinished; finish it before playing".into(),
        ));
    }
    let spec = runner::launch_spec(&install)?;
    let user_id = Account::active(db)?.map(|a| a.user_id);

    if let Some(init) = runner::prefix_init_spec(&install)? {
        emit(PlayEvent::PreparingPrefix);
        let log = dirs.logs().join(format!("prefix-{}.log", install.game_id));
        let outcome = SessionHandle::start(&req.supervisor, &init, &log)
            .await?
            .wait()
            .await?;
        if !outcome.clean
            || !install
                .runner
                .prefix()
                .is_some_and(|p| p.join("drive_c/users").is_dir())
        {
            return Err(Error::Refused(format!(
                "the Wine prefix could not be created; see {}",
                log.display()
            )));
        }
    }

    if crate::setup::pending(dirs, &install.game_id)? {
        let result = async {
            let tokens = tokens(db, dirs, http).await?;
            let forward = |e: SetupEvent| {
                emit(match e {
                    SetupEvent::Downloading => {
                        PlayEvent::SetupStep("downloading setup files".into())
                    }
                    SetupEvent::Running(label) => PlayEvent::SetupStep(label),
                    SetupEvent::NonZeroExit { label, code } => {
                        PlayEvent::SetupWarning(format!("{label} exited with code {code:?}"))
                    }
                })
            };
            crate::setup::run(
                dirs,
                http,
                &tokens,
                &install,
                &req.supervisor,
                false,
                &forward,
            )
            .await
        }
        .await;
        if let Err(e) = result {
            emit(PlayEvent::SetupSkipped(e.to_string()));
        }
    }

    if req.comet
        && install
            .runner
            .prefix()
            .is_some_and(|p| !galaxy_service::installed(p))
    {
        match galaxy_service::find(dirs) {
            Some(exe) => {
                emit(PlayEvent::SetupStep(
                    "registering the Galaxy service".into(),
                ));
                if let Err(e) = galaxy_service::ensure(dirs, &install, &exe, &req.supervisor).await
                {
                    emit(PlayEvent::SetupWarning(format!(
                        "Galaxy service not registered: {e}"
                    )));
                }
            }
            None => emit(PlayEvent::SetupWarning(format!(
                "GalaxyCommunication.exe not found (see {}); some games will not report achievements",
                galaxy_service::ENV
            ))),
        }
    }

    if req.cloud {
        match cloud_sync(db, dirs, http, &install).await {
            Ok(Some(summary)) if summary.is_clean() => emit(PlayEvent::CloudChecked(summary)),
            Ok(Some(summary)) => {
                emit(PlayEvent::Blocked(summary));
                return Ok(());
            }
            Ok(None) => emit(PlayEvent::CloudSkipped(
                "GOG has no cloud saves for this game".into(),
            )),
            Err(e) => emit(PlayEvent::CloudSkipped(e.to_string())),
        }
    }

    let comet = if req.comet {
        match start_comet(db, dirs, http, &install).await {
            Ok(started) => {
                emit(PlayEvent::CometReady);
                Some(started)
            }
            Err(e) => {
                emit(PlayEvent::CometUnavailable(e.to_string()));
                None
            }
        }
    } else {
        None
    };

    let log = dirs.logs().join(format!("game-{}.log", install.game_id));
    let started = Instant::now();
    let outcome = run_session(db, &req, &spec, &log, user_id.as_deref(), &emit, &mut stop).await;
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            if let Some((comet, _)) = comet {
                let _ = comet.stop().await;
            }
            return Err(e);
        }
    };
    emit(PlayEvent::Ended {
        seconds: started.elapsed().as_secs(),
        code: outcome.main_code,
        clean: outcome.clean,
    });

    let before = match comet {
        Some((comet, before)) => {
            comet.stop().await?;
            Some(before)
        }
        None => None,
    };

    if req.cloud {
        if !outcome.clean {
            emit(PlayEvent::CloudUploadSkipped(
                "the end of the session is uncertain".into(),
            ));
        } else {
            match cloud_sync(db, dirs, http, &install).await {
                Ok(Some(summary)) => emit(PlayEvent::CloudUploaded(summary)),
                Ok(None) => {}
                Err(e) => emit(PlayEvent::CloudUploadSkipped(e.to_string())),
            }
        }
    }

    if outcome.clean {
        match report_playtime(db, dirs, http, user_id.as_deref()).await {
            Ok(0) => {}
            Ok(minutes) => emit(PlayEvent::PlaytimeReported(minutes)),
            Err(e) => emit(PlayEvent::PlaytimeNotReported(e.to_string())),
        }
    }

    if let Some(before) = before {
        emit(
            match (before, achievements_now(db, dirs, http, &install).await) {
                (Some(before), Some(after)) => {
                    let new: Vec<String> = achievements::newly_unlocked(&before, &after)
                        .iter()
                        .map(|a| a.name.clone())
                        .collect();
                    if new.is_empty() {
                        PlayEvent::NoNewAchievement
                    } else {
                        PlayEvent::Unlocked(new)
                    }
                }
                _ => PlayEvent::AchievementsUnknown,
            },
        );
    }
    Ok(())
}

async fn run_session(
    db: &Db,
    req: &PlayRequest,
    spec: &runner::LaunchSpec,
    log: &std::path::Path,
    user_id: Option<&str>,
    emit: &(impl Fn(PlayEvent) + Send + Sync),
    stop: &mut UnboundedReceiver<()>,
) -> Result<SessionOutcome> {
    let mut handle = SessionHandle::start(&req.supervisor, spec, log).await?;
    match handle.next_event().await? {
        Some(SupervisorEvent::Started { pid }) => emit(PlayEvent::Started { pid }),
        Some(SupervisorEvent::Failed { message }) => return Err(Error::Refused(message)),
        other => return Err(Error::parse("session supervisor", format!("{other:?}"))),
    }
    let id = session::record_start(db, &req.game_id, user_id)?;
    let mut stop_open = true;
    let outcome = loop {
        tokio::select! {
            event = handle.next_event() => match event? {
                Some(SupervisorEvent::MainExited { code }) => emit(PlayEvent::LauncherExited { code }),
                Some(SupervisorEvent::Ended { main_code }) => break SessionOutcome { main_code, clean: true },
                Some(SupervisorEvent::Failed { message }) => {
                    session::record_end(db, id, &SessionOutcome { main_code: None, clean: false })?;
                    return Err(Error::Refused(message));
                }
                Some(SupervisorEvent::Started { .. }) => {}
                None => break SessionOutcome { main_code: None, clean: false },
            },
            request = stop.recv(), if stop_open => match request {
                Some(()) => {
                    emit(PlayEvent::StopRequested);
                    handle.request_stop();
                }
                None => stop_open = false,
            },
        }
    };
    session::record_end(db, id, &outcome)?;
    Ok(outcome)
}

async fn cloud_sync(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
) -> Result<Option<CloudSummary>> {
    let tokens = tokens(db, dirs, http).await?;
    let outcomes =
        cloud::sync_held(db, dirs, http, &tokens, install, SyncOptions::default()).await?;
    Ok(outcomes.map(|o| CloudSummary::from_outcomes(&o)))
}

async fn report_playtime(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    user_id: Option<&str>,
) -> Result<i64> {
    let Some(user_id) = user_id else {
        return Ok(0);
    };
    if crate::playtime::unreported(db, user_id)?.is_empty() {
        return Ok(0);
    }
    let tokens = tokens(db, dirs, http).await?;
    crate::playtime::report_pending(db, http, &tokens).await
}

async fn tokens(db: &Db, dirs: &Dirs, http: &Client) -> Result<Tokens> {
    let mut account = Account::load(db, dirs).await?;
    Ok(account.tokens(http).await?.clone())
}

async fn start_comet(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
) -> Result<(Comet, Option<Vec<Achievement>>)> {
    let bin =
        find_in_path("comet").ok_or_else(|| Error::NotFound("`comet` is not in PATH".into()))?;
    let mut account = Account::load(db, dirs).await?;
    let tokens = account.tokens(http).await?.clone();
    let comet = Comet::start(&bin, &tokens, &account.info.username, dirs).await?;
    let before = achievements_now(db, dirs, http, install).await;
    Ok((comet, before))
}

async fn achievements_now(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
) -> Option<Vec<Achievement>> {
    let tokens = tokens(db, dirs, http).await.ok()?;
    let (client_id, token) = achievements::game_token(http, &tokens, install)
        .await
        .ok()?;
    achievements::fetch(http, &tokens.user_id, &client_id, &token)
        .await
        .ok()
}

/// Sessions without a recorded end (launcher crash or kill) are marked as interrupted.
pub fn recover_unfinished(db: &Db) -> Result<Vec<session::SessionRecord>> {
    let unfinished = session::unfinished(db)?;
    for s in &unfinished {
        session::mark_interrupted(db, s.id)?;
    }
    Ok(unfinished)
}
