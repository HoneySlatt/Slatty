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
        Message::Cover("1".into(), Some("/covers/1.jpg".into())),
        Message::Image("old-image".into(), Some("/images/old.jpg".into())),
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

/// An empty shelf says what would fill it; "No game matches." is kept for a search or filters.
#[test]
fn an_empty_shelf_says_what_fills_it() {
    let mut app = library_app();
    app.installs.clear();
    for (shelf, text) in [
        (Shelf::Installed, "No game installed yet."),
        (
            Shelf::Favorites,
            "No favorite yet: the heart beside Play on a game's page adds it here.",
        ),
        (Shelf::Hidden, "No hidden game."),
    ] {
        app.shelf = shelf;
        app.lists.clear();
        assert!(render(&app).find(text).is_ok(), "{text}");
    }
    app.search = "nothing like it".into();
    app.lists.clear();
    assert!(render(&app).find("No game matches.").is_ok());
}

/// At the smallest window, with the filters open, a download running, several stopped and a
/// notice, the grid keeps room for a row of covers, and each stopped one stays in reach.
#[test]
fn the_grid_keeps_room_under_what_shows_above_it() {
    use crate::Interrupted;
    let mut app = library_app();
    app.window = Size::new(800.0, 560.0);
    app.filters_open = true;
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    app.interrupted = vec![
        ("6".into(), Interrupted::Download),
        ("7".into(), Interrupted::Paused),
        ("8".into(), Interrupted::Failed),
        ("9".into(), Interrupted::Update),
    ];
    app.notice = Some(crate::Notice {
        error: true,
        text: "[FAKE] Library not refreshed: GOG did not answer.".into(),
    });
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    snapshot(&mut ui, "library-crowded");
    let grid = ui
        .find(iced_test::selector::id(crate::library::GRID))
        .unwrap()
        .bounds();
    // More than half a row of covers (a full row is 230 pixels high at this width), the rest a
    // scroll away.
    assert!(grid.height >= 120.0, "{grid:?}");
    // The stopped ones scroll in their card: the last can be reached.
    let card = ui.find("[FAKE] Game 6").unwrap().bounds();
    ui.point_at(card.center());
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: -10.0 },
    })]);
    ui.click("Finish update").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, _)) if id == "9"
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
fn play_time_read_again_unchanged_is_not_written_again() {
    let mut app = library_app();
    let core = app.core.as_mut().unwrap();
    core.dirs = Dirs::under(&core.dirs.cache.join("playtime-test"));
    let file = core.dirs.account_cache("0").join("overview.json");
    let _ = app.update(Message::PlaytimesFetched(vec![("1".into(), 200)]));
    assert!(file.is_file());
    std::fs::remove_file(&file).unwrap();
    // A game page opened again: the same minutes.
    let _ = app.update(Message::PlaytimesFetched(vec![("1".into(), 200)]));
    assert!(!file.exists());
    let _ = app.update(Message::PlaytimesFetched(vec![("1".into(), 230)]));
    assert!(file.is_file());
    assert_eq!(app.played_seconds("1"), 230 * 60);
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

/// A game read again whose play time GOG did not give this time keeps the one known.
#[test]
fn a_play_time_not_read_again_is_kept() {
    let mut app = library_app();
    app.overview.insert(
        "3".into(),
        GameOverview {
            playtime_minutes: Some(185),
            ..Default::default()
        },
    );
    let _ = app.update(Message::OverviewFetched(
        "3".into(),
        Ok(GameOverview {
            achievements: Some((1, 4)),
            checked: true,
            ..Default::default()
        }),
    ));
    assert_eq!(app.overview["3"].playtime_minutes, Some(185));
    assert_eq!(app.overview["3"].achievements, Some((1, 4)));
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

#[test]
fn only_the_keys_the_window_answers_send_a_message() {
    use iced::keyboard::key::{Named, NativeCode, Physical};
    use iced::keyboard::{Event, Key, Location, Modifiers};
    let physical_key = Physical::Unidentified(NativeCode::Unidentified);
    let press = |key: Key| Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key,
        location: Location::Standard,
        modifiers: Modifiers::empty(),
        text: None,
        repeat: false,
    };
    assert!(crate::key_message(press(Key::Named(Named::Escape))).is_some());
    assert!(crate::key_message(press(Key::Named(Named::Tab))).is_some());
    assert!(crate::key_message(press(Key::Character("a".into()))).is_none());
    let release = Event::KeyReleased {
        key: Key::Named(Named::Escape),
        modified_key: Key::Named(Named::Escape),
        physical_key,
        location: Location::Standard,
        modifiers: Modifiers::empty(),
    };
    assert!(crate::key_message(release).is_none());
    assert!(crate::key_message(Event::ModifiersChanged(Modifiers::SHIFT)).is_none());
}

/// At start the covers already cached come in one message; only those missing are downloaded,
/// each on its own.
#[test]
fn cached_covers_show_at_once_and_only_missing_ones_are_downloaded() {
    let mut app = library_app();
    app.library[0].cover = Some("https://example.invalid/1.jpg".into());
    app.library[1].cover = Some("https://example.invalid/2.jpg".into());
    let found = std::collections::HashMap::from([
        ("1".to_string(), PathBuf::from("/cache/covers/1.jpg")),
        ("99".to_string(), PathBuf::from("/cache/covers/99.jpg")),
    ]);
    let downloads = app.covers_cached(found);
    assert!(app.covers.contains_key("1"));
    assert!(!app.covers.contains_key("99"), "not in the library");
    assert_eq!(app.covers.len(), 1);
    // Game 2 has a cover to download; the others have none.
    assert_eq!(downloads.units(), 1);
}

/// What each game sorts by is made at start, off the interface thread, from GOG's titles and the
/// user's changes: the first view, sorted by name either way, makes no key of its own.
#[test]
fn the_sort_keys_made_at_start_are_all_the_first_view_needs() {
    use slatty_core::custom::Custom;
    let mut app = library_app();
    app.customs.insert(
        "2".into(),
        Custom {
            title: Some("[FAKE] Renamed".into()),
            ..Default::default()
        },
    );
    app.customs.insert(
        "3".into(),
        Custom {
            sort_title: Some("[FAKE] Aaa".into()),
            ..Default::default()
        },
    );
    app.sort_keys
        .fill(crate::library::sort_names(&app.library, &app.customs));
    app.apply_customs();
    let made = app.sort_keys.len();
    for sort in [Sort::NameAsc, Sort::NameDesc] {
        app.sort = sort;
        assert!(!titles(&app).is_empty());
        assert_eq!(app.sort_keys.len(), made, "{sort:?}");
    }
    app.sort = Sort::NameAsc;
    assert_eq!(titles(&app)[..2], ["Game 3", "Game 1"], "its sort title");
    assert!(titles(&app).contains(&"Renamed".to_string()));
}

#[test]
fn the_games_listed_are_kept_while_scrolling_and_made_again_after_anything_else() {
    let mut app = library_app();
    assert_eq!(titles(&app)[..2], ["Game 1", "Game 2"]);
    // Changed behind the list's back: frames and scroll steps keep the order made before.
    app.library[0].title = "[FAKE] Zebra".into();
    let _ = app.update(Message::Frame(app.now));
    // Covers come as work of the account signed in.
    let _ = app.update(Message::AccountResult(
        app.account_epoch,
        Box::new(Message::Cover("2".into(), None)),
    ));
    assert_eq!(titles(&app)[..2], ["Zebra", "Game 2"]);
    // Nor does what arrives just after start: how games start, the Proton builds, the updates.
    for message in [
        Message::LaunchOptions(Vec::new(), None),
        Message::Runners(crate::runners::RunnersMsg::Listed(Vec::new(), Vec::new())),
        Message::Updates(crate::updates::UpdatesMsg::Checked(Vec::new())),
    ] {
        let _ = app.update(message);
        assert_eq!(titles(&app)[..2], ["Zebra", "Game 2"]);
    }
    // Nor do play times, unless the library is sorted by them and one changed.
    let _ = app.update(Message::PlaytimesFetched(vec![("5".into(), 30)]));
    assert_eq!(titles(&app)[..2], ["Zebra", "Game 2"]);
    app.sort = Sort::MostPlayed;
    assert_eq!(titles(&app)[..1], ["Game 5"]);
    let _ = app.update(Message::PlaytimesFetched(vec![("5".into(), 30)]));
    let _ = app.update(Message::PlaytimesFetched(vec![("1".into(), 20)]));
    assert_eq!(titles(&app)[..2], ["Game 5", "Zebra"], "made again");
    app.sort = Sort::default();
    let _ = app.update(Message::DismissNotice);
    // Any other message makes it again.
    let _ = app.update(Message::DismissNotice);
    assert_eq!(titles(&app)[..1], ["Game 2"]);
    assert_eq!(titles(&app).last().unwrap(), "Zebra");
    // What it was made for changed (here without a message): made again.
    app.search = "zeb".into();
    assert_eq!(titles(&app), ["Zebra"]);
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
    // The views above keep the list from one view to the next, as scrolling does; any other
    // message makes it again.
    let t = std::time::Instant::now();
    for _ in 0..10 {
        app.lists.clear();
        let _ = app.visible_games();
    }
    println!("list made again: {:?}", t.elapsed() / 10);
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
    let scrolled = app.grid_view.expect("scrolled");
    let offset = scrolled.absolute_offset().y;
    assert!(offset > 30_000.0, "{offset}");
    // A cover shows its title twice: on its placeholder and over it on hover.
    let built: std::collections::BTreeSet<usize> = texts(&mut render(&app))
        .iter()
        .filter_map(|t| t.strip_prefix("[FAKE] Game ")?.parse().ok())
        .collect();
    assert!(!built.contains(&1), "far above: not built");
    assert!(built.len() < 100, "a few rows: {}", built.len());
    assert!(built.iter().all(|i| *i > 1_000), "{built:?}");

    // Back from a game page, the grid is where it was left, with the same rows built.
    let _ = app.update(Message::Select("3".into()));
    let _ = app.update(Message::CloseDetail);
    assert_eq!(app.grid_view.map(|v| v.absolute_offset().y), Some(offset));
    assert!(render(&app).find("[FAKE] Game 1").is_err());
    // Through Escape too.
    let _ = app.update(Message::Select("3".into()));
    let _ = app.update(Message::Key(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::empty(),
        text: None,
        repeat: false,
    }));
    assert_eq!(app.grid_view.map(|v| v.absolute_offset().y), Some(offset));

    // Another tab, then the library again: its top.
    let _ = app.update(Message::ShowPage(Page::Settings));
    let _ = app.update(Message::ShowPage(Page::Library));
    assert!(app.grid_view.is_none());
    assert!(render(&app).find("[FAKE] Game 1").is_ok());
    // A game page reached from another tab brings no old position back.
    app.grid_view = Some(scrolled);
    let _ = app.update(Message::Select("3".into()));
    let _ = app.update(Message::ShowPage(Page::Downloads));
    let _ = app.update(Message::Select("4".into()));
    let _ = app.update(Message::CloseDetail);
    assert!(app.grid_view.is_none());
}
