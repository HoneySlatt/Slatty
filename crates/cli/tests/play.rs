use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;

use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::paths::Dirs;
use slatty_core::play::{self, PlayEvent, PlayRequest};
use slatty_core::runner::Runner;

/// The fake games run as they are: on NixOS a native game would go through `steam-run` or umu
/// when the machine has them, whose sandboxes may not see the temporary folder they live in.
fn without_wrappers() {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let kept: Vec<PathBuf> = std::env::split_paths(&path)
        .filter(|dir| !dir.join("steam-run").exists() && !dir.join("umu-run").exists())
        .collect();
    // Every test of this file sets the same value, so concurrent writes agree.
    unsafe { std::env::set_var("PATH", std::env::join_paths(kept).unwrap()) };
}

fn fake_native_game(name: &str, script: &str) -> (PathBuf, Dirs, Db) {
    without_wrappers();
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
        isolated: false,
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
    assert!(play::recover_unfinished(&db, &dirs).unwrap().is_empty());
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

/// Stop pressed while the launch is being prepared (cloud saves, Comet: long when offline) does
/// not start the game, rather than ending it, unsaved, as soon as it starts.
#[tokio::test]
async fn stop_before_the_game_starts_does_not_start_it() {
    let (root, dirs, db) = fake_native_game("stop-early", "touch \"$PWD/started\"; sleep 30");
    let events = Mutex::new(Vec::new());
    let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    stop_tx.send(()).unwrap();
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
    assert_eq!(events, [PlayEvent::StoppedBeforeStart]);
    assert!(!root.join("game/started").exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// Closing the launcher (or a crash) during a game leaves the game running: nothing else may
/// update, repair, uninstall, sync or start it until it ends.
#[tokio::test]
async fn a_game_left_running_keeps_the_game_busy() {
    let (root, dirs, db) = fake_native_game("left", "sleep 2; touch \"$PWD/done\"");
    let (_stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    let (events, mut received) = tokio::sync::mpsc::unbounded_channel();
    let http = slatty_core::http::client().unwrap();
    let run = play::play(
        &db,
        &dirs,
        &http,
        request(),
        move |e| {
            let _ = events.send(e);
        },
        stop_rx,
    );
    let started =
        async { while !matches!(received.recv().await, Some(PlayEvent::Started { .. })) {} };
    tokio::select! {
        _ = run => panic!("the session ended before the game"),
        () = started => {}
    }
    // The launcher is gone; its game runs on.
    assert!(
        slatty_core::lock::game(&dirs, "7").is_err(),
        "another operation could take a running game"
    );
    // Another launcher starting meanwhile does not take the session for an interrupted one.
    assert!(play::recover_unfinished(&db, &dirs).unwrap().is_empty());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while slatty_core::lock::game(&dirs, "7").is_err() {
        assert!(std::time::Instant::now() < deadline, "the game stays busy");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        root.join("game/done").exists(),
        "free only once the game ended"
    );
    // Its end was not recorded, the launcher being gone: it is now.
    assert_eq!(play::recover_unfinished(&db, &dirs).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

/// A game added by hand starts the program picked, with its arguments, in its working directory,
/// and its session counts in the local play time; nothing is asked over the network.
#[tokio::test]
async fn a_game_added_by_hand_plays_as_its_user_said() {
    without_wrappers();
    let root = std::env::temp_dir().join(format!("slatty-play-added-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let game = root.join("[FAKE] Game");
    std::fs::create_dir_all(game.join("data")).unwrap();
    let program = game.join("run.sh");
    std::fs::write(&program, "#!/bin/sh\necho \"$PWD $*\" > \"$0.out\"\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::open(&dirs.db_file()).unwrap();
    let install = slatty_core::added::add(
        &db,
        &dirs,
        slatty_core::added::NewGame {
            title: "[FAKE] Game".into(),
            program: program.clone(),
            args: vec!["--level".into(), "2".into()],
            working_dir: Some(game.join("data")),
            ..Default::default()
        },
    )
    .unwrap();
    // Every request would go through this proxy.
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let http = reqwest::Client::builder()
        .proxy(reqwest::Proxy::all(format!("http://{}", proxy.local_addr().unwrap())).unwrap())
        .build()
        .unwrap();
    let events = Mutex::new(Vec::new());
    let (_stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    let request = PlayRequest {
        game_id: install.game_id.clone(),
        cloud: true,
        comet: true,
        supervisor: env!("CARGO_BIN_EXE_slatty").into(),
    };
    play::play(
        &db,
        &dirs,
        &http,
        request,
        |e| events.lock().unwrap().push(e),
        stop_rx,
    )
    .await
    .unwrap();
    let real = game.canonicalize().unwrap();
    assert_eq!(
        std::fs::read_to_string(real.join("run.sh.out")).unwrap(),
        format!("{} --level 2\n", real.join("data").display())
    );
    let events = events.into_inner().unwrap();
    assert!(
        matches!(events.last(), Some(PlayEvent::Ended { clean: true, .. })),
        "{events:?}"
    );
    assert!(proxy.accept().is_err(), "nothing asked over the network");
    assert!(
        slatty_core::session::playtime(&db, None)
            .unwrap()
            .contains_key(&install.game_id)
    );
    std::fs::remove_dir_all(root).unwrap();
}
