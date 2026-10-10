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
            isolated: true,
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

/// Every text shown, in one walk of the widget tree: a `find` per text walks all of it each time.
fn texts(ui: &mut Simulator<'_, Message>) -> Vec<String> {
    use iced_test::selector::Candidate;
    let mut all = Vec::new();
    let _ = ui.find(|candidate: Candidate<'_>| -> Option<()> {
        if let Candidate::Text { content, .. } = candidate {
            all.push(content.to_string());
        }
        None
    });
    all
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

mod achievements;
mod cloud;
mod downloads;
mod edit;
mod game_page;
mod install;
mod library;
mod maintenance;
mod settings;
