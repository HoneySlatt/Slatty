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
        game_id: "0".into(),
        title: "[FICTIF]".into(),
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
