//! Real checks that an isolated game sees nothing of the home folder. They run umu's container,
//! from a folder inside the home folder (umu's runtime cannot see `/tmp/nix-shell.*`):
//! `TMPDIR=$HOME/.cache cargo test -- --ignored isolated` (the Windows one also needs
//! `SLATTY_TEST_PROTON=<Proton dir>`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use slatty_core::install::{Install, Platform};
use slatty_core::paths::Dirs;
use slatty_core::runner::{self, LaunchSpec, Runner};
use slatty_core::session::SessionHandle;

/// A throwaway game folder, beside a marker the game must not see.
fn fixture(name: &str) -> (PathBuf, Dirs) {
    let root = std::env::temp_dir().join(format!("slatty-isolated-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("game")).unwrap();
    std::fs::write(root.join("marker"), b"[FAKE] personal file").unwrap();
    assert!(
        std::env::var_os("HOME").is_some_and(|h| root.starts_with(h)),
        "run with TMPDIR inside the home folder, or the check proves nothing"
    );
    (root.clone(), Dirs::under(&root.join("slatty")))
}

async fn run(spec: &LaunchSpec, log: &Path) {
    let outcome = SessionHandle::start(Path::new(env!("CARGO_BIN_EXE_slatty")), spec, log, None)
        .await
        .unwrap()
        .wait()
        .await
        .unwrap();
    assert!(
        outcome.clean,
        "{}",
        std::fs::read_to_string(log).unwrap_or_default()
    );
}

fn result(root: &Path, file: &str) -> String {
    std::fs::read_to_string(root.join("game").join(file))
        .unwrap_or_else(|_| {
            let log = std::fs::read_to_string(root.join("game.log")).unwrap_or_default();
            panic!("the game wrote no result:\n{log}")
        })
        .trim()
        .to_string()
}

#[tokio::test]
#[ignore]
async fn an_isolated_linux_game_sees_nothing_of_the_home_folder() {
    let (root, dirs) = fixture("linux");
    let marker = root.join("marker");
    let script = root.join("game/start.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\n\
             if [ -e '{}' ]; then echo VISIBLE; else echo hidden; fi > result\n\
             echo \"$HOME\" > home\ntouch \"$HOME/written-by-the-game\"\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(root.join("game/gameinfo"), b"[FAKE] Game\n").unwrap();
    let mut install = Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Linux,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Native,
        umu_id: None,
        isolated: true,
    };
    let spec = runner::launch_spec(&dirs, &install, None).unwrap();
    run(&spec, &root.join("game.log")).await;
    assert_eq!(result(&root, "result"), "hidden");
    // Its home is its own: what it writes there lands in SlattyLauncher's folder for it.
    let home = runner::isolated_home(&dirs, "1").unwrap();
    assert!(home.join("written-by-the-game").is_file());

    // Not isolated, the same game sees the marker: the check above means something.
    install.isolated = false;
    let spec = runner::launch_spec(&dirs, &install, None).unwrap();
    run(&spec, &root.join("game.log")).await;
    assert_eq!(result(&root, "result"), "VISIBLE");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore]
async fn an_isolated_windows_game_sees_nothing_of_the_home_folder_through_z() {
    let proton = std::env::var("SLATTY_TEST_PROTON").expect("SLATTY_TEST_PROTON");
    let (root, dirs) = fixture("windows");
    let marker = format!("Z:{}", root.join("marker").display()).replace('/', "\\");
    std::fs::write(
        root.join("game/check.bat"),
        format!(
            "@echo off\r\nif exist \"{marker}\" (echo VISIBLE> result.txt) else (echo hidden> result.txt)\r\n"
        ),
    )
    .unwrap();
    let mut install = Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Umu {
            proton: proton.into(),
            prefix: root.join("pfx"),
        },
        umu_id: None,
        isolated: true,
    };
    let run_in_game = |install: &Install, program: String| {
        runner::windows_command(
            &dirs,
            install,
            vec![program, "/c".into(), "check.bat".into()],
            install.path.clone(),
        )
        .unwrap()
    };
    run(&run_in_game(&install, "cmd".into()), &root.join("game.log")).await;
    assert_eq!(result(&root, "result.txt"), "hidden");

    // A program given by its path on this computer, as a game's is.
    std::fs::remove_file(root.join("game/result.txt")).unwrap();
    let exe = root.join("game/game.exe");
    std::fs::copy(root.join("pfx/drive_c/windows/system32/cmd.exe"), &exe).unwrap();
    let exe = exe.display().to_string();
    run(&run_in_game(&install, exe.clone()), &root.join("game.log")).await;
    assert_eq!(result(&root, "result.txt"), "hidden");

    // Not isolated, the same program sees the marker: the checks above mean something.
    install.isolated = false;
    run(&run_in_game(&install, exe), &root.join("game.log")).await;
    assert_eq!(result(&root, "result.txt"), "VISIBLE");
    std::fs::remove_dir_all(root).unwrap();
}
