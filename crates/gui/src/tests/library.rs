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

/// While GOG's library is read for the first time, the empty library says so; if that read fails,
/// the notice claims no cached version, there being none.
#[test]
fn a_first_library_read_says_so_and_claims_no_cache_when_it_fails() {
    let mut app = library_app();
    app.library.clear();
    app.fetched_at = None;
    app.library_busy = true;
    assert!(render(&app).find("Reading your library from GOG…").is_ok());
    let _ = app.update(Message::LibrarySynced(Err("[FAKE] offline".into())));
    assert_eq!(
        app.notice.as_ref().map(|n| n.text.as_str()),
        Some("Library not refreshed: [FAKE] offline.")
    );
    assert!(
        render(&app)
            .find("Your library is empty: refresh it in Settings.")
            .is_ok()
    );
    app.fetched_at = Some(1_791_500_000);
    let _ = app.update(Message::LibrarySynced(Err("[FAKE] offline".into())));
    assert_eq!(
        app.notice.as_ref().map(|n| n.text.as_str()),
        Some("Library not refreshed: [FAKE] offline. Showing the cached version.")
    );
}

/// Without a GOG account (Steam's games alone), nothing points to Refresh, which is off: there is
/// no GOG library to read.
#[test]
fn without_a_gog_account_nothing_offers_a_refresh() {
    let mut app = library_app();
    app.logged_out();
    app.steam.on = true;
    assert!(
        render(&app)
            .find("Your library is empty: sign in to GOG in Settings.")
            .is_ok()
    );
    app.page = Page::Achievements;
    {
        let mut ui = render(&app);
        assert!(ui.find("No game with achievements found.").is_ok());
        ui.click("Refresh").unwrap();
        assert!(ui.into_messages().next().is_none());
    }
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    let mut ui = render_window(&app);
    ui.click("Refresh library").unwrap();
    assert!(ui.into_messages().next().is_none());
}

/// A cover leads to no install dialog for a product GOG offers nothing to install for, once known:
/// its settings open its page, which says why.
#[test]
fn a_cover_offers_no_install_for_a_product_gog_does_not_offer() {
    use iced_test::selector::Candidate;
    let install_dialogs = |app: &App| {
        let mut ui = render(app);
        // The title shows twice: on the placeholder, and over it with the buttons below.
        let mut titles = Vec::new();
        let _ = ui.find(|candidate: Candidate<'_>| -> Option<()> {
            if let Candidate::Text {
                content: "[FAKE] Game 5",
                bounds,
                ..
            } = candidate
            {
                titles.push(bounds);
            }
            None
        });
        let over = titles
            .into_iter()
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .unwrap();
        let buttons = over.y + over.height + 20.0;
        for i in 0..40 {
            ui.point_at(iced::Point::new(over.x - 10.0 + i as f32 * 5.0, buttons));
            let _ = ui.simulate(iced_test::simulator::click());
        }
        ui.into_messages()
            .filter(|m| matches!(m, Message::OpenDialog(id, Panel::Install) if id == "5"))
            .count()
    };
    let mut app = library_app();
    assert!(install_dialogs(&app) > 0, "until GOG has answered");
    app.installability
        .insert("5".into(), Some(slatty_core::library::NotInstallable::Pack));
    assert_eq!(install_dialogs(&app), 0);
}

/// A long title over a hovered cover takes two lines at most: Play and the game's settings stay on
/// the cover, the smallest offered (75 %) in the smallest window included.
#[test]
fn a_long_title_leaves_a_covers_buttons_on_it() {
    let mut app = library_app();
    app.window = Size::new(800.0, 560.0);
    app.card_width = crate::settings::CoverSize(75).width();
    app.library.retain(|g| g.id == "3");
    app.library[0].title = format!("[FAKE] {}", "A very long title ".repeat(11));
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    // The game is the first cover of the grid; its buttons take clicks only where they are, on it.
    let grid = ui
        .find(iced_test::selector::id(crate::library::GRID))
        .unwrap()
        .bounds();
    for x in (0..20).map(|i| grid.x + 4.0 + i as f32 * 6.0) {
        for y in (0..30).map(|i| grid.y + 4.0 + i as f32 * 6.0) {
            ui.point_at(iced::Point::new(x, y));
            let _ = ui.simulate(iced_test::simulator::click());
        }
    }
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::Play(id) if id == "3")),
        "Play"
    );
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::OpenDialog(id, Panel::GameSettings) if id == "3")),
        "settings"
    );
}

/// What changes no list (Steam's covers arriving, typing in a dialog, a right click, a dialog
/// opened) keeps the lists made, each made again in about 2 ms at 11,000 games; a title saved
/// does not.
#[test]
fn messages_that_change_no_list_keep_the_lists() {
    use crate::add::AddMsg;
    use crate::edit::EditMsg;
    use crate::steam::SteamMsg;
    let kept = |app: &mut App, message: Message| {
        let _ = app.games_by_achievements();
        let _ = app.update(message);
        app.lists.achievements.borrow().is_some()
    };
    let mut app = library_app();
    for message in [
        Message::Steam(SteamMsg::Cover("steam-440".into(), None)),
        Message::Edit(EditMsg::Menu("4".into())),
        Message::Edit(EditMsg::At(iced::Point::new(10.0, 10.0))),
        Message::Edit(EditMsg::Title("[FAKE] Typed".into())),
        Message::Edit(EditMsg::SortTitle("[FAKE] Typed".into())),
        Message::Edit(EditMsg::PickerQuery("[FAKE] Typed".into())),
        Message::Add(AddMsg::Title("[FAKE] Typed".into())),
        Message::Add(AddMsg::Program("/games/[FAKE]".into())),
        Message::Add(AddMsg::Args("--[FAKE]".into())),
        Message::Add(AddMsg::Folder("/games".into())),
        Message::Add(AddMsg::Installer("/downloads/[FAKE]".into())),
        Message::Add(AddMsg::Into("/games".into())),
        Message::Install(InstallMsg::RootInput("5".into(), "/games/[FAKE]".into())),
        Message::Install(InstallMsg::FreeSpace("5".into(), "/games".into(), Some(1))),
        Message::Avatar(None),
        Message::ToggleFilters,
        Message::OpenDialog("5".into(), Panel::Install),
        Message::CloseDialog,
        Message::DismissNotice,
    ] {
        let name = format!("{message:?}");
        assert!(kept(&mut app, message), "{name}");
    }
    let _ = app.update(Message::Edit(EditMsg::Open("4".into())));
    assert!(!kept(&mut app, Message::Edit(EditMsg::Save)));
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

/// The store filters keep the games of any store ticked; a GOG game owned on Steam too, shown once
/// as GOG's, is kept by either.
#[test]
fn store_filters_keep_the_games_of_the_stores_ticked() {
    let mut app = library_app();
    app.library.truncate(2);
    app.library[0].steam = Some(vec![999]);
    app.steam.on = true;
    app.steam.games = vec![
        fake_game("steam-620", "Portal"),
        fake_game("steam-999", "Game 1"),
    ];
    app.steam.games[0].os.push("linux".into());
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
    let mut only = |f: Filters| {
        app.filters = f;
        let mut t = titles(&app);
        t.sort();
        t
    };
    assert_eq!(
        only(Filters {
            gog: true,
            ..Default::default()
        }),
        ["Game 1", "Game 2"]
    );
    assert_eq!(
        only(Filters {
            steam: true,
            ..Default::default()
        }),
        ["Game 1", "Portal"]
    );
    assert_eq!(
        only(Filters {
            added: true,
            ..Default::default()
        }),
        ["Mine"]
    );
    assert_eq!(
        only(Filters {
            steam: true,
            added: true,
            ..Default::default()
        }),
        ["Game 1", "Mine", "Portal"]
    );
    assert_eq!(
        only(Filters {
            steam: true,
            linux: true,
            ..Default::default()
        }),
        ["Portal"]
    );
}

/// The grid shows a cover's copy made at the card's size once it arrives, for the cover it was
/// made from and the size it was made at; a copy made at another size is dropped.
#[test]
fn the_grid_shows_covers_made_at_the_cards_size() {
    let mut app = library_app();
    app.card_width = 150.0;
    let cover = PathBuf::from("/covers/7.jpg");
    let copy = PathBuf::from("/scaled/300/7.jpg");
    app.covers
        .insert("7".into(), iced::widget::image::Handle::from_path(&cover));
    let shown = |app: &App| match app.grid_cover("7") {
        Some(iced::widget::image::Handle::Path(_, path)) => path,
        other => panic!("{other:?}"),
    };
    assert_eq!(shown(&app), cover, "the cover until its copy is made");

    // Made only once the window's scale is known.
    let _ = app.update(Message::Rescaled(2.0));
    let _ = app.update(Message::CoversScaled(
        300,
        vec![("7".into(), cover.clone(), Some(copy.clone()))],
    ));
    assert_eq!(shown(&app), copy);

    // Another cover chosen since: shown as it is until its own copy is made.
    let mine = PathBuf::from("/custom/7/cover.jpg");
    app.customs.insert(
        "7".into(),
        slatty_core::custom::Custom {
            cover: Some(mine.clone()),
            ..Default::default()
        },
    );
    assert_eq!(shown(&app), mine);
    app.customs.clear();

    // Larger covers: made again; one made at the old size and arriving late is dropped.
    let _ = app.update(Message::Settings(crate::settings::SettingsMsg::CoverSize(
        crate::settings::CoverSize(130),
    )));
    assert_eq!(shown(&app), cover);
    let _ = app.update(Message::CoversScaled(
        300,
        vec![("7".into(), cover.clone(), Some(copy))],
    ));
    assert_eq!(shown(&app), cover);
}

/// Copies made for an account are forgotten with it, and those arriving after it was signed out
/// are dropped: they may be in its cache, which forgetting it deleted.
#[test]
fn copies_made_for_an_account_go_with_it() {
    let mut app = library_app();
    app.card_width = 150.0;
    let mine = PathBuf::from("/custom/7/cover.jpg");
    app.customs.insert(
        "7".into(),
        slatty_core::custom::Custom {
            cover: Some(mine.clone()),
            ..Default::default()
        },
    );
    let shown = |app: &App| match app.grid_cover("7") {
        Some(iced::widget::image::Handle::Path(_, path)) => path,
        other => panic!("{other:?}"),
    };
    let _ = app.update(Message::Rescaled(2.0));
    let made = |copy: &str| {
        Message::CoversScaled(300, vec![("7".into(), mine.clone(), Some(copy.into()))])
    };
    let _ = app.update(made("/scaled/300/a.jpg"));
    assert_eq!(shown(&app), PathBuf::from("/scaled/300/a.jpg"));

    let epoch = app.account_epoch;
    let _ = app.update(Message::LoggedOut(Ok(None)));
    assert_eq!(shown(&app), mine, "forgotten with the account");
    let late = Message::AccountResult(epoch, Box::new(made("/scaled/300/b.jpg")));
    let _ = app.update(late);
    assert_eq!(shown(&app), mine, "made for the account signed out");
}

/// The game page draws its key art from the copy made at the window's width, rounded up to 1920,
/// 2560 or 3840 pixels; another width draws the original until its own copy is made.
#[test]
fn key_art_is_drawn_from_its_copy_at_the_windows_width() {
    let mut app = library_app();
    app.window = Size::new(1800.0, 1000.0);
    let art = PathBuf::from("/images/art.jpg");
    let original = iced::widget::image::Handle::from_path(&art);
    let drawn = |app: &App| match app.shown_art(original.clone()) {
        iced::widget::image::Handle::Path(_, path) => path,
        other => panic!("{other:?}"),
    };
    // 1800 × 1.5 × 1.06 = 2862 pixels: made at 3840.
    let _ = app.update(Message::Rescaled(1.5));
    let copy = PathBuf::from("/art/3840/art.jpg");
    let _ = app.update(Message::ArtScaled(art.clone(), 3840, Some(copy.clone())));
    assert_eq!(drawn(&app), copy);

    // 1200 × 1.5 × 1.06 = 1908 pixels: 1920, not made yet.
    let _ = app.update(Message::WindowResized(Size::new(1200.0, 800.0)));
    assert_eq!(drawn(&app), art);
    // Made as it is (`None`): the original.
    let _ = app.update(Message::ArtScaled(art.clone(), 1920, None));
    assert_eq!(drawn(&app), art);
}

/// With Remember filters on, the filters are kept as they change and come back at the next start;
/// off, nothing is kept and the library starts unfiltered.
#[test]
fn filters_are_remembered_only_when_asked() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    let kept = |app: &App| crate::library::kept_filters(&app.core.as_ref().unwrap().db).unwrap();
    let steam_on_linux = Filters {
        steam: true,
        linux: true,
        ..Default::default()
    };
    let _ = app.update(Message::SetFilters(steam_on_linux));
    assert_eq!(kept(&app), (false, None));

    let _ = app.update(Message::Settings(SettingsMsg::RememberFilters(true)));
    assert_eq!(kept(&app), (true, Some(steam_on_linux)), "the filters set");
    let gog = Filters {
        gog: true,
        ..Default::default()
    };
    let _ = app.update(Message::SetFilters(gog));
    assert_eq!(kept(&app), (true, Some(gog)));

    let mut next = App::default();
    let Message::Booted(Ok(mut boot)) = super::game_page::booted(Vec::new(), &[]) else {
        unreachable!()
    };
    (boot.remember_filters, boot.filters) = kept(&app);
    let _ = next.update(Message::Booted(Ok(boot)));
    assert!(next.remember_filters);
    assert_eq!(next.filters, gog);

    let _ = app.update(Message::Settings(SettingsMsg::RememberFilters(false)));
    assert_eq!(kept(&app), (false, None));
    assert_eq!(app.filters, gog, "the filters shown stay");
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

/// A scan reads every game again: one read as it was leaves the lists as they were (making them
/// takes 2 ms at 10,000 games, for each game read), one whose achievements changed is listed anew.
#[test]
fn a_game_read_again_as_it_was_leaves_the_lists_as_they_were() {
    let mut app = library_app();
    app.page = Page::Achievements;
    let epoch = app.account_epoch;
    let read = |achievements| {
        let overview = GameOverview {
            achievements,
            checked: true,
            ..Default::default()
        };
        let read = Message::OverviewFetched("3".into(), Ok(overview));
        Message::AccountResult(epoch, Box::new(read))
    };
    let _ = app.update(read(Some((1, 4))));
    let _ = app.view();
    assert!(app.lists.achievements.borrow().is_some());
    let _ = app.update(read(Some((1, 4))));
    assert!(app.lists.achievements.borrow().is_some(), "kept");
    let _ = app.update(read(Some((2, 4))));
    assert!(app.lists.achievements.borrow().is_none(), "made again");
    assert!(render(&app).find("2 / 4").is_ok());
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
    // A scan of GOG's data (Refresh) reads every game again: a message for each, then a view.
    for page in [Page::Library, Page::Achievements] {
        app.page = page;
        let ids: Vec<String> = app
            .library
            .iter()
            .take(1_000)
            .map(|g| g.id.clone())
            .collect();
        let t = std::time::Instant::now();
        for id in ids {
            let read = app.overview.get(&id).cloned().unwrap_or_default();
            let read = Message::OverviewFetched(id, Ok(read));
            let _ = app.update(Message::AccountResult(app.account_epoch, Box::new(read)));
            let _ = app.view();
        }
        println!(
            "{page:?}, a game read again by a scan: {:?} per message and view",
            t.elapsed() / 1_000
        );
    }
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
    // 1,000 Steam games beside them, a tenth owned on GOG too (50 named by GOG's database, 50 by
    // title): merged as the library loads or GOG's is refreshed, then listed with GOG's.
    app.page = Page::Library;
    for (i, g) in app.library.iter_mut().take(100).enumerate() {
        if i < 50 {
            g.steam = Some(vec![i as u32 + 1]);
        } else {
            g.title = format!("[FAKE] Steam 世界 {}: The Long Journey", i + 1);
        }
    }
    app.steam.on = true;
    app.steam.games = (1..=1_000u32)
        .map(|i| {
            let mut g = fake_game(
                &slatty_core::store::steam_id(i),
                &format!("Steam 世界 {i}: The Long Journey"),
            );
            g.os = vec!["windows".into(), "linux".into()];
            g
        })
        .collect();
    let t = std::time::Instant::now();
    for _ in 0..10 {
        app.merge_steam();
    }
    println!(
        "Steam's games merged: {:?} ({} games, {} owned on both)",
        t.elapsed() / 10,
        app.library.len(),
        app.steam.twins.len()
    );
    for sort in Sort::ALL {
        app.sort = sort;
        app.lists.clear();
        time(&app, &format!("view with Steam's, {sort}"));
    }
    let t = std::time::Instant::now();
    for _ in 0..10 {
        app.lists.clear();
        let _ = app.visible_games();
    }
    println!("list made again with Steam's: {:?}", t.elapsed() / 10);
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
