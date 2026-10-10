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
    app.gog_titles = app
        .library
        .iter()
        .map(|g| (g.id.clone(), g.title.clone()))
        .collect();
    app.installs.insert(
        "3".into(),
        Install {
            umu_id: None,
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

/// The simulator with the fonts the application loads.
fn settings() -> iced::Settings {
    iced::Settings {
        fonts: crate::theme::FONTS.iter().map(|f| (*f).into()).collect(),
        default_font: crate::theme::font(),
        ..Default::default()
    }
}

fn render(app: &App) -> Simulator<'_, Message> {
    Simulator::with_size(settings(), SIZE, app.view())
}

fn snapshot(ui: &mut Simulator<'_, Message>, name: &str) {
    if let Ok(dir) = std::env::var("SLATTY_SNAPSHOT_DIR") {
        let snap = ui.snapshot(&crate::theme::theme()).unwrap();
        let path = PathBuf::from(dir).join(name);
        let _ = std::fs::remove_file(path.with_file_name(format!("{name}-wgpu.png")));
        snap.matches_image(path).unwrap();
    }
}

/// Ends the page transition, for snapshots of the page as it stays.
fn settle(app: &mut App) {
    let _ = app.update(Message::Frame(app.now + std::time::Duration::from_secs(1)));
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
        platform: Platform::Windows,
        platforms: vec![Platform::Windows],
        version: "1.0".into(),
        build_id: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: vec!["en-US".into(), "fr-FR".into()],
        download_size: 3 << 30,
        disk_size: 5 << 30,
        root: "/games".into(),
        directory: "Game 5".into(),
        proton: Some("/proton/GE-Proton".into()),
        free: Some(100 << 30),
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
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            proton: None,
            ..fake_plan()
        }),
    );
    let mut ui = render(&app);
    let _ = ui.click("Start install");
    assert!(
        !ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(_))))
    );

    let _ = app.update(Message::Install(InstallMsg::Proton(
        "5".into(),
        crate::settings::ProtonChoice("/proton/GE-Proton".into()),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "install-ready");
    ui.click("Start install").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(id)) if id == "5"))
    );
    // Once started, the panel gives way to the game page, which shows the download.
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.panel, None);
    let mut ui = render(&app);
    snapshot(&mut ui, "download-on-game-page");
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
    assert!(
        ui.find("/games/Game 5 (an interrupted install resumes where it started)")
            .is_ok()
    );
    assert!(ui.find("Browse").is_err());
}

#[test]
fn running_install_shows_progress_and_can_pause() {
    let mut app = library_app();
    // 20 MiB in two seconds.
    let mut rate = crate::install::Rate::default();
    let start = std::time::Instant::now();
    rate.record(start, 0);
    rate.record(start + std::time::Duration::from_secs(2), 20 << 20);
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 3,
                files_total: 10,
                bytes_done: 1 << 30,
                bytes_total: 4 << 30,
            },
            cancel: tokio_util::sync::CancellationToken::new(),
            cancelling: crate::install::Cancelling::No,
            rate,
        },
    );
    assert!(render(&app).find("Downloading [FAKE] Game 5").is_ok());
    // The game page shows the download; no panel holds the window.
    let _ = app.update(Message::Select("5".into()));
    let _ = app.update(Message::OpenPanel(Panel::Install));
    assert_eq!(app.panel, None);
    settle(&mut app);
    let mut ui = render(&app);
    assert!(ui.find("1.00 GiB / 4.00 GiB · 10.0 MiB/s").is_ok());
    assert!(ui.find("25 %").is_ok(), "at the end of the bar");
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
    assert!(ui.find("4.00 GiB").is_ok() && ui.find("7.00 GiB").is_ok());
    assert!(ui.find("100.00 GiB").is_ok(), "free space");
    assert!(
        ui.find("[FAKE] Other DLC (2.00 GiB)").is_err(),
        "not owned: not listed"
    );
    snapshot(&mut ui, "install-dlc");
    ui.click("[FAKE] Owned DLC (2.00 GiB)").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Install(InstallMsg::ToggleDlc(g, d))] if g == "5" && d == "21")
    );
    for m in messages {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    assert!(ui.find("3.00 GiB").is_ok() && ui.find("5.00 GiB").is_ok());
}

#[test]
fn game_settings_panel_applies_dlc_changes_only_when_something_changed() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
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
        "Default installation path",
        "Default Proton",
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
    settle(&mut app);
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
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 1,
                files_total: 4,
                bytes_done: 1 << 30,
                bytes_total: 2 << 30,
            },
            cancel: cancel.clone(),
            cancelling: crate::install::Cancelling::No,
            rate: Default::default(),
        },
    );
    let _ = app.update(Message::CloseRequested);
    let mut ui = render(&app);
    assert!(
        ui.find(
            "Downloading [FAKE] Game 5 (50 %). It stops here and resumes where it stopped when SlattyLauncher starts again."
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
        !cancel.is_cancelled(),
        "left as it stands, so that the next start resumes it"
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
            .any(|m| matches!(m, Message::OpenDialog(id, Panel::Install) if id == "5"))
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
    assert_eq!(app.interrupted, [("7".to_string(), Interrupted::Paused)]);
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

#[test]
fn an_installed_game_can_change_its_proton() {
    let mut app = library_app();
    let core = app.core.clone().unwrap();
    app.installs["3"].save(&core.db).unwrap();
    let other = std::env::temp_dir().join(format!("slatty-gui-proton-{}", std::process::id()));
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("proton"), b"").unwrap();
    app.proton_choices = vec!["/proton/GE-Proton".into(), other.clone()];
    open(&mut app, "3", Some(Panel::GameSettings));
    {
        let mut ui = render(&app);
        assert!(ui.find("Used from the next launch.").is_ok());
        snapshot(&mut ui, "game-settings-proton");
    }

    let _ = app.update(Message::Settings(crate::settings::SettingsMsg::GameProton(
        "3".into(),
        crate::settings::ProtonChoice(other.clone()),
    )));
    let saved = slatty_core::install::Install::get(&core.db, "3")
        .unwrap()
        .unwrap();
    for install in [&app.installs["3"], &saved] {
        assert_eq!(
            install.runner,
            Runner::Umu {
                proton: other.clone(),
                prefix: "/prefixes/jeu3".into()
            }
        );
    }
    std::fs::remove_dir_all(other).unwrap();
}

#[test]
fn cloud_saves_wait_for_the_first_launch_to_be_written() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Cloud));
    let mut ui = render(&app);
    assert!(
        ui.find("Your cloud saves are downloaded when the game first starts, before it runs.")
            .is_ok()
    );
    assert!(ui.find("Check").is_ok());
    let _ = ui.click("Sync now");
    assert!(
        ui.into_messages().next().is_none(),
        "the fixture's prefix was never created: Sync now waits"
    );
}

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
fn a_narrow_window_lays_the_achievements_drawer_over_the_page() {
    use iced::mouse::{Button, Event as Mouse};
    let mut app = app_with_achievements();
    let click_page = |app: &App| -> Vec<Message> {
        let mut ui = Simulator::with_size(settings(), app.window, app.view());
        ui.point_at(iced::Point::new(100.0, 300.0));
        let _ = ui.simulate([
            iced::Event::Mouse(Mouse::ButtonPressed(Button::Left)),
            iced::Event::Mouse(Mouse::ButtonReleased(Button::Left)),
        ]);
        ui.into_messages().collect()
    };
    assert!(
        !click_page(&app)
            .iter()
            .any(|m| matches!(m, Message::ClosePanel)),
        "beside the page in a wide window"
    );
    let _ = app.update(Message::WindowResized(Size::new(800.0, 560.0)));
    {
        let mut ui = Simulator::with_size(settings(), app.window, app.view());
        snapshot(&mut ui, "drawer-narrow");
    }
    assert!(
        click_page(&app)
            .iter()
            .any(|m| matches!(m, Message::ClosePanel)),
        "over the page in a narrow window, closed by a click beside it"
    );
}

#[test]
fn a_new_page_fades_in_then_stops_drawing_frames() {
    use std::time::Duration;
    let mut app = library_app();
    assert!(!app.page_shown.is_animating(app.now));
    let _ = app.update(Message::Select("3".into()));
    let start = app.now;
    assert!(app.page_shown.is_animating(start), "opening a game");
    let _ = app.update(Message::Frame(start + Duration::from_secs(1)));
    assert!(
        !app.page_shown.is_animating(app.now),
        "done once its time is over"
    );
    let _ = app.update(Message::OpenPanel(Panel::Achievements));
    assert!(
        !app.page_shown.is_animating(app.now),
        "a panel is not a new page"
    );
    let _ = app.update(Message::CloseDetail);
    assert!(app.page_shown.is_animating(app.now), "back to the library");
    let _ = app.update(Message::Frame(app.now + Duration::from_secs(1)));
    let _ = app.update(Message::ShowPage(Page::Achievements));
    assert!(app.page_shown.is_animating(app.now), "another tab");
}

#[test]
fn game_settings_show_the_language_and_switch_it_when_gog_offers_others() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = |languages: &[&str]| ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: languages.iter().map(|l| l.to_string()).collect(),
        chosen_language: "en-US".into(),
        dlcs: Vec::new(),
        chosen_dlcs: Vec::new(),
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(&["en-US"])),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("English").is_ok(), "named, not en-US");
        assert!(ui.find("Switch language").is_err(), "nothing to switch to");
        snapshot(&mut ui, "game-settings-one-language");
    }

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(&["en-US", "fr-FR"])),
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ChooseLanguage(
        "3".into(),
        "fr-FR".into(),
    )));
    let mut ui = render(&app);
    ui.click("Switch language").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Language(l))) if id == "3" && l == "fr-FR"
    )));
}

#[test]
fn the_cover_does_not_stand_in_while_the_key_art_downloads() {
    let mut app = library_app();
    let art = "https://images.example/[FAKE]-art.jpg".to_string();
    app.library[2].background = Some(art.clone());
    app.covers.insert(
        "3".into(),
        iced::widget::image::Handle::from_bytes(vec![0u8]),
    );
    let _ = app.update(Message::Select("3".into()));
    settle(&mut app);
    let game = app.library[2].clone();
    assert!(app.images_requested.contains(&art));
    assert!(app.hero_art(&game).is_none(), "plain while downloading");

    let _ = app.update(Message::Image(art.clone(), Some(vec![1u8])));
    assert!(app.hero_art(&game).is_some());
    assert!(app.animating(), "the key art fades in");
    settle(&mut app);
    assert!(!app.animating());

    // Without key art (download failed), the cover stands in.
    app.images.clear();
    let _ = app.update(Message::Image(art, None));
    assert_eq!(
        app.hero_art(&game).map(|h| h.id()),
        app.covers.get("3").map(|h| h.id())
    );
}

#[test]
fn cancelling_a_download_asks_then_deletes_it() {
    use crate::install::Cancelling;
    let mut app = library_app();
    open(&mut app, "5", None);
    let cancel = tokio_util::sync::CancellationToken::new();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 3,
                files_total: 10,
                bytes_done: 3 << 30,
                bytes_total: 8 << 30,
            },
            cancel: cancel.clone(),
            cancelling: Cancelling::No,
            rate: Default::default(),
        },
    );
    let cancelling = |app: &App| match app.install_views.get("5") {
        Some(InstallView::Running { cancelling, .. }) => Some(*cancelling),
        _ => None,
    };
    let clicked = |app: &App, label: &str| -> Vec<Message> {
        let mut ui = render(app);
        ui.click(label).unwrap();
        ui.into_messages().collect()
    };

    for m in clicked(&app, "Cancel") {
        let _ = app.update(m);
    }
    assert_eq!(cancelling(&app), Some(Cancelling::Asked));
    {
        let mut ui = render(&app);
        assert!(
            ui.find(
                "Cancel the download of [FAKE] Game 5 and delete the 3.00 GiB downloaded so far?"
            )
            .is_ok()
        );
        snapshot(&mut ui, "install-cancel-confirm");
    }
    for m in clicked(&app, "Keep downloading") {
        let _ = app.update(m);
    }
    assert_eq!(cancelling(&app), Some(Cancelling::No));
    assert!(!cancel.is_cancelled(), "asking stops nothing");

    let _ = app.update(Message::Install(InstallMsg::AskCancel("5".into())));
    for m in clicked(&app, "Cancel download") {
        let _ = app.update(m);
    }
    assert!(cancel.is_cancelled());
    assert_eq!(cancelling(&app), Some(Cancelling::Confirmed));

    // Once stopped, the partial download is deleted rather than offered for resuming.
    let _ = app.update(Message::Install(InstallMsg::Done("5".into(), Err(None))));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    assert!(!app.install_views.contains_key("5"));
    assert_eq!(app.panel, None);
    assert!(app.interrupted.is_empty());
}

#[test]
fn a_build_without_language_packs_says_the_game_chooses() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            language: "*".into(),
            languages: Vec::new(),
            ..fake_plan()
        }),
    );
    let mut ui = render(&app);
    assert!(
        ui.find("Language: one download holds every language; choose it in the game.")
            .is_ok()
    );
}

#[test]
fn download_speed_follows_the_last_seconds() {
    use std::time::Duration;
    let mut rate = crate::install::Rate::default();
    let start = std::time::Instant::now();
    let at = |s: u64| start + Duration::from_millis(s * 250);
    rate.record(at(0), 0);
    assert_eq!(rate.per_second(), None, "not before a second");
    rate.record(at(2), 1 << 20);
    assert_eq!(rate.per_second(), None);
    rate.record(at(4), 2 << 20);
    assert_eq!(rate.per_second(), Some((2 << 20) as f64));
    // A fast start no longer counts once it is more than three seconds old.
    for i in 5..=40 {
        rate.record(at(i), (2 << 20) + (i - 4) * (256 << 10));
    }
    assert_eq!(rate.per_second(), Some((1 << 20) as f64));
}

#[test]
fn default_installation_path_is_browsed_or_typed_without_a_save_button() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let db = app.core.clone().unwrap().db;
    let saved = || slatty_core::settings::library_root(&db).unwrap();
    {
        let mut ui = render(&app);
        assert!(ui.find("Save").is_err());
        ui.click("Browse").unwrap();
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::Settings(SettingsMsg::BrowseRoot)))
        );
    }
    let _ = app.update(Message::Settings(SettingsMsg::BrowsedRoot(Some(
        "/games/picked".into(),
    ))));
    assert_eq!(app.library_root, "/games/picked");
    assert_eq!(saved(), PathBuf::from("/games/picked"));

    let _ = app.update(Message::Settings(SettingsMsg::RootInput("games".into())));
    assert_eq!(
        saved(),
        PathBuf::from("/games/picked"),
        "not absolute: kept"
    );
    assert!(app.notice.is_none(), "no error while typing");
    let _ = app.update(Message::Settings(SettingsMsg::SaveRoot));
    assert!(app.notice.is_some(), "Enter says why");
    let _ = app.update(Message::Settings(SettingsMsg::RootInput(
        "/games/typed".into(),
    )));
    assert_eq!(saved(), PathBuf::from("/games/typed"));
}

#[test]
fn install_panel_shows_the_free_space_and_warns_when_short() {
    use crate::ui::format::human_size;
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    let plan = fake_plan();
    app.install_views
        .insert("5".into(), InstallView::Ready(plan.clone()));
    assert!(render(&app).find("100.00 GiB").is_ok());

    // Another folder: its drive is measured again, and an answer for the old one is ignored.
    let _ = app.update(Message::Install(InstallMsg::RootInput(
        "5".into(),
        "/small".into(),
    )));
    let _ = app.update(Message::Install(InstallMsg::FreeSpace(
        "5".into(),
        "/games".into(),
        Some(100 << 30),
    )));
    assert!(render(&app).find("—").is_ok(), "unknown yet");
    let _ = app.update(Message::Install(InstallMsg::FreeSpace(
        "5".into(),
        "/small".into(),
        Some(1 << 30),
    )));
    let mut ui = render(&app);
    assert!(
        ui.find(format!(
            "Not enough space on this drive: 1.00 GiB free, {} needed.",
            human_size(plan.total_disk())
        ))
        .is_ok()
    );
    snapshot(&mut ui, "install-short-of-space");
}

#[test]
fn game_settings_list_only_owned_dlc() {
    use crate::maintenance::ContentInfo;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = |dlcs| ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: vec!["en-US".into()],
        chosen_language: "en-US".into(),
        dlcs,
        chosen_dlcs: vec!["21".into()],
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(vec![
            fake_dlc("21", "[FAKE] Owned DLC", true, true),
            fake_dlc("22", "[FAKE] Other DLC", false, false),
        ])),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Owned DLC (2.00 GiB)").is_ok());
        assert!(ui.find("[FAKE] Other DLC (2.00 GiB)").is_err());
    }

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(vec![fake_dlc(
            "22",
            "[FAKE] Other DLC",
            false,
            false,
        )])),
    )));
    assert!(
        render(&app)
            .find("You own none of this game's DLC.")
            .is_ok()
    );
}

#[test]
fn a_cut_off_download_resumes_by_itself_at_start_up() {
    use crate::Interrupted;
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    app.interrupted = vec![
        ("6".into(), Interrupted::Paused),
        ("5".into(), Interrupted::Download),
    ];
    let _ = app.resume_interrupted();
    assert_eq!(app.auto_resume.as_deref(), Some("5"), "not the paused one");
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.interrupted, [("6".to_string(), Interrupted::Paused)]);
    assert!(app.installing().is_some());

    // Offline at start-up: it stays listed, and the reason is shown.
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    app.interrupted = vec![("5".into(), Interrupted::Download)];
    let _ = app.resume_interrupted();
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Err("network error".into()),
    )));
    assert!(app.installing().is_none());
    assert_eq!(app.interrupted.len(), 1);
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
}

#[test]
fn a_cut_off_update_resumes_by_itself_at_start_up() {
    use crate::Interrupted;
    let mut app = library_app();
    app.installs.insert(
        "4".into(),
        Install {
            game_id: "4".into(),
            ..app.installs["3"].clone()
        },
    );
    app.interrupted = vec![
        ("3".into(), Interrupted::Update),
        ("4".into(), Interrupted::UpdatePaused),
    ];
    let _ = app.resume_interrupted();
    assert!(app.maintenance.get("3").is_some_and(|m| m.busy));
    assert!(
        !app.maintenance.get("4").is_some_and(|m| m.busy),
        "paused on request: waits for Finish update"
    );
    let mut ui = render(&app);
    assert!(
        ui.find("Update paused. The game cannot start until it is finished.")
            .is_ok()
    );
    assert!(ui.find("Finishing the update…").is_ok());
}

#[test]
fn cloud_drawer_shows_the_folder_and_what_a_sync_would_do() {
    use crate::cloud::{CloudRequest, CloudResult, CloudStatus, Counts, SaveLocation};
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Cloud));
    let location = |counts| SaveLocation {
        name: "saves".into(),
        folder: "/prefixes/jeu3/drive_c/users/steamuser/[FAKE] Saves".into(),
        counts,
        notes: Vec::new(),
    };
    let checked = CloudResult {
        lines: Vec::new(),
        locations: vec![location(Some(Counts {
            upload: 2,
            unchanged: 20,
            ..Default::default()
        }))],
        conflicts: false,
        status: CloudStatus::Pending(2),
        summary: None,
    };
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Ok(checked.clone()),
    ));
    {
        let mut ui = render(&app);
        assert!(
            ui.find("/prefixes/jeu3/drive_c/users/steamuser/[FAKE] Saves")
                .is_ok()
        );
        assert!(ui.find("To upload").is_ok() && ui.find("20").is_ok());
        snapshot(&mut ui, "cloud-drawer");
    }

    // A sync is checked again right away, and what it did stays shown.
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Sync,
        Ok(CloudResult {
            locations: vec![location(None)],
            status: CloudStatus::UpToDate,
            summary: Some("Last sync: 2 file(s) uploaded, 0 downloaded.".into()),
            ..checked.clone()
        }),
    ));
    assert!(app.cloud["3"].busy, "checking again");
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Ok(CloudResult {
            status: CloudStatus::UpToDate,
            ..checked
        }),
    ));
    assert!(
        render(&app)
            .find("Last sync: 2 file(s) uploaded, 0 downloaded.")
            .is_ok()
    );
}

fn fake_versions() -> Vec<crate::install::Version> {
    crate::install::versions(&[
        slatty_core::galaxy::Build {
            build_id: "b2".into(),
            version_name: "1.1".into(),
            link: String::new(),
            branch: None,
            generation: 2,
            date_published: Some("2026-03-27T06:59:13+0000".into()),
        },
        slatty_core::galaxy::Build {
            build_id: "b1".into(),
            version_name: "1.0".into(),
            link: String::new(),
            branch: None,
            generation: 2,
            date_published: Some("2025-08-28T07:29:12+0000".into()),
        },
    ])
}

#[test]
fn versions_are_named_with_their_date_and_the_latest_marked() {
    let labels: Vec<String> = fake_versions().into_iter().map(|v| v.label).collect();
    assert_eq!(labels, ["1.1 · 27 Mar 2026 (latest)", "1.0 · 28 Aug 2025"]);
}

#[test]
fn another_version_can_be_chosen_before_install_and_after() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            build_id: "b2".into(),
            versions: fake_versions(),
            ..fake_plan()
        }),
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Game version").is_ok());
        snapshot(&mut ui, "install-version");
    }
    let _ = app.update(Message::Install(InstallMsg::Version(
        "5".into(),
        "b1".into(),
    )));
    assert!(
        matches!(app.install_views.get("5"), Some(InstallView::Planning)),
        "planned again for that version"
    );

    // Installed: switching asks for that build.
    open(&mut app, "3", Some(Panel::GameSettings));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(ContentInfo {
            build_id: "b2".into(),
            chosen_build: "b2".into(),
            versions: fake_versions(),
            language: "en-US".into(),
            languages: vec!["en-US".into()],
            chosen_language: "en-US".into(),
            dlcs: Vec::new(),
            chosen_dlcs: Vec::new(),
        }),
    )));
    {
        let mut ui = render(&app);
        let _ = ui.click("Switch version");
        assert!(ui.into_messages().next().is_none(), "nothing chosen yet");
    }
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ChooseVersion(
        "3".into(),
        "b1".into(),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "game-settings-version");
    ui.click("Switch version").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Build(b))) if id == "3" && b == "b1"
    )));
}

#[test]
fn a_second_install_says_why_it_waits() {
    let mut app = library_app();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: Default::default(),
            cancel: tokio_util::sync::CancellationToken::new(),
            cancelling: crate::install::Cancelling::No,
            rate: Default::default(),
        },
    );
    open(&mut app, "6", Some(Panel::Install));
    app.install_views
        .insert("6".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(
        ui.find("[FAKE] Game 5 is downloading; this one can start once it is done.")
            .is_ok()
    );
    let _ = ui.click("Start install");
    assert!(ui.into_messages().next().is_none());
}

#[test]
fn the_theme_file_can_be_created_from_settings() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let path = crate::theme::file(&app.core.as_ref().unwrap().dirs.config);
    let _ = std::fs::remove_file(&path);
    {
        let mut ui = render(&app);
        ui.click("Create theme file").unwrap();
        for m in ui.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("[colors]") && written.contains("accent = \"#c4b1fa\""));
    let mut ui = render(&app);
    assert!(ui.find("Edit").is_ok() && ui.find("Reload").is_ok());
    snapshot(&mut ui, "settings-appearance");
    drop(ui);
    // Reloading the defaults just written changes nothing for the other tests.
    let _ = app.update(Message::Settings(SettingsMsg::ReloadTheme));
    assert!(app.notice.as_ref().is_some_and(|n| !n.error));
}

#[test]
fn the_interface_font_is_chosen_in_settings() {
    use crate::settings::{FontChoice, SettingsMsg};
    let mut app = library_app();
    let _ = app.update(Message::ShowPage(Page::Settings));
    assert!(!app.font_families.is_empty(), "listed when Settings opens");
    assert!(render(&app).find("Font").is_ok());
    // The default leaves the font every other test draws with.
    let _ = app.update(Message::Settings(SettingsMsg::Font(FontChoice::Default)));
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(slatty_core::settings::interface_font(&db).unwrap(), None);
    assert!(app.notice.is_none());
}

#[test]
fn cover_size_is_set_in_settings_and_kept() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    let _ = app.update(Message::ShowPage(Page::Settings));
    {
        let mut ui = render(&app);
        assert!(ui.find("Cover size").is_ok());
        snapshot(&mut ui, "settings-appearance");
    }
    let _ = app.update(Message::Settings(SettingsMsg::CoverSize(
        crate::settings::CoverSize(130),
    )));
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(
        slatty_core::settings::cover_width(&db).unwrap(),
        Some(195.0)
    );
    assert_eq!(app.card_width, 195.0);
    // A size saved by the former slider shows as the nearest one offered.
    assert_eq!(crate::settings::CoverSize::of(200.0).0, 130);
}

#[test]
fn the_library_order_is_kept() {
    let mut app = library_app();
    let _ = app.update(Message::SortBy(Sort::MostPlayed));
    let db = app.core.as_ref().unwrap().db.clone();
    let saved = slatty_core::settings::library_sort(&db).unwrap();
    assert_eq!(
        saved.as_deref().and_then(Sort::from_key),
        Some(Sort::MostPlayed)
    );
    for s in Sort::ALL {
        assert_eq!(Sort::from_key(s.key()), Some(s));
    }
}

#[test]
fn installing_from_the_library_stays_on_the_library() {
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    let _ = app.update(Message::OpenDialog("5".into(), Panel::Install));
    assert_eq!(app.dialog, Some(("5".to_string(), Panel::Install)));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("Start install").is_ok() && ui.find("Proton").is_ok());
        snapshot(&mut ui, "install-dialog");
    }
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert_eq!(app.dialog, None, "the dialog gives way once started");
    assert_eq!(app.selected, None, "still on the library");
    assert!(
        render(&app).find("Downloading [FAKE] Game 5").is_ok(),
        "its banner"
    );

    // Escape closes the dialog without leaving the library.
    let _ = app.update(Message::OpenDialog("6".into(), Panel::Install));
    app.go_back();
    assert_eq!(app.dialog, None);
}

#[test]
fn game_settings_open_over_the_library_too() {
    use crate::maintenance::ContentInfo;
    let mut app = library_app();
    let _ = app.update(Message::OpenDialog("3".into(), Panel::GameSettings));
    assert!(
        app.maintenance.get("3").is_some_and(|m| m.busy),
        "reading languages and DLC"
    );
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(ContentInfo {
            build_id: "b1".into(),
            chosen_build: "b1".into(),
            versions: Vec::new(),
            language: "en-US".into(),
            languages: vec!["en-US".into()],
            chosen_language: "en-US".into(),
            dlcs: Vec::new(),
            chosen_dlcs: Vec::new(),
        }),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Game settings").is_ok() && ui.find("Used from the next launch.").is_ok());
    snapshot(&mut ui, "settings-dialog");
    drop(ui);
    assert_eq!(app.selected, None, "still on the library");
    let _ = app.update(Message::CloseDialog);
    assert_eq!(app.dialog, None);
}

#[test]
fn a_linux_build_can_be_chosen_and_needs_no_proton() {
    let mut app = library_app();
    app.library.iter_mut().find(|g| g.id == "5").unwrap().os =
        vec!["windows".into(), "linux".into(), "osx".into()];
    assert_eq!(app.platforms("5"), vec![Platform::Windows, Platform::Linux]);
    assert_eq!(app.platforms("3"), vec![Platform::Windows]);
    open(&mut app, "5", Some(Panel::Install));
    let both = vec![Platform::Windows, Platform::Linux];
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            platforms: both.clone(),
            proton: None,
            ..fake_plan()
        }),
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Proton").is_ok());
        assert!(ui.find("Choose a Proton build to start.").is_ok());
    }
    // Another platform is planned again.
    let _ = app.update(Message::Install(InstallMsg::Platform(
        "5".into(),
        Platform::Linux,
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            platform: Platform::Linux,
            platforms: both,
            proton: None,
            ..fake_plan()
        }),
    );
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Proton").is_err(), "a Linux build runs without");
        assert!(ui.find("Game version").is_err());
        assert!(
            ui.find(
                "GOG keeps no cloud saves for Linux builds, and they do not report achievements."
            )
            .is_ok()
        );
        snapshot(&mut ui, "install-linux");
        ui.click("Start install").unwrap();
        ui.into_messages().collect()
    };
    assert!(
        matches!(&messages[..], [Message::Install(InstallMsg::Start(id))] if id == "5"),
        "{messages:?}"
    );
}

#[test]
fn the_default_platform_is_kept() {
    use crate::settings::{PlatformChoice, SettingsMsg};
    let mut app = library_app();
    let _ = app.update(Message::Settings(SettingsMsg::Platform(PlatformChoice(
        Platform::Linux,
    ))));
    assert_eq!(app.default_platform, Platform::Linux);
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(
        slatty_core::settings::default_platform(&db).unwrap(),
        Platform::Linux
    );
}

#[test]
fn the_session_shows_in_a_drawer_with_stop() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Session));
    {
        let mut ui = render(&app);
        assert!(ui.find("No session yet.").is_ok());
    }
    let (stop, _stopped) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(crate::play::PlayState {
        game_id: "3".into(),
        log: vec!["Preparing…".into(), "Game started.".into()],
        stop,
        running: true,
    });
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Preparing…").is_ok());
        assert!(ui.find("Game started.").is_ok());
        snapshot(&mut ui, "session-drawer");
        ui.click("Stop game").unwrap();
        ui.into_messages().collect()
    };
    assert!(matches!(&messages[..], [Message::StopGame]), "{messages:?}");
    // The page stays usable beside it: the drawer closes like the others.
    let _ = app.update(Message::ClosePanel);
    assert_eq!(app.panel, None);
}

/// Game `id` with its install choices made, as its install panel holds them.
fn ready_to_install(app: &mut App, id: &str) {
    app.install_views.insert(
        id.into(),
        InstallView::Ready(crate::install::PlanInfo {
            title: format!("[FAKE] Game {id}"),
            directory: format!("Game {id}"),
            ..fake_plan()
        }),
    );
}

#[test]
fn installs_started_during_a_download_wait_in_a_queue_that_can_be_reordered() {
    use crate::downloads::{DownloadsMsg, ROW_HEIGHT, ROW_SPACING};
    use slatty_core::installer::InstallJob;
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    for id in ["5", "6", "7"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.queue, ["6", "7"]);
    assert!(matches!(
        app.install_views.get("6"),
        Some(InstallView::Queued(_))
    ));
    // Each waits as a job, in an order kept for the next start.
    assert!(InstallJob::load(&db, "6").unwrap().unwrap().is_queued());
    assert_eq!(
        slatty_core::settings::download_queue(&db).unwrap(),
        ["6", "7"]
    );

    app.page = Page::Downloads;
    {
        let mut ui = render(&app);
        assert!(ui.find("Queue · 2").is_ok());
        assert!(ui.find("[FAKE] Game 7").is_ok());
        assert!(ui.find("/games/Game 5").is_ok(), "where the download goes");
        snapshot(&mut ui, "downloads");
    }

    // The second row is dragged above the first.
    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(1)));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drag(ROW_HEIGHT / 2.0)));
    assert_eq!(app.queue_order(), ["7", "6"], "shown where it would land");
    assert_eq!(app.queue, ["6", "7"], "not moved before it is dropped");
    let _ = app.update(Message::Downloads(DownloadsMsg::Drop));
    assert_eq!(app.queue, ["7", "6"]);
    assert_eq!(
        slatty_core::settings::download_queue(&db).unwrap(),
        ["7", "6"]
    );
    // Below the last row, it goes last.
    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(0)));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drag(
        5.0 * (ROW_HEIGHT + ROW_SPACING),
    )));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drop));
    assert_eq!(app.queue, ["6", "7"]);

    // The download ends: it is listed as completed, and the next one starts.
    let _ = app.update(Message::Install(InstallMsg::Progress(
        "5".into(),
        slatty_core::installer::Progress {
            files_done: 10,
            files_total: 10,
            bytes_done: 4 << 30,
            bytes_total: 4 << 30,
        },
    )));
    let install = Install {
        umu_id: None,
        game_id: "5".into(),
        title: "[FAKE] Game 5".into(),
        platform: Platform::Windows,
        path: PathBuf::from("/games/Game 5"),
        client_id: None,
        runner: Runner::Umu {
            proton: "/proton/GE-Proton".into(),
            prefix: "/prefixes/jeu5".into(),
        },
    };
    let _ = app.update(Message::Install(InstallMsg::Done("5".into(), Ok(install))));
    assert_eq!(app.completed, [("5".to_string(), 4 << 30)]);
    assert!(matches!(
        app.install_views.get("6"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.queue, ["7"]);
    {
        let mut ui = render(&app);
        assert!(ui.find("Completed · 1").is_ok());
        assert!(ui.find("Play").is_ok());
    }

    // Taken out of the queue, an install is forgotten.
    let _ = app.update(Message::Downloads(DownloadsMsg::Remove("7".into())));
    assert!(app.queue.is_empty());
    assert!(
        slatty_core::settings::download_queue(&db)
            .unwrap()
            .is_empty()
    );

    // A pause holds the queue.
    ready_to_install(&mut app, "8");
    let _ = app.update(Message::Install(InstallMsg::Start("8".into())));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(app.queue, ["8"]);
    assert!(app.installing().is_none());
}

#[test]
fn the_side_list_of_settings_brings_a_part_to_the_top() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    assert_eq!(app.settings_view.section, Section::Account);
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        for s in Section::ALL {
            assert!(ui.find(s.title()).is_ok(), "{}", s.title());
        }
        ui.click("Installs").unwrap();
        ui.into_messages().collect()
    };
    assert!(
        matches!(
            &messages[..],
            [Message::Settings(SettingsMsg::Show(Section::Installs))]
        ),
        "{messages:?}"
    );
    for m in messages {
        let _ = app.update(m);
    }
    assert_eq!(app.settings_view.section, Section::Installs);

    // In a narrow window each name goes above its setting.
    app.window = Size::new(940.0, 1000.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    assert!(ui.find("Default installation path").is_ok());
    snapshot(&mut ui, "settings-narrow");
}

#[test]
fn the_side_list_of_settings_follows_the_scroll() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    let scrolled = |app: &mut App, offset: f32| {
        let _ = app.update(Message::Settings(SettingsMsg::Scrolled {
            offset,
            max: 900.0,
        }));
        app.settings_view.section
    };
    // Measured first; the parts start 0, 200, 450, 800 and 1100 below the top.
    assert_eq!(scrolled(&mut app, 300.0), Section::Account);
    for (s, y) in Section::ALL
        .into_iter()
        .zip([0.0, 200.0, 450.0, 800.0, 1100.0])
    {
        let _ = app.update(Message::Settings(SettingsMsg::Measured(s, y)));
    }
    assert_eq!(scrolled(&mut app, 0.0), Section::Account);
    assert_eq!(scrolled(&mut app, 210.0), Section::Library);
    assert_eq!(
        scrolled(&mut app, 440.0),
        Section::Installs,
        "nearly at the top"
    );
    assert_eq!(scrolled(&mut app, 900.0), Section::About, "at the end");

    // An entry chosen stays highlighted while the page goes where it asked, even when its part
    // cannot reach the top; scrolling away hands over to the page again.
    let _ = app.update(Message::Settings(SettingsMsg::Show(Section::Appearance)));
    let _ = app.update(Message::Settings(SettingsMsg::ScrollTo(Some(800.0))));
    assert_eq!(scrolled(&mut app, 300.0), Section::Library);
    let _ = app.update(Message::Settings(SettingsMsg::Show(Section::Appearance)));
    let _ = app.update(Message::Settings(SettingsMsg::ScrollTo(Some(1100.0))));
    assert_eq!(scrolled(&mut app, 900.0), Section::Appearance);
    assert_eq!(scrolled(&mut app, 500.0), Section::Installs);

    // A new window size measures again.
    let _ = app.update(Message::WindowResized(Size::new(1000.0, 800.0)));
    assert!(app.settings_view.tops.iter().all(Option::is_none));
}

#[test]
fn what_leaves_the_computer_can_be_turned_off() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(app.umu_lookup && app.report_playtime, "on until turned off");
    let _ = app.update(Message::Settings(SettingsMsg::UmuLookup(false)));
    let _ = app.update(Message::Settings(SettingsMsg::ReportPlaytime(false)));
    let _ = app.update(Message::Settings(SettingsMsg::GameAchievements(false)));
    assert!(!app.game_achievements);
    assert!(!slatty_core::settings::game_achievements(&db).unwrap());
    assert!(!app.umu_lookup && !app.report_playtime);
    assert!(!slatty_core::settings::umu_lookup(&db).unwrap());
    assert!(!slatty_core::settings::report_playtime(&db).unwrap());
    app.window = Size::new(1440.0, 1900.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    assert!(ui.find("Play time on GOG").is_ok());
    snapshot(&mut ui, "settings-privacy");
}

#[test]
fn a_game_with_several_launch_options_asks_once_which_to_start() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    open(&mut app, "3", None);
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "Configuration Tool".into()],
    );
    let _ = app.update(Message::Play("3".into()));
    assert_eq!(app.launch_prompt.as_deref(), Some("3"));
    assert!(app.play.is_none(), "nothing starts before the choice");
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Default").is_ok());
        snapshot(&mut ui, "launch-choice");
        ui.click("Configuration Tool").unwrap();
        ui.into_messages().collect()
    };
    for m in messages {
        let _ = app.update(m);
    }
    assert!(app.launch_prompt.is_none());
    assert!(app.play.is_some(), "it starts once chosen");
    assert_eq!(
        slatty_core::settings::launch_choice(&db, "3")
            .unwrap()
            .as_deref(),
        Some("Configuration Tool")
    );

    // Kept: the next Play does not ask, and Game settings shows it.
    app.play = None;
    let _ = app.update(Message::Play("3".into()));
    assert!(app.launch_prompt.is_none() && app.play.is_some());
    app.play = None;
    app.panel = Some(Panel::GameSettings);
    let mut ui = render(&app);
    assert!(ui.find("Launch").is_ok());
    assert_eq!(app.launch_choices["3"], "Configuration Tool");
    snapshot(&mut ui, "launch-in-game-settings");
}

#[test]
fn the_queue_goes_on_once_nothing_downloads_or_waits() {
    use crate::Interrupted;
    let mut app = library_app();
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    ready_to_install(&mut app, "6");
    let _ = app.update(Message::Install(InstallMsg::Start("6".into())));
    assert_eq!(app.queue, ["6"]);

    // Paused: the queue waits.
    let _ = app.update(Message::Install(InstallMsg::Done("5".into(), Err(None))));
    app.interrupted = vec![("5".into(), Interrupted::Paused)];
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    )));
    assert!(app.installing().is_none());
    assert_eq!(app.queue, ["6"]);

    // Discarded: the queue goes on.
    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    assert!(matches!(
        app.install_views.get("6"),
        Some(InstallView::Running { .. })
    ));
    assert!(app.queue.is_empty());
}

#[test]
fn a_cut_off_download_that_cannot_resume_waits_instead_of_being_retried() {
    use crate::Interrupted;
    let mut app = library_app();
    app.interrupted = vec![("5".into(), Interrupted::Download)];
    app.queue = vec!["6".into()];
    // Resumed first, before the queue.
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Err("offline".into()),
    )));
    assert_eq!(app.interrupted, [("5".to_string(), Interrupted::Paused)]);
    assert_eq!(app.queue, ["6"], "held by the download waiting");
    assert!(app.auto_resume.is_none());
}
