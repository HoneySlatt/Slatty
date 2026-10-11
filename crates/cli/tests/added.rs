use std::path::Path;
use std::process::{Command, Output};

/// `slatty` with a home and data folders of its own.
fn slatty(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_slatty"))
        .args(args)
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A game is added by its program, listed without a GOG account, started with its arguments, and
/// removed with its files left where they are.
#[test]
fn a_game_is_added_listed_and_removed_on_the_command_line() {
    let root = std::env::temp_dir().join(format!("slatty-cli-added-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let game = root.join("games/[FAKE] Game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join("run.sh"), "#!/bin/sh\n").unwrap();
    let program = game.join("run.sh");

    let added = stdout(&slatty(
        &root,
        &[
            "add",
            program.to_str().unwrap(),
            "--title",
            "[FAKE] Game",
            "--",
            "--level",
            "2",
        ],
    ));
    assert!(added.contains("Added [FAKE] Game as local-1"), "{added}");
    let listed = stdout(&slatty(&root, &["library", "list"]));
    assert!(
        listed.contains("local-1") && listed.contains("[FAKE] Game"),
        "{listed}"
    );
    let spec = stdout(&slatty(&root, &["launch-spec", "local-1"]));
    assert!(spec.contains(r#""--level", "2""#), "{spec}");

    let forget = slatty(&root, &["forget", "local-1"]);
    assert!(!forget.status.success(), "removed, not forgotten");
    stdout(&slatty(&root, &["remove", "local-1"]));
    assert!(program.is_file(), "the game's files stay");
    assert!(!stdout(&slatty(&root, &["installs"])).contains("local-1"));

    // A folder holding the home folder is refused.
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("game.sh"), "#!/bin/sh\n").unwrap();
    let wide = slatty(
        &root,
        &[
            "add",
            home.join("game.sh").to_str().unwrap(),
            "--title",
            "[FAKE] Wide",
            "--folder",
            root.to_str().unwrap(),
        ],
    );
    assert!(!wide.status.success());

    // What an installer put in a prefix, the game never added, is thrown away by `remove`.
    let prefix = root.join("data/slatty/prefixes/local-9");
    std::fs::create_dir_all(prefix.join("drive_c/Games")).unwrap();
    stdout(&slatty(&root, &["remove", "local-9"]));
    assert!(!prefix.exists());
    std::fs::remove_dir_all(root).unwrap();
}
