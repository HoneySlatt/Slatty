use slatty_core::library::{LibraryCache, MetadataSource};
use slatty_core::steam::client::Action;
use slatty_core::steam::library::SteamLibrary;
use slatty_core::steam::local::{Installed, Played, SteamUser, Update};
use slatty_core::steam::web::OwnedGame;

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
        steam: Vec::new(),
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
        unreadable: None,
        played: std::collections::HashMap::from([(
            "steam-440".to_string(),
            Played {
                minutes: 754,
                last_played: 1_791_590_000,
            },
        )]),
    }
}

fn installed(app: u32, size: u64, ready: bool, update: Option<Update>) -> Installed {
    Installed {
        app_id: app,
        name: format!("[FAKE] {app}"),
        dir: format!("/games/steamapps/common/{app}").into(),
        size,
        ready,
        update,
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
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![installed(
        440, 1, true, None,
    )])));
    assert!(!app.watching_steam());
    let ask = Message::Steam(SteamMsg::Ask("steam-440".into(), Action::Play));
    let _ = app.update(ask.clone());
    assert!(app.watching_steam(), "looked for once asked");
    let _ = app.update(Message::Steam(SteamMsg::Asked(Err(
        "[FAKE] no steam:// handler".into(),
    ))));
    assert!(!app.watching_steam(), "Steam was not reached");

    let _ = app.update(ask);
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

/// Steam's achievements, read from the client's files, show in the Achievements tab sorted with
/// GOG's and on the game's page, without any GOG account; they are never unlocked or cleared by
/// hand, and a share of players Steam does not know is not made up.
#[test]
fn steam_achievements_show_with_gogs_and_are_never_changed_by_hand() {
    let mut app = library_app();
    app.manual_achievements = true;
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    app.overview.insert(
        "1".into(),
        slatty_core::overview::GameOverview {
            achievements: Some((1, 2)),
            checked: true,
            ..Default::default()
        },
    );
    let counts = std::collections::HashMap::from([("steam-440".to_string(), (1, 3))]);
    let _ = app.update(Message::Steam(SteamMsg::Counted(Some((
        76561198000000009,
        counts.clone(),
    )))));
    assert!(app.steam.achievements.is_empty(), "another account's");
    let _ = app.update(Message::Steam(SteamMsg::Counted(Some((
        76561198000000002,
        counts,
    )))));
    let order: Vec<&str> = app
        .games_by_achievements()
        .iter()
        .map(|(g, ..)| g.id.as_str())
        .collect();
    assert_eq!(
        order,
        ["1", "steam-440"],
        "by share unlocked, GOG's and Steam's together"
    );

    app.account = None;
    let _ = app.update(Message::LoadAchievements("steam-440".into()));
    assert!(matches!(
        app.achievements.get("steam-440"),
        Some(crate::Loadable::Loading)
    ));
    let mut known = fake_achievement("Known", true);
    known.rarity = 12.5;
    let mut unknown = fake_achievement("Unknown", false);
    unknown.rarity = -1.0;
    let _ = app.update(Message::Achievements(
        "steam-440".into(),
        Ok(vec![known, unknown, fake_achievement("Other", false)]),
    ));
    assert_eq!(app.steam.achievements["steam-440"], (1, 3));
    open(&mut app, "steam-440", Some(Panel::Achievements));
    {
        let mut ui = render(&app);
        assert!(ui.find("12.5% of players").is_ok());
        assert!(ui.find("-1.0% of players").is_err(), "not known, not shown");
        assert!(ui.find("Unlock").is_err() && ui.find("Clear").is_err());
        assert!(ui.find("Unlock all").is_err());
    }
    let _ = app.update(Message::AskAchievementChange(
        "steam-440".into(),
        vec![crate::achievements::AchievementChange {
            achievement_id: "id-Unknown".into(),
            name: "[FAKE] Unknown".into(),
            unlock: true,
        }],
    ));
    assert!(app.pending_change.is_none(), "never asked of Steam");

    // Another account signed in to Steam meanwhile: the first one's achievements go.
    let mut other = steam_library();
    other.user.id64 = 76561198000000009;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(other))));
    assert!(!app.achievements.contains_key("steam-440"));
    assert!(app.steam.achievements.is_empty());
}

/// An install Steam makes shows on the game's page and in the Downloads tab, its manifest read
/// only while it goes on, and its end is told once.
#[test]
fn a_download_steam_makes_shows_until_it_is_done() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let downloading = Update {
        done: 1_000,
        total: 4_000,
        started: true,
        paused: false,
    };
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![installed(
        620,
        0,
        false,
        Some(downloading),
    )])));
    assert!(app.watching_steam_downloads());
    let _ = app.update(Message::Select("steam-620".into()));
    assert!(render(&app).find("Downloading 25 %").is_ok());
    app.selected = None;
    app.page = Page::Downloads;
    {
        let mut ui = render(&app);
        let all = texts(&mut ui);
        assert!(
            all.iter().any(|t| t == "In Steam · 1") && all.iter().any(|t| t == "[FAKE] Portal"),
            "{all:?}"
        );
        assert!(all.iter().any(|t| t.starts_with("1 active")), "{all:?}");
        assert!(
            all.iter()
                .any(|t| t.starts_with("Nothing is downloading from GOG")),
            "{all:?}"
        );
    }

    let paused = Update {
        paused: true,
        ..downloading
    };
    let _ = app.update(Message::Steam(SteamMsg::Downloads(
        [(
            "steam-620".into(),
            Some(installed(620, 0, false, Some(paused))),
        )]
        .into(),
    )));
    assert!(!app.watching_steam_downloads(), "Steam's to resume");
    assert!(toasted(&app).is_empty());
    let _ = app.update(Message::Steam(SteamMsg::Downloads(
        [("steam-620".into(), Some(installed(620, 4_000, true, None)))].into(),
    )));
    assert_eq!(toasted(&app).len(), 1, "{:?}", toasted(&app));
    assert_eq!(toasted(&app)[0].0, "Download complete");
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![installed(
        620, 4_000, true, None,
    )])));
    assert_eq!(toasted(&app).len(), 1, "told once");
    assert!(!app.watching_steam_downloads());
}

/// A game owned on GOG and on Steam shows once, as GOG's: matched by the Steam app GOG's database
/// names, else by title; its page offers the Steam build through Steam, it counts as installed
/// when either is, and its play time adds both stores' sessions.
#[test]
fn a_game_owned_on_both_shows_once_as_gogs() {
    let mut app = library_app();
    app.library[0].steam = vec![440];
    app.library[1].title = "[FAKE] Portal".into();
    app.gog_titles.insert("2".into(), "[FAKE] Portal".into());
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    assert_eq!(app.library.len(), 14, "{:?}", ids(&app));
    assert_eq!(app.steam_twin("1"), Some("steam-440"));
    assert_eq!(app.steam_twin("2"), Some("steam-620"));

    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![installed(
        440, 1, true, None,
    )])));
    assert!(app.is_installed("1"), "installed through Steam");
    let _ = app.update(Message::PlaytimesFetched(vec![("1".into(), 60)]));
    assert_eq!(app.played_seconds("1"), (60 + 754) * 60);
    assert_eq!(app.last_played("1"), Some(1_791_590_000));

    let asked = |app: &mut App, id: &str, label: &str| {
        let _ = app.update(Message::Select(id.into()));
        let mut ui = render(app);
        assert!(ui.find("Also in your Steam library").is_ok());
        ui.click(label).unwrap();
        let messages: Vec<Message> = ui.into_messages().collect();
        messages.into_iter().find_map(|m| match m {
            Message::Steam(SteamMsg::Ask(game, action)) => Some((game, action)),
            _ => None,
        })
    };
    assert_eq!(
        asked(&mut app, "1", "Play the Steam version"),
        Some(("steam-440".into(), Action::Play))
    );
    assert_eq!(
        asked(&mut app, "2", "Install the Steam version"),
        Some(("steam-620".into(), Action::Install))
    );

    // Signed out of GOG, the Steam games show on their own again.
    let mut signed_out = library_app();
    signed_out.library[0].steam = vec![440];
    signed_out.steam.on = true;
    let _ = signed_out.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    signed_out.logged_out();
    assert_eq!(ids(&signed_out), ["steam-440", "steam-620"]);
    assert!(signed_out.steam_twin("1").is_none());

    // Turned off, Steam's games go and GOG's stay as they are.
    let _ = app.update(Message::Steam(SteamMsg::Switch(false)));
    assert!(app.steam_twin("1").is_none() && !app.is_installed("1"));
    assert_eq!(app.library.len(), 14);
}

/// What can go wrong with Steam's files is said, not hidden: a list of apps Steam changed the
/// format of leaves no game to name, and Settings says why. A library folder slow to answer (a
/// network drive waking up) is not asked again at each focus of the window meanwhile.
#[test]
fn steam_files_that_cannot_be_read_are_told_and_slow_folders_not_asked_twice() {
    let mut app = library_app();
    app.steam.on = true;
    app.steam.found = Some("[FAKE] Player".into());
    let mut library = steam_library();
    library.games.clear();
    library.unreadable = Some("[FAKE] unknown version 0x7564430".into());
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(library))));
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    {
        let mut ui = render_window(&app);
        let all = texts(&mut ui);
        assert!(
            all.iter()
                .any(|t| t.contains("[FAKE] unknown version 0x7564430")),
            "{all:?}"
        );
    }

    assert!(app.steam.reading_installed, "read after the library");
    let _ = app.update(Message::Notify(crate::notify::NotifyMsg::Focus(true)));
    assert!(app.steam.reading_installed);
    let _ = app.update(Message::Steam(SteamMsg::Installed(Vec::new())));
    assert!(!app.steam.reading_installed);
    let _ = app.update(Message::Notify(crate::notify::NotifyMsg::Focus(true)));
    assert!(app.steam.reading_installed, "read again at the next focus");
}

/// Steam's Web API is off until turned on, and its key, once typed, is never shown; the games it
/// names that the client has not described join the library, with the play time of those never
/// played here, and go with it.
#[test]
fn steams_web_api_fills_in_once_turned_on_and_its_games_go_with_it() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    app.steam.on = true;
    app.steam.found = Some("[FAKE] Player".into());
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2600.0);
    {
        let mut ui = render_window(&app);
        assert!(ui.find("Steam Web API").is_ok() && ui.find("Web API key").is_err());
    }
    let _ = app.update(Message::Steam(SteamMsg::WebApi(true)));
    assert!(slatty_core::settings::steam_web_api(&db).unwrap());
    let _ = app.update(Message::Steam(SteamMsg::WebKey(Ok(false))));
    let _ = app.update(Message::Steam(SteamMsg::WebKeyInput("[FAKE]KEY123".into())));
    {
        let mut ui = render_window(&app);
        let all = texts(&mut ui);
        assert!(all.iter().any(|t| t == "Web API key"), "{all:?}");
        assert!(
            !all.iter().any(|t| t.contains("[FAKE]KEY123")),
            "typed, never shown"
        );
    }
    let _ = app.update(Message::Steam(SteamMsg::WebKey(Ok(true))));
    assert!(
        render_window(&app)
            .find("Saved in the system keyring")
            .is_ok()
    );
    assert!(app.asks_steam_web());

    let owned = |app_id, name: &str, minutes| OwnedGame {
        app_id,
        name: name.into(),
        minutes,
        last_played: 0,
    };
    let games = vec![
        owned(70, "[FAKE] Life", 30),
        owned(440, "[FAKE] Fortress", 900),
    ];
    let _ = app.update(Message::Steam(SteamMsg::Owned(
        76561198000000009,
        Ok(games.clone()),
    )));
    assert!(!ids(&app).contains(&"steam-70"), "another account's");
    let _ = app.update(Message::Steam(SteamMsg::Owned(
        76561198000000002,
        Ok(games),
    )));
    assert!(ids(&app).contains(&"steam-70"));
    assert_eq!(app.played_seconds("steam-70"), 30 * 60, "never played here");
    assert_eq!(
        app.played_seconds("steam-440"),
        754 * 60,
        "the client's own record stays"
    );

    let _ = app.update(Message::Steam(SteamMsg::WebApi(false)));
    assert!(!ids(&app).contains(&"steam-70") && ids(&app).contains(&"steam-440"));

    // Steam's games turned off and on keep the switches as they are saved: what Settings shows
    // is what the next start does.
    let _ = app.update(Message::Steam(SteamMsg::WebApi(true)));
    let _ = app.update(Message::Steam(SteamMsg::Switch(false)));
    let _ = app.update(Message::Steam(SteamMsg::Switch(true)));
    assert_eq!(
        (app.steam.web, app.steam.web_key),
        (true, Some(true)),
        "on, as saved"
    );
    assert!(slatty_core::settings::steam_web_api(&db).unwrap());
}

/// A Steam game renamed in Edit game gets Steam's own title back with Reset to default.
#[test]
fn a_renamed_steam_game_gets_steams_title_back() {
    use crate::edit::EditMsg;
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let edit = |app: &mut App, m: EditMsg| drop(app.update(Message::Edit(m)));
    edit(&mut app, EditMsg::Open("steam-440".into()));
    edit(&mut app, EditMsg::Title("[FAKE] Renamed".into()));
    edit(&mut app, EditMsg::Save);
    assert!(titles(&app).contains(&"Renamed".to_string()));

    edit(&mut app, EditMsg::Open("steam-440".into()));
    edit(&mut app, EditMsg::Reset);
    assert_eq!(
        app.edit.as_ref().unwrap().title,
        "[FAKE] Fortress",
        "Steam's own"
    );
    edit(&mut app, EditMsg::Save);
    assert!(app.customs.is_empty());
    assert!(
        titles(&app).contains(&"Fortress".to_string()),
        "{:?}",
        titles(&app)
    );
}

/// Settings → Library counts the games GOG's refresh is about, and Settings → Steam Steam's.
#[test]
fn each_store_counts_its_own_games_in_settings() {
    let mut app = library_app();
    app.steam.on = true;
    app.steam.found = Some("[FAKE] Player".into());
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    let mut ui = render_window(&app);
    let all = texts(&mut ui);
    assert!(
        all.iter()
            .any(|t| t == "14 owned · Last refreshed 09/10/2026 00:53"),
        "{all:?}"
    );
    assert!(
        all.iter()
            .any(|t| t.contains("[FAKE] Player") && t.contains("2 games")),
        "{all:?}"
    );
}

/// GOG is asked about GOG's games alone: Steam's never count as games GOG could not describe.
#[test]
fn steam_games_are_not_gogs_to_read() {
    let mut app = library_app();
    for g in &app.library {
        app.overview.insert(
            g.id.clone(),
            slatty_core::overview::GameOverview {
                checked: true,
                ..Default::default()
            },
        );
    }
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    assert!(app.overview_complete(), "nothing left to read from GOG");
    assert_eq!(app.overview_status(), None);
}

/// A Steam game's page plays, installs, verifies and uninstalls through Steam, and says it runs
/// through Steam rather than offering GOG's cloud and achievements.
#[test]
fn a_steam_games_page_asks_steam() {
    let mut app = library_app();
    app.steam.on = true;
    let _ = app.update(Message::Steam(SteamMsg::Loaded(Some(steam_library()))));
    let _ = app.update(Message::Steam(SteamMsg::Installed(vec![installed(
        440,
        25_000_000_000,
        true,
        None,
    )])));
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
