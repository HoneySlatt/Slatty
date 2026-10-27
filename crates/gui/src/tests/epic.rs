use slatty_core::epic::library::{EpicGame, EpicLibrary};

use super::*;
use crate::epic::EpicMsg;

const ACCOUNT: &str = "0123456789abcdef0123456789abcdef";

fn epic_game(app: &str, title: &str) -> EpicGame {
    EpicGame {
        app_name: app.into(),
        namespace: format!("[FAKE] {app}"),
        catalog_item_id: format!("[FAKE] item {app}"),
        title: title.into(),
        cover: None,
        background: None,
        logo: None,
        os: vec!["windows".into()],
    }
}

fn epic_library(account: &str) -> EpicLibrary {
    EpicLibrary {
        account_id: account.into(),
        display_name: "[FAKE] Player".into(),
        fetched_at: 1_791_500_000,
        games: vec![
            epic_game("Alpha", "[FAKE] Alpha"),
            epic_game("Beta", "[FAKE] Beta"),
        ],
        left_out: Vec::new(),
    }
}

/// The app, signed in to Epic, with its library read.
fn epic_app() -> App {
    let mut app = library_app();
    app.epic.account = Some(ACCOUNT.into());
    let _ = app.update(Message::Epic(EpicMsg::Read(
        ACCOUNT.into(),
        Ok(epic_library(ACCOUNT)),
    )));
    app
}

fn epic_ids(app: &App) -> Vec<&str> {
    app.library
        .iter()
        .map(|g| g.id.as_str())
        .filter(|id| id.starts_with("epic-"))
        .collect()
}

/// The games read show beside GOG's, the Epic filter keeps them alone, and they leave the library
/// on signing out.
#[test]
fn epic_games_show_while_signed_in() {
    let mut app = epic_app();
    assert_eq!(epic_ids(&app), ["epic-Alpha", "epic-Beta"]);
    assert_eq!(app.library.len(), 14 + 2, "GOG's stay");
    let only = Filters {
        epic: true,
        ..Default::default()
    };
    let shown: Vec<&str> = app
        .find_games("", Shelf::All, only, Sort::NameAsc)
        .into_iter()
        .map(|i| app.library[i].id.as_str())
        .collect();
    assert_eq!(shown, ["epic-Alpha", "epic-Beta"]);

    let _ = app.update(Message::Epic(EpicMsg::SignOut));
    assert!(epic_ids(&app).is_empty());
    assert_eq!(app.epic.account, None);
}

/// A read that lands after signing out, or for another account, shows nothing.
#[test]
fn a_library_read_for_another_account_is_dropped() {
    let mut app = library_app();
    let other = "fedcba9876543210fedcba9876543210";
    app.epic.account = Some(ACCOUNT.into());
    let _ = app.update(Message::Epic(EpicMsg::Read(
        other.into(),
        Ok(epic_library(other)),
    )));
    assert!(epic_ids(&app).is_empty());
    app.epic.account = None;
    let _ = app.update(Message::Epic(EpicMsg::Read(
        ACCOUNT.into(),
        Ok(epic_library(ACCOUNT)),
    )));
    assert!(epic_ids(&app).is_empty());
}

/// An Epic game's page offers no install, and asks GOG nothing about it: neither whether it can
/// be installed nor its achievements.
#[test]
fn an_epic_game_is_not_installable_yet() {
    let mut app = epic_app();
    let _ = app.update(Message::Select("epic-Alpha".into()));
    assert!(!app.achievements.contains_key("epic-Alpha"));
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Alpha").is_ok());
    assert!(ui.click("Not installable yet").is_ok());
    assert!(
        ui.into_messages().next().is_none(),
        "nothing to press while it cannot be installed"
    );
}

/// Signed out, Settings → Epic Games offers to sign in; signed in, it names the account, its
/// games, and signs out.
#[test]
fn settings_sign_in_to_epic_and_out() {
    let mut app = library_app();
    app.page = Page::Settings;
    // Below the window's bottom: found, not clicked.
    assert!(render(&app).find("Open the Epic sign-in page").is_ok());
    let _ = app.update(Message::Epic(EpicMsg::Input("not a code".into())));
    let _ = app.update(Message::Epic(EpicMsg::SignIn));
    assert!(
        !app.epic.signing_in,
        "a pasted text without a code is refused"
    );

    let mut app = epic_app();
    app.page = Page::Settings;
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Player").is_ok());
    assert!(ui.find("2 games · Last read 09/10/2026 00:53").is_ok());
    assert!(ui.find("Sign out").is_ok());
}
