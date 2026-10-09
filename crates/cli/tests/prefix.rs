use std::path::Path;

use slatty_core::install::{Install, Platform};
use slatty_core::runner::{self, Runner};
use slatty_core::session::SessionHandle;

/// Real Proton check: `SLATTY_TEST_PROTON=<dir> TMPDIR=$HOME/.cache cargo test -- --ignored prefix`.
#[tokio::test]
#[ignore]
async fn fresh_prefix_is_initialised_with_user_folders() {
    let proton = std::env::var("SLATTY_TEST_PROTON").expect("SLATTY_TEST_PROTON");
    let root = std::env::temp_dir().join(format!("slatty-prefix-{}", std::process::id()));
    std::fs::create_dir_all(root.join("game")).unwrap();
    let install = Install {
        umu_id: None,
        game_id: "0".into(),
        title: "[FAKE]".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Umu {
            proton: proton.into(),
            prefix: root.join("pfx"),
        },
    };
    let spec = runner::prefix_init_spec(&install)
        .unwrap()
        .expect("prefix needs initialising");
    let outcome = SessionHandle::start(
        Path::new(env!("CARGO_BIN_EXE_slatty")),
        &spec,
        &root.join("init.log"),
    )
    .await
    .unwrap()
    .wait()
    .await
    .unwrap();
    assert!(
        outcome.clean,
        "{}",
        std::fs::read_to_string(root.join("init.log")).unwrap_or_default()
    );
    assert!(root.join("pfx/drive_c/users/steamuser").is_dir());
    assert!(
        runner::prefix_init_spec(&install).unwrap().is_none(),
        "second call must be a no-op"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Real Proton check of the dummy Galaxy service, from the dev shell (it sets
/// SLATTY_GALAXY_COMMUNICATION): `SLATTY_TEST_PROTON=<dir> TMPDIR=$HOME/.cache cargo test -- --ignored galaxy_service`.
#[tokio::test]
#[ignore]
async fn galaxy_service_is_registered_and_stops_with_the_session() {
    use slatty_core::galaxy_service;
    use slatty_core::paths::Dirs;

    let proton = std::env::var("SLATTY_TEST_PROTON").expect("SLATTY_TEST_PROTON");
    let exe = std::env::var(galaxy_service::ENV).expect(galaxy_service::ENV);
    let root = std::env::temp_dir().join(format!("slatty-service-{}", std::process::id()));
    std::fs::create_dir_all(root.join("game")).unwrap();
    let install = Install {
        umu_id: None,
        game_id: "0".into(),
        title: "[FAKE]".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Umu {
            proton: proton.into(),
            prefix: root.join("pfx"),
        },
    };
    let supervisor = Path::new(env!("CARGO_BIN_EXE_slatty"));
    let dirs = Dirs::under(&root.join("slatty"));
    let run = |args: &[&str], log: &str| {
        let spec = runner::windows_command(
            &install,
            args.iter().map(|s| s.to_string()).collect(),
            install.path.clone(),
        )
        .unwrap();
        let log = root.join(log);
        async move {
            SessionHandle::start(supervisor, &spec, &log)
                .await
                .unwrap()
                .wait()
                .await
                .unwrap()
        }
    };
    run(&["wineboot", "-u"], "init.log").await;

    let changed = galaxy_service::ensure(&dirs, &install, Path::new(&exe), supervisor)
        .await
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(changed);
    assert!(galaxy_service::installed(&root.join("pfx")));
    let again = galaxy_service::ensure(&dirs, &install, Path::new(&exe), supervisor)
        .await
        .unwrap();
    assert!(!again, "second call must be a no-op");

    // A game wakes the service up; the session must still end once the game is gone.
    let started = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        run(&["sc", "start", "GalaxyCommunication"], "start.log"),
    )
    .await
    .expect("the session did not end while the service was running");
    let log = std::fs::read_to_string(root.join("start.log")).unwrap_or_default();
    assert!(started.clean, "{log}");
    println!("sc start: {:?}\n{log}", started.main_code);
    std::fs::remove_dir_all(root).unwrap();
}
