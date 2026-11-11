// Its checks are Linux's: umu, the Secret Service over D-Bus.
#![cfg(unix)]

use std::process::Command;

/// `slatty doctor` fails when a check fails, so that a script sees it without reading the lines:
/// here no tool can be found and no keyring reached (a session bus that does not exist, so the
/// user's own is never asked).
#[test]
fn doctor_fails_when_a_check_fails() {
    let root = std::env::temp_dir().join(format!("slatty-cli-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slatty"))
        .arg("doctor")
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        )
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("[FAIL] umu-launcher"), "{stdout}");
    assert!(!output.status.success(), "{stdout}");
    assert!(stderr.contains("checks failed"), "{stderr}");
    std::fs::remove_dir_all(&root).unwrap();
}

/// Slatty runs with core dumps off, and so do the programs it starts, as its memory holds the
/// session tokens (ADR-0037): here a fake `bwrap`, which `doctor` starts, writes the core size
/// limit it was given, Slatty having been started with the highest one allowed.
#[test]
fn core_dumps_are_off_for_slatty_and_what_it_starts() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("slatty-cli-core-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("bin")).unwrap();
    let bwrap = root.join("bin/bwrap");
    std::fs::write(
        &bwrap,
        "#!/bin/sh\nulimit -c > \"$SLATTY_TEST_LIMIT\"\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&bwrap, std::fs::Permissions::from_mode(0o755)).unwrap();
    let limit = root.join("limit");
    Command::new("/bin/sh")
        .args([
            "-c",
            "ulimit -S -c \"$(ulimit -H -c)\"; ulimit -c > \"$SLATTY_TEST_LIMIT.start\"; \
             exec \"$0\" doctor",
        ])
        .arg(env!("CARGO_BIN_EXE_slatty"))
        .env_clear()
        .env("PATH", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.join("no-bus").display()),
        )
        .env("SLATTY_TEST_LIMIT", &limit)
        .output()
        .unwrap();
    // Started at 0, Slatty would pass without doing anything.
    let start = std::fs::read_to_string(root.join("limit.start")).unwrap();
    assert_ne!(
        start.trim(),
        "0",
        "a hard limit of 0 here: nothing to prove"
    );
    assert_eq!(std::fs::read_to_string(&limit).unwrap().trim(), "0");
    std::fs::remove_dir_all(&root).unwrap();
}
