use slatty_core::library::{LibraryCache, MetadataSource};
use slatty_core::steam::client::Action;
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{Installed, Played, SteamUser};

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
        played: std::collections::HashMap::from([(
            "steam-440".to_string(),
            Played {
                minutes: 754,
                last_played: 1_791_590_000,
            },
        )]),
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

/// Images the Steam client has not saved come from Steam's servers only once turned on; a cover
/// downloaded stays through a refresh of GOG's library, and one arriving after Steam's games were
/// turned off is dropped.
#[test]
fn images_from_steams_servers_once_turned_on() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    app.steam.found = Some("[FAKE] Player".into());
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    assert!(render_window(&app).find("Images from Steam").is_err());
    let _ = app.update(Message::Steam(SteamMsg::Switch(true)));
    assert!(render_window(&app).find("Images from Steam").is_ok());
    assert!(!slatty_core::settings::steam_images(&db).unwrap());
    let _ = app.update(Message::Steam(SteamMsg::Images(true)));
    assert!(app.steam.images && slatty_core::settings::steam_images(&db).unwrap());

    let mut library = steam_library();
    library.games[1].cover = Some(
        "https://shared.steamstatic.com/store_item_assets/steam/apps/620/library_600x900.jpg"
            .into(),
    );
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(library))));
    let cover = std::env::temp_dir().join("[FAKE]-steam-620.jpg");
    let _ = app.update(Message::Steam(SteamMsg::Cover(
        "steam-620".into(),
        Some(cover.clone()),
    )));
    assert!(app.covers.contains_key("steam-620"));
    let _ = app.set_library(LibraryCache {
        user_id: "0".into(),
        fetched_at: 0,
        games: vec![fake_game("1", "Game 1")],
    });
    assert!(
        app.covers.contains_key("steam-620"),
        "kept through GOG's refresh"
    );

    // Asked for without a GOG account too.
    app.account = None;
    let url = "https://shared.steamstatic.com/store_item_assets/steam/apps/620/library_hero.jpg";
    let _ = app.request_images(vec![url.into(), "https://images.gog.com/[FAKE].jpg".into()]);
    assert!(app.images_requested.contains(url));
    assert!(
        !app.images_requested
            .contains("https://images.gog.com/[FAKE].jpg")
    );

    let _ = app.update(Message::Steam(SteamMsg::Switch(false)));
    let _ = app.update(Message::Steam(SteamMsg::Cover(
        "steam-620".into(),
        Some(cover),
    )));
    assert!(
        !app.covers.contains_key("steam-620"),
        "Steam's games are off"
    );
}

/// Steam's play time and last session show on a Steam game's page and sort the library with GOG's
/// games; another account signed in to Steam meanwhile is not mixed with the one shown.
#[test]
fn steam_play_time_shows_and_sorts_with_gogs() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let _ = app.update(Message::PlaytimesFetched(vec![("2".into(), 60)]));
    app.sort = crate::library::Sort::MostPlayed;
    app.lists.clear();
    assert_eq!(&titles(&app)[..3], ["Fortress", "Game 2", "Game 1"]);
    app.sort = crate::library::Sort::RecentlyPlayed;
    app.lists.clear();
    assert_eq!(titles(&app)[0], "Fortress");

    let _ = app.update(Message::Select("steam-440".into()));
    {
        let mut ui = render(&app);
        assert!(ui.find("12 h 34 m").is_ok(), "{:?}", texts(&mut ui));
    }

    let mut other = std::collections::HashMap::new();
    other.insert("steam-440".to_string(), Played::default());
    let _ = app.update(Message::Steam(SteamMsg::Played(Some((
        76561198000000009,
        other,
    )))));
    assert_eq!(
        app.steam.played["steam-440"].minutes, 754,
        "read again, all of it"
    );
    let mut later = steam_library().played;
    later.get_mut("steam-440").unwrap().minutes = 800;
    let _ = app.update(Message::Steam(SteamMsg::Played(Some((
        76561198000000002,
        later,
    )))));
    assert_eq!(app.played_seconds("steam-440"), 800 * 60);
}

/// A game asked of Steam is looked for until it runs, then shows as running until it ends; only
/// meanwhile are Steam's games looked at.
#[test]
fn a_steam_game_shows_as_running_while_steam_runs_it() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![Installed {
        app_id: 440,
        name: "[FAKE] Fortress".into(),
        dir: "/games/steamapps/common/Fortress".into(),
        size: 1,
        ready: true,
    }])));
    assert!(!app.watching_steam());
    let _ = app.update(Message::Steam(SteamMsg::Ask(
        "steam-440".into(),
        Action::Play,
    )));
    assert!(app.watching_steam(), "looked for once asked");

    let _ = app.update(Message::Steam(SteamMsg::Running([440].into())));
    assert!(app.steam.starting.is_none() && app.watching_steam());
    let _ = app.update(Message::Select("steam-440".into()));
    {
        let mut ui = render(&app);
        assert!(ui.find("Running").is_ok());
        assert!(ui.click("Running").is_ok());
        assert!(
            !ui.into_messages()
                .any(|m| matches!(m, Message::Steam(SteamMsg::Ask(..)))),
            "nothing to ask of Steam while it runs the game"
        );
    }
    let _ = app.update(Message::Steam(SteamMsg::Running(Default::default())));
    assert!(!app.watching_steam(), "it ended");
    assert!(render(&app).find("Play").is_ok());
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
