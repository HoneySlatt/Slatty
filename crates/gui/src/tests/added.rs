use super::*;
use crate::cloud::CloudRequest;
use crate::edit::EditMsg;

/// The library with a Linux game added by hand.
fn added_app() -> App {
    let mut app = library_app();
    app.installs.insert(
        "local-1".into(),
        Install {
            umu_id: None,
            game_id: "local-1".into(),
            title: "[FAKE] Mine".into(),
            platform: Platform::Linux,
            path: PathBuf::from("/games/Mine"),
            client_id: None,
            runner: Runner::Native,
            isolated: false,
        },
    );
    app.merge_steam();
    app
}

/// A game added by hand shows beside GOG's, with its own play time, and nothing about it is asked
/// of GOG: no achievements, cloud saves, versions or languages.
#[test]
fn a_game_added_by_hand_shows_in_the_library_and_asks_gog_nothing() {
    let mut app = added_app();
    assert!(
        app.library
            .iter()
            .any(|g| g.id == "local-1" && g.title == "[FAKE] Mine")
    );
    app.playtime.insert(
        "local-1".into(),
        Playtime {
            seconds: 3600,
            last_played: 1_791_500_000,
        },
    );
    assert_eq!(app.played_seconds("local-1"), 3600);

    let _ = app.update(Message::Select("local-1".into()));
    settle(&mut app);
    {
        let mut ui = render(&app);
        assert!(ui.find("Added by hand").is_ok());
        snapshot(&mut ui, "added-game");
    }
    let _ = app.update(Message::LoadAchievements("local-1".into()));
    assert!(!app.achievements.contains_key("local-1"));
    let _ = app.update(Message::Cloud("local-1".into(), CloudRequest::Check));
    assert!(!app.cloud.contains_key("local-1"));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::LoadContent(
        "local-1".into(),
    )));
    assert!(!app.maintenance.contains_key("local-1"));
}

/// Signed out of GOG, or never signed in, the games added by hand still show.
#[test]
fn games_added_by_hand_show_without_a_gog_account() {
    let mut app = added_app();
    app.logged_out();
    assert_eq!(
        app.library
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        ["local-1"]
    );
    assert!(
        render(&app).find("[FAKE] Mine").is_ok(),
        "not the sign-in page"
    );
}

/// Without Steam, GOG's sign-in opened from Settings goes back to the games added by hand, by its
/// button or Escape; Settings says which games show without GOG.
#[test]
fn gogs_sign_in_from_settings_goes_back_to_the_games_added_by_hand() {
    let mut app = added_app();
    app.logged_out();
    app.page = Page::Settings;
    assert!(
        render(&app)
            .find("Not signed in: only the games added by hand show.")
            .is_ok()
    );
    let _ = app.update(Message::ShowLogin(true));
    let mut ui = render(&app);
    ui.click("Back to the library").unwrap();
    for m in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(render(&app).find("Open the GOG sign-in page").is_err());

    let _ = app.update(Message::ShowLogin(true));
    app.go_back();
    assert!(!app.login_open, "Escape goes back too");
}

/// Its title is the one it was added with until the user renames it, and comes back with Reset.
#[test]
fn a_game_added_by_hand_gets_its_own_title_back_with_reset() {
    let mut app = added_app();
    let _ = app.update(Message::Edit(EditMsg::Open("local-1".into())));
    let _ = app.update(Message::Edit(EditMsg::Title("[FAKE] Renamed".into())));
    let _ = app.update(Message::Edit(EditMsg::Reset));
    assert_eq!(app.edit.as_ref().unwrap().title, "[FAKE] Mine");
}

/// Its settings offer to find it where it was moved, and keep the folder found.
#[test]
fn a_moved_game_added_by_hand_is_located_from_its_settings() {
    use crate::settings::SettingsMsg;
    let mut app = added_app();
    open(&mut app, "local-1", Some(Panel::GameSettings));
    let mut ui = render(&app);
    assert!(ui.find("Game version").is_err(), "nothing of GOG's");
    ui.click("Locate…").unwrap();
    assert!(
        ui.into_messages().any(
            |m| matches!(m, Message::Settings(SettingsMsg::LocateGame(id)) if id == "local-1")
        )
    );
    let mut moved = app.installs["local-1"].clone();
    moved.path = PathBuf::from("/games/Moved");
    let _ = app.update(Message::Settings(SettingsMsg::Located(
        "local-1".into(),
        Ok(moved),
    )));
    assert_eq!(app.installs["local-1"].path, PathBuf::from("/games/Moved"));
    assert!(render(&app).find("/games/Moved").is_ok());
}

/// Removing one asks first, says its files stay, and takes it out of the library.
#[test]
fn removing_a_game_added_by_hand_asks_first() {
    let mut app = added_app();
    open(&mut app, "local-1", Some(Panel::Manage));
    let mut ui = render(&app);
    assert!(
        ui.find("Verify files").is_err(),
        "nothing of GOG's to verify"
    );
    ui.click("Remove from library…").unwrap();
    for m in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    assert!(
        ui.find(
            "Only what SlattyLauncher unpacked for it is deleted: your own files stay where they \
             are."
        )
        .is_ok()
    );
    assert!(
        ui.find("Also delete the prefix (backed up)").is_err(),
        "a Linux game has no prefix"
    );
    ui.click("Remove from library").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, false)) if id == "local-1")
    ));

    let _ = app.update(Message::Maintenance(MaintenanceMsg::Uninstalled(
        "local-1".into(),
        Ok(Vec::new()),
    )));
    assert!(app.library.iter().all(|g| g.id != "local-1"));
    assert_eq!(app.selected, None);
}

/// The dialog takes a program, names the game after it until given a title, offers Proton for a
/// Windows program, says why a game was not added, and opens the page of the one added.
#[test]
fn a_game_is_added_from_the_library() {
    use crate::add::AddMsg;
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    let _ = app.update(Message::Add(AddMsg::Open));
    assert!(render(&app).find("Add a game").is_ok());
    assert!(render(&app).find("Proton").is_err(), "no program yet");
    let _ = app.update(Message::Add(AddMsg::PickedProgram(Some(
        "/games/[FAKE] Quest/Quest.exe".into(),
    ))));
    let d = app.add_game.as_ref().unwrap();
    assert_eq!(
        (d.title.as_str(), d.isolated),
        ("Quest", app.isolate_new_games)
    );
    assert_eq!(
        d.proton.as_deref(),
        Some(std::path::Path::new("/proton/GE-Proton"))
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Proton").is_ok());
        snapshot(&mut ui, "add-game");
    }

    let _ = app.update(Message::Add(AddMsg::Saved(Err("[FAKE] refused".into()))));
    assert!(render(&app).find("[FAKE] refused").is_ok());

    let install = Install {
        umu_id: None,
        game_id: "local-4".into(),
        title: "[FAKE] Quest".into(),
        platform: Platform::Windows,
        path: PathBuf::from("/games/[FAKE] Quest"),
        client_id: None,
        runner: Runner::Umu {
            proton: "/proton/GE-Proton".into(),
            prefix: "/prefixes/local-4".into(),
        },
        isolated: true,
    };
    let _ = app.update(Message::Add(AddMsg::Saved(Ok((
        install,
        Some("1207658924".into()),
    )))));
    assert!(app.add_game.is_none());
    assert!(app.library.iter().any(|g| g.id == "local-4"));
    assert_eq!(app.selected.as_deref(), Some("local-4"));
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|n| n.text.contains("GOG game (1207658924)"))
    );
}

/// With an installer, it runs first; the dialog stays open while it does, then asks for the
/// program it installed; closing it then throws that away.
#[test]
fn a_windows_installer_runs_before_the_program_is_picked() {
    use crate::add::AddMsg;
    let mut app = library_app();
    let _ = app.update(Message::Add(AddMsg::Open));
    let _ = app.update(Message::Add(AddMsg::WithInstaller(true)));
    let _ = app.update(Message::Add(AddMsg::PickedInstaller(Some(
        "/downloads/[FAKE] setup_quest.exe".into(),
    ))));
    assert_eq!(app.add_game.as_ref().unwrap().title, "[FAKE] setup_quest");
    {
        let mut ui = render(&app);
        assert!(ui.find("Run installer").is_ok());
        assert!(ui.find("Program").is_err(), "picked once installed");
        assert!(ui.find("Unpack into").is_err(), "a Windows one runs");
        snapshot(&mut ui, "add-game-installer");
    }
    // A Linux one is unpacked, into the default installation path until told.
    app.library_root = "/games".into();
    let _ = app.update(Message::Add(AddMsg::Open));
    let _ = app.update(Message::Add(AddMsg::WithInstaller(true)));
    let _ = app.update(Message::Add(AddMsg::PickedInstaller(Some(
        "/downloads/[FAKE] quest.tar.gz".into(),
    ))));
    let d = app.add_game.as_ref().unwrap();
    assert_eq!(
        (d.into.as_str(), d.title.as_str()),
        ("/games", "[FAKE] quest")
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Unpack into").is_ok() && ui.find("Unpack").is_ok());
        snapshot(&mut ui, "add-game-archive");
    }
    let _ = app.update(Message::Add(AddMsg::PickedInstaller(Some(
        "/downloads/[FAKE] setup_quest.exe".into(),
    ))));
    app.add_game.as_mut().unwrap().busy = true;
    let _ = app.update(Message::Add(AddMsg::Cancel));
    assert!(app.add_game.is_some(), "not while the installer runs");

    let _ = app.update(Message::Add(AddMsg::InstallerEnded(Ok((
        "local-3".into(),
        "/prefixes/local-3/drive_c".into(),
    )))));
    assert_eq!(
        app.add_game.as_ref().unwrap().installed_in.as_deref(),
        Some(std::path::Path::new("/prefixes/local-3/drive_c")),
        "where the program is picked"
    );
    let d = app.add_game.as_ref().unwrap();
    assert_eq!(d.installed_by.as_deref(), Some("local-3"));
    assert!(!d.busy);
    assert!(
        render(&app)
            .find("The installer ended: choose the game's program where it installed it.")
            .is_ok()
    );
    let _ = app.update(Message::Add(AddMsg::WithInstaller(false)));
    assert!(
        app.add_game.as_ref().unwrap().with_installer,
        "it ran already"
    );
    let _ = app.update(Message::Add(AddMsg::Cancel));
    assert!(app.add_game.is_none());
}

/// Once an installer ran, a click beside the dialog leaves it open, and the library under it: only
/// Cancel throws away what the installer installed.
#[test]
fn a_click_beside_the_dialog_keeps_what_an_installer_installed() {
    use crate::add::AddMsg;
    let beside = iced::Point::new(100.0, 450.0);
    let mut app = library_app();
    let _ = app.update(Message::Add(AddMsg::Open));
    {
        let mut ui = render(&app);
        ui.point_at(beside);
        let _ = ui.simulate(iced_test::simulator::click());
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::Add(AddMsg::Cancel))),
            "nothing to lose yet"
        );
    }
    app.add_game.as_mut().unwrap().installed_by = Some("local-2".into());
    let mut ui = render(&app);
    ui.point_at(beside);
    let _ = ui.simulate(iced_test::simulator::click());
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.is_empty(), "{messages:?}");
}
