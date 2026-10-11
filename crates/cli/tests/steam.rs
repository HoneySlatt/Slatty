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

/// Steam's `appinfo.vdf` (version 41) describing one game.
fn appinfo(app_id: u32, name: &str) -> Vec<u8> {
    let key = |index: u32| index.to_le_bytes();
    let mut kv = vec![0x00];
    kv.extend(key(0)); // appinfo
    kv.push(0x00);
    kv.extend(key(1)); // common
    for (k, v) in [(2, name), (3, "game")] {
        kv.push(0x01);
        kv.extend(key(k));
        kv.extend(v.as_bytes());
        kv.push(0);
    }
    kv.extend([0x08, 0x08, 0x08]);
    let mut body = Vec::new();
    body.extend(app_id.to_le_bytes());
    body.extend(((60 + kv.len()) as u32).to_le_bytes());
    body.extend([0u8; 60]);
    body.extend(kv);
    body.extend(0u32.to_le_bytes());
    let mut file = Vec::new();
    file.extend(0x0756_4429u32.to_le_bytes());
    file.extend(1u32.to_le_bytes());
    file.extend((16 + body.len() as u64).to_le_bytes());
    file.extend(body);
    file.extend(4u32.to_le_bytes());
    for s in ["appinfo", "common", "name", "type"] {
        file.extend(s.as_bytes());
        file.push(0);
    }
    file
}

/// `slatty steam status` reads the Steam client's files and changes nothing; `slatty library list`
/// shows Steam's games once they are turned on, with no GOG account needed.
#[test]
fn steam_games_are_listed_once_turned_on() {
    let root = std::env::temp_dir().join(format!("slatty-cli-steam-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).unwrap();
    assert!(stdout(&slatty(&root, &["steam", "status"])).contains("Steam was not found"));

    let steam = root.join("home/.local/share/Steam");
    let write = |rel: &str, data: &[u8]| {
        let path = steam.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    };
    write(
        "config/loginusers.vdf",
        br#""users" { "76561198000000002" { "PersonaName" "[FAKE] Player" "Timestamp" "200" } }"#,
    );
    write("userdata/39734274/config/librarycache/440.json", b"[]");
    write("appcache/appinfo.vdf", &appinfo(440, "[FAKE] Fortress"));
    write(
        "steamapps/appmanifest_440.acf",
        br#""AppState" { "appid" "440" "name" "[FAKE] Fortress" "installdir" "Fortress" "StateFlags" "4" }"#,
    );
    let before = std::fs::read_dir(steam.join("config")).unwrap().count();

    let status = stdout(&slatty(&root, &["steam", "status"]));
    assert!(status.contains("[FAKE] Player"), "{status}");
    assert!(status.contains("Games:        1 (1 installed"), "{status}");
    assert!(status.contains("In the library: off"), "{status}");
    assert!(
        !slatty(&root, &["library", "list"]).status.success(),
        "no GOG account and Steam's games off"
    );

    let db = slatty_core::db::Db::open(&root.join("data/slatty/state.db")).unwrap();
    slatty_core::settings::set_steam_games(&db, true).unwrap();
    drop(db);
    let list = stdout(&slatty(&root, &["library", "list"]));
    assert!(
        list.contains("steam-440") && list.contains("[FAKE] Fortress"),
        "{list}"
    );
    assert!(
        stdout(&slatty(&root, &["library", "list", "nothing"]))
            .lines()
            .count()
            == 1
    );
    assert_eq!(
        std::fs::read_dir(steam.join("config")).unwrap().count(),
        before,
        "nothing written into Steam's folders"
    );
    std::fs::remove_dir_all(root).unwrap();
}
