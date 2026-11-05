//! A GOG game played on Windows as the command line plays it: started directly, followed through
//! the supervisor's job object, stopped, its play time recorded. The game is this test program,
//! copied into a fake game folder, where it runs `fake_game` as its `fake-game.txt` says.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::installer::InstallRecord;
use slatty_core::paths::Dirs;
use slatty_core::play::{self, PlayEvent, PlayRequest};
use slatty_core::runner::Runner;

const GAME_ID: &str = "7";

/// What the fake game does, read from `fake-game.txt` in the folder it runs in: `exit <code>`,
/// `sleep` (a minute, until stopped), `detach` (starts itself in `child`, then exits), `late`
/// (writes `late-done` after two seconds). Every one first writes `ran`. Without the file it does
/// nothing, so that running every ignored test does not wait on it.
#[test]
#[ignore = "the fake game, started by the tests below"]
fn fake_game() {
    let Ok(what) = std::fs::read_to_string("fake-game.txt") else {
        return;
    };
    std::fs::write("ran", "").unwrap();
    match what.trim().split_once(' ').unwrap_or((what.trim(), "")) {
        ("exit", code) => std::process::exit(code.parse().unwrap()),
        ("sleep", _) => std::thread::sleep(Duration::from_secs(60)),
        ("detach", _) => {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(game_arguments())
                .current_dir("child")
                .spawn()
                .unwrap();
        }
        ("late", _) => {
            std::thread::sleep(Duration::from_secs(2));
            std::fs::write("late-done", "").unwrap();
        }
        (other, _) => panic!("unknown behaviour {other}"),
    }
}

/// How the fake game is started: this program running `fake_game` alone.
fn game_arguments() -> [&'static str; 4] {
    ["fake_game", "--exact", "--ignored", "--nocapture"]
}

/// A GOG game installed by Slatty on Windows, started directly from its `goggame-<id>.info`, in
/// its `bin` folder, doing `behaviour`.
fn fake_gog_game(name: &str, behaviour: &str) -> (PathBuf, Dirs, Db) {
    let root = std::env::temp_dir().join(format!("slatty-play-win-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let game = root.join("[FAKE] Game");
    std::fs::create_dir_all(game.join("bin")).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), game.join("bin/Game.exe")).unwrap();
    std::fs::write(game.join("bin/fake-game.txt"), behaviour).unwrap();
    let info = serde_json::json!({
        "gameId": GAME_ID,
        "name": "[FAKE] Game",
        "playTasks": [{
            "type": "FileTask",
            "isPrimary": true,
            "path": "bin\\Game.exe",
            "workingDir": "bin",
            "arguments": game_arguments().join(" "),
        }],
    });
    std::fs::write(
        game.join(format!("goggame-{GAME_ID}.info")),
        info.to_string(),
    )
    .unwrap();
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::open(&dirs.db_file()).unwrap();
    Install {
        umu_id: None,
        game_id: GAME_ID.into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: game,
        client_id: None,
        runner: Runner::Native,
        isolated: false,
    }
    .save(&db)
    .unwrap();
    (root, dirs, db)
}

fn request(setup_agreed: bool) -> PlayRequest {
    PlayRequest {
        game_id: GAME_ID.into(),
        cloud: false,
        comet: false,
        supervisor: env!("CARGO_BIN_EXE_slatty").into(),
        setup_agreed,
    }
}

/// Plays the game to its end; with `stop`, asks it to stop once it started, then again that long
/// after.
async fn play(dirs: &Dirs, db: &Db, req: PlayRequest, stop: Option<Duration>) -> Vec<PlayEvent> {
    let events = Mutex::new(Vec::new());
    let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    let http = slatty_core::http::client().unwrap();
    let played = play::play(
        db,
        dirs,
        &http,
        req,
        |e| {
            if stop.is_some() && matches!(e, PlayEvent::Started { .. }) {
                let stop_tx = stop_tx.clone();
                tokio::spawn(async move {
                    // Asked twice: the fake game has no window to close.
                    let _ = stop_tx.send(());
                    tokio::time::sleep(stop.unwrap()).await;
                    let _ = stop_tx.send(());
                });
            }
            events.lock().unwrap().push(e)
        },
        stop_rx,
    );
    tokio::time::timeout(Duration::from_secs(60), played)
        .await
        .expect("the session ends")
        .unwrap();
    events.into_inner().unwrap()
}

fn ended(events: &[PlayEvent]) -> (u64, Option<i32>, bool) {
    events
        .iter()
        .find_map(|e| match e {
            PlayEvent::Ended {
                seconds,
                code,
                clean,
            } => Some((*seconds, *code, *clean)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no end in {events:?}"))
}

fn sessions(db: &Db) -> Vec<(String, Option<i32>)> {
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT state, exit_code FROM sessions ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The game starts from its own `goggame-<id>.info`, directly, in its working folder; its exit
/// code and its session are recorded.
#[tokio::test]
async fn a_gog_game_is_played_directly() {
    let (root, dirs, db) = fake_gog_game("direct", "exit 3");
    let events = play(&dirs, &db, request(false), None).await;
    assert!(matches!(events[0], PlayEvent::Started { .. }), "{events:?}");
    assert_eq!(ended(&events).1, Some(3));
    assert!(ended(&events).2, "the supervisor saw the end");
    assert!(
        root.join("[FAKE] Game/bin/ran").exists(),
        "in its working folder"
    );
    assert_eq!(sessions(&db), [("ended".to_string(), Some(3))]);
    let _ = std::fs::remove_dir_all(root);
}

/// A launcher that starts the game and exits: the session lasts until the game it started
/// ends, which the job object holds with it.
#[tokio::test]
async fn the_session_lasts_until_what_the_game_started_ends() {
    let (root, dirs, db) = fake_gog_game("detached", "detach");
    let child = root.join("[FAKE] Game/bin/child");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(child.join("fake-game.txt"), "late").unwrap();
    let events = play(&dirs, &db, request(false), None).await;
    assert!(
        events.contains(&PlayEvent::LauncherExited { code: Some(0) }),
        "{events:?}"
    );
    assert!(
        child.join("late-done").exists(),
        "ended after the game it started"
    );
    let (seconds, _, clean) = ended(&events);
    assert!(clean && seconds >= 2, "{events:?}");
    let _ = std::fs::remove_dir_all(root);
}

/// Stopping the game ends it: its windows asked to close, then its processes ended.
#[tokio::test]
async fn a_game_is_stopped_when_asked() {
    let (root, dirs, db) = fake_gog_game("stop", "sleep");
    let started = Instant::now();
    let events = play(&dirs, &db, request(false), Some(Duration::from_secs(1))).await;
    assert!(events.contains(&PlayEvent::StopRequested), "{events:?}");
    assert!(ended(&events).2, "{events:?}");
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "not its whole minute"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// GOG's setup, pending, runs on Windows only once the person agreed, as it may ask for
/// administrator rights; the game starts all the same. Agreed to, it is tried (here without an
/// account, so it waits for the next launch).
#[tokio::test]
async fn a_pending_setup_waits_for_the_persons_agreement() {
    let (root, dirs, db) = fake_gog_game("setup", "exit 0");
    InstallRecord {
        build_id: "[FAKE] build".into(),
        version: "1.0".into(),
        language: "en-US".into(),
        path: Some(root.join("[FAKE] Game")),
        dlcs: Vec::new(),
        setup_build: None,
        setup_failures: 0,
        files: Vec::new(),
    }
    .save(&dirs, GAME_ID)
    .unwrap();
    let events = play(&dirs, &db, request(false), None).await;
    assert_eq!(events[0], PlayEvent::SetupNotAgreed);
    assert_eq!(ended(&events).1, Some(0));

    let events = play(&dirs, &db, request(true), None).await;
    assert!(
        matches!(events[0], PlayEvent::SetupSkipped(_)),
        "{events:?}"
    );
    assert_eq!(ended(&events).1, Some(0));
    assert!(
        slatty_core::setup::pending(&dirs, GAME_ID).unwrap(),
        "still to run"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// What only Linux has is refused on Windows, before anything starts: a game set to Proton.
#[tokio::test]
async fn a_game_set_to_proton_is_refused() {
    let (root, dirs, db) = fake_gog_game("proton", "exit 0");
    let mut install = Install::get(&db, GAME_ID).unwrap().unwrap();
    install.runner = Runner::Umu {
        proton: Path::new("C:\\[FAKE] Proton").into(),
        prefix: root.join("prefix"),
    };
    install.save(&db).unwrap();
    let (_stop, stop) = tokio::sync::mpsc::unbounded_channel();
    let http = slatty_core::http::client().unwrap();
    let refused = play::play(&db, &dirs, &http, request(true), |_| {}, stop)
        .await
        .unwrap_err();
    assert!(
        matches!(refused, slatty_core::Error::Unsupported(_)),
        "{refused}"
    );
    assert!(!root.join("[FAKE] Game/bin/ran").exists());
    let _ = std::fs::remove_dir_all(root);
}

/// The command line refuses Proton on Windows before GOG or GitHub is asked anything: an install
/// with a Proton build, the `proton` commands. It runs on the profile of the Wine prefix the tests
/// run in, and writes no game.
#[test]
fn the_command_line_refuses_proton() {
    for args in [
        &["install", "1", "--proton", "C:\\[FAKE] Proton"][..],
        &["proton", "list"],
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_slatty"))
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            stderr.contains("not available on Windows: Proton"),
            "{args:?}: {stderr}"
        );
    }
}
