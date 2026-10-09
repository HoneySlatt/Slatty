use std::path::PathBuf;
use std::sync::Arc;

use iced::Size;
use iced_test::simulator::Simulator;
use slatty_core::account::AccountInfo;
use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::library::{LibraryGame, MetadataSource};
use slatty_core::overview::GameOverview;
use slatty_core::paths::Dirs;
use slatty_core::runner::Runner;
use slatty_core::session::Playtime;

use crate::install::{InstallMsg, InstallView};
use crate::maintenance::MaintenanceMsg;
use crate::{App, Core, Filters, Message, Page, Panel, Shelf, Sort};

const SIZE: Size = Size::new(1440.0, 900.0);

fn core() -> Core {
    let root = std::env::temp_dir().join(format!("slatty-gui-test-{}", std::process::id()));
    Core {
        dirs: Dirs::under(&root),
        db: Arc::new(Db::in_memory().unwrap()),
        http: slatty_core::http::client().unwrap(),
        downloads: Arc::new(tokio::sync::Semaphore::new(1)),
    }
}

/// Fictitious data for interface tests only; titles say so.
fn fake_game(id: &str, title: &str) -> LibraryGame {
    LibraryGame {
        id: id.into(),
        title: format!("[FAKE] {title}"),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: vec!["windows".into()],
        metadata: MetadataSource::Missing,
    }
}

fn library_app() -> App {
    let mut app = App {
        core: Some(core()),
        ..Default::default()
    };
    app.account = Some(AccountInfo {
        user_id: "0".into(),
        username: "testeur".into(),
    });
    app.library = (1..=14)
        .map(|i| fake_game(&i.to_string(), &format!("Game {i}")))
        .collect();
    app.library[1].os.push("linux".into());
    app.fetched_at = Some(1_791_500_000);
    app.installs.insert(
        "3".into(),
        Install {
            game_id: "3".into(),
            title: "[FAKE] Game 3".into(),
            platform: Platform::Windows,
            path: PathBuf::from("/games/Game 3"),
            client_id: Some("1".into()),
            runner: Runner::Umu {
                proton: "/proton/GE-Proton".into(),
                prefix: "/prefixes/jeu3".into(),
            },
        },
    );
    app
}

fn open(app: &mut App, game: &str, panel: Option<Panel>) {
    app.selected = Some(game.into());
    app.panel = panel;
}

fn render(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(Default::default(), SIZE, app.view())
}

fn snapshot(ui: &mut Simulator<'_, Message>, name: &str) {
    if let Ok(dir) = std::env::var("SLATTY_SNAPSHOT_DIR") {
        let snap = ui.snapshot(&crate::theme::theme()).unwrap();
        let path = PathBuf::from(dir).join(name);
        let _ = std::fs::remove_file(path.with_file_name(format!("{name}-wgpu.png")));
        snap.matches_image(path).unwrap();
    }
}

fn titles(app: &App) -> Vec<String> {
    app.visible_games()
        .iter()
        .map(|g| g.title.replace("[FAKE] ", ""))
        .collect()
}

#[test]
fn login_screen_offers_browser_login_without_password_field() {
    let app = App {
        core: Some(core()),
        ..Default::default()
    };
    let mut ui = render(&app);
    assert!(ui.find("Sign in").is_ok());
    assert!(ui.find("Password").is_err());
    snapshot(&mut ui, "login");
    ui.click("Open the GOG sign-in page").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenLoginPage))
    );
}

#[test]
fn top_bar_offers_library_achievements_and_settings() {
    let app = library_app();
    let mut ui = render(&app);
    for tab in ["Library", "Achievements"] {
        assert!(ui.find(tab).is_ok(), "{tab}");
    }
    for later in ["Activity", "Store", "Friends"] {
        assert!(ui.find(later).is_err(), "{later}");
    }
    snapshot(&mut ui, "library");
    ui.click(iced_test::selector::id("settings-tab")).unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::ShowPage(Page::Settings)))
    );
}

#[test]
fn search_filters_the_library() {
    let mut app = library_app();
    app.search = "game 1".into();
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Game 12").is_ok());
    assert!(ui.find("[FAKE] Game 3").is_err());
}

#[test]
fn shelves_filters_and_sort_select_games() {
    let mut app = library_app();
    app.shelf = Shelf::Installed;
    assert_eq!(titles(&app), ["Game 3"]);

    app.shelf = Shelf::All;
    let _ = app.update(Message::ToggleFavorite("7".into()));
    app.shelf = Shelf::Favorites;
    assert_eq!(titles(&app), ["Game 7"]);
    assert_eq!(
        slatty_core::settings::favorites(&app.core.as_ref().unwrap().db).unwrap(),
        ["7"]
    );

    app.shelf = Shelf::All;
    app.filters = Filters {
        linux: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 2"]);

    app.overview.insert(
        "4".into(),
        GameOverview {
            achievements: Some((1, 10)),
            cloud_saves: false,
            ..Default::default()
        },
    );
    app.overview.insert(
        "5".into(),
        GameOverview {
            achievements: None,
            cloud_saves: true,
            ..Default::default()
        },
    );
    app.filters = Filters {
        achievements: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 4"]);
    app.filters = Filters {
        cloud_saves: true,
        ..Default::default()
    };
    assert_eq!(titles(&app), ["Game 5"]);

    app.filters = Filters::default();
    app.playtime.insert(
        "9".into(),
        Playtime {
            seconds: 60,
            last_played: 200,
        },
    );
    app.playtime.insert(
        "2".into(),
        Playtime {
            seconds: 6000,
            last_played: 100,
        },
    );
    app.sort = Sort::RecentlyPlayed;
    assert_eq!(titles(&app)[..2], ["Game 9", "Game 2"]);
    for (id, minutes) in [("2", 100), ("9", 1)] {
        app.overview.entry(id.into()).or_default().playtime_minutes = Some(minutes);
    }
    app.sort = Sort::MostPlayed;
    assert_eq!(titles(&app)[..2], ["Game 2", "Game 9"]);
    app.sort = Sort::NameDesc;
    assert_eq!(titles(&app)[0], "Game 9");
}

#[test]
fn installed_game_page_can_be_played_and_shows_no_store_text() {
    let mut app = library_app();
    open(&mut app, "3", None);
    app.overview.insert(
        "3".into(),
        GameOverview {
            playtime_minutes: Some(3 * 60 + 5),
            ..Default::default()
        },
    );
    app.playtime.insert(
        "3".into(),
        Playtime {
            seconds: 99 * 3600,
            last_played: 1_791_500_000,
        },
    );
    let mut ui = render(&app);
    assert!(ui.find("3 h 5 m").is_ok(), "GOG's total, not the local one");
    assert!(ui.find("Time played").is_ok());
    assert!(ui.find("Cloud saves").is_ok());
    assert!(ui.find("Manage").is_ok());
    snapshot(&mut ui, "game");
    ui.click("Play").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Play(id) if id == "3"))
    );

    app.maintenance.insert(
        "3".into(),
        crate::maintenance::MaintenanceView {
            busy: true,
            ..Default::default()
        },
    );
    let mut ui = render(&app);
    let _ = ui.click("Play");
    assert!(
        !ui.into_messages().any(|m| matches!(m, Message::Play(_))),
        "no launch while the game's files are being changed"
    );
}

#[test]
fn uninstalled_game_offers_install_but_not_play_or_cloud_tools() {
    let mut app = library_app();
    open(&mut app, "5", None);
    let mut ui = render(&app);
    assert!(ui.find("Play").is_err());
    assert!(ui.find("Manage").is_err());
    ui.click("Install").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenPanel(Panel::Install)))
    );
}

fn fake_achievement(key: &str, unlocked: bool) -> slatty_core::achievements::Achievement {
    slatty_core::achievements::Achievement {
        achievement_id: format!("id-{key}"),
        achievement_key: key.into(),
        name: format!("[FAKE] {key}"),
        description: String::new(),
        visible: true,
        date_unlocked: unlocked.then(|| "2026-10-09T10:00:00+0000".into()),
        rarity: 0.0,
        image_url_unlocked: String::new(),
        image_url_locked: String::new(),
    }
}

fn app_with_achievements() -> App {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Achievements));
    app.achievements.insert(
        "5".into(),
        crate::Loadable::Ready(vec![
            fake_achievement("Alpha", false),
            fake_achievement("Beta", true),
        ]),
    );
    app
}

#[test]
fn game_page_summarises_achievements() {
    let mut app = app_with_achievements();
    app.panel = None;
    let mut ui = render(&app);
    assert!(ui.find("1 / 2").is_ok());
    assert!(ui.find("50%").is_ok());
    assert!(ui.find("[FAKE] Beta").is_err(), "named on hover only");
    let icon = ui
        .find(iced_test::selector::id("latest-unlock-id-Beta"))
        .expect("latest unlock shown");
    let position = icon.bounds().center();
    ui.point_at(position);
    let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
        position,
    })]);
    snapshot(&mut ui, "game-achievements");
}

#[test]
fn unlocking_only_asks_for_confirmation() {
    let mut app = app_with_achievements();
    let mut ui = render(&app);
    assert!(ui.find("Clear").is_ok());
    ui.click("Unlock").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(matches!(
        messages.as_slice(),
        [Message::AskAchievementChange(game, changes)] if game == "5" && changes.len() == 1 && changes[0].unlock
    ));
    for m in messages {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_some());

    let mut ui = render(&app);
    snapshot(&mut ui, "achievements-confirm");
    ui.click("Confirm").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::ConfirmAchievementChange))
    );
}

#[test]
fn cancelling_drops_the_pending_change() {
    let mut app = app_with_achievements();
    let _ = app.update(Message::AskAchievementChange(
        "5".into(),
        vec![crate::AchievementChange {
            achievement_id: "id-Alpha".into(),
            name: "[FAKE] Alpha".into(),
            unlock: true,
        }],
    ));
    let mut ui = render(&app);
    ui.click("Cancel").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_none());
}

#[test]
fn achievements_tab_lists_games_by_completion() {
    let mut app = library_app();
    app.page = Page::Achievements;
    for (id, done, total) in [
        ("2", 1, 4),
        ("6", 4, 4),
        ("8", 0, 10),
        ("9", 6, 80),
        ("10", 1, 50),
        ("11", 2, 8),
    ] {
        app.overview.insert(
            id.into(),
            GameOverview {
                achievements: Some((done, total)),
                cloud_saves: false,
                ..Default::default()
            },
        );
    }
    let order: Vec<&str> = app
        .games_by_achievements()
        .iter()
        .map(|(g, _, _)| g.title.as_str())
        .collect();
    assert_eq!(
        order,
        [
            "[FAKE] Game 6",
            "[FAKE] Game 11",
            "[FAKE] Game 2",
            "[FAKE] Game 9",
            "[FAKE] Game 10",
            "[FAKE] Game 8"
        ],
        "highest share first, even with fewer unlocked; equal shares by title"
    );
    let mut ui = render(&app);
    assert!(ui.find("4 / 4").is_ok());
    assert!(ui.find("25%").is_ok());
    assert!(
        ui.find("[FAKE] Game 1").is_err(),
        "no achievements, not listed"
    );
    snapshot(&mut ui, "achievements-tab");
    ui.click("[FAKE] Game 6").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenAchievements(id) if id == "6"))
    );
}

#[test]
fn games_known_only_by_play_time_are_read_again() {
    let mut app = library_app();
    app.library.truncate(2);
    app.page = Page::Achievements;
    app.overview.insert(
        "1".into(),
        GameOverview {
            achievements: Some((1, 2)),
            checked: true,
            ..Default::default()
        },
    );
    let _ = app.update(Message::PlaytimesFetched(vec![("2".into(), 30)]));
    assert!(
        !app.overview_complete(),
        "play time says nothing of achievements"
    );

    let _ = app.update(Message::ScanOverview);
    assert!(app.overview_busy);
    assert!(
        render(&app)
            .find("1 / 2 unlocked · Reading GOG data… 1/2 games · 0 completed")
            .is_ok()
    );
    let _ = app.update(Message::OverviewFetched(
        "2".into(),
        Err("[FAKE] offline".into()),
    ));
    let _ = app.update(Message::OverviewDone);
    assert!(
        render(&app)
            .find("1 / 2 unlocked · 1 game could not be read from GOG · 0 completed")
            .is_ok(),
        "no endless reading message after a failure"
    );
}

fn fake_plan() -> crate::install::PlanInfo {
    crate::install::PlanInfo {
        title: "[FAKE] Game 5".into(),
        version: "1.0".into(),
        language: "en-US".into(),
        languages: vec!["en-US".into(), "fr-FR".into()],
        download_size: 3 << 30,
        disk_size: 5 << 30,
        root: "/games".into(),
        directory: "Game 5".into(),
        dependencies: vec!["MSVC2017".into()],
        resumable: false,
        dlcs: vec![
            fake_dlc("21", "[FAKE] Owned DLC", true, true),
            fake_dlc("22", "[FAKE] Other DLC", false, false),
        ],
    }
}

fn fake_dlc(
    id: &str,
    name: &str,
    owned: bool,
    selected: bool,
) -> slatty_core::installer::DlcChoice {
    slatty_core::installer::DlcChoice {
        id: id.into(),
        name: name.into(),
        owned,
        selected,
        download_size: 1 << 30,
        disk_size: 2 << 30,
    }
}

#[test]
fn install_needs_a_proton_choice_then_starts() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(ui.find("Choose a Proton version in Settings.").is_ok());
    let _ = ui.click("Start install");
    assert!(
        !ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(_))))
    );

    app.proton = Some("/proton/GE-Proton".into());
    let mut ui = render(&app);
    snapshot(&mut ui, "install-ready");
    ui.click("Start install").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(id)) if id == "5"))
    );
}

#[test]
fn install_folder_can_be_typed_or_browsed_but_must_be_absolute() {
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    open(&mut app, "5", Some(Panel::Install));
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(ui.find("Game folder: /games/Game 5").is_ok());
    ui.click("Browse").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Browse(id)) if id == "5"))
    );

    let _ = app.update_install(InstallMsg::Browsed("5".into(), Some("/mnt/ssd".into())));
    assert!(render(&app).find("Game folder: /mnt/ssd/Game 5").is_ok());
    let _ = app.update_install(InstallMsg::Browsed("5".into(), None));
    assert!(render(&app).find("Game folder: /mnt/ssd/Game 5").is_ok());

    let _ = app.update_install(InstallMsg::RootInput("5".into(), "games".into()));
    let _ = app.update_install(InstallMsg::Start("5".into()));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Ready(_))
    ));
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
}

#[test]
fn a_resumed_install_keeps_its_folder() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    );
    let _ = app.update_install(InstallMsg::RootInput("5".into(), "/elsewhere".into()));
    let mut ui = render(&app);
    assert!(ui.find("Folder: /games/Game 5").is_ok());
    assert!(ui.find("Browse").is_err());
}

#[test]
fn running_install_shows_progress_and_can_pause() {
    let mut app = library_app();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 3,
                files_total: 10,
                bytes_done: 1 << 30,
                bytes_total: 4 << 30,
            },
            cancel: tokio_util::sync::CancellationToken::new(),
        },
    );
    assert!(render(&app).find("Downloading [FAKE] Game 5").is_ok());
    open(&mut app, "5", None);
    assert!(render(&app).find("Downloading 25 %").is_ok());
    app.panel = Some(Panel::Install);
    let mut ui = render(&app);
    snapshot(&mut ui, "install-running");
    ui.click("Pause").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Pause(id)) if id == "5"))
    );
}

#[test]
fn uninstall_requires_an_explicit_choice() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let mut ui = render(&app);
    ui.click("Uninstall…").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Maintenance(MaintenanceMsg::AskUninstall(id))] if id == "3")
    );
    for m in messages {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    snapshot(&mut ui, "uninstall-confirm");
    ui.click("Uninstall, keep the prefix").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, false)) if id == "3")
    ));
    let mut ui = render(&app);
    ui.click("Also delete the prefix (backed up)").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, true)) if id == "3")
    ));
}

#[test]
fn update_button_appears_only_when_an_update_exists() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    assert!(render(&app).find("Update now").is_err());
    let _ = app.update(Message::Maintenance(MaintenanceMsg::UpdateChecked(
        "3".into(),
        Ok(Some("1.0 → 1.1".into())),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Update available: 1.0 → 1.1").is_ok());
    snapshot(&mut ui, "update-available");
    ui.click("Update now").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Maintenance(MaintenanceMsg::Apply(id, slatty_core::maintenance::Change::Update)) if id == "3"))
    );
}

#[test]
fn an_update_shows_its_progress_and_can_be_paused() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Apply(
        "3".into(),
        slatty_core::maintenance::Change::Update,
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Progress(
        "3".into(),
        slatty_core::installer::Progress {
            bytes_done: 1 << 30,
            bytes_total: 4 << 30,
            files_done: 3,
            files_total: 10,
        },
    )));
    let mut ui = render(&app);
    assert!(ui.find("1.00 GiB / 4.00 GiB · files 3/10").is_ok());
    snapshot(&mut ui, "update-progress");
    ui.click("Pause").unwrap();
    for m in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(app.maintenance["3"].cancel.as_ref().unwrap().is_cancelled());

    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Err(None),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Paused. Apply it again to resume.").is_ok());
    assert!(ui.find("Pause").is_err());
}

#[test]
fn owned_dlc_can_be_deselected_before_install_and_sizes_follow() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.proton = Some("/proton/GE-Proton".into());
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(ui.find("Download 4.00 GiB · on disk 7.00 GiB").is_ok());
    assert!(ui.find("[FAKE] Other DLC (2.00 GiB) — not owned").is_ok());
    snapshot(&mut ui, "install-dlc");
    ui.click("[FAKE] Owned DLC (2.00 GiB)").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Install(InstallMsg::ToggleDlc(g, d))] if g == "5" && d == "21")
    );
    for m in messages {
        let _ = app.update(m);
    }
    assert!(
        render(&app)
            .find("Download 3.00 GiB · on disk 5.00 GiB")
            .is_ok()
    );
}

#[test]
fn game_settings_panel_applies_dlc_changes_only_when_something_changed() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = ContentInfo {
        language: "en-US".into(),
        languages: vec!["en-US".into(), "fr-FR".into()],
        chosen_language: "en-US".into(),
        dlcs: vec![fake_dlc("21", "[FAKE] Owned DLC", true, true)],
        chosen_dlcs: vec!["21".into()],
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content),
    )));
    let mut ui = render(&app);
    let _ = ui.click("Apply DLC changes");
    assert!(
        ui.into_messages().next().is_none(),
        "no change, button disabled"
    );

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ToggleContentDlc(
        "3".into(),
        "21".into(),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "game-settings");
    ui.click("Apply DLC changes").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Dlcs(d))) if id == "3" && d.is_empty()
    )));
}

#[test]
fn escape_closes_the_panel_then_the_game_page() {
    use iced::keyboard::{Event, Key, Location, Modifiers, key};
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let escape = || {
        Message::Key(Event::KeyPressed {
            key: Key::Named(key::Named::Escape),
            modified_key: Key::Named(key::Named::Escape),
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: None,
            repeat: false,
        })
    };
    let _ = app.update(escape());
    assert_eq!((app.panel, app.selected.as_deref()), (None, Some("3")));
    let _ = app.update(escape());
    assert_eq!(app.selected, None);
}

#[test]
fn settings_page_holds_account_library_and_install_options() {
    let mut app = library_app();
    app.page = Page::Settings;
    let mut ui = render(&app);
    for label in [
        "testeur",
        "Log out",
        "Refresh library",
        "Games folder",
        "Proton",
    ] {
        assert!(ui.find(label).is_ok(), "{label}");
    }
    snapshot(&mut ui, "settings");
}

#[test]
fn achievements_tab_opens_a_dedicated_page_per_game() {
    let mut app = app_with_achievements();
    app.selected = None;
    app.panel = None;
    let _ = app.update(Message::OpenAchievements("5".into()));
    assert_eq!(
        (app.page, app.achievements_game.as_deref()),
        (Page::Achievements, Some("5"))
    );
    let mut ui = render(&app);
    assert!(ui.find("1 / 2 unlocked").is_ok());
    assert!(ui.find("Time played").is_err(), "not the game page");
    snapshot(&mut ui, "achievements-game");
    ui.click("Unlock").unwrap();
    assert!(
        ui.into_messages().any(
            |m| matches!(m, Message::AskAchievementChange(id, c) if id == "5" && c.len() == 1)
        )
    );
    let _ = app.update(Message::ShowPage(Page::Achievements));
    assert_eq!(app.achievements_game, None);
}

#[test]
fn play_time_comes_from_gog_even_for_games_played_elsewhere() {
    let mut app = library_app();
    open(&mut app, "5", None);
    app.overview.insert(
        "5".into(),
        GameOverview {
            playtime_minutes: Some(3175),
            ..Default::default()
        },
    );
    let mut ui = render(&app);
    assert!(ui.find("52 h 55 m").is_ok());
    assert!(
        ui.find("Never").is_err(),
        "GOG does not tell when it was last played"
    );
    drop(ui);

    app.playtime.insert(
        "5".into(),
        Playtime {
            seconds: 4000 * 60,
            last_played: 1_791_500_000,
        },
    );
    assert_eq!(
        app.played_seconds("5"),
        3175 * 60,
        "only GOG's total counts"
    );
}

#[test]
fn closing_asks_first_only_when_something_is_running() {
    let mut app = library_app();
    let _ = app.update(Message::CloseRequested);
    assert!(app.quit_confirm.is_none(), "nothing running: quits at once");

    let cancel = tokio_util::sync::CancellationToken::new();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 1,
                files_total: 4,
                bytes_done: 1 << 30,
                bytes_total: 2 << 30,
            },
            cancel: cancel.clone(),
        },
    );
    let _ = app.update(Message::CloseRequested);
    let mut ui = render(&app);
    assert!(
        ui.find(
            "Downloading [FAKE] Game 5 (50 %). Quitting pauses it; it resumes where it stopped."
        )
        .is_ok()
    );
    snapshot(&mut ui, "quit-confirm");
    ui.click("Keep running").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.quit_confirm.is_none());
    assert!(!cancel.is_cancelled());

    let _ = app.update(Message::CloseRequested);
    let _ = app.update(Message::ConfirmQuit);
    assert!(
        cancel.is_cancelled(),
        "the download is paused before quitting"
    );
}

#[test]
fn interrupted_work_is_listed_with_a_way_to_finish_it() {
    use crate::Interrupted;
    let mut app = library_app();
    app.interrupted = vec![
        ("5".into(), Interrupted::Download),
        ("3".into(), Interrupted::Update),
    ];
    let mut ui = render(&app);
    assert!(
        ui.find("Download interrupted. It resumes where it stopped.")
            .is_ok()
    );
    assert!(
        ui.find("Update interrupted. The game cannot start until it is finished.")
            .is_ok()
    );
    snapshot(&mut ui, "interrupted");
    ui.click("Resume").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::SelectWith(id, Panel::Install) if id == "5"))
    );
    let mut ui = render(&app);
    ui.click("Finish update").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, slatty_core::maintenance::Change::Update)) if id == "3"
    )));

    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Ok("[FAKE] done".into()),
    )));
    assert!(app.interrupted.is_empty());

    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Err(Some("[FAKE] network down before anything changed".into())),
    )));
    let _ = app.update(Message::Install(InstallMsg::Done(
        "8".into(),
        Err(Some("[FAKE] refused before downloading".into())),
    )));
    assert!(
        app.interrupted.is_empty(),
        "failures that left nothing unfinished are not listed"
    );

    slatty_core::installer::InstallJob {
        game_id: "7".into(),
        build_id: "b".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 7".into(),
        state: "paused".into(),
        dlcs: vec![],
    }
    .save(&app.core.as_ref().unwrap().db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done("7".into(), Err(None))));
    assert_eq!(app.interrupted, [("7".to_string(), Interrupted::Download)]);
}

#[test]
fn achievements_go_from_the_most_common_to_the_rarest() {
    let mut app = app_with_achievements();
    let Some(crate::Loadable::Ready(list)) = app.achievements.get_mut("5") else {
        unreachable!()
    };
    list[0].rarity = 4.5;
    list[1].rarity = 62.0;
    list.push(slatty_core::achievements::Achievement {
        rarity: 30.0,
        ..fake_achievement("Gamma", false)
    });
    let place = |ui: &mut iced_test::Simulator<'_, Message>, name: &str| {
        let b = ui.find(name).unwrap().bounds();
        (b.y as i32, b.x as i32)
    };
    let expected = ["[FAKE] Beta", "[FAKE] Gamma", "[FAKE] Alpha"];

    {
        let mut ui = render(&app);
        let drawer: Vec<_> = expected.iter().map(|n| place(&mut ui, n)).collect();
        assert!(drawer.is_sorted(), "drawer: {drawer:?}");
    }

    app.panel = None;
    let _ = app.update(Message::OpenAchievements("5".into()));
    let mut ui = render(&app);
    let page: Vec<_> = expected.iter().map(|n| place(&mut ui, n)).collect();
    assert!(page.is_sorted(), "page, row by row: {page:?}");
}
