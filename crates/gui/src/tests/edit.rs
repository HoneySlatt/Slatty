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
