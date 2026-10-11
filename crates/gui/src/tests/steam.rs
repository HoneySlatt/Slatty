use slatty_core::library::{LibraryCache, MetadataSource};
use slatty_core::steam::client::Action;
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{Installed, SteamUser};

use super::*;
use crate::steam::SteamMsg;

fn steam_game(app: u32, title: &str) -> LibraryGame {
    LibraryGame {
        id: slatty_core::store::steam_id(app),
        title: title.into(),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: vec!["windows".into()],
        metadata: MetadataSource::Product,
    }
}

fn steam_library() -> SteamLibrary {
    SteamLibrary {
        user: SteamUser {
            id64: 76561198000000002,
            name: "[FAKE] Player".into(),
        },
        games: vec![
            steam_game(440, "[FAKE] Fortress"),
            steam_game(620, "[FAKE] Portal"),
        ],
        covers: std::collections::HashMap::new(),
    }
}

fn ids(app: &App) -> Vec<&str> {
    app.library.iter().map(|g| g.id.as_str()).collect()
}

/// Someone who uses Steam alone: GOG's sign-in offers Steam's games, which then show without a
/// GOG account, GOG's sign-in one click away in Settings.
#[test]
fn steam_games_show_without_gog_once_chosen() {
    let mut app = App {
        core: Some(core()),
        ..Default::default()
    };
    let db = app.core.as_ref().unwrap().db.clone();
    app.steam.found = Some("[FAKE] Player".into());
    assert!(render(&app).find("Use my Steam games without GOG").is_ok());

    let _ = app.update(Message::Steam(SteamMsg::Switch(true)));
    assert!(app.steam.on && slatty_core::settings::steam_games(&db).unwrap());
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    assert_eq!(ids(&app), ["steam-440", "steam-620"]);
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Fortress").is_ok());
        assert!(ui.find("Open the GOG sign-in page").is_err());
        snapshot(&mut ui, "steam-only-library");
    }

    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    {
        let mut ui = render_window(&app);
        assert!(ui.find("Sign in to GOG").is_ok() && ui.find("Steam games").is_ok());
    }
    let _ = app.update(Message::ShowLogin(true));
    assert!(render(&app).find("Back to the library").is_ok());
}

/// GOG's and Steam's games share one library: a refresh of GOG's or signing out of GOG keeps
/// Steam's, and turning Steam off takes them out.
#[test]
fn gog_and_steam_games_share_the_library() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    assert_eq!(app.library.len(), 16);

    let _ = app.set_library(LibraryCache {
        user_id: "0".into(),
        fetched_at: 0,
        games: vec![fake_game("1", "Game 1")],
    });
    assert_eq!(ids(&app), ["1", "steam-440", "steam-620"]);
    app.logged_out();
    assert_eq!(ids(&app), ["steam-440", "steam-620"]);
    let _ = app.update(Message::Steam(SteamMsg::Switch(false)));
    assert!(app.library.is_empty());
}

/// A Steam game's page plays, installs, verifies and uninstalls through Steam, and says it runs
/// through Steam rather than offering GOG's cloud and achievements.
#[test]
fn a_steam_games_page_asks_steam() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![Installed {
        app_id: 440,
        name: "[FAKE] Fortress".into(),
        dir: "/games/steamapps/common/Fortress".into(),
        size: 25_000_000_000,
        ready: true,
    }])));
    assert!(app.is_installed("steam-440") && !app.is_installed("steam-620"));

    let asked = |app: &mut App, id: &str, label: &str| {
        let _ = app.update(Message::Select(id.into()));
        let mut ui = render(app);
        ui.click(label).unwrap();
        let messages: Vec<Message> = ui.into_messages().collect();
        messages.into_iter().find_map(|m| match m {
            Message::Steam(SteamMsg::Ask(game, action)) => Some((game, action)),
            _ => None,
        })
    };
    assert_eq!(
        asked(&mut app, "steam-440", "Play"),
        Some(("steam-440".into(), Action::Play))
    );
    assert_eq!(
        asked(&mut app, "steam-440", "Verify files in Steam"),
        Some(("steam-440".into(), Action::Verify))
    );
    assert_eq!(
        asked(&mut app, "steam-620", "Install"),
        Some(("steam-620".into(), Action::Install))
    );
    let _ = app.update(Message::Select("steam-440".into()));
    settle(&mut app);
    let mut ui = render(&app);
    assert!(ui.find("Runs through Steam").is_ok());
    assert!(ui.find("Cloud saves").is_err());
    snapshot(&mut ui, "steam-game-page");
}
