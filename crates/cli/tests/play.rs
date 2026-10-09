use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::paths::Dirs;
use slatty_core::play::{self, PlayEvent, PlayRequest};
use slatty_core::runner::Runner;

fn fake_native_game(name: &str, script: &str) -> (PathBuf, Dirs, Db) {
    let root = std::env::temp_dir().join(format!("slatty-play-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let game = root.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let start = game.join("start.sh");
    std::fs::write(&start, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&start, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::open(&dirs.db_file()).unwrap();
    Install {
        umu_id: None,
        game_id: "7".into(),
        title: "Fake".into(),
        platform: Platform::Linux,
        path: game,
        client_id: None,
        runner: Runner::Native,
    }
    .save(&db)
    .unwrap();
    (root, dirs, db)
}

fn request() -> PlayRequest {
    PlayRequest {
        game_id: "7".into(),
        cloud: false,
        comet: false,
        supervisor: env!("CARGO_BIN_EXE_slatty").into(),
    }
}

#[tokio::test]
async fn play_reports_the_real_end_of_a_detached_game() {
    let (root, dirs, db) = fake_native_game("detached", "(sleep 1; touch \"$PWD/done\") & exit 0");
    let events = Mutex::new(Vec::new());
    let (_stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    let http = slatty_core::http::client().unwrap();
    play::play(
        &db,
        &dirs,
        &http,
        request(),
        |e| events.lock().unwrap().push(e),
        stop_rx,
    )
    .await
    .unwrap();
    let events = events.into_inner().unwrap();
    assert!(matches!(events[0], PlayEvent::Started { .. }));
    assert_eq!(events[1], PlayEvent::LauncherExited { code: Some(0) });
    assert!(matches!(
        events[2],
        PlayEvent::Ended {
            seconds: 1..,
            clean: true,
            ..
        }
    ));
    assert!(root.join("game/done").exists());
    assert!(play::recover_unfinished(&db).unwrap().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn play_can_stop_the_game() {
    let (root, dirs, db) = fake_native_game("stop", "sleep 30");
    let events = Mutex::new(Vec::new());
    let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    let http = slatty_core::http::client().unwrap();
    let stopper = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        stop_tx.send(()).unwrap();
    };
    let run = play::play(
        &db,
        &dirs,
        &http,
        request(),
        |e| events.lock().unwrap().push(e),
        stop_rx,
    );
    let (result, ()) = tokio::join!(run, stopper);
    result.unwrap();
    let events = events.into_inner().unwrap();
    assert!(events.contains(&PlayEvent::StopRequested));
    assert!(matches!(
        events.last(),
        Some(PlayEvent::Ended { clean: true, .. })
    ));
    std::fs::remove_dir_all(root).unwrap();
}
