use std::path::PathBuf;
use std::sync::Arc;

use iced::{Size, Theme};
use iced_test::simulator::Simulator;
use slatty_core::account::AccountInfo;
use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::library::{LibraryGame, MetadataSource};
use slatty_core::paths::Dirs;
use slatty_core::runner::Runner;

use crate::{App, Core, Message};

const SIZE: Size = Size::new(1280.0, 820.0);

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

fn snapshot(ui: &mut Simulator<'_, Message>, name: &str) {
    if let Ok(dir) = std::env::var("SLATTY_SNAPSHOT_DIR") {
        let snap = ui.snapshot(&Theme::TokyoNight).unwrap();
        let path = PathBuf::from(dir).join(name);
        let _ = std::fs::remove_file(path.with_extension("png"));
        snap.matches_image(path).unwrap();
    }
}

#[test]
fn login_screen_offers_browser_login_without_password_field() {
    let app = App {
        core: Some(core()),
        ..Default::default()
    };
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Sign in").is_ok());
    assert!(ui.find("Mot de passe").is_err());
    snapshot(&mut ui, "login");
    ui.click("Open the GOG sign-in page").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenLoginPage))
    );
}

#[test]
fn search_filters_the_library() {
    let mut app = library_app();
    app.search = "game 1".into();
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("[FAKE] Game 12").is_ok());
    assert!(ui.find("[FAKE] Game 3").is_err());
}

#[test]
fn installed_game_detail_can_be_played() {
    let mut app = library_app();
    app.selected = Some("3".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    snapshot(&mut ui, "detail");
    ui.click("Play").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Play(id) if id == "3"))
    );
}

#[test]
fn uninstalled_game_offers_achievements_but_not_play_or_cloud() {
    let mut app = library_app();
    app.selected = Some("5".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Play").is_err());
    assert!(ui.find("Check").is_err());
    assert!(ui.find("Show achievements").is_ok());
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
    }
}

fn app_with_achievements() -> App {
    let mut app = library_app();
    app.selected = Some("5".into());
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
fn unlocking_only_asks_for_confirmation() {
    let mut app = app_with_achievements();
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
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
    assert!(matches!(
        app.achievements.get("5"),
        Some(crate::Loadable::Ready(_))
    ));

    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
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
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    ui.click("Cancel").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_none());
}

fn fake_plan() -> crate::installs::PlanInfo {
    crate::installs::PlanInfo {
        title: "[FAKE] Game 5".into(),
        version: "1.0".into(),
        language: "en-US".into(),
        languages: vec!["en-US".into(), "fr-FR".into()],
        download_size: 3 << 30,
        disk_size: 5 << 30,
        folder: "/games/Game 5".into(),
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
    app.selected = Some("5".into());
    app.install_views
        .insert("5".into(), crate::installs::InstallView::Ready(fake_plan()));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Choose a Proton version in Settings.").is_ok());
    let _ = ui.click("Install");
    assert!(
        !ui.into_messages()
            .any(|m| matches!(m, Message::Install(crate::installs::InstallMsg::Start(_))))
    );

    app.proton = Some("/proton/GE-Proton".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    snapshot(&mut ui, "install-ready");
    ui.click("Install").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Install(crate::installs::InstallMsg::Start(id)) if id == "5")
    ));
}

#[test]
fn running_install_shows_progress_and_can_pause() {
    let mut app = library_app();
    app.selected = Some("5".into());
    app.install_views.insert(
        "5".into(),
        crate::installs::InstallView::Running {
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
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Downloading [FAKE] Game 5").is_ok());
    snapshot(&mut ui, "install-running");
    ui.click("Pause").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Install(crate::installs::InstallMsg::Pause(id)) if id == "5")
    ));
}

#[test]
fn uninstall_requires_an_explicit_choice() {
    use crate::installs::MaintenanceMsg;
    let mut app = library_app();
    app.selected = Some("3".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    ui.click("Uninstall…").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Maintenance(MaintenanceMsg::AskUninstall(id))] if id == "3")
    );
    for m in messages {
        let _ = app.update(m);
    }
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    snapshot(&mut ui, "uninstall-confirm");
    ui.click("Uninstall, keep the prefix").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, false)) if id == "3")
    ));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    ui.click("Also delete the prefix (backed up)").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, true)) if id == "3")
    ));
}

#[test]
fn update_button_appears_only_when_an_update_exists() {
    use crate::installs::MaintenanceMsg;
    let mut app = library_app();
    app.selected = Some("3".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Update now").is_err());
    drop(ui);
    let _ = app.update(Message::Maintenance(MaintenanceMsg::UpdateChecked(
        "3".into(),
        Ok(Some("1.0 → 1.1".into())),
    )));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Update available: 1.0 → 1.1").is_ok());
    snapshot(&mut ui, "update-available");
    ui.click("Update now").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Maintenance(MaintenanceMsg::Apply(id, slatty_core::maintenance::Change::Update)) if id == "3"))
    );
}

#[test]
fn owned_dlc_can_be_deselected_before_install_and_sizes_follow() {
    use crate::installs::{InstallMsg, InstallView};
    let mut app = library_app();
    app.selected = Some("5".into());
    app.proton = Some("/proton/GE-Proton".into());
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
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
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("Download 3.00 GiB · on disk 5.00 GiB").is_ok());
}

#[test]
fn content_panel_applies_dlc_changes_only_when_something_changed() {
    use crate::installs::{ContentInfo, MaintenanceMsg};
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    app.selected = Some("3".into());
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
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    let _ = ui.click("Apply DLC changes");
    assert!(
        ui.into_messages().next().is_none(),
        "no change, button disabled"
    );

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ToggleContentDlc(
        "3".into(),
        "21".into(),
    )));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    snapshot(&mut ui, "content-panel");
    ui.click("Apply DLC changes").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Dlcs(d))) if id == "3" && d.is_empty()
    )));
}
