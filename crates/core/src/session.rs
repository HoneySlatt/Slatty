use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use nix::errno::Errno;
use nix::sys::signal::{self, SaFlags, SigAction, SigHandler, SigSet, Signal};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::Pid;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdout};

use crate::db::Db;
use crate::error::{Error, Result};
use crate::runner::LaunchSpec;

/// First argument that turns a launcher binary into a session supervisor.
pub const SUPERVISE_ARG: &str = "__slatty-supervise";

#[derive(Debug, Serialize, Deserialize)]
pub struct SuperviseRequest {
    pub spec: LaunchSpec,
    pub log: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SupervisorEvent {
    Started { pid: u32 },
    MainExited { code: Option<i32> },
    Ended { main_code: Option<i32> },
    Failed { message: String },
}

static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop(_: nix::libc::c_int) {
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}

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

    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&request.log);
    let (stdout, stderr) = match log.and_then(|f| Ok((f.try_clone()?, f))) {
        Ok((a, b)) => (Stdio::from(a), Stdio::from(b)),
        Err(_) => (Stdio::null(), Stdio::null()),
    };
    let spec = &request.spec;
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

    let mut main_code = None;
    let mut stop_sent = 0;
    loop {
        match waitpid(None, None) {
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
    }
    emit(&SupervisorEvent::Ended { main_code });
    0
}

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
    /// Spawns `supervisor SUPERVISE_ARG` and hands it the launch request.
    pub async fn start(supervisor: &Path, spec: &LaunchSpec, log: &Path) -> Result<SessionHandle> {
        if let Some(parent) = log.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let mut child = tokio::process::Command::new(supervisor)
            .arg(SUPERVISE_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| Error::io("starting the session supervisor", e))?;
        let request = serde_json::to_vec(&SuperviseRequest {
            spec: spec.clone(),
            log: log.to_path_buf(),
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

    /// Asks the supervisor to terminate the game; a second call escalates to SIGKILL.
    pub fn request_stop(&self) {
        if let Some(pid) = self.child.id() {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
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
    db.conn().execute(
        "INSERT INTO sessions (game_id, user_id, started_at, state) VALUES (?1, ?2, ?3, 'running')",
        params![game_id, user_id, Utc::now().timestamp()],
    )?;
    Ok(db.conn().last_insert_rowid())
}

pub fn record_end(db: &Db, id: i64, outcome: &SessionOutcome) -> Result<()> {
    db.conn().execute(
        "UPDATE sessions SET ended_at = ?2, exit_code = ?3, state = ?4 WHERE id = ?1",
        params![
            id,
            Utc::now().timestamp(),
            outcome.main_code,
            if outcome.clean { "ended" } else { "lost" }
        ],
    )?;
    Ok(())
}

/// Sessions that never recorded an end (launcher crash or kill).
pub fn unfinished(db: &Db) -> Result<Vec<SessionRecord>> {
    let mut stmt = db.conn().prepare(
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
