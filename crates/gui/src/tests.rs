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
        title: format!("[FICTIF] {title}"),
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
        .map(|i| fake_game(&i.to_string(), &format!("Jeu {i}")))
        .collect();
    app.fetched_at = Some(1_791_500_000);
    app.installs.insert(
        "3".into(),
        Install {
            game_id: "3".into(),
            title: "[FICTIF] Jeu 3".into(),
            platform: Platform::Windows,
            path: PathBuf::from("/jeux/Jeu 3"),
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
    assert!(ui.find("Valider").is_ok());
    assert!(ui.find("Mot de passe").is_err());
    snapshot(&mut ui, "login");
    ui.click("Ouvrir la page de connexion GOG").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenLoginPage))
    );
}

#[test]
fn search_filters_the_library() {
    let mut app = library_app();
    app.search = "jeu 1".into();
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    assert!(ui.find("[FICTIF] Jeu 12").is_ok());
    assert!(ui.find("[FICTIF] Jeu 3").is_err());
}

#[test]
fn installed_game_detail_can_be_played() {
    let mut app = library_app();
    app.selected = Some("3".into());
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    snapshot(&mut ui, "detail");
    ui.click("Jouer").unwrap();
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
    assert!(ui.find("Jouer").is_err());
    assert!(ui.find("Vérifier").is_err());
    assert!(ui.find("Afficher les achievements").is_ok());
}

fn fake_achievement(key: &str, unlocked: bool) -> slatty_core::achievements::Achievement {
    slatty_core::achievements::Achievement {
        achievement_id: format!("id-{key}"),
        achievement_key: key.into(),
        name: format!("[FICTIF] {key}"),
        description: String::new(),
        visible: true,
        date_unlocked: unlocked.then(|| "2026-10-09T10:00:00+0000".into()),
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
    assert!(ui.find("Réinitialiser").is_ok());
    ui.click("Débloquer").unwrap();
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
    ui.click("Confirmer").unwrap();
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
            name: "[FICTIF] Alpha".into(),
            unlock: true,
        }],
    ));
    let mut ui = Simulator::with_size(Default::default(), SIZE, app.view());
    ui.click("Annuler").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(app.pending_change.is_none());
}
