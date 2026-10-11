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

/// Builds that cannot be linked into Steam's folder (here a file where its folder should be) are
/// only left out of Steam and Heroic: the other `slatty proton` commands still work, sharing can
/// still be turned off, and turning it on says it failed.
#[test]
fn a_build_that_cannot_be_shared_stops_no_proton_command() {
    let root = std::env::temp_dir().join(format!("slatty-share-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let steam = root.join("home/.local/share/Steam");
    std::fs::create_dir_all(&steam).unwrap();
    std::fs::write(steam.join("compatibilitytools.d"), b"").unwrap();
    std::fs::create_dir_all(root.join("runtime")).unwrap();
    let build = root.join("data/slatty/protons/GE-Proton11-7-x86_64");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("proton"), b"").unwrap();

    let on = slatty(&root, &["proton", "share", "on"]);
    assert!(!on.status.success(), "the change asked for failed");

    let list = slatty(&root, &["proton", "list"]);
    assert!(
        list.status.success(),
        "{}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(String::from_utf8_lossy(&list.stdout).contains("GE-Proton11-7-x86_64"));
    assert!(String::from_utf8_lossy(&list.stderr).contains("not shared with Steam and Heroic"));

    let off = slatty(&root, &["proton", "share", "off"]);
    assert!(
        off.status.success(),
        "{}",
        String::from_utf8_lossy(&off.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}
