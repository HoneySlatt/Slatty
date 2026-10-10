use super::*;

#[test]
fn login_screen_offers_browser_login_without_password_field() {
    let app = App {
        core: Some(core()),
        ..Default::default()
    };
    let mut ui = render(&app);
    assert!(ui.find("Sign in").is_ok());
    assert!(ui.find("Password").is_err());
    snapshot(&mut ui, "login");
    ui.click("Open the GOG sign-in page").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenLoginPage))
    );
}

#[test]
fn top_bar_offers_library_achievements_and_settings() {
    let app = library_app();
    let mut ui = render(&app);
    for tab in ["Library", "Achievements"] {
        assert!(ui.find(tab).is_ok(), "{tab}");
    }
    for later in ["Activity", "Store", "Friends"] {
        assert!(ui.find(later).is_err(), "{later}");
    }
    snapshot(&mut ui, "library");
    ui.click(iced_test::selector::id("settings-tab")).unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::ShowPage(Page::Settings)))
    );
}

#[test]
fn search_filters_the_library() {
    let mut app = library_app();
    app.search = "game 1".into();
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Game 12").is_ok());
    assert!(ui.find("[FAKE] Game 3").is_err());
}

#[test]
fn shelves_filters_and_sort_select_games() {
    let mut app = library_app();
    app.shelf = Shelf::Installed;
    assert_eq!(titles(&app), ["Game 3"]);

    app.shelf = Shelf::All;
    let _ = app.update(Message::ToggleFavorite("7".into()));
    app.shelf = Shelf::Favorites;
    assert_eq!(titles(&app), ["Game 7"]);
    assert_eq!(
        slatty_core::settings::favorites(&app.core.as_ref().unwrap().db).unwrap(),
        ["7"]
    );

    app.shelf = Shelf::All;
    app.filters = Filters {
        linux: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 2"]);

    app.overview.insert(
        "4".into(),
        GameOverview {
            achievements: Some((1, 10)),
            cloud_saves: false,
            ..Default::default()
        },
    );
    app.overview.insert(
        "5".into(),
        GameOverview {
            achievements: None,
            cloud_saves: true,
            ..Default::default()
        },
    );
    app.filters = Filters {
        achievements: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 4"]);
    app.filters = Filters {
        cloud_saves: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 5"]);

    app.filters = Filters::default();
    app.playtime.insert(
        "9".into(),
        Playtime {
            seconds: 60,
            last_played: 200,
        },
    );
    app.playtime.insert(
        "2".into(),
        Playtime {
            seconds: 6000,
            last_played: 100,
        },
    );
    app.sort = Sort::RecentlyPlayed;
    assert_eq!(titles(&app)[..2], ["Game 9", "Game 2"]);
    for (id, minutes) in [("2", 100), ("9", 1)] {
        app.overview.entry(id.into()).or_default().playtime_minutes = Some(minutes);
    }
    app.sort = Sort::MostPlayed;
    assert_eq!(titles(&app)[..2], ["Game 2", "Game 9"]);
    app.sort = Sort::NameDesc;
    // Numbers by their value: 14 comes after 9.
    assert_eq!(titles(&app)[..2], ["Game 14", "Game 13"]);
}

#[test]
fn games_known_only_by_play_time_are_read_again() {
    let mut app = library_app();
    app.library.truncate(2);
    app.page = Page::Achievements;
    app.overview.insert(
        "1".into(),
        GameOverview {
            achievements: Some((1, 2)),
            checked: true,
            ..Default::default()
        },
    );
    let _ = app.update(Message::PlaytimesFetched(vec![("2".into(), 30)]));
    assert!(
        !app.overview_complete(),
        "play time says nothing of achievements"
    );

    let _ = app.update(Message::ScanOverview);
    assert!(app.overview_busy);
    assert!(
        render(&app)
            .find("1 / 2 unlocked · Reading GOG data… 1/2 games · 0 completed")
            .is_ok()
    );
    let _ = app.update(Message::OverviewFetched(
        "2".into(),
        Err("[FAKE] offline".into()),
    ));
    let _ = app.update(Message::OverviewDone);
    assert!(
        render(&app)
            .find("1 / 2 unlocked · 1 game could not be read from GOG · 0 completed")
            .is_ok(),
        "no endless reading message after a failure"
    );
}

#[test]
fn a_new_page_fades_in_then_stops_drawing_frames() {
    use std::time::Duration;
    let mut app = library_app();
    assert!(!app.page_shown.is_animating(app.now));
    let _ = app.update(Message::Select("3".into()));
    let start = app.now;
    assert!(app.page_shown.is_animating(start), "opening a game");
    let _ = app.update(Message::Frame(start + Duration::from_secs(1)));
    assert!(
        !app.page_shown.is_animating(app.now),
        "done once its time is over"
    );
    let _ = app.update(Message::OpenPanel(Panel::Achievements));
    assert!(
        !app.page_shown.is_animating(app.now),
        "a panel is not a new page"
    );
    let _ = app.update(Message::CloseDetail);
    assert!(app.page_shown.is_animating(app.now), "back to the library");
    let _ = app.update(Message::Frame(app.now + Duration::from_secs(1)));
    let _ = app.update(Message::ShowPage(Page::Achievements));
    assert!(app.page_shown.is_animating(app.now), "another tab");
}

#[test]
fn the_library_order_is_kept() {
    let mut app = library_app();
    let _ = app.update(Message::SortBy(Sort::MostPlayed));
    let db = app.core.as_ref().unwrap().db.clone();
    let saved = slatty_core::settings::library_sort(&db).unwrap();
    assert_eq!(
        saved.as_deref().and_then(Sort::from_key),
        Some(Sort::MostPlayed)
    );
    for s in Sort::ALL {
        assert_eq!(Sort::from_key(s.key()), Some(s));
    }
}

/// Times the library at 10,000 games: the size it must stay fluid at. Run with
/// `cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture`.
#[test]
#[ignore]
fn library_at_10000_games() {
    let mut app = library_app();
    app.library = (1..=10_000)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    let time = |app: &App, what: &str| {
        let _ = app.view();
        let t = std::time::Instant::now();
        for _ in 0..10 {
            let _ = app.view();
        }
        println!("{what}: {:?} per view", t.elapsed() / 10);
    };
    for sort in Sort::ALL {
        app.sort = sort;
        time(&app, &format!("view, {sort}"));
    }
    app.sort = Sort::NameAsc;
    app.search = "game 99".into();
    time(&app, "view, searching");
    app.search.clear();
    // Laying the page out, beyond building it: a window first with almost nothing, then the library.
    let base = {
        let mut empty = library_app();
        empty.library.clear();
        let t = std::time::Instant::now();
        let _ = Simulator::with_size(settings(), SIZE, empty.view());
        t.elapsed()
    };
    let t = std::time::Instant::now();
    let _ = Simulator::with_size(settings(), SIZE, app.view());
    println!("layout: {:?} (window alone {base:?})", t.elapsed());
}

#[test]
fn a_library_of_10000_games_builds_only_the_covers_in_view() {
    use iced::mouse::{Event as Mouse, ScrollDelta};
    let mut app = library_app();
    app.library = (1..=10_000)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Game 1").is_ok());
        assert!(ui.find("[FAKE] Game 500").is_err(), "far below: not built");
    }
    // Scrolled far down with the wheel: the covers there are built, the first ones are not.
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        ui.point_at(iced::Point::new(600.0, 500.0));
        let _ = ui.simulate([iced::Event::Mouse(Mouse::WheelScrolled {
            delta: ScrollDelta::Pixels {
                x: 0.0,
                y: -40_000.0,
            },
        })]);
        ui.into_messages().collect()
    };
    for m in messages {
        let _ = app.update(m);
    }
    let offset = app.grid_view.expect("scrolled").absolute_offset().y;
    assert!(offset > 30_000.0, "{offset}");
    let built: Vec<usize> = {
        let mut ui = render(&app);
        (1..=10_000)
            .filter(|i| ui.find(format!("[FAKE] Game {i}")).is_ok())
            .collect()
    };
    assert!(!built.contains(&1), "far above: not built");
    assert!(built.len() < 100, "a few rows: {}", built.len());
    assert!(built.iter().all(|i| *i > 1_000), "{built:?}");

    // Back from a game page, the grid shows its top again.
    open(&mut app, "3", None);
    let _ = app.update(Message::CloseDetail);
    assert!(app.grid_view.is_none());
    assert!(render(&app).find("[FAKE] Game 1").is_ok());
}
