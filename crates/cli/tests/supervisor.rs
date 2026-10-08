use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slatty_core::runner::LaunchSpec;
use slatty_core::session::{SessionHandle, SupervisorEvent};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slatty-sup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sh(script: &str, dir: &Path) -> LaunchSpec {
    LaunchSpec {
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: dir.to_path_buf(),
        env: vec![("MARK".into(), dir.join("mark").display().to_string())],
    }
}

async fn run_session(spec: &LaunchSpec, dir: &Path) -> (Vec<SupervisorEvent>, Duration) {
    let started = Instant::now();
    let mut handle = SessionHandle::start(
        Path::new(env!("CARGO_BIN_EXE_slatty")),
        spec,
        &dir.join("game.log"),
    )
    .await
    .unwrap();
    let mut events = Vec::new();
    while let Some(e) = handle.next_event().await.unwrap() {
        events.push(e);
    }
    (events, started.elapsed())
}

#[tokio::test]
async fn session_outlives_a_launcher_that_exits_immediately() {
    let dir = scratch("orphan");
    let spec = sh("(sleep 1; touch \"$MARK\") & exit 3", &dir);
    let (events, elapsed) = run_session(&spec, &dir).await;
    assert!(
        dir.join("mark").exists(),
        "session ended before the detached process finished"
    );
    assert!(elapsed >= Duration::from_secs(1));
    assert!(events.contains(&SupervisorEvent::MainExited { code: Some(3) }));
    assert_eq!(
        events.last(),
        Some(&SupervisorEvent::Ended { main_code: Some(3) })
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn session_follows_double_forked_new_session_process() {
    let dir = scratch("setsid");
    let spec = sh(
        "setsid sh -c 'sleep 1; touch \"$MARK\"' </dev/null >/dev/null 2>&1 & exit 0",
        &dir,
    );
    let (events, _) = run_session(&spec, &dir).await;
    assert!(dir.join("mark").exists());
    assert_eq!(
        events.last(),
        Some(&SupervisorEvent::Ended { main_code: Some(0) })
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn missing_program_is_reported() {
    let dir = scratch("missing");
    let spec = LaunchSpec {
        program: dir.join("nope"),
        args: vec![],
        cwd: dir.clone(),
        env: vec![],
    };
    let (events, _) = run_session(&spec, &dir).await;
    assert!(matches!(
        events.as_slice(),
        [SupervisorEvent::Failed { .. }]
    ));
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn stop_request_terminates_the_game() {
    let dir = scratch("stop");
    let spec = sh("sleep 30 & wait", &dir);
    let started = Instant::now();
    let mut handle = SessionHandle::start(
        Path::new(env!("CARGO_BIN_EXE_slatty")),
        &spec,
        &dir.join("game.log"),
    )
    .await
    .unwrap();
    assert!(matches!(
        handle.next_event().await.unwrap(),
        Some(SupervisorEvent::Started { .. })
    ));
    handle.request_stop();
    let outcome = handle.wait().await.unwrap();
    assert!(outcome.clean);
    assert!(started.elapsed() < Duration::from_secs(10));
    std::fs::remove_dir_all(dir).unwrap();
}

/// Real Proton check: `SLATTY_TEST_PROTON=<dir> cargo test -- --ignored proton`.
/// Creates a throwaway prefix; `start` detaches ping from cmd, which exits at once.
#[tokio::test]
#[ignore]
async fn proton_session_waits_for_detached_windows_process() {
    let proton = std::env::var("SLATTY_TEST_PROTON").expect("SLATTY_TEST_PROTON");
    let umu = slatty_core::doctor::find_in_path("umu-run").expect("umu-run in PATH");
    let dir = scratch("proton");
    let cwd = std::env::var("SLATTY_TEST_CWD")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.clone());
    let spec = LaunchSpec {
        program: umu,
        args: [
            "cmd",
            "/c",
            "start",
            "",
            "cmd",
            "/c",
            "ping -n 6 127.0.0.1 >nul & echo done>C:\\mark.txt",
        ]
        .map(String::from)
        .to_vec(),
        cwd: cwd.clone(),
        env: vec![
            ("WINEPREFIX".into(), dir.join("pfx").display().to_string()),
            ("PROTONPATH".into(), proton),
            ("GAMEID".into(), "umu-0".into()),
            ("UMU_RUNTIME_UPDATE".into(), "0".into()),
            (
                "STEAM_COMPAT_INSTALL_PATH".into(),
                cwd.display().to_string(),
            ),
        ],
    };
    let started = Instant::now();
    let mut handle = SessionHandle::start(
        Path::new(env!("CARGO_BIN_EXE_slatty")),
        &spec,
        &dir.join("game.log"),
    )
    .await
    .unwrap();
    let mut main_exit = None;
    while let Some(e) = handle.next_event().await.unwrap() {
        eprintln!("{:>6.1}s {e:?}", started.elapsed().as_secs_f32());
        if let SupervisorEvent::MainExited { .. } = e {
            main_exit = Some(started.elapsed());
        }
    }
    let total = started.elapsed();
    eprintln!(
        "--- game.log ---\n{}",
        std::fs::read_to_string(dir.join("game.log")).unwrap_or_default()
    );
    assert!(
        dir.join("pfx/drive_c/mark.txt").exists(),
        "detached process had not finished when the session ended"
    );
    let lingering = std::process::Command::new("pgrep")
        .args(["-x", "ping.exe"])
        .output()
        .unwrap();
    assert!(
        lingering.stdout.is_empty(),
        "ping.exe still running after the session ended"
    );
    eprintln!("main exited at {main_exit:?}, session ended at {total:?}");
    std::fs::remove_dir_all(dir).unwrap();
}
