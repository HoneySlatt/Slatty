use super::*;
use crate::cloud::CloudRequest;
use crate::emulators::EmulatorsMsg;
use slatty_core::emulated::Emulator;

fn emulated_install(id: &str, title: &str) -> Install {
    Install {
        umu_id: None,
        game_id: id.into(),
        title: format!("[FAKE] {title}"),
        platform: Platform::Linux,
        path: PathBuf::from("/games/PS2"),
        client_id: None,
        runner: Runner::Native,
        isolated: false,
    }
}

/// The library with a PlayStation 2 game run in PCSX2.
fn emulated_app() -> App {
    let mut app = library_app();
    app.installs
        .insert("emu-1".into(), emulated_install("emu-1", "Disc"));
    app.emulated.insert("emu-1".into(), Emulator::Pcsx2);
    app.merge_steam();
    app
}

/// An emulated game shows beside GOG's, says what runs it, and nothing about it is asked of GOG.
#[test]
fn an_emulated_game_shows_in_the_library_and_asks_gog_nothing() {
    let mut app = emulated_app();
    let game = app.library.iter().find(|g| g.id == "emu-1").unwrap();
    assert_eq!(
        (game.title.as_str(), game.os.as_slice()),
        ("[FAKE] Disc", &["ps2".to_string()][..])
    );

    let _ = app.update(Message::Select("emu-1".into()));
    settle(&mut app);
    {
        let mut ui = render(&app);
        assert!(ui.find("Emulated").is_ok());
        assert!(ui.find("Runs in PCSX2").is_ok());
        assert!(ui.find("PlayStation 2").is_ok());
        assert!(ui.find("Native Linux build").is_err());
        snapshot(&mut ui, "emulated-game");
    }
    let _ = app.update(Message::LoadAchievements("emu-1".into()));
    assert!(!app.achievements.contains_key("emu-1"));
    let _ = app.update(Message::Cloud("emu-1".into(), CloudRequest::Check));
    assert!(!app.cloud.contains_key("emu-1"));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::LoadContent(
        "emu-1".into(),
    )));
    assert!(!app.maintenance.contains_key("emu-1"));
}

/// Its settings name its emulator, and neither Proton nor isolation.
#[test]
fn an_emulated_games_settings_name_its_emulator() {
    let mut app = emulated_app();
    open(&mut app, "emu-1", Some(Panel::GameSettings));
    let mut ui = render(&app);
    assert!(ui.find("Emulator").is_ok());
    assert!(ui.find("PCSX2").is_ok());
    assert!(
        ui.find("Not found: install it, or choose its program in Settings → Emulators.")
            .is_ok()
    );
    assert!(ui.find("Isolation").is_err());
    assert!(ui.find("Locate…").is_ok(), "found again where it was moved");
}

/// The Emulated filter keeps the emulated games alone, and a system those of that system; the
/// system is kept with the other filters.
#[test]
fn emulated_games_are_filtered_by_system() {
    let mut app = emulated_app();
    app.installs
        .insert("emu-2".into(), emulated_install("emu-2", "Cartridge"));
    app.emulated.insert("emu-2".into(), Emulator::Eden);
    app.merge_steam();
    let _ = app.update(Message::SetFilters(Filters {
        emulated: true,
        ..Default::default()
    }));
    assert_eq!(titles(&app), ["Cartridge", "Disc"]);
    let _ = app.update(Message::SetFilters(Filters {
        system: Some(Emulator::Eden),
        ..Default::default()
    }));
    assert_eq!(titles(&app), ["Cartridge"]);

    let db = app.core.as_ref().unwrap().db.clone();
    slatty_core::settings::set_remember_filters(&db, true).unwrap();
    app.remember_filters = true;
    let _ = app.update(Message::SetFilters(Filters {
        emulated: true,
        system: Some(Emulator::Eden),
        ..Default::default()
    }));
    let (_, kept) = crate::library::kept_filters(&db).unwrap();
    assert_eq!(kept, Some(app.filters));
}

/// Without emulated games, the library's filters say nothing of them.
#[test]
fn the_filters_offer_no_system_without_emulated_games() {
    let mut app = library_app();
    app.filters_open = true;
    let mut ui = render(&app);
    assert!(ui.find("Added").is_ok());
    assert!(ui.find("Emulated").is_err());
    assert!(ui.find("All systems").is_err());
}

/// Settings → Emulators lists every emulator, whether its program was found, and the folders
/// scanned; a scan's games join the library.
#[test]
fn a_scan_adds_its_games_to_the_library() {
    let mut app = library_app();
    app.page = Page::Settings;
    app.emulators
        .programs
        .insert(Emulator::Dolphin, "/bin/dolphin-emu".into());
    app.emulators
        .folders
        .insert(Emulator::Dolphin, vec!["/games/[FAKE] GameCube".into()]);
    app.window = Size::new(1440.0, 4000.0);
    {
        let mut ui = render_window(&app);
        let shown = texts(&mut ui);
        for e in Emulator::ALL {
            assert!(shown.iter().any(|t| t == e.name()), "{e:?}");
        }
        assert!(
            shown
                .iter()
                .any(|t| t == "GameCube / Wii · Found: /bin/dolphin-emu")
        );
        assert!(shown.iter().any(|t| t == "/games/[FAKE] GameCube"));
        snapshot(&mut ui, "settings-emulators");
        ui.click("Scan").unwrap();
        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Emulators(EmulatorsMsg::Scan)))
        );
    }
    let _ = app.update(Message::Emulators(EmulatorsMsg::Scan));
    assert!(app.emulators.scanning);
    let mut found = emulated_install("emu-3", "Found");
    found.path = "/games/[FAKE] GameCube".into();
    let _ = app.update(Message::Emulators(EmulatorsMsg::Scanned(Ok((
        vec![(found, Emulator::Dolphin)],
        0,
    )))));
    assert!(!app.emulators.scanning);
    assert!(app.library.iter().any(|g| g.id == "emu-3"));
    assert_eq!(app.emulated.get("emu-3"), Some(&Emulator::Dolphin));
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|n| n.text == "1 game was added to the library.")
    );
}

/// Removed, an emulated game leaves the library, its files untouched.
#[test]
fn a_removed_emulated_game_leaves_the_library() {
    let mut app = emulated_app();
    let _ = app.update(Message::Select("emu-1".into()));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Uninstalled(
        "emu-1".into(),
        Ok(Default::default()),
    )));
    assert!(!app.library.iter().any(|g| g.id == "emu-1"));
    assert!(!app.emulated.contains_key("emu-1"));
    assert!(app.selected.is_none());
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|n| n.text.contains("its files are untouched"))
    );
}

/// The Add dialog adds an emulated game by its file, guessing its emulator from it.
#[test]
fn an_emulated_game_is_added_from_the_library() {
    use crate::add::AddMsg;
    let mut app = library_app();
    let _ = app.update(Message::Add(AddMsg::Open));
    let _ = app.update(Message::Add(AddMsg::Emulated(true)));
    {
        let mut ui = render(&app);
        assert!(ui.find("Emulator").is_ok());
        assert!(ui.find("Proton").is_err());
        assert!(ui.find("Isolated from your files").is_err());
    }
    let _ = app.update(Message::Add(AddMsg::PickedGame(Some((
        "/games/[FAKE] Switch/Game.nsp".into(),
        vec![Emulator::Eden],
    )))));
    let d = app.add_game.as_ref().unwrap();
    assert_eq!(d.emulator, Some(Emulator::Eden));
    assert_eq!(d.game, "/games/[FAKE] Switch/Game.nsp");
    {
        // The emulator's menu cannot be read by the simulator: the state says it.
        let mut ui = render(&app);
        assert!(ui.find("Game file").is_ok());
        snapshot(&mut ui, "add-emulated-game");
    }
    let _ = app.update(Message::Add(AddMsg::SavedEmulated(Err(
        "[FAKE] refused".into()
    ))));
    assert!(render(&app).find("[FAKE] refused").is_ok());
    let _ = app.update(Message::Add(AddMsg::SavedEmulated(Ok((
        emulated_install("emu-5", "Game"),
        Emulator::Eden,
    )))));
    assert!(app.add_game.is_none());
    assert!(app.library.iter().any(|g| g.id == "emu-5"));
    assert_eq!(app.selected.as_deref(), Some("emu-5"));
}
