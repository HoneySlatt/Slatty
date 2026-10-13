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

/// Scrolled down to the last row, the window grows (moved to a television): the row with the focus
/// still shows, rather than an empty grid.
#[test]
fn the_row_with_the_focus_shows_after_the_window_grows() {
    let mut app = couch_app();
    app.library = (1..=60)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    app.lists.clear();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    for _ in 0..12 {
        nav(&mut app, Nav::Down);
    }
    let _ = app.update(Message::WindowResized(Size::new(3840.0, 2160.0)));
    // A cover of the focus's row, other than the one with the focus, whose title the header shows.
    assert!(render_window(&app).find("[FAKE] Game 50").is_ok());
}

/// Iced would squash a row of covers that does not fit to the room left, its covers cropped to
/// thin strips: only rows that fit whole are shown.
#[test]
fn only_rows_that_fit_whole_are_shown() {
    let mut app = couch_app();
    // At 1440x900: one row of Home under its key art; two rows of six in Library.
    let mut ui = render(&app);
    assert!(ui.find("Continue playing").is_ok());
    assert!(ui.find("Favorites").is_err(), "the next row does not fit");
    drop(ui);
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Game 12").is_ok());
    assert!(
        ui.find("[FAKE] Game 13").is_err(),
        "a third row does not fit"
    );
}

/// Home's rows of covers fit whole under its key art at any size, the key art shrinking on a small
/// window so that the row with the focus does.
#[test]
fn home_fits_its_rows_whole_at_any_size() {
    use crate::couch::{GAP, HINTS, TOP_BAR};
    let mut app = couch_app();
    for (width, height) in [
        (800.0, 560.0),
        (1280.0, 800.0),
        (1920.0, 1080.0),
        (3840.0, 2160.0),
    ] {
        app.window = Size::new(width, height);
        let (hero, rows, row) = app.home_layout();
        let used = hero + rows as f32 * row + (rows - 1) as f32 * GAP;
        assert!(used <= height - TOP_BAR - HINTS - 8.0, "{width}x{height}");
        assert!(hero >= height * 0.25, "{width}x{height}: the key art stays");
    }
}

/// A notice above the pages takes room from them: the rows shown still fit whole.
#[test]
fn rows_fit_whole_under_a_notice() {
    use crate::couch::{GAP, HINTS, NOTICE, SHELVES, TOP_BAR};
    let mut app = couch_app();
    app.window = Size::new(1280.0, 800.0);
    app.notice = Some(crate::Notice {
        error: true,
        text: "[FAKE] cannot be installed".into(),
    });
    let (hero, rows, row) = app.home_layout();
    let used = hero + rows as f32 * row + (rows - 1) as f32 * GAP;
    assert!(used <= 800.0 - TOP_BAR - HINTS - 8.0 - NOTICE, "Home");
    let (rows, row) = (app.couch_rows(), app.couch_card() * 4.0 / 3.0 + GAP);
    assert!(
        rows as f32 * row - GAP <= 800.0 - TOP_BAR - HINTS - SHELVES - NOTICE,
        "Library"
    );
}

/// The couch mode's covers are larger than the desktop grid's: the copies made for the grid would
/// be enlarged, softly, so the couch mode draws the cover itself.
#[test]
fn couch_covers_are_not_enlarged_from_the_grids_copies() {
    let mut app = couch_app();
    app.card_width = 150.0;
    let cover = PathBuf::from("/covers/5.jpg");
    let copy = PathBuf::from("/scaled/150/5.jpg");
    app.covers
        .insert("5".into(), iced::widget::image::Handle::from_path(&cover));
    let _ = app.update(Message::Rescaled(1.0));
    let _ = app.update(Message::CoversScaled(
        150,
        vec![("5".into(), cover.clone(), Some(copy.clone()))],
    ));
    let path = |h: Option<iced::widget::image::Handle>| match h {
        Some(iced::widget::image::Handle::Path(_, p)) => p,
        other => panic!("{other:?}"),
    };
    assert_eq!(path(app.grid_cover("5")), copy, "the desktop's grid");
    assert_eq!(path(app.cover_at("5", app.couch_card())), cover);
    // Drawn no wider than the copy (Downloads' small covers): the copy.
    assert_eq!(path(app.cover_at("5", 66.0)), copy);
    // A little wider (a Steam Deck's 173 pixels): the copy still, enlarged a little, rather than
    // a cover two or three times as wide, which Iced would shrink by sampling a few of its pixels.
    app.window = Size::new(1280.0, 800.0);
    assert_eq!(path(app.cover_at("5", app.couch_card() - 8.0)), copy);
}

/// Home's key art is drawn from its copy at the window's width, as a game page's: the original can
/// be 3840 pixels wide, decoded anew at each move.
#[test]
fn home_draws_the_key_art_from_its_copy() {
    let mut app = couch_app();
    let art = PathBuf::from("/custom/5/art.jpg");
    app.customs.insert(
        "5".into(),
        slatty_core::custom::Custom {
            background: Some(art.clone()),
            ..Default::default()
        },
    );
    let _ = app.update(Message::Rescaled(1.0));
    // Down to Favorites, on game 5, then on to 7 at once: nothing made for 5, passed over.
    nav(&mut app, Nav::Down);
    assert_eq!(app.home_focus().unwrap().2.id, "5");
    nav(&mut app, Nav::Right);
    let rested = |app: &mut App, id: &str| {
        app.update(Message::Couch(CouchMsg::Rested(id.into())))
            .units()
    };
    assert_eq!(rested(&mut app, "5"), 0);
    // Back on 5, where the focus rests: its copy is asked for then.
    nav(&mut app, Nav::Left);
    assert!(rested(&mut app, "5") > 0, "its copy asked for");
    let copy = PathBuf::from("/art/1920/art.jpg");
    let _ = app.update(Message::ArtScaled(art, 1920, Some(copy.clone())));
    let g = app.home_focus().unwrap().2.clone();
    assert!(matches!(
        app.hero_art(&g),
        Some(iced::widget::image::Handle::Path(_, p)) if p == copy
    ));
}

/// Key art arriving while the focus passes over a game of Home makes no copy before the focus rests
/// there, nor does another image arriving meanwhile: one copy per game passed over is the burst the
/// rest avoids. Once rested, it is made.
#[test]
fn home_makes_no_key_art_copy_before_the_focus_rests() {
    let mut app = couch_app();
    let url = "https://images.example/[FAKE]-5.jpg".to_string();
    assert_eq!(app.library[4].id, "5");
    app.library[4].background = Some(url.clone());
    let _ = app.update(Message::Rescaled(1.0));
    nav(&mut app, Nav::Down);
    assert_eq!(app.home_focus().unwrap().2.id, "5");
    let mut arrived = |url: String| {
        let file = root().join("[FAKE]-art.jpg");
        app.update(Message::Image(url, Some(file))).units()
    };
    assert_eq!(arrived(url.clone()), 0, "no copy before the rest");
    assert_eq!(arrived("https://images.example/[FAKE]-9.jpg".into()), 0);
    assert!(
        app.update(Message::Couch(CouchMsg::Rested("5".into())))
            .units()
            > 0
    );

    // Rested before its art arrived: the copy is made as it arrives.
    let mut app = couch_app();
    app.library[4].background = Some(url.clone());
    let _ = app.update(Message::Rescaled(1.0));
    nav(&mut app, Nav::Down);
    assert_eq!(
        app.update(Message::Couch(CouchMsg::Rested("5".into())))
            .units(),
        0
    );
    let file = root().join("[FAKE]-art.jpg");
    assert!(app.update(Message::Image(url, Some(file))).units() > 0);
}

/// On a screen larger than 1920×1080 (a 4K television at scale 1), the couch mode is laid out as on
/// a 1080p one and drawn larger, its text and key art sharp at the screen's pixels, rather than
/// small in a corner; at 1080p or a Steam Deck's 1280×800 it is drawn as it is, the desktop never.
#[test]
fn the_couch_mode_fills_a_large_screen() {
    let mut app = couch_app();
    let art = PathBuf::from("/custom/5/art.jpg");
    app.customs.insert(
        "5".into(),
        slatty_core::custom::Custom {
            background: Some(art.clone()),
            ..Default::default()
        },
    );
    let _ = app.update(Message::Rescaled(1.0));
    let _ = app.update(Message::WindowResized(Size::new(3840.0, 2160.0)));
    assert_eq!(app.ui_scale, 2.0);
    assert_eq!(app.window, Size::new(1920.0, 1080.0));
    nav(&mut app, Nav::Down);
    let copy = PathBuf::from("/art/3840/art.jpg");
    let _ = app.update(Message::ArtScaled(art, 3840, Some(copy.clone())));
    let g = app.home_focus().unwrap().2.clone();
    assert!(matches!(
        app.hero_art(&g),
        Some(iced::widget::image::Handle::Path(_, p)) if p == copy
    ));
    // Once Iced draws at that scale, it gives the window's size in the interface's units: the
    // same screen, the same scale, nothing that grows each time.
    assert_eq!(app.scale_factor(), 2.0);
    let _ = app.update(Message::WindowResized(Size::new(1920.0, 1080.0)));
    assert_eq!(app.ui_scale, 2.0);
    assert_eq!(app.window, Size::new(1920.0, 1080.0));
    // Back on the desktop, the screen at its own scale.
    let _ = app.update(Message::Couch(CouchMsg::Leave));
    assert_eq!(app.ui_scale, 1.0);
    assert_eq!(app.window, Size::new(3840.0, 2160.0));

    for screen in [Size::new(1920.0, 1080.0), Size::new(1280.0, 800.0)] {
        let mut app = couch_app();
        let _ = app.update(Message::WindowResized(screen));
        assert_eq!(app.ui_scale, 1.0);
        assert_eq!(app.window, screen);
    }
    let mut app = library_app();
    let _ = app.update(Message::WindowResized(Size::new(3840.0, 2160.0)));
    assert_eq!(app.ui_scale, 1.0);
}

/// On a large screen the couch mode draws covers wider than GOG makes them (342 pixels): those in
/// view are enlarged as key art is, sharper than the enlarging Iced does as it draws; at 1080p the
/// grid's copies are about as wide, and nothing is made.
#[test]
fn the_couch_mode_enlarges_its_covers_in_view_on_a_large_screen() {
    let mut app = couch_app();
    app.card_width = 225.0;
    let cover = PathBuf::from("/covers/5.jpg");
    app.covers
        .insert("5".into(), iced::widget::image::Handle::from_path(&cover));
    let _ = app.update(Message::Rescaled(1.0));
    let _ = app.update(Message::WindowResized(Size::new(1920.0, 1080.0)));
    nav(&mut app, Nav::Down);
    assert!(app.couch_in_view().contains(&4), "game 5, a favorite");
    assert_eq!(app.scale_couch_covers().units(), 0);

    let resized = app.update(Message::WindowResized(Size::new(3840.0, 2160.0)));
    assert!(resized.units() > 0, "its enlarged copy asked for");
    let width = app.couch_cover_width();
    let pixels = (width * 2.0).ceil() as u32;
    let path = |h: Option<iced::widget::image::Handle>| match h {
        Some(iced::widget::image::Handle::Path(_, p)) => p,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        path(app.cover_at("5", width)),
        cover,
        "until its copy is made"
    );
    let copy = PathBuf::from(format!("/art/{pixels}/5.jpg"));
    let _ = app.update(Message::CouchCoversScaled(
        pixels,
        vec![("5".into(), cover.clone(), Some(copy.clone()))],
    ));
    assert_eq!(path(app.cover_at("5", width)), copy);
    // Asked for once.
    assert_eq!(app.scale_couch_covers().units(), 0);
}

/// The couch mode's Home draws key art as wide as the window, not enlarged as a game page does: on
/// a 1080p television its copy is 1920 pixels wide, not 2560.
#[test]
fn couch_key_art_copies_are_as_wide_as_the_window() {
    let mut app = couch_app();
    app.window = Size::new(1920.0, 1080.0);
    let art = PathBuf::from("/custom/5/art.jpg");
    app.customs.insert(
        "5".into(),
        slatty_core::custom::Custom {
            background: Some(art.clone()),
            ..Default::default()
        },
    );
    let _ = app.update(Message::Rescaled(1.0));
    nav(&mut app, Nav::Down);
    let copy = PathBuf::from("/art/1920/art.jpg");
    let _ = app.update(Message::ArtScaled(art, 1920, Some(copy.clone())));
    let g = app.home_focus().unwrap().2.clone();
    assert!(matches!(
        app.hero_art(&g),
        Some(iced::widget::image::Handle::Path(_, p)) if p == copy
    ));
}

/// Home's focus can land on a game without a move, or come back to Home's own width: back from a
/// game page (whose copy is wider), after a favorite toggled moves it, at a return to the tab. The
/// copy is asked for then too, once the focus rests.
#[test]
fn home_asks_for_its_key_art_copy_wherever_its_focus_lands() {
    let custom = |app: &mut App, id: &str| {
        app.customs.insert(
            id.into(),
            slatty_core::custom::Custom {
                background: Some(PathBuf::from(format!("/custom/{id}/art.jpg"))),
                ..Default::default()
            },
        );
    };
    // Back from a game page.
    let mut app = couch_app();
    app.window = Size::new(1920.0, 1080.0);
    custom(&mut app, "5");
    let _ = app.update(Message::Rescaled(1.0));
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Confirm);
    let art = PathBuf::from("/custom/5/art.jpg");
    let page = PathBuf::from("/art/2560/art.jpg");
    let _ = app.update(Message::ArtScaled(art, 2560, Some(page)));
    let _ = app.update(Message::Couch(CouchMsg::Rested("5".into())));
    assert!(app.update(Message::Couch(CouchMsg::Nav(Nav::Back))).units() > 0);

    // A favorite toggled: 5 leaves Favorites, the focus lands on 7.
    let mut app = couch_app();
    custom(&mut app, "7");
    let _ = app.update(Message::Rescaled(1.0));
    nav(&mut app, Nav::Down);
    let _ = app.update(Message::Couch(CouchMsg::Rested("5".into())));
    let toggled = app.update(Message::Couch(CouchMsg::Nav(Nav::Action)));
    assert_eq!(app.home_focus().unwrap().2.id, "7");
    assert!(toggled.units() > 0, "a rest for 7");

    // Back at the tab.
    nav(&mut app, Nav::NextTab);
    assert!(
        app.update(Message::Couch(CouchMsg::Nav(Nav::PrevTab)))
            .units()
            > 0
    );
    nav(&mut app, Nav::NextTab);
    let tab = app.update(Message::Couch(CouchMsg::Tab(crate::couch::Tab::Home)));
    assert!(tab.units() > 0);
}

/// The couch mode's game page draws key art over the whole page, wider than the window when the art
/// is wide (a Steam hero): its copy is made a step larger, as the desktop's page makes it.
#[test]
fn the_couch_game_page_copies_key_art_a_step_larger() {
    let mut app = couch_app();
    app.window = Size::new(1920.0, 1080.0);
    let art = PathBuf::from("/custom/5/art.jpg");
    app.customs.insert(
        "5".into(),
        slatty_core::custom::Custom {
            background: Some(art.clone()),
            ..Default::default()
        },
    );
    let _ = app.update(Message::Rescaled(1.0));
    let _ = app.update(Message::Select("5".into()));
    let copy = PathBuf::from("/art/2560/art.jpg");
    let _ = app.update(Message::ArtScaled(art, 2560, Some(copy.clone())));
    let g = app.selected_game().unwrap().clone();
    assert!(matches!(
        app.hero_art(&g),
        Some(iced::widget::image::Handle::Path(_, p)) if p == copy
    ));
}

/// Back at a tab, with nowhere else to go, opens the menu and its way out; so does a click on the
/// menu's hint.
#[test]
fn back_at_a_tab_opens_the_menu() {
    let mut app = couch_app();
    key(&mut app, Key::Nav(Nav::Back));
    assert_eq!(couch(&app).menu, Some(0));
    key(&mut app, Key::Nav(Nav::Back));
    assert_eq!(couch(&app).menu, None);
    let mut ui = render(&app);
    ui.click("Menu").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Couch(CouchMsg::OpenMenu)))
    );
}

/// The menu's hint, a button, sits on the same line as the others.
#[test]
fn the_hints_are_on_one_line() {
    use iced_test::selector::Candidate;
    let app = couch_app();
    let mut ui = render(&app);
    let mut middle = |wanted: &str| {
        let bounds = ui
            .find(|c: Candidate<'_>| match c {
                Candidate::Text {
                    content, bounds, ..
                } if content == wanted => Some(bounds),
                _ => None,
            })
            .unwrap();
        bounds.center_y()
    };
    let (search, menu) = (middle("Search"), middle("Menu"));
    assert!((search - menu).abs() < 1.0, "{search} and {menu}");
}

/// On the shelves above the covers, the hints say what A and X do there; Y does nothing.
#[test]
fn the_hints_follow_the_focus_onto_the_shelves() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    nav(&mut app, Nav::Up);
    assert!(couch(&app).on_shelves.is_some());
    let mut ui = render(&app);
    assert!(ui.find("Choose").is_ok());
    assert!(ui.find("Favorite").is_err());
}

/// Backspace goes back as B does, but at a tab, where B opens the menu, it does nothing: one too
/// many after a search would open the menu, and the next Enter leave the couch mode.
#[test]
fn backspace_at_a_tab_opens_no_menu() {
    let mut app = couch_app();
    key(&mut app, Key::Char('a'));
    key(&mut app, Key::Nav(Nav::Confirm));
    assert!(couch(&app).keyboard.is_none());
    key(&mut app, Key::Erase);
    assert_eq!(couch(&app).menu, None);
    // On a game's page it still goes back.
    let _ = app.update(Message::Select("3".into()));
    key(&mut app, Key::Erase);
    assert_eq!(app.selected, None);
}

/// Arrows and Enter alone (a television's remote, which sends keys) still type on the keyboard on
/// the screen: Enter ends the search only after letters typed.
#[test]
fn arrows_and_enter_type_on_the_keyboard_on_the_screen() {
    let mut app = couch_app();
    key(&mut app, Key::Nav(Nav::Search));
    key(&mut app, Key::Nav(Nav::Right));
    key(&mut app, Key::Nav(Nav::Confirm));
    assert_eq!(couch(&app).search, "w");
    assert!(couch(&app).keyboard.is_some());
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

/// A way clicked or tapped in the launch question leaves its focus for the next question back on
/// the first way.
#[test]
fn the_launch_question_starts_on_the_first_way_each_time() {
    let mut app = couch_app();
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "[FAKE] Settings".into()],
    );
    let _ = app.update(Message::Select("3".into()));
    nav(&mut app, Nav::Confirm);
    nav(&mut app, Nav::Down);
    assert_eq!(couch(&app).launch, 1);
    let _ = app.update(Message::LaunchAs("3".into(), "[FAKE] Settings".into()));
    assert_eq!(couch(&app).launch, 0);
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

/// Remove on a download that started sets it aside among those stopped, its row where the queue's
/// was: the focus lands on its Resume, so that a second press does not discard it.
#[test]
fn remove_on_a_started_download_leaves_the_focus_on_its_resume() {
    use super::offline::installing_6;
    use slatty_core::installer::{DOWNLOADING, InstallJob};
    let mut app = couch_app();
    let db = app.core.as_ref().unwrap().db.clone();
    installing_6(&mut app);
    InstallJob {
        game_id: "7".into(),
        build_id: "b1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 7".into(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    app.queue = vec!["7".into(), "8".into()];
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Downloads)));
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Right);
    assert_eq!(app.download_rows()[1].buttons[1].0, "Remove");
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.set_aside, ["7"]);
    let rows = app.download_rows();
    assert_eq!(rows[1].buttons[0].0, "Resume");
    assert_eq!(couch(&app).download, (1, 0));
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

/// The quit question answered by a tap starts again on Keep running: a press of A then never quits
/// by a focus left on Quit.
#[test]
fn the_quit_question_starts_on_keep_running_each_time() {
    use super::offline::installing_6;
    let mut app = couch_app();
    installing_6(&mut app);
    let _ = app.update(Message::Couch(CouchMsg::Menu(MenuItem::Quit)));
    nav(&mut app, Nav::Right);
    assert_eq!(couch(&app).quit, 1);
    let _ = app.update(Message::CancelQuit);
    let _ = app.update(Message::Couch(CouchMsg::Menu(MenuItem::Quit)));
    assert_eq!(couch(&app).quit, 0);
}
