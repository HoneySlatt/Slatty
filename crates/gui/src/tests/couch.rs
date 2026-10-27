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

/// The library's rows fill the height: where more than half of a row more would fit under covers
/// as wide as Home's, they are made smaller so that it fits whole, and more go across, rather than
/// leave an empty band under the grid (two rows, then 200 pixels, at 1920×1080).
#[test]
fn the_library_grid_fills_the_screens_height() {
    use crate::couch::{GAP, HINTS, MARGIN, SHELVES, TOP_BAR};
    let mut app = couch_app();
    for (width, height) in [(1920.0, 1080.0), (1280.0, 800.0), (1280.0, 720.0)] {
        app.window = Size::new(width, height);
        let (across, down, card) = app.couch_grid();
        let room = height - TOP_BAR - HINTS - SHELVES;
        let used = down as f32 * (card * 4.0 / 3.0 + GAP) - GAP;
        assert!(used <= room, "{width}×{height}: rows fit whole");
        assert!(
            room - used < (card * 4.0 / 3.0 + GAP) / 2.0,
            "{width}×{height}: {down} rows leave {} pixels",
            room - used
        );
        assert!(across as f32 * (card + GAP) - GAP <= width - 2.0 * MARGIN);
        assert!(card <= app.couch_card(), "never larger than Home's");
    }
    app.window = Size::new(1920.0, 1080.0);
    assert_eq!(app.couch_grid().1, 3);
    app.library
        .extend((15..=30).map(|i| fake_game(&i.to_string(), &format!("Game {i}"))));
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    assert_eq!(app.couch_in_view().len(), 24, "three rows of eight");
    let mut ui = render_window(&app);
    snapshot(&mut ui, "couch-library-1080");
}

#[test]
fn the_library_grid_moves_by_rows_and_keeps_the_focus_in_view() {
    let mut app = couch_app();
    nav(&mut app, Nav::NextTab);
    assert_eq!(couch(&app).tab, Tab::Library);
    let (across, down, _) = app.couch_grid();
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
    let (_, rows, card) = app.couch_grid();
    let row = card * 4.0 / 3.0 + GAP;
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

/// In a gamescope session (a Steam Deck's Game Mode) the window is drawn at the screen's pixels:
/// gamescope's Xwayland gives the Deck's small screen a scale of about 2, at which the couch mode
/// showed three covers where the Desktop Mode shows six. A scale the user chose stays.
#[test]
fn gamescope_draws_at_the_screens_pixels() {
    use std::ffi::OsStr;
    assert_eq!(crate::couch::x11_scale(true, None), Some("1"));
    assert_eq!(crate::couch::x11_scale(true, Some(OsStr::new("2"))), None);
    assert_eq!(crate::couch::x11_scale(false, None), None);
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

/// The menu and the questions dim the page under them, but not the hints, which name their
/// buttons: drawn after them (Iced draws a stack's layers in order), once, where they are with
/// nothing open.
#[test]
fn the_hints_are_drawn_over_the_menu_and_the_questions() {
    use super::offline::installing_6;
    use iced_test::selector::Candidate;
    let last_four = |app: &App| {
        let all = texts(&mut render(app));
        all[all.len() - 4..].to_vec()
    };
    let hint_middle = |app: &App| {
        render(app)
            .find(|c: Candidate<'_>| match c {
                Candidate::Text {
                    content: "A",
                    bounds,
                    ..
                } => Some(bounds.center_y()),
                _ => None,
            })
            .unwrap()
    };
    // The menu.
    let mut app = couch_app();
    let resting = hint_middle(&app);
    nav(&mut app, Nav::Menu);
    assert_eq!(last_four(&app), ["A", "Select", "B", "Close"]);
    assert!((hint_middle(&app) - resting).abs() < 0.5, "the bar stays");
    let all = texts(&mut render(&app));
    assert_eq!(all.iter().filter(|t| *t == "Select").count(), 1, "once");
    let mut ui = render(&app);
    ui.click("Close").unwrap();
    assert!(ui.into_messages().next().is_none(), "a hint is no button");
    // The launch question.
    let mut app = couch_app();
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "[FAKE] Settings".into()],
    );
    let _ = app.update(Message::Select("3".into()));
    nav(&mut app, Nav::Confirm);
    assert!(app.launch_prompt.is_some());
    assert_eq!(last_four(&app), ["A", "Start", "B", "Cancel"]);
    // The quit question.
    let mut app = couch_app();
    installing_6(&mut app);
    let _ = app.update(Message::Couch(CouchMsg::Menu(MenuItem::Quit)));
    assert!(app.quit_confirm.is_some());
    let mut ui = render(&app);
    snapshot(&mut ui, "couch-quit");
    let all = texts(&mut ui);
    assert_eq!(all[all.len() - 4..], ["A", "Choose", "B", "Keep running"]);
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

/// A game whose achievements are all unlocked has them lit with the accent: on its line under the
/// title, and Completed beside the list's title.
#[test]
fn a_completed_game_says_so_in_the_couch_mode() {
    let mut app = couch_app();
    app.achievements.insert(
        "3".into(),
        crate::Loadable::Ready(vec![
            fake_achievement("Alpha", true),
            fake_achievement("Beta", true),
        ]),
    );
    app.overview.insert(
        "3".into(),
        GameOverview {
            achievements: Some((2, 2)),
            ..Default::default()
        },
    );
    let _ = app.update(Message::Select("3".into()));
    let mut ui = render(&app);
    assert!(ui.find("2 / 2 achievements").is_ok());
    assert!(ui.find("Completed").is_err(), "the list is closed");
    snapshot(&mut ui, "couch-completed-game");
    drop(ui);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Right);
    nav(&mut app, Nav::Confirm);
    assert_eq!(couch(&app).achievements, Some(0));
    let mut ui = render(&app);
    assert!(ui.find("Achievements  ·  2 / 2").is_ok());
    assert!(ui.find("Completed").is_ok());
    snapshot(&mut ui, "couch-completed-achievements");
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

/// Home's focus: its row's title, its place, its game.
fn home_focus_of(app: &App) -> (&'static str, usize, String) {
    let (row, col, g) = app.home_focus().unwrap();
    (app.home_rows()[row].title, col, g.id.clone())
}

/// Game 3's install record, for another fake game.
fn installed(app: &App, id: &str) -> Install {
    Install {
        game_id: id.into(),
        title: format!("[FAKE] Game {id}"),
        ..app.installs["3"].clone()
    }
}

/// An install ending adds a game before the one with the focus in Installed: the focus stays on
/// its game, and A opens that one.
#[test]
fn home_keeps_its_focus_on_its_game_when_an_install_ends() {
    let mut app = couch_app();
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Down);
    assert_eq!(home_focus_of(&app), ("Installed", 0, "3".into()));
    let one = installed(&app, "1");
    let _ = app.update(Message::Install(InstallMsg::Done("1".into(), Ok(one))));
    assert_eq!(home_focus_of(&app), ("Installed", 1, "3".into()));
    nav(&mut app, Nav::Confirm);
    assert_eq!(
        app.selected.as_deref(),
        Some("3"),
        "A opens the game chosen"
    );
}

/// A game played goes first in Continue playing: the focus goes with it.
#[test]
fn home_keeps_its_focus_on_its_game_after_it_was_played() {
    let mut app = couch_app();
    app.playtime.insert(
        "9".into(),
        Playtime {
            seconds: 60,
            last_played: 1_791_300_000,
        },
    );
    app.lists.clear();
    nav(&mut app, Nav::Right);
    assert_eq!(home_focus_of(&app), ("Continue playing", 1, "9".into()));
    nav(&mut app, Nav::Confirm);
    // Its session ends: play times read again, as `PlayMsg::Done` does.
    app.playtime.insert(
        "9".into(),
        Playtime {
            seconds: 120,
            last_played: 1_791_500_000,
        },
    );
    nav(&mut app, Nav::Back);
    assert_eq!(app.selected, None);
    assert_eq!(home_focus_of(&app), ("Continue playing", 0, "9".into()));
}

/// Favorites appears above the row with the focus, or goes when its last game is taken out: the
/// focus stays on its game, in its row, or follows it into another.
#[test]
fn home_keeps_its_focus_on_its_game_when_favorites_come_and_go() {
    let mut app = couch_app();
    app.favorites.clear();
    let one = installed(&app, "1");
    app.installs.insert("1".into(), one);
    app.lists.clear();
    nav(&mut app, Nav::Down);
    nav(&mut app, Nav::Right);
    assert_eq!(home_focus_of(&app), ("Installed", 1, "3".into()));
    nav(&mut app, Nav::Action);
    assert!(app.favorites.contains("3"));
    assert_eq!(home_focus_of(&app), ("Installed", 1, "3".into()));
    nav(&mut app, Nav::Left);
    nav(&mut app, Nav::Up);
    assert_eq!(home_focus_of(&app), ("Favorites", 0, "3".into()));
    nav(&mut app, Nav::Action);
    assert!(!app.favorites.contains("3"));
    assert_eq!(home_focus_of(&app), ("Installed", 1, "3".into()));
}

/// The couch library's shelf changes under the focus (an install ending on Installed): the focus
/// stays on its game; when its game leaves the shelf, it takes the one now at its place.
#[test]
fn the_couch_library_keeps_its_focus_on_its_game_when_its_shelf_changes() {
    let shown = |app: &App| -> Vec<String> {
        let games = app.couch_games();
        games.iter().map(|&i| app.library[i].id.clone()).collect()
    };
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let _ = app.update(Message::Couch(CouchMsg::Shelf(Shelf::Installed)));
    // The focus chosen (a press, here at the end of the shelf).
    nav(&mut app, Nav::Right);
    let one = installed(&app, "1");
    let _ = app.update(Message::Install(InstallMsg::Done("1".into(), Ok(one))));
    assert_eq!(shown(&app), ["1", "3"]);
    assert_eq!(couch(&app).card, 1, "still on game 3");
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.selected.as_deref(), Some("3"));
    let _ = app.update(Message::CloseDetail);
    let _ = app.update(Message::Couch(CouchMsg::Shelf(Shelf::Favorites)));
    nav(&mut app, Nav::Action);
    assert_eq!(shown(&app), ["7"]);
    nav(&mut app, Nav::Confirm);
    assert_eq!(app.selected.as_deref(), Some("7"));
}

/// A favorite taken out and put back on a game's page: back from it, the focus is on that game,
/// on Home and in Library, not on the one that took its place a moment.
#[test]
fn back_from_a_page_the_focus_is_on_its_game() {
    let mut app = couch_app();
    nav(&mut app, Nav::Down);
    assert_eq!(home_focus_of(&app), ("Favorites", 0, "5".into()));
    nav(&mut app, Nav::Confirm);
    nav(&mut app, Nav::Action);
    nav(&mut app, Nav::Action);
    assert!(app.favorites.contains("5"));
    nav(&mut app, Nav::Back);
    assert_eq!(home_focus_of(&app), ("Favorites", 0, "5".into()));
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let _ = app.update(Message::Couch(CouchMsg::Shelf(Shelf::Favorites)));
    nav(&mut app, Nav::Confirm);
    nav(&mut app, Nav::Action);
    nav(&mut app, Nav::Action);
    nav(&mut app, Nav::Back);
    assert_eq!(couch(&app).card, 0);
}

/// Before the player chooses anything on Home, its focus stays on the first cover of the first row
/// as the rows fill in: Continue playing coming above is not left out of view.
#[test]
fn home_starts_on_its_first_row_whatever_fills_the_rows() {
    let mut app = library_app();
    app.window = SIZE;
    app.favorites.extend(["5".to_string(), "7".to_string()]);
    let _ = app.update(Message::Couch(CouchMsg::Enter));
    assert_eq!(home_focus_of(&app), ("Favorites", 0, "5".into()));
    app.playtime.insert(
        "9".into(),
        Playtime {
            seconds: 60,
            last_played: 1_791_500_000,
        },
    );
    let _ = app.update(Message::CloseDetail);
    assert_eq!(home_focus_of(&app), ("Continue playing", 0, "9".into()));
}

/// Before the player moves the library's focus, it stays on the first cover as the shelf fills in
/// (Steam's games, a library sync): the new first games are not left above the view.
#[test]
fn the_library_starts_on_its_first_cover_whatever_fills_the_shelf() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    for i in 0..20 {
        app.library.push(fake_game(
            &(100 + i).to_string(),
            &format!("[FAKE] A game {i}"),
        ));
    }
    let _ = app.update(Message::CloseDetail);
    assert_eq!(couch(&app).card, 0, "still on the first cover");
    assert_eq!(app.library_top(couch(&app), Some(0)), 0);
}

/// Back on the library, the covers in view are asked for around the focus once found again: the
/// one with the focus is enlarged with the others on a large screen.
#[test]
fn the_library_asks_for_the_covers_around_its_focus_found_again() {
    let mut app = couch_app();
    // Three rows of eight at 3840×2160.
    app.library
        .extend((16..=30).map(|i| fake_game(&i.to_string(), &format!("Game {i}"))));
    app.lists.clear();
    app.card_width = 225.0;
    let _ = app.update(Message::Rescaled(1.0));
    let _ = app.update(Message::WindowResized(Size::new(3840.0, 2160.0)));
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let (across, ..) = app.couch_grid();
    nav(&mut app, Nav::Down);
    for _ in 1..across {
        nav(&mut app, Nav::Right);
    }
    nav(&mut app, Nav::PrevTab);
    app.library.push(fake_game("15", "[FAKE] Game 0"));
    for g in &app.library {
        let path = format!("/covers/{}.jpg", g.id);
        app.covers
            .insert(g.id.clone(), iced::widget::image::Handle::from_path(path));
    }
    // Forgets the lists, as an install ending would.
    let _ = app.update(Message::CloseDetail);
    nav(&mut app, Nav::NextTab);
    assert_eq!(couch(&app).card, 2 * across);
    assert_eq!(
        app.scale_couch_covers().units(),
        0,
        "asked for with the tab"
    );
}

/// The focus found again on its game in another row of the library brings that row into view, as a
/// move would: the next press does not scroll the grid for nothing.
#[test]
fn the_library_focus_found_again_keeps_its_row_in_view() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let (across, ..) = app.couch_grid();
    for _ in 1..across {
        nav(&mut app, Nav::Right);
    }
    nav(&mut app, Nav::Down);
    app.library.push(fake_game("15", "[FAKE] Game 0"));
    let _ = app.update(Message::CloseDetail);
    let card = couch(&app).card;
    assert_eq!(card, 2 * across, "on its game, a row down");
    let shown = app.library_top(couch(&app), Some(card));
    nav(&mut app, Nav::Up);
    let now = app.library_top(couch(&app), Some(couch(&app).card));
    assert_eq!(now, shown, "the rows shown stay");
}

/// Another shelf, or a search, starts from its first cover, wherever the game with the focus is in
/// it.
#[test]
fn a_new_shelf_or_search_starts_from_the_first_cover() {
    let mut app = couch_app();
    let _ = app.update(Message::Couch(CouchMsg::Tab(Tab::Library)));
    let (across, ..) = app.couch_grid();
    nav(&mut app, Nav::Down);
    assert_eq!(couch(&app).card, across);
    let _ = app.update(Message::Couch(CouchMsg::Shelf(Shelf::Favorites)));
    assert_eq!(couch(&app).card, 0);
    let _ = app.update(Message::Couch(CouchMsg::Shelf(Shelf::All)));
    nav(&mut app, Nav::Down);
    key(&mut app, Key::Char('g'));
    assert_eq!(couch(&app).search, "g");
    assert_eq!(couch(&app).card, 0);
}
