use std::collections::HashMap;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
#[cfg(unix)]
use nix::errno::Errno;
#[cfg(unix)]
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal};
#[cfg(unix)]
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
#[cfg(unix)]
use nix::unistd::Pid;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::runner::LaunchSpec;

#[cfg(windows)]
mod windows;

/// First argument that turns a launcher binary into a session supervisor.
pub const SUPERVISE_ARG: &str = "__slatty-supervise";

/// The running binary, to start supervisors with. On Linux, unlike `current_exe`, it still works
/// once the file was replaced on disk (an update or a rebuild while the launcher runs); Windows
/// keeps a running program's file from being replaced.
pub fn supervisor_exe() -> PathBuf {
    #[cfg(unix)]
    {
        PathBuf::from("/proc/self/exe")
    }
    #[cfg(windows)]
    {
        std::env::current_exe().unwrap_or_default()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuperviseRequest {
    pub spec: LaunchSpec,
    pub log: PathBuf,
    /// Lock held until no process of the session is left (`lock::session`).
    pub busy: Option<PathBuf>,
    /// On Windows, whether a program that needs administrator rights is started through Windows'
    /// prompt for them (`SessionHandle::start_elevating`). Unused on Linux.
    #[serde(default)]
    pub elevate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SupervisorEvent {
    Started { pid: u32 },
    MainExited { code: Option<i32> },
    Ended { main_code: Option<i32> },
    Failed { message: String },
}

#[cfg(unix)]
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_stop(_: nix::libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

/// Only wakes the supervisor from `sigsuspend`; its loop reaps the children.
#[cfg(unix)]
extern "C" fn on_child(_: nix::libc::c_int) {}

/// Runs in the supervisor process. The session ends only when no descendant is left,
/// which covers launchers that exit early and Wine's detached processes.
pub fn supervise_main() -> i32 {
    let mut out = std::io::stdout().lock();
    let mut emit = |e: &SupervisorEvent| {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string(e).expect("event serializes")
        );
        let _ = out.flush();
    };
    let mut input = String::new();
    let request: SuperviseRequest = match std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())
        .and_then(|_| serde_json::from_str(&input).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(message) => {
            emit(&SupervisorEvent::Failed { message });
            return 2;
        }
    };
    // Opened close-on-exec: the game does not inherit it, and it is released when this ends.
    let _busy = match request.busy.as_deref().map(crate::lock::try_acquire) {
        None => None,
        Some(Ok(Some(lock))) => Some(lock),
        Some(Ok(None)) => {
            emit(&SupervisorEvent::Failed {
                message: "this game is already running".into(),
            });
            return 1;
        }
        Some(Err(e)) => {
            emit(&SupervisorEvent::Failed {
                message: e.to_string(),
            });
            return 2;
        }
    };
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&request.log);
    let (stdout, stderr) = match log.and_then(|f| Ok((f.try_clone()?, f))) {
        Ok((a, b)) => (Stdio::from(a), Stdio::from(b)),
        Err(_) => (Stdio::null(), Stdio::null()),
    };
    #[cfg(unix)]
    return supervise(&request.spec, stdout, stderr, emit);
    #[cfg(windows)]
    return windows::supervise(&request.spec, request.elevate, stdout, stderr, emit);
}

/// Starts the game and follows its processes until none is left, as a subreaper; SIGTERM or
/// SIGINT stops them, a second one kills them.
#[cfg(unix)]
fn supervise(
    spec: &LaunchSpec,
    stdout: Stdio,
    stderr: Stdio,
    mut emit: impl FnMut(&SupervisorEvent),
) -> i32 {
    if let Err(e) = nix::sys::prctl::set_child_subreaper(true) {
        emit(&SupervisorEvent::Failed {
            message: format!("PR_SET_CHILD_SUBREAPER: {e}"),
        });
        return 2;
    }
    let action = SigAction::new(
        SigHandler::Handler(on_stop),
        SaFlags::empty(),
        SigSet::empty(),
    );
    for sig in [Signal::SIGTERM, Signal::SIGINT] {
        // SAFETY: the handler only stores to an atomic.
        unsafe { signal::sigaction(sig, &action) }.expect("installing a signal handler");
    }
    let woken = SigAction::new(
        SigHandler::Handler(on_child),
        SaFlags::SA_NOCLDSTOP,
        SigSet::empty(),
    );
    // SAFETY: the handler does nothing.
    unsafe { signal::sigaction(Signal::SIGCHLD, &woken) }.expect("installing a signal handler");

    let child = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .process_group(0)
        .spawn();
    let main_pid = match child {
        Ok(c) => Pid::from_raw(c.id() as i32),
        Err(e) => {
            emit(&SupervisorEvent::Failed {
                message: format!("starting {}: {e}", spec.program.display()),
            });
            return 1;
        }
    };
    emit(&SupervisorEvent::Started {
        pid: main_pid.as_raw() as u32,
    });

    // From here the signals are taken only inside `suspend`: a stop asked for between the look
    // at the flag and the wait would otherwise wait for the next child to end.
    let mut watched = SigSet::empty();
    for sig in [Signal::SIGCHLD, Signal::SIGTERM, Signal::SIGINT] {
        watched.add(sig);
    }
    let unblocked = watched
        .thread_swap_mask(SigmaskHow::SIG_BLOCK)
        .expect("blocking signals");
    let mut main_code = None;
    let mut stop_sent = 0;
    loop {
        if STOP_REQUESTED.swap(false, Ordering::SeqCst) {
            stop_sent += 1;
            let sig = if stop_sent > 1 {
                Signal::SIGKILL
            } else {
                Signal::SIGTERM
            };
            let _ = signal::killpg(main_pid, sig);
            for pid in direct_children() {
                let _ = signal::kill(pid, sig);
            }
        }
        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                let _ = unblocked.suspend();
            }
            Ok(WaitStatus::Exited(pid, code)) if pid == main_pid => {
                main_code = Some(code);
                emit(&SupervisorEvent::MainExited { code: Some(code) });
            }
            Ok(WaitStatus::Signaled(pid, _, _)) if pid == main_pid => {
                emit(&SupervisorEvent::MainExited { code: None });
            }
            Ok(_) => {}
            Err(Errno::ECHILD) => break,
            Err(Errno::EINTR) => {}
            Err(e) => {
                emit(&SupervisorEvent::Failed {
                    message: format!("waitpid: {e}"),
                });
                return 1;
            }
        }
    }
    emit(&SupervisorEvent::Ended { main_code });
    0
}

#[cfg(unix)]
fn direct_children() -> Vec<Pid> {
    let me = std::process::id().to_string();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|s| {
                    s.rsplit_once(')')
                        .map(|(_, rest)| rest.split_whitespace().nth(1) == Some(&me))
                })
                .unwrap_or(false)
        })
        .map(Pid::from_raw)
        .collect()
}

pub struct SessionHandle {
    child: Child,
    events: Lines<BufReader<ChildStdout>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOutcome {
    pub main_code: Option<i32>,
    pub clean: bool,
}

impl SessionHandle {
    /// Spawns `supervisor SUPERVISE_ARG` and hands it the launch request. With `busy`
    /// (`lock::session`), the supervisor holds that lock until the session ends.
    pub async fn start(
        supervisor: &Path,
        spec: &LaunchSpec,
        log: &Path,
        busy: Option<&Path>,
    ) -> Result<SessionHandle> {
        Self::launch(supervisor, spec, log, busy, false).await
    }

    /// `start`, for GOG's setup: on Windows a program that needs administrator rights is started
    /// through Windows' prompt for them (UAC), once the person agreed. Such a program runs outside
    /// the session's job object, which cannot hold it: the session follows it alone, its output is
    /// not logged, and it cannot be stopped. On Linux, `start`.
    pub async fn start_elevating(
        supervisor: &Path,
        spec: &LaunchSpec,
        log: &Path,
        busy: Option<&Path>,
    ) -> Result<SessionHandle> {
        Self::launch(supervisor, spec, log, busy, true).await
    }

    async fn launch(
        supervisor: &Path,
        spec: &LaunchSpec,
        log: &Path,
        busy: Option<&Path>,
        elevate: bool,
    ) -> Result<SessionHandle> {
        if let Some(parent) = log.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        // In a process group of its own: a Ctrl+C or a terminal closing reaches the launcher alone,
        // which asks the supervisor to stop the game once; the supervisor outlives the launcher.
        let mut command = tokio::process::Command::new(supervisor);
        command
            .arg(SUPERVISE_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        command.process_group(0);
        // Without a console window of its own either.
        #[cfg(windows)]
        command.creation_flags(
            windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP
                | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
        );
        let mut child = command
            .spawn()
            .map_err(|e| Error::io("starting the session supervisor", e))?;
        let request = serde_json::to_vec(&SuperviseRequest {
            spec: spec.clone(),
            log: log.to_path_buf(),
            busy: busy.map(Path::to_path_buf),
            elevate,
        })
        .map_err(|e| Error::parse("supervise request", e))?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        stdin
            .write_all(&request)
            .await
            .map_err(|e| Error::io("sending the launch request", e))?;
        drop(stdin);
        let stdout = child.stdout.take().expect("stdout is piped");
        Ok(SessionHandle {
            child,
            events: BufReader::new(stdout).lines(),
        })
    }

    pub fn supervisor_pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub async fn next_event(&mut self) -> Result<Option<SupervisorEvent>> {
        let line = self
            .events
            .next_line()
            .await
            .map_err(|e| Error::io("reading supervisor events", e))?;
        line.map(|l| serde_json::from_str(&l).map_err(|e| Error::parse("supervisor event", e)))
            .transpose()
    }

    /// Asks the supervisor to terminate the game; a second call escalates to SIGKILL (on Windows,
    /// closing its windows, then ending its processes).
    pub fn request_stop(&self) {
        if let Some(pid) = self.child.id() {
            #[cfg(unix)]
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            #[cfg(windows)]
            windows::request_stop(pid);
        }
    }

    pub async fn wait(mut self) -> Result<SessionOutcome> {
        let mut main_code = None;
        while let Some(event) = self.next_event().await? {
            match event {
                SupervisorEvent::Ended { main_code: code } => {
                    let _ = self.child.wait().await;
                    return Ok(SessionOutcome {
                        main_code: code.or(main_code),
                        clean: true,
                    });
                }
                SupervisorEvent::MainExited { code } => main_code = code,
                SupervisorEvent::Failed { message } => return Err(Error::Refused(message)),
                SupervisorEvent::Started { .. } => {}
            }
        }
        let _ = self.child.wait().await;
        Ok(SessionOutcome {
            main_code,
            clean: false,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: i64,
    pub game_id: String,
    pub started_at: i64,
}

pub fn record_start(db: &Db, game_id: &str, user_id: Option<&str>) -> Result<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sessions (game_id, user_id, started_at, state) VALUES (?1, ?2, ?3, 'running')",
        params![game_id, user_id, Utc::now().timestamp()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Records how a session ended, `played` after its start: measured on a clock that stops while
/// the computer sleeps (`platform::Awake`), it leaves out suspend and is not moved by the wall
/// clock being set meanwhile, which would otherwise reach GOG and the local play time.
pub fn record_end(
    db: &Db,
    id: i64,
    outcome: &SessionOutcome,
    played: std::time::Duration,
) -> Result<()> {
    db.conn().execute(
        "UPDATE sessions SET ended_at = started_at + ?2, exit_code = ?3, state = ?4 WHERE id = ?1",
        params![
            id,
            i64::try_from(played.as_secs()).unwrap_or(i64::MAX),
            outcome.main_code,
            if outcome.clean { "ended" } else { "lost" }
        ],
    )?;
    Ok(())
}

/// Sessions that never recorded an end (launcher crash or kill).
pub fn unfinished(db: &Db) -> Result<Vec<SessionRecord>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, game_id, started_at FROM sessions WHERE state = 'running' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SessionRecord {
            id: r.get(0)?,
            game_id: r.get(1)?,
            started_at: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn mark_interrupted(db: &Db, id: i64) -> Result<()> {
    db.conn().execute(
        "UPDATE sessions SET state = 'interrupted' WHERE id = ?1",
        [id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Playtime {
    /// Total length of the sessions that recorded their end.
    pub seconds: i64,
    pub last_played: i64,
}

/// Play time per game, from the sessions Slatty supervised for the account `user_id` (or
/// with none signed in): another account's play history is its own. A game added by hand is no
/// account's: its sessions count whoever was signed in.
pub fn playtime(db: &Db, user_id: Option<&str>) -> Result<HashMap<String, Playtime>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT game_id, SUM(COALESCE(ended_at - started_at, 0)), MAX(COALESCE(ended_at, started_at))
         FROM sessions WHERE user_id IS ?1 OR game_id LIKE 'local-%' GROUP BY game_id",
    )?;
    let rows = stmt.query_map([user_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            Playtime {
                seconds: r.get(1)?,
                last_played: r.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playtime_sums_ended_sessions_and_keeps_the_latest_one() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (game_id, user_id, started_at, ended_at, state) VALUES
                   ('1', 'a', 100, 160, 'ended'), ('1', 'a', 1000, 1300, 'ended'),
                   ('1', 'a', 2000, NULL, 'interrupted'), ('2', 'a', 50, 80, 'lost');",
            )
            .unwrap();
        let p = playtime(&db, Some("a")).unwrap();
        assert_eq!(
            p["1"],
            Playtime {
                seconds: 360,
                last_played: 2000
            }
        );
        assert_eq!(p["2"].seconds, 30);
    }

    /// A game added by hand is no GOG account's: its play time shows whatever account is signed
    /// in, or none, and forgetting an account keeps it, sessions recorded under an account before
    /// included.
    #[test]
    fn the_play_time_of_a_game_added_by_hand_is_no_accounts() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (game_id, user_id, started_at, ended_at, state) VALUES
                   ('local-1', '111', 0, 3600, 'ended'), ('1', '111', 0, 60, 'ended');",
            )
            .unwrap();
        for user in [Some("222"), None] {
            let p = playtime(&db, user).unwrap();
            assert_eq!(p.get("local-1").map(|p| p.seconds), Some(3600), "{user:?}");
            assert!(!p.contains_key("1"), "the account's own");
        }
        let root = std::env::temp_dir().join(format!("slatty-local-time-{}", std::process::id()));
        let dirs = crate::paths::Dirs::under(&root);
        crate::account::forget_data(&db, &dirs, "111").unwrap();
        let p = playtime(&db, None).unwrap();
        assert_eq!(p.get("local-1").map(|p| p.seconds), Some(3600));
        let _ = std::fs::remove_dir_all(root);
    }

    /// A session lasts what was played, measured without the wall clock: a night of suspend, or
    /// the clock set forward or back meanwhile (NTP correcting a dual-boot offset), changes
    /// neither what GOG receives nor the local play time.
    #[test]
    fn a_session_lasts_what_was_played_whatever_the_clock_did() {
        let db = Db::in_memory().unwrap();
        let id = record_start(&db, "1", Some("a")).unwrap();
        // The wall clock jumps two hours ahead before the end is recorded.
        db.conn()
            .execute(
                "UPDATE sessions SET started_at = started_at - 7200 WHERE id = ?1",
                [id],
            )
            .unwrap();
        let outcome = SessionOutcome {
            main_code: Some(0),
            clean: true,
        };
        record_end(&db, id, &outcome, std::time::Duration::from_secs(90)).unwrap();
        assert_eq!(playtime(&db, Some("a")).unwrap()["1"].seconds, 90);
    }

    /// The next account signed in on this computer sees nothing of what the previous one played.
    #[test]
    fn playtime_is_each_accounts_own() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO sessions (game_id, user_id, started_at, ended_at, state) VALUES
                   ('1', 'a', 100, 160, 'ended'), ('2', 'b', 1000, 1300, 'ended'),
                   ('3', NULL, 10, 20, 'ended');",
            )
            .unwrap();
        let games = |user| {
            let mut ids: Vec<String> = playtime(&db, user).unwrap().into_keys().collect();
            ids.sort();
            ids
        };
        assert_eq!(games(Some("a")), ["1"]);
        assert_eq!(games(Some("b")), ["2"]);
        assert_eq!(games(None), ["3"]);
    }
}
