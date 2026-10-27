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
