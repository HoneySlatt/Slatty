use super::*;

#[test]
fn game_page_summarises_achievements() {
    let mut app = app_with_achievements();
    app.panel = None;
    let mut ui = render(&app);
    assert!(ui.find("1 / 2").is_ok());
    assert!(ui.find("50%").is_ok());
    assert!(ui.find("[FAKE] Beta").is_err(), "named on hover only");
    let icon = ui
        .find(iced_test::selector::id("latest-unlock-id-Beta"))
        .expect("latest unlock shown");
    let position = icon.bounds().center();
    ui.point_at(position);
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position,
    })]);
    snapshot(&mut ui, "game-achievements");
}

#[test]
fn unlocking_only_asks_for_confirmation() {
    let mut app = app_with_achievements();
    app.manual_achievements = true;
    let mut ui = render(&app);
    assert!(ui.find("Clear").is_ok());
    ui.click("Unlock").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        messages.as_slice(),
        [Message::AskAchievementChange(game, changes)] if game == "5" && changes.len() == 1 && changes[0].unlock
    ));
    for m in messages {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_some());

    let mut ui = render(&app);
    snapshot(&mut ui, "achievements-confirm");
    ui.click("Confirm").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::ConfirmAchievementChange))
    );
}

#[test]
fn cancelling_drops_the_pending_change() {
    let mut app = app_with_achievements();
    app.manual_achievements = true;
    let _ = app.update(Message::AskAchievementChange(
        "5".into(),
        vec![crate::AchievementChange {
            achievement_id: "id-Alpha".into(),
            name: "[FAKE] Alpha".into(),
            unlock: true,
        }],
    ));
    let mut ui = render(&app);
    ui.click("Cancel").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_none());
}

#[test]
fn achievements_tab_lists_games_by_completion() {
    let mut app = library_app();
    app.page = Page::Achievements;
    for (id, done, total) in [
        ("2", 1, 4),
        ("6", 4, 4),
        ("8", 0, 10),
        ("9", 6, 80),
        ("10", 1, 50),
        ("11", 2, 8),
    ] {
        app.overview.insert(
            id.into(),
            GameOverview {
                achievements: Some((done, total)),
                cloud_saves: false,
                ..Default::default()
            },
        );
    }
    let order: Vec<&str> = app
        .games_by_achievements()
        .iter()
        .map(|(g, _, _)| g.title.as_str())
        .collect();
    assert_eq!(
        order,
        [
            "[FAKE] Game 6",
            "[FAKE] Game 11",
            "[FAKE] Game 2",
            "[FAKE] Game 9",
            "[FAKE] Game 10",
            "[FAKE] Game 8"
        ],
        "highest share first, even with fewer unlocked; equal shares by title"
    );
    let mut ui = render(&app);
    assert!(ui.find("4 / 4").is_ok());
    assert!(ui.find("25%").is_ok());
    assert!(
        ui.find("[FAKE] Game 1").is_err(),
        "no achievements, not listed"
    );
    snapshot(&mut ui, "achievements-tab");
    ui.click("[FAKE] Game 6").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenAchievements(id) if id == "6"))
    );
}

#[test]
fn achievements_tab_opens_a_dedicated_page_per_game() {
    let mut app = app_with_achievements();
    app.manual_achievements = true;
    app.selected = None;
    app.panel = None;
    let _ = app.update(Message::OpenAchievements("5".into()));
    assert_eq!(
        (app.page, app.achievements_game.as_deref()),
        (Page::Achievements, Some("5"))
    );
    settle(&mut app);
    let mut ui = render(&app);
    assert!(ui.find("1 / 2 unlocked").is_ok());
    assert!(ui.find("Time played").is_err(), "not the game page");
    snapshot(&mut ui, "achievements-game");
    ui.click("Unlock").unwrap();
    assert!(
        ui.into_messages().any(
            |m| matches!(m, Message::AskAchievementChange(id, c) if id == "5" && c.len() == 1)
        )
    );
    let _ = app.update(Message::ShowPage(Page::Achievements));
    assert_eq!(app.achievements_game, None);
}

#[test]
fn achievements_go_from_the_most_common_to_the_rarest() {
    let mut app = app_with_achievements();
    let Some(crate::Loadable::Ready(list)) = app.achievements.get_mut("5") else {
        unreachable!()
    };
    list[0].rarity = 4.5;
    list[1].rarity = 62.0;
    list.push(slatty_core::achievements::Achievement {
        rarity: 30.0,
        ..fake_achievement("Gamma", false)
    });
    let place = |ui: &mut iced_test::Simulator<'_, Message>, name: &str| {
        let b = ui.find(name).unwrap().bounds();
        (b.y as i32, b.x as i32)
    };
    let expected = ["[FAKE] Beta", "[FAKE] Gamma", "[FAKE] Alpha"];

    {
        let mut ui = render(&app);
        let drawer: Vec<_> = expected.iter().map(|n| place(&mut ui, n)).collect();
        assert!(drawer.is_sorted(), "drawer: {drawer:?}");
    }

    app.panel = None;
    let _ = app.update(Message::OpenAchievements("5".into()));
    let mut ui = render(&app);
    let page: Vec<_> = expected.iter().map(|n| place(&mut ui, n)).collect();
    assert!(page.is_sorted(), "page, row by row: {page:?}");
}

#[test]
fn a_narrow_window_lays_the_achievements_drawer_over_the_page() {
    use iced::mouse::{Button, Event as Mouse};
    let mut app = app_with_achievements();
    let click_page = |app: &App| -> Vec<Message> {
        let mut ui = Simulator::with_size(settings(), app.window, app.view());
        ui.point_at(iced::Point::new(100.0, 300.0));
        let _ = ui.simulate([
            iced::Event::Mouse(Mouse::ButtonPressed(Button::Left)),
            iced::Event::Mouse(Mouse::ButtonReleased(Button::Left)),
        ]);
        ui.into_messages().collect()
    };
    assert!(
        !click_page(&app)
            .iter()
            .any(|m| matches!(m, Message::ClosePanel)),
        "beside the page in a wide window"
    );
    let _ = app.update(Message::WindowResized(Size::new(800.0, 560.0)));
    {
        let mut ui = Simulator::with_size(settings(), app.window, app.view());
        snapshot(&mut ui, "drawer-narrow");
    }
    assert!(
        click_page(&app)
            .iter()
            .any(|m| matches!(m, Message::ClosePanel)),
        "over the page in a narrow window, closed by a click beside it"
    );
}

#[test]
fn the_achievements_tab_builds_only_the_tiles_in_view_at_10000_games() {
    let mut app = library_app();
    app.library = (1..=10_000)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    for g in &app.library {
        app.overview.entry(g.id.clone()).or_default().achievements = Some((3, 10));
    }
    app.page = Page::Achievements;
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Game 1").is_ok());
    assert!(ui.find("[FAKE] Game 5000").is_err(), "far below: not built");
}

#[test]
fn manual_changes_are_offered_only_once_turned_on_in_advanced_settings() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = app_with_achievements();
    assert!(!app.manual_achievements, "off until turned on");
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Alpha").is_ok());
        for label in ["Unlock", "Clear", "Unlock all"] {
            assert!(ui.find(label).is_err(), "{label} shown while off");
        }
    }
    // Asked anyway (a message left from before it was turned off): nothing to confirm.
    let _ = app.update(Message::AskAchievementChange(
        "5".into(),
        vec![crate::achievements::AchievementChange {
            achievement_id: "id-Alpha".into(),
            name: "[FAKE] Alpha".into(),
            unlock: true,
        }],
    ));
    assert!(app.pending_change.is_none());

    app.page = Page::Settings;
    app.selected = None;
    let mut ui = render(&app);
    assert!(ui.find(Section::Advanced.title()).is_ok());
    assert!(ui.find("Manual achievements").is_ok());
    drop(ui);
    let _ = app.update(Message::Settings(SettingsMsg::ManualAchievements(true)));
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(
        slatty_core::settings::manual_achievements(&db).unwrap(),
        "kept"
    );

    open(&mut app, "5", Some(Panel::Achievements));
    let mut ui = render(&app);
    assert!(ui.find("Unlock").is_ok() && ui.find("Clear").is_ok());
    assert!(ui.find("Unlock all").is_ok());
}

#[test]
fn clearing_warns_that_the_game_may_unlock_it_again() {
    let mut app = app_with_achievements();
    app.manual_achievements = true;
    let warning = "A game that keeps its own record of its achievements can unlock a cleared one \
                   again the next time it runs.";
    let ask = |app: &mut App, unlock| {
        let _ = app.update(Message::AskAchievementChange(
            "5".into(),
            vec![crate::AchievementChange {
                achievement_id: "id-Beta".into(),
                name: "[FAKE] Beta".into(),
                unlock,
            }],
        ));
    };
    ask(&mut app, false);
    assert!(render(&app).find(warning).is_ok());
    ask(&mut app, true);
    assert!(render(&app).find(warning).is_err(), "not for an unlock");
}
