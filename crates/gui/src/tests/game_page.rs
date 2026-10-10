use super::*;

#[test]
fn installed_game_page_can_be_played_and_shows_no_store_text() {
    let mut app = library_app();
    open(&mut app, "3", None);
    app.overview.insert(
        "3".into(),
        GameOverview {
            playtime_minutes: Some(3 * 60 + 5),
            ..Default::default()
        },
    );
    app.playtime.insert(
        "3".into(),
        Playtime {
            seconds: 99 * 3600,
            last_played: 1_791_500_000,
        },
    );
    let mut ui = render(&app);
    assert!(ui.find("3 h 5 m").is_ok(), "GOG's total, not the local one");
    assert!(ui.find("Time played").is_ok());
    assert!(ui.find("Cloud saves").is_ok());
    assert!(ui.find("Manage").is_ok());
    snapshot(&mut ui, "game");
    ui.click("Play").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Play(id) if id == "3"))
    );

    app.maintenance.insert(
        "3".into(),
        crate::maintenance::MaintenanceView {
            busy: true,
            ..Default::default()
        },
    );
    let mut ui = render(&app);
    let _ = ui.click("Play");
    assert!(
        !ui.into_messages().any(|m| matches!(m, Message::Play(_))),
        "no launch while the game's files are being changed"
    );
}

#[test]
fn uninstalled_game_offers_install_but_not_play_or_cloud_tools() {
    let mut app = library_app();
    open(&mut app, "5", None);
    let mut ui = render(&app);
    assert!(ui.find("Play").is_err());
    assert!(ui.find("Manage").is_err());
    ui.click("Install").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenPanel(Panel::Install)))
    );
}

#[test]
fn escape_closes_the_panel_then_the_game_page() {
    use iced::keyboard::{Event, Key, Location, Modifiers, key};
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let escape = || {
        Message::Key(Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Named(key::Named::Escape),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })
    };
    let _ = app.update(escape());
    assert_eq!((app.panel, app.selected.as_deref()), (None, Some("3")));
    let _ = app.update(escape());
    assert_eq!(app.selected, None);
}

#[test]
fn play_time_comes_from_gog_even_for_games_played_elsewhere() {
    let mut app = library_app();
    open(&mut app, "5", None);
    app.overview.insert(
        "5".into(),
        GameOverview {
            playtime_minutes: Some(3175),
            ..Default::default()
        },
    );
    let mut ui = render(&app);
    assert!(ui.find("52 h 55 m").is_ok());
    assert!(
        ui.find("Never").is_err(),
        "GOG does not tell when it was last played"
    );
    drop(ui);

    app.playtime.insert(
        "5".into(),
        Playtime {
            seconds: 4000 * 60,
            last_played: 1_791_500_000,
        },
    );
    assert_eq!(
        app.played_seconds("5"),
        3175 * 60,
        "only GOG's total counts"
    );
}

#[test]
fn closing_asks_first_only_when_something_is_running() {
    let mut app = library_app();
    let _ = app.update(Message::CloseRequested);
    assert!(app.quit_confirm.is_none(), "nothing running: quits at once");

    let cancel = tokio_util::sync::CancellationToken::new();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 1,
                files_total: 4,
                bytes_done: 1 << 30,
                bytes_total: 2 << 30,
            },
            cancel: cancel.clone(),
            cancelling: crate::install::Cancelling::No,
            rate: Default::default(),
        },
    );
    let _ = app.update(Message::CloseRequested);
    let mut ui = render(&app);
    assert!(
        ui.find(
            "Downloading [FAKE] Game 5 (50 %). It stops here and resumes where it stopped when SlattyLauncher starts again."
        )
        .is_ok()
    );
    snapshot(&mut ui, "quit-confirm");
    ui.click("Keep running").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.quit_confirm.is_none());
    assert!(!cancel.is_cancelled());

    let _ = app.update(Message::CloseRequested);
    let _ = app.update(Message::ConfirmQuit);
    assert!(
        !cancel.is_cancelled(),
        "left as it stands, so that the next start resumes it"
    );
}

#[test]
fn the_cover_does_not_stand_in_while_the_key_art_downloads() {
    let mut app = library_app();
    let art = "https://images.example/[FAKE]-art.jpg".to_string();
    app.library[2].background = Some(art.clone());
    app.covers.insert(
        "3".into(),
        iced::widget::image::Handle::from_bytes(vec![0u8]),
    );
    let _ = app.update(Message::Select("3".into()));
    settle(&mut app);
    let game = app.library[2].clone();
    assert!(app.images_requested.contains(&art));
    assert!(app.hero_art(&game).is_none(), "plain while downloading");

    let _ = app.update(Message::Image(art.clone(), Some(vec![1u8])));
    assert!(app.hero_art(&game).is_some());
    assert!(app.animating(), "the key art fades in");
    settle(&mut app);
    assert!(!app.animating());

    // Without key art (download failed), the cover stands in.
    app.images.clear();
    let _ = app.update(Message::Image(art, None));
    assert_eq!(
        app.hero_art(&game).map(|h| h.id()),
        app.covers.get("3").map(|h| h.id())
    );
}

#[test]
fn the_session_shows_in_a_drawer_with_stop() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Session));
    {
        let mut ui = render(&app);
        assert!(ui.find("No session yet.").is_ok());
    }
    let (stop, _stopped) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(crate::play::PlayState {
        game_id: "3".into(),
        log: vec!["Preparing…".into(), "Game started.".into()],
        stop,
        running: true,
    });
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Preparing…").is_ok());
        assert!(ui.find("Game started.").is_ok());
        snapshot(&mut ui, "session-drawer");
        ui.click("Stop game").unwrap();
        ui.into_messages().collect()
    };
    assert!(matches!(&messages[..], [Message::StopGame]), "{messages:?}");
    // The page stays usable beside it: the drawer closes like the others.
    let _ = app.update(Message::ClosePanel);
    assert_eq!(app.panel, None);
}

#[test]
fn a_game_with_several_launch_options_asks_once_which_to_start() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    open(&mut app, "3", None);
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "Configuration Tool".into()],
    );
    let _ = app.update(Message::Play("3".into()));
    assert_eq!(app.launch_prompt.as_deref(), Some("3"));
    assert!(app.play.is_none(), "nothing starts before the choice");
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Default").is_ok());
        snapshot(&mut ui, "launch-choice");
        ui.click("Configuration Tool").unwrap();
        ui.into_messages().collect()
    };
    for m in messages {
        let _ = app.update(m);
    }
    assert!(app.launch_prompt.is_none());
    assert!(app.play.is_some(), "it starts once chosen");
    assert_eq!(
        slatty_core::settings::launch_choice(&db, "3")
            .unwrap()
            .as_deref(),
        Some("Configuration Tool")
    );

    // Kept: the next Play does not ask, and Game settings shows it.
    app.play = None;
    let _ = app.update(Message::Play("3".into()));
    assert!(app.launch_prompt.is_none() && app.play.is_some());
    app.play = None;
    app.panel = Some(Panel::GameSettings);
    let mut ui = render(&app);
    assert!(ui.find("Launch").is_ok());
    assert_eq!(app.launch_choices["3"], "Configuration Tool");
    snapshot(&mut ui, "launch-in-game-settings");
}
