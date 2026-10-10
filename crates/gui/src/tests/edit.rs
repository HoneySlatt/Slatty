use super::*;

#[test]
fn a_game_can_be_renamed_and_sorted_under_another_name() {
    use crate::edit::EditMsg;
    let mut app = library_app();
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));
    edit(&mut app, EditMsg::Open("7".into()));
    {
        let mut ui = render(&app);
        assert!(ui.find("Edit game").is_ok());
        snapshot(&mut ui, "edit-game");
    }
    edit(&mut app, EditMsg::Title("[FAKE] Renamed".into()));
    edit(&mut app, EditMsg::SortTitle("[FAKE] 0 first".into()));
    edit(&mut app, EditMsg::Save);
    assert!(app.edit.is_none());
    assert_eq!(titles(&app)[0], "Renamed", "sorted by its sorting title");
    assert!(render(&app).find("[FAKE] Renamed").is_ok());

    let reloaded = slatty_core::custom::all(&app.core.as_ref().unwrap().db).unwrap();
    assert_eq!(reloaded["7"].title.as_deref(), Some("[FAKE] Renamed"));

    edit(&mut app, EditMsg::Open("7".into()));
    edit(&mut app, EditMsg::Reset);
    edit(&mut app, EditMsg::Save);
    assert!(app.customs.is_empty());
    assert!(titles(&app).contains(&"Game 7".to_string()));
    assert_ne!(titles(&app)[0], "Game 7");
}

#[test]
fn a_cover_can_be_replaced_by_an_image_file_only() {
    use crate::edit::{Art, EditMsg};
    let mut app = library_app();
    let dir = std::env::temp_dir().join(format!("slatty-gui-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("cover.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n[FAKE]").unwrap();
    let text = dir.join("notes.txt");
    std::fs::write(&text, b"[FAKE]").unwrap();
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));

    edit(&mut app, EditMsg::Open("4".into()));
    edit(&mut app, EditMsg::Picked(Art::Cover, Some(text)));
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    edit(&mut app, EditMsg::Picked(Art::Cover, Some(png.clone())));
    edit(&mut app, EditMsg::Save);
    let saved = app.customs["4"].cover.clone().unwrap();
    assert!(saved.starts_with(&app.core.as_ref().unwrap().dirs.data));
    assert!(app.cover("4").is_some(), "shown in the library");

    edit(&mut app, EditMsg::Open("4".into()));
    edit(&mut app, EditMsg::Title("[FAKE] Not kept".into()));
    let _ = app.update(Message::Key(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Unidentified(
            iced::keyboard::key::NativeCode::Unidentified,
        ),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    }));
    assert!(app.edit.is_none(), "Escape cancels");
    assert!(app.customs["4"].title.is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn right_clicking_a_cover_opens_a_menu_with_edit_game() {
    use crate::edit::EditMsg;
    use iced::mouse::{Button, Event as Mouse};
    let mut app = library_app();
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        let at = ui.find("[FAKE] Game 7").unwrap().bounds().center();
        ui.point_at(at);
        let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Right))]);
        ui.into_messages().collect()
    };
    assert!(
        matches!(&messages[..], [Message::Edit(EditMsg::Menu(id)), Message::Edit(EditMsg::At(..))] if id == "7"),
        "{messages:?}"
    );
    for m in messages {
        let _ = app.update(m);
    }
    let menu = app.context_menu.clone().expect("menu open");
    assert_eq!(menu.game_id, "7");
    let mut ui = render(&app);
    snapshot(&mut ui, "cover-menu");
    ui.click("Edit game").unwrap();
    for m in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(app.context_menu.is_none());
    assert_eq!(app.edit.as_ref().map(|d| d.game_id.as_str()), Some("7"));

    // A right click elsewhere closes the menu; near the right edge it opens inward.
    let _ = app.update(Message::Edit(EditMsg::Menu("7".into())));
    let _ = app.update(Message::Edit(EditMsg::At(iced::Point::new(1435.0, 10.0))));
    let menu = app.context_menu.clone().unwrap();
    assert!(menu.at.x + crate::edit::MENU_WIDTH <= app.window.width);
    let _ = app.update(Message::Edit(EditMsg::At(iced::Point::new(50.0, 50.0))));
    assert!(app.context_menu.is_none());
}

#[test]
fn right_clicking_the_key_art_of_a_game_page_opens_the_edit_drawer() {
    use crate::edit::EditMsg;
    use iced::mouse::{Button, Event as Mouse};
    let mut app = library_app();
    open(&mut app, "3", None);
    let right_click = |app: &App, at: iced::Point| -> Vec<Message> {
        let mut ui = render(app);
        ui.point_at(at);
        let _ = ui.simulate([iced::Event::Mouse(Mouse::ButtonPressed(Button::Right))]);
        ui.into_messages().collect()
    };
    // The title and the buttons over the art keep the menu closed.
    for label in ["[FAKE] Game 3", "Play"] {
        let at = render(&app).find(label).unwrap().bounds().center();
        let messages = right_click(&app, at);
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Edit(EditMsg::Menu(_)))),
            "{label}: {messages:?}"
        );
    }
    app.panel = Some(Panel::GameSettings);
    let messages = right_click(&app, iced::Point::new(500.0, 300.0));
    assert!(
        matches!(&messages[..], [Message::Edit(EditMsg::Menu(id)), Message::Edit(EditMsg::At(..))] if id == "3"),
        "{messages:?}"
    );
    for m in messages {
        let _ = app.update(m);
    }
    {
        let mut ui = render(&app);
        ui.click("Edit game").unwrap();
        for m in ui.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    assert_eq!(app.edit.as_ref().map(|d| d.game_id.as_str()), Some("3"));
    assert_eq!(app.selected.as_deref(), Some("3"), "the game page stays");
    assert_eq!(
        app.panel, None,
        "the edit drawer takes the open panel's place"
    );
    {
        let mut ui = render(&app);
        snapshot(&mut ui, "edit-drawer");
        assert!(ui.find("Sorting title").is_ok());
        assert!(ui.find("Cancel").is_ok());
    }

    // Its close button drops the draft; so does opening a panel.
    let _ = app.update(Message::ClosePanel);
    assert!(app.edit.is_none());
    let _ = app.update(Message::Edit(EditMsg::Open("3".into())));
    let _ = app.update(Message::OpenPanel(Panel::GameSettings));
    assert!(app.edit.is_none());
    assert_eq!(app.panel, Some(Panel::GameSettings));
}

#[test]
fn a_hidden_game_shows_only_under_hidden_games() {
    use crate::edit::EditMsg;
    let mut app = library_app();
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));
    assert_eq!(
        app.shelves(),
        Shelf::ALL,
        "no Hidden games while none is hidden"
    );

    // Game 3 is installed: it leaves the Installed shelf too.
    edit(&mut app, EditMsg::Open("3".into()));
    assert!(
        render(&app)
            .find("Shown only under Hidden games in the library.")
            .is_ok()
    );
    edit(&mut app, EditMsg::Hidden(true));
    edit(&mut app, EditMsg::Save);
    assert!(slatty_core::custom::all(&app.core.as_ref().unwrap().db).unwrap()["3"].hidden);
    assert!(!titles(&app).contains(&"Game 3".to_string()));
    assert_eq!(titles(&app).len(), 13);
    app.shelf = Shelf::Installed;
    assert!(titles(&app).is_empty());
    app.shelf = Shelf::All;
    let _ = app.update(Message::Search("Game 3".into()));
    assert!(titles(&app).is_empty(), "search leaves it out too");
    let _ = app.update(Message::Search(String::new()));

    assert_eq!(app.shelves().last(), Some(&Shelf::Hidden));
    let _ = app.update(Message::ShowShelf(Shelf::Hidden));
    assert_eq!(titles(&app), ["Game 3"]);
    snapshot(&mut render(&app), "hidden-games");

    // Shown again: the shelf, now empty, leaves the menu.
    edit(&mut app, EditMsg::Open("3".into()));
    edit(&mut app, EditMsg::Hidden(false));
    edit(&mut app, EditMsg::Save);
    assert_eq!(app.shelf, Shelf::All);
    assert_eq!(app.shelves(), Shelf::ALL);
    assert_eq!(titles(&app).len(), 14);
}

#[test]
fn steamgriddb_art_is_searched_by_name_once_turned_on() {
    use crate::edit::{Art, EditMsg};
    use slatty_core::steamgriddb::{self, Game};
    let mut app = library_app();
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));
    edit(&mut app, EditMsg::Open("4".into()));
    assert!(
        render(&app).find("From SteamGridDB").is_err(),
        "off until turned on"
    );

    // The search starts from the title the library shows, which can be changed.
    app.steamgriddb = true;
    edit(&mut app, EditMsg::Browse(Art::Cover));
    let picker = app.edit.as_ref().unwrap().picker.as_ref().unwrap();
    assert_eq!(picker.query, "[FAKE] Game 4");
    assert_eq!(picker.searched, "[FAKE] Game 4", "searched at once");
    assert!(render(&app).find("Searching SteamGridDB…").is_ok());
    edit(&mut app, EditMsg::PickerQuery("Hollow Knight".into()));
    edit(&mut app, EditMsg::Search);
    let game = |id, name: &str| Game {
        id,
        name: name.into(),
        release_date: Some(1_488_326_400),
    };
    // The answer to the first search arrives late: it is dropped.
    edit(
        &mut app,
        EditMsg::Found("[FAKE] Game 4".into(), Ok(vec![game(9, "[FAKE] Other")])),
    );
    assert!(render(&app).find("[FAKE] Other (2017)").is_err());
    edit(
        &mut app,
        EditMsg::Found(
            "Hollow Knight".into(),
            Ok(vec![
                game(5254, "[FAKE] Hollow Knight"),
                game(77, "[FAKE] Silksong"),
            ]),
        ),
    );
    let picker = app.edit.as_ref().unwrap().picker.as_ref().unwrap();
    assert_eq!(picker.game, Some(5254), "the closest game is shown first");
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Hollow Knight (2017)").is_ok());
        assert!(ui.find("Loading its pictures…").is_ok());
    }
    let offer = |n: u32| steamgriddb::Art {
        url: format!("https://cdn2.steamgriddb.com/grid/{n}.png"),
        thumb: format!("https://cdn2.steamgriddb.com/thumb/{n}.jpg"),
        width: 600,
        height: 900,
    };
    // Another game is chosen; the pictures of the first one, arriving late, are dropped.
    edit(&mut app, EditMsg::ChooseGame(77));
    edit(&mut app, EditMsg::Offered(5254, Ok(vec![offer(1)])));
    assert!(matches!(
        app.edit.as_ref().unwrap().picker.as_ref().unwrap().choices,
        Some(crate::Loadable::Loading)
    ));
    edit(&mut app, EditMsg::Offered(77, Ok(vec![offer(1), offer(2)])));
    {
        let mut ui = render(&app);
        assert!(ui.find("Cover from SteamGridDB").is_ok());
        assert!(ui.find("Save").is_ok(), "the buttons stay in view");
        snapshot(&mut ui, "edit-steamgriddb");
    }
    edit(&mut app, EditMsg::Choose(1));
    assert!(render(&app).find("Downloading…").is_ok());

    // Something else than an image is refused; an image goes into the draft, then is saved.
    edit(
        &mut app,
        EditMsg::Fetched(Art::Cover, Ok(b"<html>[FAKE]</html>".to_vec())),
    );
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    assert!(app.edit.as_ref().unwrap().picker.is_none());
    edit(&mut app, EditMsg::Browse(Art::Cover));
    edit(
        &mut app,
        EditMsg::Found("[FAKE] Game 4".into(), Ok(vec![game(5254, "[FAKE] HK")])),
    );
    edit(&mut app, EditMsg::Offered(5254, Ok(vec![offer(1)])));
    edit(&mut app, EditMsg::Choose(0));
    let png = b"\x89PNG\r\n\x1a\n[FAKE]".to_vec();
    edit(&mut app, EditMsg::Fetched(Art::Cover, Ok(png.clone())));
    let draft = app.edit.as_ref().unwrap();
    assert!(draft.picker.is_none());
    assert!(draft.previews.contains_key(&Art::Cover), "shown at once");
    edit(&mut app, EditMsg::Save);
    let saved = app.customs["4"].cover.clone().unwrap();
    assert!(saved.starts_with(&app.core.as_ref().unwrap().dirs.data));
    assert_eq!(std::fs::read(&saved).unwrap(), png);
    let _ = std::fs::remove_file(saved);
}

#[test]
fn a_steamgriddb_failure_is_said_in_the_picker() {
    use crate::edit::{Art, EditMsg};
    let mut app = library_app();
    app.steamgriddb = true;
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));
    edit(&mut app, EditMsg::Open("4".into()));
    edit(&mut app, EditMsg::Browse(Art::Background));
    edit(
        &mut app,
        EditMsg::Found(
            String::new(),
            Err("Add your SteamGridDB API key in Settings → Advanced.".into()),
        ),
    );
    assert!(
        render(&app)
            .find("Add your SteamGridDB API key in Settings → Advanced.")
            .is_ok()
    );
    edit(&mut app, EditMsg::Search);
    edit(
        &mut app,
        EditMsg::Found("[FAKE] Game 4".into(), Ok(Vec::new())),
    );
    assert!(
        render(&app)
            .find("No game of that name on SteamGridDB.")
            .is_ok()
    );
    edit(&mut app, EditMsg::ChooseGame(5254));
    edit(&mut app, EditMsg::Offered(5254, Ok(Vec::new())));
    assert!(
        render(&app)
            .find("SteamGridDB has no background for this game.")
            .is_ok()
    );
    edit(&mut app, EditMsg::ClosePicker);
    assert!(app.edit.as_ref().unwrap().picker.is_none());
}
