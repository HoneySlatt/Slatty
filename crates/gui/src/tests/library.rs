use super::*;

#[test]
fn a_library_response_after_logout_cannot_restore_the_old_account() {
    let mut app = library_app();
    app.library_busy = true;
    app.overview_busy = true;
    let cache = slatty_core::library::LibraryCache {
        user_id: "0".into(),
        fetched_at: 1,
        games: app.library.clone(),
    };
    app.logged_out();
    let _ = app.update(Message::LibrarySynced(Ok(cache)));
    assert!(app.library.is_empty());
    assert!(!app.library_busy && !app.overview_busy);
}

#[test]
fn late_account_results_are_ignored_after_signing_back_in() {
    let mut app = library_app();
    let epoch = app.account_epoch;
    let account = app.account.clone();
    app.logged_out();
    app.account = account;
    app.overview_busy = true;
    let results = [
        Message::Avatar(Some("https://invalid.example/old-avatar".into())),
        Message::Cover("1".into(), Some(vec![1])),
        Message::Image("old-image".into(), Some(vec![1])),
        Message::OverviewFetched("1".into(), Ok(GameOverview::default())),
        Message::OverviewDone,
        Message::PlaytimesFetched(vec![("1".into(), 200)]),
        Message::Achievements("1".into(), Ok(vec![fake_achievement("old", true)])),
        Message::LibrarySynced(Err("old failure".into())),
    ];
    for result in results {
        let _ = app.update(Message::AccountResult(epoch, Box::new(result)));
    }
    assert!(app.avatar.is_none() && app.covers.is_empty() && app.images.is_empty());
    assert!(app.overview.is_empty() && app.achievements.is_empty());
    assert!(app.overview_busy && app.notice.is_none());
    let _ = app.update(Message::AccountResult(
        app.account_epoch,
        Box::new(Message::OverviewFetched(
            "1".into(),
            Ok(GameOverview::default()),
        )),
    ));
    assert!(app.overview.contains_key("1"));
}

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
fn filter_status_remains_readable_at_the_minimum_window_size() {
    let mut app = library_app();
    app.window = Size::new(800.0, 560.0);
    app.filters_open = true;
    app.filters.achievements = true;
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    snapshot(&mut ui, "narrow-filters");
    let status = ui.find("14 games could not be read from GOG").unwrap();
    let bounds = status.bounds();
    assert!(bounds.x + bounds.width <= app.window.width, "{bounds:?}");
    assert!(bounds.height <= 40.0, "at most two lines: {bounds:?}");
    ui.click("Has cloud saves").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::SetFilters(Filters {
            cloud_saves: true,
            ..
        })
    )));
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
    // Titles as GOG's can be: long, accented, not only Latin, in no particular order.
    let games = |count: usize| -> Vec<LibraryGame> {
        (1..=count)
            .map(|i| {
                let n = (i * 7919) % count;
                fake_game(
                    &n.to_string(),
                    &format!("Épopée 世界 {n}: The Long Journey"),
                )
            })
            .collect()
    };
    let mut app = library_app();
    app.library = games(10_000);
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
    app.search = "世界 99".into();
    time(&app, "view, searching");
    app.search.clear();
    // The Achievements tab, every game with achievements.
    for g in &app.library {
        app.overview.entry(g.id.clone()).or_default().achievements = Some((3, 10));
    }
    app.page = Page::Achievements;
    time(&app, "achievements tab");
    // Laying out, beyond building: each page at 10,000 games against the same page at 14, both
    // timed alike (the median of a few windows, which carry the test renderer's own start-up).
    let laid_out = |app: &App| {
        let mut runs: Vec<std::time::Duration> = (0..5)
            .map(|_| {
                let t = std::time::Instant::now();
                let _ = Simulator::with_size(settings(), SIZE, app.view());
                t.elapsed()
            })
            .collect();
        runs.sort();
        runs[2]
    };
    let mut small = library_app();
    small.library = games(14);
    for page in [Page::Library, Page::Achievements] {
        for a in [&mut app, &mut small] {
            a.page = page;
            let ids: Vec<String> = a.library.iter().map(|g| g.id.clone()).collect();
            for id in ids {
                a.overview.entry(id).or_default().achievements = Some((3, 10));
            }
        }
        let (big, base) = (laid_out(&app), laid_out(&small));
        println!(
            "layout, {page:?}: {big:?} at 10,000 games, {base:?} at 14 ({:?} more)",
            big.saturating_sub(base)
        );
    }
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
    // A cover shows its title twice: on its placeholder and over it on hover.
    let built: std::collections::BTreeSet<usize> = texts(&mut render(&app))
        .iter()
        .filter_map(|t| t.strip_prefix("[FAKE] Game ")?.parse().ok())
        .collect();
    assert!(!built.contains(&1), "far above: not built");
    assert!(built.len() < 100, "a few rows: {}", built.len());
    assert!(built.iter().all(|i| *i > 1_000), "{built:?}");

    // Back from a game page, the grid shows its top again.
    open(&mut app, "3", None);
    let _ = app.update(Message::CloseDetail);
    assert!(app.grid_view.is_none());
    assert!(render(&app).find("[FAKE] Game 1").is_ok());
}
