use super::*;
use crate::sidebar::SidebarMsg;

fn open_app() -> App {
    let mut app = library_app();
    app.sidebar.open = true;
    app
}

fn titles_listed(app: &App) -> Vec<String> {
    app.sidebar_games()
        .iter()
        .map(|&i| app.library[i].title.replace("[FAKE] ", ""))
        .collect()
}

/// The list holds every game but the hidden ones, A to Z, whatever the library shows; its search
/// is its own; a click on a game opens its page.
#[test]
fn the_list_holds_every_game_and_is_searched_apart() {
    let mut app = open_app();
    app.customs.insert(
        "5".into(),
        slatty_core::custom::Custom {
            hidden: true,
            ..Default::default()
        },
    );
    app.shelf = Shelf::Installed;
    app.lists.clear();
    let listed = titles_listed(&app);
    assert_eq!(listed.len(), 13, "every game but the hidden one");
    assert_eq!(listed[..3], ["Game 1", "Game 2", "Game 3"]);
    assert!(!listed.contains(&"Game 5".to_string()));

    let _ = app.update(Message::Sidebar(SidebarMsg::Search("game 1".into())));
    assert_eq!(
        titles_listed(&app),
        [
            "Game 1", "Game 10", "Game 11", "Game 12", "Game 13", "Game 14"
        ]
    );
    assert!(app.search.is_empty(), "the library's search is apart");

    let mut ui = render(&app);
    snapshot(&mut ui, "library-sidebar");
    assert!(ui.find("Game 1").is_err(), "names are shown whole");
    ui.click("[FAKE] Game 12").unwrap();
    let opened: Vec<Message> = ui.into_messages().collect();
    assert!(
        opened
            .iter()
            .any(|m| matches!(m, Message::Select(id) if id == "12"))
    );
    let _ = app.update(Message::Sidebar(SidebarMsg::Search("nothing".into())));
    assert!(render(&app).find("No game matches.").is_ok());
}

/// Closed, the list takes no room (the library's toolbar opens it again), and the choice is kept
/// for the next start.
#[test]
fn the_list_closes_and_the_choice_is_kept() {
    let mut app = open_app();
    let _ = app.update(Message::Sidebar(SidebarMsg::Toggle));
    assert!(!app.sidebar.open);
    let db = &app.core.as_ref().unwrap().db;
    assert!(!slatty_core::settings::sidebar(db).unwrap());
    assert_eq!(app.sidebar_room(), 0.0, "closed, it takes no room");
    assert!(render(&app).find("Search").is_err());
    let _ = app.update(Message::Sidebar(SidebarMsg::Toggle));
    assert!(slatty_core::settings::sidebar(&app.core.as_ref().unwrap().db).unwrap());
    // Only beside the library.
    app.page = Page::Settings;
    assert_eq!(app.sidebar_room(), 0.0);
}

/// Only the icons of the games in view are asked for: a library of a thousand games asks for a
/// few dozen, then for those scrolled to.
#[test]
fn only_the_icons_in_view_are_asked_for() {
    let mut app = open_app();
    app.library = (0..1000)
        .map(|i| {
            let mut g = fake_game(&format!("{}", 100 + i), &format!("Game {i:04}"));
            g.icon = Some(format!("https://images.gog.example/{i:04}.png"));
            g
        })
        .collect();
    app.lists.clear();
    let _ = app.update(Message::Sidebar(SidebarMsg::Search(String::new())));
    let asked = |app: &App| {
        app.images_requested
            .iter()
            .filter(|u| u.starts_with("https://images.gog.example/"))
            .count()
    };
    let first = asked(&app);
    assert!(first > 10 && first < 100, "{first}");
    assert!(
        app.images_requested
            .contains("https://images.gog.example/0000.png")
    );
    assert!(
        !app.images_requested
            .contains("https://images.gog.example/0999.png")
    );
}

/// What the window sends for a press of the mouse's button at `at`, on the app as it is.
fn pressed_at(app: &App, at: iced::Point) -> Vec<Message> {
    use iced::mouse::{Button, Event as Mouse};
    let mut ui = render(app);
    ui.point_at(at);
    let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Left))]);
    ui.into_messages().collect()
}

/// The cover grid's width told before the list was opened, closed or dragged is another one: it
/// is worked out from the window until the grid tells it again.
#[test]
fn the_grids_width_is_told_again_once_the_list_opens_closes_or_is_dragged() {
    use iced::mouse::{Event as Mouse, ScrollDelta};
    let mut app = open_app();
    app.library = (1..=200)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    let scrolled: Vec<Message> = {
        let mut ui = render(&app);
        ui.point_at(iced::Point::new(900.0, 500.0));
        let _ = ui.simulate([iced::Event::Mouse(Mouse::WheelScrolled {
            delta: ScrollDelta::Pixels { x: 0.0, y: -500.0 },
        })]);
        ui.into_messages()
            .filter(|m| matches!(m, Message::GridScrolled(_)))
            .collect()
    };
    for m in scrolled {
        let _ = app.update(m);
    }
    let told = app.grid_measured().0.expect("told with the list as it is");
    let _ = app.update(Message::Sidebar(SidebarMsg::Toggle));
    assert_eq!(app.grid_measured().0, None, "closed since");
    let _ = app.update(Message::Sidebar(SidebarMsg::Toggle));
    assert_eq!(app.grid_measured().0, Some(told));
    let _ = app.update(Message::Sidebar(SidebarMsg::Grab));
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(400.0)));
    assert_eq!(app.grid_measured().0, None, "dragged since");
}

/// Its edge grabbed, the list follows the pointer within its bounds, and keeps its width for the
/// next start; a move with nothing grabbed changes nothing.
#[test]
fn the_lists_edge_is_dragged_and_its_width_kept() {
    let mut app = open_app();
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(400.0)));
    assert_eq!(app.sidebar.width, crate::sidebar::WIDTH, "nothing grabbed");
    let grabbed = |messages: Vec<Message>| {
        messages
            .iter()
            .any(|m| matches!(m, Message::Sidebar(SidebarMsg::Grab)))
    };
    let edge = crate::sidebar::WIDTH - 2.0;
    assert!(grabbed(pressed_at(&app, iced::Point::new(edge, 500.0))));
    assert!(!grabbed(pressed_at(
        &app,
        iced::Point::new(edge - 40.0, 500.0)
    )));

    let _ = app.update(Message::Sidebar(SidebarMsg::Grab));
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(400.0)));
    assert_eq!(app.sidebar.width, 400.0);
    assert_eq!(app.sidebar_room(), 400.0, "the grid beside it is narrower");
    assert!(
        grabbed(pressed_at(&app, iced::Point::new(398.0, 500.0))),
        "the edge moved with it"
    );
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(20.0)));
    assert_eq!(
        app.sidebar.width,
        crate::sidebar::WIDTHS.0,
        "never too narrow to read"
    );
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(5000.0)));
    assert_eq!(app.sidebar.width, crate::sidebar::WIDTHS.1);
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(350.0)));
    let _ = app.update(Message::Sidebar(SidebarMsg::Drop));
    assert!(!app.sidebar.dragging);
    let db = &app.core.as_ref().unwrap().db;
    assert_eq!(
        slatty_core::settings::sidebar_width(db).unwrap(),
        Some(350.0)
    );
    let _ = app.update(Message::Sidebar(SidebarMsg::Drag(500.0)));
    assert_eq!(app.sidebar.width, 350.0, "let go");
}
