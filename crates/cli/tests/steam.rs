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
    assert!(status.contains("Steam Web API: off"), "{status}");
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

/// `slatty installs --json` lists what the Installed shelf shows: Steam's installed games, to start
/// through Steam, and the others with the title and cover given in the library, hidden ones left
/// out.
#[test]
fn installed_games_are_printed_as_json() {
    let root = std::env::temp_dir().join(format!("slatty-cli-installed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let steam = root.join("home/.local/share/Steam");
    let write = |path: &Path, data: &[u8]| {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    };
    write(
        &steam.join("config/loginusers.vdf"),
        br#""users" { "76561198000000002" { "PersonaName" "[FAKE] Player" "Timestamp" "200" } }"#,
    );
    write(
        &steam.join("userdata/39734274/config/librarycache/440.json"),
        b"[]",
    );
    write(
        &steam.join("appcache/appinfo.vdf"),
        &appinfo(440, "[FAKE] Fortress"),
    );
    let steam_cover = steam.join("appcache/librarycache/440/library_600x900.jpg");
    write(&steam_cover, b"[FAKE] cover");
    write(
        &steam.join("steamapps/appmanifest_440.acf"),
        br#""AppState" { "appid" "440" "name" "[FAKE] Fortress" "installdir" "Fortress" "StateFlags" "4" }"#,
    );
    for title in ["[FAKE] Added", "[FAKE] Hidden"] {
        let program = root.join("games").join(title).join("run.sh");
        write(&program, b"#!/bin/sh\n");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        stdout(&slatty(
            &root,
            &["add", program.to_str().unwrap(), "--title", title],
        ));
    }

    let db = slatty_core::db::Db::open(&root.join("data/slatty/state.db")).unwrap();
    slatty_core::settings::set_steam_games(&db, true).unwrap();
    let mut dirs = slatty_core::paths::Dirs::under(&root);
    dirs.data = root.join("data/slatty");
    let changes = |title: &str, hidden| slatty_core::custom::Changes {
        title: title.into(),
        sort_title: String::new(),
        hidden,
        cover: slatty_core::custom::ImageChange::Keep,
        background: slatty_core::custom::ImageChange::Keep,
    };
    slatty_core::custom::save(&db, &dirs, "local-1", changes("[FAKE] Renamed", false)).unwrap();
    slatty_core::custom::save(&db, &dirs, "local-2", changes("", true)).unwrap();
    // An Epic game installed, and one only owned, as the library was last read from Epic.
    let account = "0123456789abcdef0123456789abcdef";
    dirs.cache = root.join("cache/slatty");
    slatty_core::settings::set_epic_account(&db, Some(account)).unwrap();
    let epic_game = |app: &str, title: &str| slatty_core::epic::library::EpicGame {
        app_name: app.into(),
        namespace: "fakens".into(),
        catalog_item_id: format!("fakeitem{app}"),
        title: title.into(),
        cover: None,
        background: None,
        logo: None,
        os: vec!["windows".into()],
    };
    slatty_core::epic::library::save_cache(
        &dirs,
        &slatty_core::epic::library::EpicLibrary {
            account_id: account.into(),
            display_name: "[FAKE] Player".into(),
            fetched_at: 1_791_500_000,
            games: vec![
                epic_game("FakeEpic", "[FAKE] Epic"),
                epic_game("FakeOwned", "[FAKE] Owned"),
            ],
            left_out: Vec::new(),
        },
    )
    .unwrap();
    let epic_cover = root.join(format!(
        "cache/slatty/epic/{account}/covers/epic-FakeEpic.jpg"
    ));
    write(&epic_cover, b"[FAKE] cover");
    slatty_core::install::Install {
        umu_id: None,
        game_id: "epic-FakeEpic".into(),
        title: "[FAKE] Epic".into(),
        platform: slatty_core::install::Platform::Windows,
        path: root.join("games/FakeEpic"),
        client_id: None,
        runner: slatty_core::runner::Runner::Umu {
            proton: "/proton".into(),
            prefix: root.join("prefixes/epic-FakeEpic"),
        },
        isolated: false,
    }
    .save(&db)
    .unwrap();
    drop(db);

    let json: serde_json::Value =
        serde_json::from_str(&stdout(&slatty(&root, &["installs", "--json"]))).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            {
                "id": "epic-FakeEpic",
                "title": "[FAKE] Epic",
                "cover": epic_cover,
                "steam_app": null
            },
            {
                "id": "steam-440",
                "title": "[FAKE] Fortress",
                "cover": steam_cover,
                "steam_app": 440
            },
            { "id": "local-1", "title": "[FAKE] Renamed", "cover": null, "steam_app": null }
        ])
    );
    std::fs::remove_dir_all(root).unwrap();
}
