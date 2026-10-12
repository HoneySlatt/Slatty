use slatty_core::session::Playtime;

use super::*;
use crate::couch::{CouchMsg, MenuItem, Tab};
use crate::pad::{Key, Nav};

/// The library in the couch mode: game 2 played lately, games 5 and 7 favorites, game 3
/// installed.
fn couch_app() -> App {
    let mut app = library_app();
    app.window = SIZE;
    app.playtime.insert(
        "2".into(),
        Playtime {
            seconds: 7200,
            last_played: 1_791_400_000,
        },
    );
    app.favorites.extend(["5".to_string(), "7".to_string()]);
    let _ = app.update(Message::Couch(CouchMsg::Enter));
    app
}

fn nav(app: &mut App, nav: Nav) {
    let _ = app.update(Message::Couch(CouchMsg::Nav(nav)));
}

fn key(app: &mut App, key: Key) {
    let _ = app.update(Message::Couch(CouchMsg::Key(key)));
}

fn couch(app: &App) -> &crate::couch::Couch {
    app.couch.as_ref().expect("couch mode")
}

#[test]
fn the_couch_mode_opens_from_the_top_bar_and_closes_from_its_menu() {
    let mut app = library_app();
    let mut ui = render(&app);
    ui.click(iced_test::selector::id("couch-tab")).unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        messages.as_slice(),
        [Message::Couch(CouchMsg::Enter)]
    ));
    // From a game's page too: the couch mode starts on its Home.
    app.selected = Some("5".into());
    let _ = app.update(Message::Couch(CouchMsg::Enter));
    assert!(app.couch.is_some());
    assert_eq!(app.selected, None);
    nav(&mut app, Nav::Menu);
    assert_eq!(couch(&app).menu, Some(0));
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-menu");
    assert!(ui.find("Desktop mode").is_ok());
    drop(ui);
    nav(&mut app, Nav::Confirm);
    assert!(app.couch.is_none(), "back to the desktop mode");
}

#[test]
fn home_shows_the_games_played_lately_the_favorites_and_those_installed() {
    let mut app = couch_app();
    let rows = app.home_rows();
    let titles: Vec<&str> = rows.iter().map(|r| r.title).collect();
    assert_eq!(titles, ["Continue playing", "Favorites", "Installed"]);
    let ids = |r: usize| -> Vec<String> {
        rows[r]
            .games
            .iter()
            .map(|&i| app.library[i].id.clone())
            .collect()
    };
    assert_eq!(ids(0), ["2"]);
    assert_eq!(ids(1), ["5", "7"]);
    assert_eq!(ids(2), ["3"]);
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-home");
    assert!(ui.find("Continue playing").is_ok());
    assert!(
        ui.find("[FAKE] Game 2").is_ok(),
        "the game with the focus, over its art"
    );
    drop(ui);

    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Right);
    let (row, col, g) = app.home_focus().unwrap();
    assert_eq!((row, col, g.id.as_str()), (1, 1, "7"));
    // Each row keeps where it was left.
    nav(&mut app, Nav::Up);
    nav(&mut app, Nav::Down);
    assert_eq!(app.home_focus().unwrap().2.id, "7");
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.selected.as_deref(), Some("7"));
    nav(&mut app, Nav::Back);
    assert_eq!(app.selected, None);
}

#[test]
fn a_new_library_shows_whole_on_the_home_page() {
    let mut app = library_app();
    app.installs.clear();
    app.window = SIZE;
    let _ = app.update(Message::Couch(CouchMsg::Enter));
    let rows = app.home_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Your library");
    assert_eq!(rows[0].games.len(), app.library.len());
}

#[test]
fn y_marks_a_favorite_where_the_focus_is() {
    let mut app = couch_app();
    nav(&mut app, Nav::Action);
    assert!(app.favorites.contains("2"));
    // The rows follow.
    assert!(
        app.home_rows()
            .iter()
            .any(|r| r.title == "Favorites" && r.games.len() == 3)
    );
}

#[test]
fn the_library_grid_moves_by_rows_and_keeps_the_focus_in_view() {
    let mut app = couch_app();
    nav(&mut app, Nav::NextTab);
    assert_eq!(couch(&app).tab, Tab::Library);
    let across = app.couch_columns();
    let down = app.couch_rows();
    assert!(across > 1 && down >= 1);
    nav(&mut app, Nav::Right);
    assert_eq!(couch(&app).card, 1);
    nav(&mut app, Nav::Down);
    assert_eq!(couch(&app).card, 1 + across);
    // Past the rows shown, the grid follows.
    for _ in 0..down {
        nav(&mut app, Nav::Down);
    }
    let c = couch(&app);
    let row = c.card / across;
    assert!(
        c.top <= row && row < c.top + down,
        "row {row} shown from {}",
        c.top
    );
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-library");
    drop(ui);
    // Up from the first row reaches the shelves.
    for _ in 0..20 {
        nav(&mut app, Nav::Up);
    }
    assert_eq!(couch(&app).on_shelves, Some(0));
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).shelf, crate::library::Shelf::Installed);
    let games: Vec<String> = app
        .couch_games()
        .iter()
        .map(|&i| app.library[i].id.clone())
        .collect();
    assert_eq!(games, ["3"]);
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.selected.as_deref(), Some("3"));
}

#[test]
fn the_search_types_with_the_keyboard_on_screen_and_a_real_one() {
    let mut app = couch_app();
    nav(&mut app, Nav::Search);
    let c = couch(&app);
    assert_eq!((c.tab, c.keyboard), (Tab::Library, Some((1, 0))));
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-keyboard");
    assert!(ui.find("Type a title").is_ok());
    drop(ui);
    // 1 then 2, from the row of digits.
    nav(&mut app, Nav::Up);
    nav(&mut app, Nav::Confirm);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).search, "12");
    let found: Vec<String> = app
        .couch_games()
        .iter()
        .map(|&i| app.library[i].title.clone())
        .collect();
    assert_eq!(found, ["[FAKE] Game 12"]);
    nav(&mut app, Nav::Search);
    key(&mut app, Key::Char('3'));
    assert_eq!(couch(&app).search, "13");
    key(&mut app, Key::Erase);
    key(&mut app, Key::Erase);
    assert_eq!(couch(&app).search, "");
    // Done, on the row of actions.
    for _ in 0..5 {
        nav(&mut app, Nav::Down);
    }
    for _ in 0..5 {
        nav(&mut app, Nav::Right);
    }
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).keyboard, None);
}

#[test]
fn typing_from_the_home_page_starts_a_search() {
    let mut app = couch_app();
    key(&mut app, Key::Char('9'));
    let c = couch(&app);
    assert_eq!(c.tab, Tab::Library);
    assert_eq!(c.search, "9");
    assert!(c.keyboard.is_some());
}

#[test]
fn a_game_not_installed_installs_with_the_defaults() {
    let mut app = couch_app();
    let _ = app.update(Message::Select("5".into()));
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-game");
    assert!(ui.find("Install").is_ok());
    drop(ui);
    nav(&mut app, Nav::Confirm);
    assert!(app.start_planned.contains("5"));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(crate::install::PlanInfo {
            title: "[FAKE] Game 5".into(),
            ..fake_plan()
        }),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert!(app.start_planned.is_empty());
}

#[test]
fn an_install_that_cannot_be_planned_says_why() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Install("5".into())));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Err("[FAKE] no build".into()),
    )));
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    assert!(!matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
}

/// Two installs asked before GOG answered for the first: both start, one after the other.
#[test]
fn installs_asked_one_after_the_other_all_start() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Install("5".into())));
    let _ = app.update(Message::Couch(CouchMsg::Install("7".into())));
    for id in ["5", "7"] {
        let _ = app.update(Message::Install(InstallMsg::Planned(
            id.into(),
            Ok(crate::install::PlanInfo {
                title: format!("[FAKE] Game {id}"),
                ..fake_plan()
            }),
        )));
    }
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.queue, ["7"], "the second waits for the first");
}

/// The download running takes a moment to stop for Download now: the tab says so at once, and
/// offers nothing that would undo it meanwhile.
#[test]
fn the_downloads_tab_shows_a_download_stopping_for_another() {
    use super::offline::installing_6;
    use crate::downloads::DownloadsMsg;
    let mut app = couch_app();
    installing_6(&mut app);
    app.queue = vec!["7".into(), "8".into()];
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow("7".into())));
    let rows = app.download_rows();
    assert_eq!(rows[0].status, "Pausing…");
    assert!(rows[0].buttons.is_empty());
    let seven = rows.iter().find(|r| r.game_id == "7").unwrap();
    assert_eq!(seven.status, "Starting…");
    assert!(
        rows.iter()
            .all(|r| r.buttons.iter().all(|(label, _)| *label != "Download now"))
    );
}

#[test]
fn a_download_discarded_while_resuming_is_not_started_by_its_plan() {
    let mut app = couch_app();
    app.interrupted = vec![("5".into(), crate::Interrupted::Paused)];
    let _ = app.update(Message::Couch(CouchMsg::Install("5".into())));
    let _ = app.update(Message::Install(InstallMsg::Discard("5".into())));
    // GOG answers before the discard ends.
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    )));
    assert!(app.installing().is_none(), "not started");
    assert!(!app.queue.contains(&"5".to_string()), "not queued");
    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    assert!(app.installing().is_none());
    assert!(!app.install_views.contains_key("5"));
}

#[test]
fn an_installed_game_plays_from_its_page() {
    let mut app = couch_app();
    app.launch_options.insert("3".into(), Vec::new());
    let _ = app.update(Message::Select("3".into()));
    nav(&mut app, Nav::Confirm);
    assert!(
        app.play
            .as_ref()
            .is_some_and(|p| p.running && p.game_id == "3")
    );
    // Moves from a gamepad belong to the game while it runs.
    nav(&mut app, Nav::Back);
    assert_eq!(app.selected.as_deref(), Some("3"));
}

/// Typed on a real keyboard, the search ends with Enter, as it would elsewhere; A on a gamepad
/// still presses the key with the focus.
#[test]
fn enter_on_a_keyboard_ends_the_search() {
    let mut app = couch_app();
    key(&mut app, Key::Nav(Nav::Search));
    key(&mut app, Key::Char('1'));
    key(&mut app, Key::Nav(Nav::Confirm));
    assert_eq!(couch(&app).search, "1");
    assert!(couch(&app).keyboard.is_none());
    nav(&mut app, Nav::Search);
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).search, "1q", "the key with the focus");
}

/// A cover tapped above the keyboard opens its game: the keyboard, hidden then, takes no press.
#[test]
fn the_keyboard_hidden_by_a_game_page_takes_nothing() {
    let mut app = couch_app();
    app.launch_options.insert("3".into(), Vec::new());
    nav(&mut app, Nav::Search);
    assert!(couch(&app).keyboard.is_some());
    let _ = app.update(Message::Select("3".into()));
    nav(&mut app, Nav::Confirm);
    key(&mut app, Key::Char('x'));
    assert_eq!(couch(&app).search, "");
    assert!(
        app.play
            .as_ref()
            .is_some_and(|p| p.running && p.game_id == "3")
    );
}

/// A notice (an install that cannot start, Steam not reached) closes with B, before anything else
/// B does; the hints say so.
#[test]
fn a_notice_closes_with_b() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    app.notice = Some(crate::Notice {
        error: true,
        text: "[FAKE] cannot be installed".into(),
    });
    assert!(render(&app).find("Close the notice").is_ok());
    nav(&mut app, Nav::Back);
    assert!(app.notice.is_none());
    assert_eq!(couch(&app).tab, Tab::Library, "nothing else");
}

#[test]
fn a_game_started_several_ways_asks_which() {
    let mut app = couch_app();
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "[FAKE] Settings".into()],
    );
    let _ = app.update(Message::Select("3".into()));
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.launch_prompt.as_deref(), Some("3"));
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-launch");
    assert!(ui.find("[FAKE] Settings").is_ok(), "the ways are shown");
    drop(ui);
    // Back leaves the question, which then holds nothing (updates wait while it is asked).
    nav(&mut app, Nav::Back);
    assert!(app.launch_prompt.is_none());
    assert_eq!(app.selected.as_deref(), Some("3"));
    nav(&mut app, Nav::Confirm);
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Confirm);
    assert!(app.launch_prompt.is_none());
    assert!(
        app.play
            .as_ref()
            .is_some_and(|p| p.running && p.game_id == "3")
    );
    assert_eq!(
        app.launch_choices.get("3").map(String::as_str),
        Some("[FAKE] Settings")
    );
}

#[test]
fn the_achievements_list_opens_from_the_game_page() {
    let mut app = couch_app();
    app.achievements.insert(
        "3".into(),
        crate::Loadable::Ready(vec![
            fake_achievement("Alpha", false),
            fake_achievement("Beta", true),
        ]),
    );
    let _ = app.update(Message::Select("3".into()));
    let buttons: Vec<String> = app
        .game_buttons(&app.library[2].clone())
        .into_iter()
        .map(|b| b.1)
        .collect();
    assert_eq!(buttons, ["Play", "Add to favorites", "Achievements"]);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).achievements, Some(0));
    nav(&mut app, Nav::Down);
    assert_eq!(couch(&app).achievements, Some(1));
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-achievements");
    assert!(ui.find("Achievements  ·  1 / 2").is_ok());
    drop(ui);
    nav(&mut app, Nav::Back);
    assert_eq!(couch(&app).achievements, None);
    assert_eq!(app.selected.as_deref(), Some("3"), "the page stays");
}

#[test]
fn the_downloads_tab_runs_the_queue_from_the_gamepad() {
    use super::offline::installing_6;
    let mut app = couch_app();
    installing_6(&mut app);
    app.queue = vec!["7".into(), "8".into()];
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Downloads)));
    let rows = app.download_rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].buttons[0].0, "Pause");
    assert_eq!(rows[2].buttons[0].0, "Download now");
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-downloads");
    drop(ui);
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.queue, ["7"], "Remove on the last row");
}

#[test]
fn a_gamepad_is_ignored_while_the_window_is_behind_another() {
    let mut app = couch_app();
    app.unfocused = true;
    nav(&mut app, Nav::NextTab);
    assert_eq!(couch(&app).tab, Tab::Home);
    // The keyboard reaches only the window in front.
    key(&mut app, Key::Nav(Nav::NextTab));
    assert_eq!(couch(&app).tab, Tab::Library);
}

#[test]
fn quitting_with_work_running_asks_first() {
    use super::offline::installing_6;
    let mut app = couch_app();
    installing_6(&mut app);
    let _ = app.update(Message::Couch(CouchMsg::Menu(MenuItem::Quit)));
    assert!(app.quit_confirm.is_some());
    let mut ui = render(&app);
    assert!(ui.find("Keep running").is_ok());
    drop(ui);
    nav(&mut app, Nav::Back);
    assert!(app.quit_confirm.is_none());
}
