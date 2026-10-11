use std::path::PathBuf;
use std::time::{Duration, Instant};

use reqwest::Client;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::account::Account;
use crate::achievements::{self, Achievement, Unlock};
use crate::auth::Tokens;
use crate::cloud::{self, LocationOutcome, sync::SyncOptions};
use crate::comet::{self, Comet};
use crate::db::Db;
use crate::doctor::find_in_path;
use crate::error::{Error, Result};
use crate::galaxy_service;
use crate::install::{Install, Platform};
use crate::paths::Dirs;
use crate::runner;
use crate::session::{self, SessionHandle, SessionOutcome, SupervisorEvent};
use crate::setup::SetupEvent;

/// How often Comet's database is looked at for unlocks while a game runs.
const UNLOCKS_EVERY: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CloudSummary {
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: Vec<String>,
    pub problems: Vec<String>,
    pub pending_deletions: Vec<String>,
    /// Cloud files left out (`SyncReport::skipped_remote`); they do not stop a launch.
    pub left_out: Vec<String>,
    /// Locations GOG's cloud storage could not be reached for (`Error::is_temporary`): synced
    /// neither way, as when the cloud cannot be reached at all, they do not stop a launch.
    pub unreachable: Vec<String>,
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
                    s.left_out
                        .extend(r.skipped_remote.iter().map(|p| format!("{}/{p}", o.name)));
                    s.backup_dir = s.backup_dir.take().or_else(|| r.backup_dir.clone());
                }
                Err(e) if e.is_temporary() => s.unreachable.push(format!("{}: {e}", o.name)),
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
    /// The game runs without access to the user's files (see `runner`).
    Isolated,
    Started {
        pid: u32,
    },
    LauncherExited {
        code: Option<i32>,
    },
    StopRequested,
    /// Achievements the game unlocked just now, as Comet records them; it sends them to GOG.
    Achieved(Vec<Unlock>),
    /// Stop was asked for while the launch was being prepared: the game was not started.
    StoppedBeforeStart,
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
    /// Achievements GOG records as unlocked since the session started.
    Unlocked(Vec<Unlock>),
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

/// Full play flow: cloud check, Comet, supervised session (unlocks told as they come), cloud
/// upload, achievement diff.
/// Each message on `stop` asks the game to quit; the second one forces it. One sent before the game
/// starts keeps it from starting.
pub async fn play(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    req: PlayRequest,
    emit: impl Fn(PlayEvent) + Send + Sync,
    mut stop: UnboundedReceiver<()>,
) -> Result<()> {
    let _busy = crate::lock::game(dirs, &req.game_id)?;
    let user_id = Account::active(db)?.map(|a| a.user_id);
    let mut install = Install::get(db, &req.game_id)?
        .ok_or_else(|| Error::NotFound(format!("{} is not imported", req.game_id)))?;
    crate::umu::resolve(db, http, &mut install).await;
    if crate::maintenance::update_pending(db, &req.game_id)?.is_some() {
        return Err(Error::Refused(
            "an update of this game is unfinished; finish it before playing".into(),
        ));
    }
    let choice = crate::settings::launch_choice(db, &install.game_id)?;
    // Checked now, before the prefix and the setup, so a game that cannot start says so first.
    runner::launch_spec(dirs, &install, choice.as_deref())?;
    if install.isolated {
        runner::refresh_runtimes(db, dirs).await;
    }
    // GOG keeps no cloud saves for Linux builds, and they lack the Galaxy SDK that Comet talks to.
    // A game added by hand is not GOG's at all: GOG is not asked anything about it.
    let native = install.platform == Platform::Linux;
    let added = crate::store::is_local(&install.game_id);
    let cloud = req.cloud && !native && !added;
    // Comet acts for the account and listens on this computer while it runs: only for a game that
    // can talk to it, and only if allowed in Settings.
    let use_comet = req.comet
        && !native
        && !added
        && crate::settings::game_achievements(db)?
        && crate::comet::uses_galaxy(dirs, &install);

    if let Some(init) = runner::prefix_init_spec(dirs, &install)? {
        emit(PlayEvent::PreparingPrefix);
        let log = crate::logs::of_game(dirs, "prefix", &install.game_id);
        crate::logs::start_run(&log);
        let busy = crate::lock::session(dirs, &install.game_id);
        let outcome = SessionHandle::start(&req.supervisor, &init, &log, Some(&busy))
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
            let tokens = tokens(db, dirs, http, user_id.as_deref()).await?;
            let forward = |e: SetupEvent| {
                emit(match e {
                    SetupEvent::Downloading => {
                        PlayEvent::SetupStep("downloading setup files".into())
                    }
                    SetupEvent::Running(label) => PlayEvent::SetupStep(label),
                    SetupEvent::NonZeroExit { label, code } => {
                        PlayEvent::SetupWarning(format!("{label} exited with code {code:?}"))
                    }
                    SetupEvent::Unfinished { again: true } => PlayEvent::SetupWarning(
                        "the setup runs again at the next launch".into(),
                    ),
                    SetupEvent::Unfinished { again: false } => PlayEvent::SetupWarning(format!(
                        "the setup failed {} times and is left as it is; `slatty setup {} --force` runs it again",
                        crate::setup::SETUP_RUNS,
                        install.game_id
                    )),
                })
            };
            crate::setup::run_held(
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

    if use_comet
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

    // Built again: the setup steps above ran the game's own programs in its folder, which an
    // isolated launch checks for what umu would share (`runner::launch_spec`).
    let spec = runner::launch_spec(dirs, &install, choice.as_deref())?;

    if cloud {
        match cloud_sync(db, dirs, http, &install, user_id.as_deref()).await {
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

    let mut comet = if use_comet {
        match start_comet(db, dirs, http, &install, user_id.as_deref()).await {
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

    // Stop asked for while all this ran (it can take long offline): the game is not started, rather
    // than started and asked to quit at once, its saves then left out of the upload.
    if stop.try_recv().is_ok() {
        if let Some((comet, _, _)) = comet {
            let _ = comet.stop().await;
        }
        emit(PlayEvent::StoppedBeforeStart);
        return Ok(());
    }
    if install.isolated {
        emit(PlayEvent::Isolated);
    }
    let started = Instant::now();
    let unlocks = comet.as_mut().and_then(|(_, _, unlocks)| unlocks.as_mut());
    let outcome = run_session(
        db,
        dirs,
        &req,
        &spec,
        user_id.as_deref(),
        &emit,
        &mut stop,
        unlocks,
    )
    .await;
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            if let Some((comet, _, _)) = comet {
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
        Some((comet, before, _)) => {
            comet.stop().await?;
            Some(before)
        }
        None => None,
    };

    if cloud {
        if !outcome.clean {
            emit(PlayEvent::CloudUploadSkipped(
                "the end of the session is uncertain".into(),
            ));
        } else {
            match cloud_sync(db, dirs, http, &install, user_id.as_deref()).await {
                Ok(Some(summary)) => emit(PlayEvent::CloudUploaded(summary)),
                Ok(None) => {}
                Err(e) => emit(PlayEvent::CloudUploadSkipped(e.to_string())),
            }
        }
    }

    // Sessions of GOG's games waiting to be sent go with the next one of those: GOG does not learn
    // when a game added by hand was played.
    if outcome.clean && !added {
        match report_playtime(db, dirs, http, user_id.as_deref()).await {
            Ok(0) => {}
            Ok(minutes) => emit(PlayEvent::PlaytimeReported(minutes)),
            Err(e) => emit(PlayEvent::PlaytimeNotReported(e.to_string())),
        }
    }

    if let Some(before) = before {
        emit(
            match (
                before,
                achievements_now(db, dirs, http, &install, user_id.as_deref())
                    .await
                    .map(|(_, after)| after),
            ) {
                (Some(before), Some(after)) => {
                    let new: Vec<Unlock> = achievements::newly_unlocked(&before, &after)
                        .iter()
                        .map(|a| Unlock {
                            key: a.achievement_key.clone(),
                            name: a.name.clone(),
                        })
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

#[allow(clippy::too_many_arguments)]
async fn run_session(
    db: &Db,
    dirs: &Dirs,
    req: &PlayRequest,
    spec: &runner::LaunchSpec,
    user_id: Option<&str>,
    emit: &(impl Fn(PlayEvent) + Send + Sync),
    stop: &mut UnboundedReceiver<()>,
    mut unlocks: Option<&mut comet::Unlocks>,
) -> Result<SessionOutcome> {
    let log = crate::logs::of_game(dirs, "game", &req.game_id);
    crate::logs::start_run(&log);
    let busy = crate::lock::session(dirs, &req.game_id);
    let mut handle = SessionHandle::start(&req.supervisor, spec, &log, Some(&busy)).await?;
    match handle.next_event().await? {
        Some(SupervisorEvent::Started { pid }) => emit(PlayEvent::Started { pid }),
        Some(SupervisorEvent::Failed { message }) => return Err(Error::Refused(message)),
        other => return Err(Error::parse("session supervisor", format!("{other:?}"))),
    }
    let id = session::record_start(db, &req.game_id, user_id)?;
    // Monotonic: suspend is not play time, and the wall clock can be set meanwhile.
    let played = Instant::now();
    let mut stop_open = true;
    let mut look = tokio::time::interval(UNLOCKS_EVERY);
    let outcome = loop {
        tokio::select! {
            event = handle.next_event() => match event? {
                Some(SupervisorEvent::MainExited { code }) => emit(PlayEvent::LauncherExited { code }),
                Some(SupervisorEvent::Ended { main_code }) => break SessionOutcome { main_code, clean: true },
                Some(SupervisorEvent::Failed { message }) => {
                    let lost = SessionOutcome { main_code: None, clean: false };
                    end_session(db, id, &lost, user_id, played.elapsed())?;
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
            _ = look.tick(), if unlocks.is_some() => {
                let new = unlocks.as_deref_mut().map(comet::Unlocks::new_ones).unwrap_or_default();
                if !new.is_empty() {
                    emit(PlayEvent::Achieved(new));
                }
            }
        }
    };
    // A last look, for what the game unlocked in its last second.
    let new = unlocks.map(comet::Unlocks::new_ones).unwrap_or_default();
    if !new.is_empty() {
        emit(PlayEvent::Achieved(new));
    }
    end_session(db, id, &outcome, user_id, played.elapsed())?;
    Ok(outcome)
}

/// Records a session's end. With play time reporting off it is kept from GOG at once, rather than
/// after the steps that follow the game (Comet's stop, the cloud): a launcher closed meanwhile
/// would have left it to be sent later.
fn end_session(
    db: &Db,
    id: i64,
    outcome: &SessionOutcome,
    user_id: Option<&str>,
    played: Duration,
) -> Result<()> {
    session::record_end(db, id, outcome, played)?;
    if let Some(user_id) = user_id
        && !crate::settings::report_playtime(db)?
    {
        crate::playtime::keep_private(db, user_id)?;
    }
    Ok(())
}

async fn cloud_sync(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
    user_id: Option<&str>,
) -> Result<Option<CloudSummary>> {
    let tokens = tokens(db, dirs, http, user_id).await?;
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
    if !crate::settings::report_playtime(db)? {
        crate::playtime::keep_private(db, user_id)?;
        return Ok(0);
    }
    if crate::playtime::unreported(db, user_id)?.is_empty() {
        return Ok(0);
    }
    let tokens = tokens(db, dirs, http, Some(user_id)).await?;
    crate::playtime::report_pending(db, dirs, http, &tokens).await
}

async fn tokens(db: &Db, dirs: &Dirs, http: &Client, user_id: Option<&str>) -> Result<Tokens> {
    let mut account = Account::load_for(db, dirs, user_id.ok_or(Error::NotLoggedIn)?).await?;
    Ok(account.tokens(http).await?.clone())
}

async fn start_comet(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
    user_id: Option<&str>,
) -> Result<(Comet, Option<Vec<Achievement>>, Option<comet::Unlocks>)> {
    let bin =
        find_in_path("comet").ok_or_else(|| Error::NotFound("`comet` is not in PATH".into()))?;
    let mut account = Account::load_for(db, dirs, user_id.ok_or(Error::NotLoggedIn)?).await?;
    let tokens = account.tokens(http).await?.clone();
    let comet = Comet::start(&bin, &tokens, &account.info.username, dirs).await?;
    let (client_id, before) = match achievements_now(db, dirs, http, install, user_id).await {
        Some((client_id, before)) => (Some(client_id), Some(before)),
        None => (install.client_id.clone(), None),
    };
    let unlocks = client_id.map(|client_id| {
        comet::Unlocks::new(
            dirs,
            &client_id,
            &tokens.user_id,
            before.as_deref().unwrap_or_default(),
        )
    });
    Ok((comet, before, unlocks))
}

/// The game's achievements on GOG, with its client id.
async fn achievements_now(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
    user_id: Option<&str>,
) -> Option<(String, Vec<Achievement>)> {
    let tokens = tokens(db, dirs, http, user_id).await.ok()?;
    let (client_id, token) = achievements::game_token(http, &tokens, install)
        .await
        .ok()?;
    let achievements = achievements::fetch(http, &tokens.user_id, &client_id, &token)
        .await
        .ok()?;
    Some((client_id, achievements))
}

/// Sessions without a recorded end (launcher crash or kill) are marked as interrupted. One whose
/// game still runs under its supervisor (started by another launcher open meanwhile, or by one
/// that closed) is left as it is.
pub fn recover_unfinished(db: &Db, dirs: &Dirs) -> Result<Vec<session::SessionRecord>> {
    let mut unfinished = session::unfinished(db)?;
    unfinished.retain(|s| {
        crate::paths::check_owned_id(&s.game_id).is_err()
            || crate::lock::try_acquire(&crate::lock::session(dirs, &s.game_id))
                .is_ok_and(|free| free.is_some())
    });
    for s in &unfinished {
        session::mark_interrupted(db, s.id)?;
    }
    Ok(unfinished)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GOG's cloud storage out of reach for a location (a dropped connection, a server error) is
    /// not a conflict: the game starts with its local saves, as when the cloud cannot be reached
    /// at all, and the summary says which location was not reached. A location refused for good
    /// still stops the launch.
    #[test]
    fn a_cloud_out_of_reach_does_not_stop_a_launch() {
        let failing = |status| LocationOutcome {
            name: "saves".into(),
            template: "<?SAVED_GAMES?>/[FAKE]".into(),
            root: None,
            result: Err(Error::Http {
                context: "listing cloud saves",
                status,
            }),
        };
        let summary = CloudSummary::from_outcomes(&[failing(503)]);
        assert!(summary.is_clean(), "{summary:?}");
        assert_eq!(summary.unreachable.len(), 1);
        assert!(!CloudSummary::from_outcomes(&[failing(403)]).is_clean());
    }

    /// A game added by hand that still runs under its supervisor keeps its session going, as a
    /// GOG game's does.
    #[test]
    fn a_game_added_by_hand_that_still_runs_is_not_interrupted() {
        let root = std::env::temp_dir().join(format!("slatty-play-local-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        session::record_start(&db, "local-1", None).unwrap();
        let held = crate::lock::try_acquire(&crate::lock::session(&dirs, "local-1"))
            .unwrap()
            .unwrap();
        assert!(recover_unfinished(&db, &dirs).unwrap().is_empty());
        drop(held);
        assert_eq!(recover_unfinished(&db, &dirs).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// With play time reporting off, a session is kept from GOG as it ends: a launcher closed or
    /// crashed during the steps after the game (Comet's stop, the cloud) left it to be sent later.
    #[test]
    fn a_session_played_with_reporting_off_is_kept_private_as_it_ends() {
        let db = Db::in_memory().unwrap();
        crate::settings::set_report_playtime(&db, false).unwrap();
        let id = session::record_start(&db, "1", Some("1234")).unwrap();
        let outcome = SessionOutcome {
            main_code: Some(0),
            clean: true,
        };
        end_session(&db, id, &outcome, Some("1234"), Duration::from_secs(120)).unwrap();
        crate::settings::set_report_playtime(&db, true).unwrap();
        let reported: i64 = db
            .conn()
            .query_row("SELECT reported FROM sessions WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(reported, 2, "never to be sent");
    }
}
