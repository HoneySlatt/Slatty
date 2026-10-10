//! Real checks that an isolated game sees nothing of the home folder. They run umu's container,
//! from a folder inside the home folder (umu's runtime cannot see `/tmp/nix-shell.*`):
//! `TMPDIR=$HOME/.cache cargo test -- --ignored isolated` (the Windows one also needs
//! `SLATTY_TEST_PROTON=<Proton dir>`).
//!
//! Each leak closed here is shown first: the same launch without the measure, or not isolated,
//! sees what the isolated one must not.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use slatty_core::error::Error;
use slatty_core::install::{Install, Platform};
use slatty_core::paths::Dirs;
use slatty_core::runner::{self, LaunchSpec, Runner};
use slatty_core::session::SessionHandle;

/// A throwaway game folder, beside a marker the game must not see and a `secret` folder the
/// user's environment could share.
fn fixture(name: &str) -> (PathBuf, Dirs) {
    let root = std::env::temp_dir().join(format!("slatty-isolated-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("game")).unwrap();
    std::fs::create_dir_all(root.join("secret")).unwrap();
    std::fs::write(root.join("marker"), b"[FAKE] personal file").unwrap();
    std::fs::write(root.join("secret/file"), b"[FAKE] personal file").unwrap();
    assert!(
        std::env::var_os("HOME").is_some_and(|h| root.starts_with(h)),
        "run with TMPDIR inside the home folder, or the check proves nothing"
    );
    (root.clone(), Dirs::under(&root.join("slatty")))
}

/// What the user's own environment could tell umu and its container, seen by every launch.
fn user_environment(root: &Path) {
    let secret = root.join("secret");
    // SAFETY: the ignored tests run one at a time (`--test-threads=1`), and every launch here
    // is meant to see these.
    unsafe {
        std::env::set_var("PRESSURE_VESSEL_SHARE_HOME", "1");
        std::env::set_var("STEAM_COMPAT_MOUNT_PATHS", &secret);
        std::env::set_var("PROTON_LOG_DIR", &secret);
        std::env::set_var("WINEPREFIX", &secret);
    }
}

/// The same launch without the given measures: what it sees is what they close.
fn without(spec: &LaunchSpec, keys: &[&str]) -> LaunchSpec {
    let mut spec = spec.clone();
    spec.env.retain(|(k, _)| !keys.contains(&k.as_str()));
    spec
}

/// Runs the launch under the supervisor, with umu kept from downloading anything and from
/// making its default prefix in `~/Games`.
async fn run(spec: &LaunchSpec, log: &Path) {
    let mut spec = spec.clone();
    if !spec.env.iter().any(|(k, _)| k == "WINEPREFIX") {
        let prefix = log.parent().unwrap().join("native-pfx");
        spec.env
            .push(("WINEPREFIX".into(), prefix.display().to_string()));
    }
    spec.env.push(("UMU_RUNTIME_UPDATE".into(), "0".into()));
    for key in ["http_proxy", "https_proxy", "all_proxy"] {
        spec.env.push((key.into(), "http://127.0.0.1:9".into()));
        spec.env
            .push((key.to_uppercase(), "http://127.0.0.1:9".into()));
    }
    let outcome = SessionHandle::start(Path::new(env!("CARGO_BIN_EXE_slatty")), &spec, log, None)
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

fn clear_results(root: &Path) {
    for entry in std::fs::read_dir(root.join("game")).unwrap().flatten() {
        if entry.file_name().to_string_lossy().starts_with("out-") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
}

/// The user's Steam installation, which the container shares with games unless told otherwise.
fn steam_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".steam"))
        .filter(|p| p.is_dir())
}

/// A script telling what it sees of the files beside the game: `out-marker`, `out-secret`,
/// `out-steam` (the user's `~/.steam`, `none` without one), the D-Bus buses, and its home.
fn probe(root: &Path, script: &Path) {
    let game = script.parent().unwrap();
    let see = |label: &str, path: String| {
        format!("if [ -e '{path}' ]; then echo VISIBLE; else echo hidden; fi > out-{label}\n")
    };
    let steam = match steam_dir() {
        Some(dir) => see("steam", dir.display().to_string()),
        None => "echo none > out-steam\n".into(),
    };
    std::fs::write(
        script,
        format!(
            "#!/bin/sh\ncd '{}'\n{}{}{}\
             if [ -S /run/pressure-vessel/bus ] || [ -S \"$XDG_RUNTIME_DIR/bus\" ]; \
             then echo REACHABLE; else echo hidden; fi > out-bus\n\
             if [ -S /run/dbus/system_bus_socket ]; \
             then echo REACHABLE; else echo hidden; fi > out-system-bus\n\
             echo \"$HOME\" > out-home\ntouch \"$HOME/written-by-the-game\"\n",
            game.display(),
            see("marker", root.join("marker").display().to_string()),
            see("secret", root.join("secret/file").display().to_string()),
            steam,
        ),
    )
    .unwrap();
    std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn linux_install(root: &Path) -> Install {
    Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Linux,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Native,
        umu_id: None,
        isolated: true,
    }
}

#[tokio::test]
#[ignore]
async fn an_isolated_linux_game_sees_nothing_of_the_home_folder() {
    let (root, dirs) = fixture("linux");
    user_environment(&root);
    probe(&root, &root.join("game/start.sh"));
    std::fs::write(root.join("game/gameinfo"), b"[FAKE] Game\n").unwrap();
    let mut install = linux_install(&root);
    let spec = runner::launch_spec(&dirs, &install, None).unwrap();
    let log = root.join("game.log");

    // Without the measures, the user's environment shares the home folder and the folders it
    // names, umu its prefix, and the container the Steam installation: the checks below mean
    // something.
    let leaks = without(
        &spec,
        &[
            "PRESSURE_VESSEL_SHARE_HOME",
            "STEAM_COMPAT_MOUNT_PATHS",
            "PROTON_LOG_DIR",
            "WINEPREFIX",
        ],
    );
    run(&leaks, &log).await;
    assert_eq!(result(&root, "out-marker"), "VISIBLE");
    assert_eq!(result(&root, "out-secret"), "VISIBLE");
    clear_results(&root);
    let leaks = without(&spec, &["STEAM_COMPAT_MOUNT_PATHS"]);
    run(&leaks, &log).await;
    assert_eq!(result(&root, "out-marker"), "hidden");
    assert_eq!(result(&root, "out-secret"), "VISIBLE");
    clear_results(&root);
    let leaks = without(&spec, &["HOME"]);
    run(&leaks, &log).await;
    assert_eq!(result(&root, "out-marker"), "hidden");
    if steam_dir().is_some() {
        assert_eq!(
            result(&root, "out-steam"),
            "VISIBLE",
            "the Steam installation"
        );
    }
    clear_results(&root);

    run(&spec, &log).await;
    assert_eq!(result(&root, "out-marker"), "hidden");
    assert_eq!(result(&root, "out-secret"), "hidden");
    assert_ne!(result(&root, "out-steam"), "VISIBLE");
    assert_eq!(result(&root, "out-bus"), "hidden", "the D-Bus session bus");
    assert_eq!(
        result(&root, "out-system-bus"),
        "hidden",
        "the D-Bus system bus"
    );
    // Its home is its own: what it writes there lands in SlattyLauncher's folder for it.
    let home = runner::isolated_home(&dirs, "1").unwrap();
    assert_eq!(result(&root, "out-home"), home.display().to_string());
    assert!(home.join("written-by-the-game").is_file());
    clear_results(&root);

    // Not isolated, the same game sees the marker and the bus: the checks above mean something.
    install.isolated = false;
    let spec = runner::launch_spec(&dirs, &install, None).unwrap();
    run(&spec, &log).await;
    assert_eq!(result(&root, "out-marker"), "VISIBLE");
    assert_eq!(result(&root, "out-bus"), "REACHABLE");
    assert_eq!(result(&root, "out-system-bus"), "REACHABLE");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore]
async fn an_isolated_game_cannot_plant_a_program_umu_would_take_for_its_own() {
    let (root, dirs) = fixture("planted");
    probe(&root, &root.join("game/start.sh"));
    let install = linux_install(&root);
    let spec = runner::launch_spec(&dirs, &install, None).unwrap();
    assert_eq!(spec.args[0], "sh");

    // A file named like the interpreter, left in the game's folder by an earlier session: umu
    // takes the folder for the game's and shares the whole filesystem under it.
    std::fs::copy(root.join("game/start.sh"), root.join("game/sh")).unwrap();
    run(&spec, &root.join("game.log")).await;
    assert_eq!(result(&root, "out-marker"), "VISIBLE");

    // The launch, checked again just before the game starts, refuses it.
    assert!(matches!(
        runner::launch_spec(&dirs, &install, None),
        Err(Error::Refused(_))
    ));
    std::fs::remove_file(root.join("game/sh")).unwrap();
    assert!(runner::launch_spec(&dirs, &install, None).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore]
async fn an_isolated_game_cannot_lead_its_working_directory_out_through_a_link() {
    let (root, dirs) = fixture("cwd-link");
    probe(&root, &root.join("game/start.sh"));
    let install = linux_install(&root);
    let mut spec = runner::launch_spec(&dirs, &install, None).unwrap();

    // A link the game made in its folder, named as the working directory of its next launch:
    // the container shares the working directory as it is on disk.
    std::os::unix::fs::symlink(root.join("secret"), root.join("game/wd")).unwrap();
    spec.cwd = root.join("game/wd");
    run(&spec, &root.join("game.log")).await;
    assert_eq!(result(&root, "out-marker"), "hidden");
    assert_eq!(result(&root, "out-secret"), "VISIBLE");

    // A Windows game starts in the working directory its `goggame-*.info` names: refused when
    // that leads out of the game folder.
    std::fs::create_dir_all(root.join("proton")).unwrap();
    std::fs::write(root.join("proton/proton"), b"").unwrap();
    let windows = Install {
        platform: Platform::Windows,
        runner: Runner::Umu {
            proton: root.join("proton"),
            prefix: root.join("pfx"),
        },
        ..install
    };
    let start = |cwd: PathBuf| {
        runner::windows_command(
            &dirs,
            &windows,
            vec![root.join("game/game.exe").display().to_string()],
            cwd,
        )
    };
    assert!(matches!(
        start(root.join("game/wd")),
        Err(Error::Refused(_))
    ));
    assert!(start(root.join("game")).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore]
async fn a_game_folder_the_container_cannot_be_told_is_refused_and_a_backslash_is_fine() {
    let (root, dirs) = fixture("names");
    for (name, shared) in [("Game\\Back", true), ("Game: Remastered", false)] {
        let game = root.join(name);
        std::fs::create_dir_all(&game).unwrap();
        probe(&root, &game.join("start.sh"));
        let install = Install {
            path: game.clone(),
            ..linux_install(&root)
        };
        let spec = runner::launch_spec(&dirs, &install, None);
        if !shared {
            // pressure-vessel splits its lists on every `:`: the folder would not be shared and
            // the game would not start, rather than start with more.
            assert!(matches!(spec, Err(Error::Refused(_))), "{name}");
            continue;
        }
        let spec = spec.unwrap();
        run(&spec, &root.join("game.log")).await;
        // The script writes beside the game: its folder was shared, writable.
        let marker = std::fs::read_to_string(game.join("out-marker")).unwrap();
        assert_eq!(marker.trim(), "hidden", "{name}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore]
async fn an_isolated_windows_game_sees_nothing_of_the_home_folder_through_z() {
    let proton = std::env::var("SLATTY_TEST_PROTON").expect("SLATTY_TEST_PROTON");
    let (root, dirs) = fixture("windows");
    user_environment(&root);
    let windows = |p: &Path| format!("Z:{}", p.display()).replace('/', "\\");
    let see = |label: &str, path: String| {
        format!(
            "if exist \"{path}\" (echo VISIBLE> {label}.txt) else (echo hidden> {label}.txt)\r\n"
        )
    };
    let steam = match steam_dir() {
        Some(dir) => see("steam", windows(&dir)),
        None => "echo none> steam.txt\r\n".into(),
    };
    std::fs::write(
        root.join("game/check.bat"),
        format!(
            "@echo off\r\n{}{}{steam}",
            see("result", windows(&root.join("marker"))),
            see("secret", windows(&root.join("secret/file")))
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
    };
    let log = root.join("game.log");
    let spec = run_in_game(&install, "cmd".into()).unwrap();
    // Without the measures, the user's environment shares the home folder and the folder it
    // names, and the container the Steam installation: the checks below mean something.
    run(
        &without(&spec, &["PRESSURE_VESSEL_SHARE_HOME", "HOME"]),
        &log,
    )
    .await;
    assert_eq!(result(&root, "result.txt"), "VISIBLE");
    assert_eq!(result(&root, "secret.txt"), "VISIBLE");
    if steam_dir().is_some() {
        assert_eq!(result(&root, "steam.txt"), "VISIBLE");
    }
    run(&spec, &log).await;
    assert_eq!(result(&root, "result.txt"), "hidden");
    assert_eq!(result(&root, "secret.txt"), "hidden");
    assert_ne!(result(&root, "steam.txt"), "VISIBLE");

    // A program given by its path on this computer, as a game's is.
    std::fs::remove_file(root.join("game/result.txt")).unwrap();
    let exe = root.join("game/game.exe");
    std::fs::copy(root.join("pfx/drive_c/windows/system32/cmd.exe"), &exe).unwrap();
    let exe = exe.display().to_string();
    run(&run_in_game(&install, exe.clone()).unwrap(), &log).await;
    assert_eq!(result(&root, "result.txt"), "hidden");

    // A file named like the program as umu sees it (`Z:\...`, or `sc` for the Galaxy service),
    // left in the game's folder: umu would take the folder for the game's and share the whole
    // filesystem under it.
    let planted = root.join("game").join(windows(Path::new(&exe)));
    std::fs::write(&planted, b"").unwrap();
    assert!(matches!(
        run_in_game(&install, exe.clone()),
        Err(Error::Refused(_))
    ));
    std::fs::remove_file(planted).unwrap();
    std::fs::write(root.join("game/sc"), b"").unwrap();
    assert!(matches!(
        run_in_game(&install, "sc".into()),
        Err(Error::Refused(_))
    ));
    assert!(run_in_game(&install, exe.clone()).is_ok());

    // Not isolated, the same program sees the marker: the checks above mean something.
    install.isolated = false;
    run(&run_in_game(&install, exe).unwrap(), &log).await;
    assert_eq!(result(&root, "result.txt"), "VISIBLE");
    std::fs::remove_dir_all(root).unwrap();
}
