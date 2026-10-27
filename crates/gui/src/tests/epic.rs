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

/// An Epic game's page offers Install, which opens the same drawer as a GOG game's; once planned,
/// it downloads as any other. GOG is asked nothing about it: neither whether it can be installed
/// nor its achievements.
#[test]
fn an_epic_game_installs_through_the_install_drawer() {
    use crate::install::{InstallMsg, InstallView};
    let mut app = epic_app();
    let _ = app.update(Message::Select("epic-Alpha".into()));
    assert!(!app.achievements.contains_key("epic-Alpha"));
    assert!(!app.installability.contains_key("epic-Alpha"));
    {
        let mut ui = render(&app);
        assert!(ui.click("Install").is_ok());
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::OpenPanel(Panel::Install)))
        );
    }
    assert_eq!(
        app.epic_game("epic-Alpha").map(|g| g.app_name),
        Some("Alpha".into()),
        "the plan and the download are given the game as Epic describes it"
    );
    let plan = crate::install::PlanInfo {
        title: "[FAKE] Alpha".into(),
        language: "*".into(),
        languages: Vec::new(),
        dlcs: Vec::new(),
        dependencies: Vec::new(),
        ..fake_plan()
    };
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "epic-Alpha".into(),
        Ok(plan),
    )));
    open(&mut app, "epic-Alpha", Some(Panel::Install));
    assert!(
        render(&app)
            .find("Windows (the only build Epic offers)")
            .is_ok()
    );
    let _ = app.update(Message::Install(InstallMsg::Start("epic-Alpha".into())));
    assert!(matches!(
        app.install_views.get("epic-Alpha"),
        Some(InstallView::Running { .. })
    ));
}

/// An installed Epic game is verified, repaired, updated and uninstalled from Manage; it is not
/// offered GOG's offline installers.
#[test]
fn an_installed_epic_game_is_verified_updated_and_uninstalled_from_manage() {
    use crate::maintenance::MaintenanceMsg;
    let mut app = epic_app();
    app.installs.insert("epic-Alpha".into(), epic_install());
    open(&mut app, "epic-Alpha", Some(Panel::Manage));
    let mut ui = render(&app);
    assert!(ui.find("Uninstall…").is_ok());
    assert!(ui.find("Repair").is_ok());
    assert!(ui.find("Download installers…").is_err());
    ui.click("Verify files").unwrap();
    ui.click("Check for update").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(messages.iter().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Check(id, false)) if id == "epic-Alpha"
    )));
    assert!(messages.iter().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::CheckUpdate(id)) if id == "epic-Alpha"
    )));
}

/// Signed in to Epic alone, an Epic game's update found is applied by itself.
#[test]
fn an_epic_games_update_is_applied_without_a_gog_account() {
    use crate::updates::UpdatesMsg;
    let mut app = epic_app();
    app.account = None;
    app.updates.on = true;
    app.installs.insert("epic-Alpha".into(), epic_install());
    let _ = app.update(Message::Updates(UpdatesMsg::Checked(vec![(
        "epic-Alpha".into(),
        Ok(Some(slatty_core::maintenance::UpdateCheck {
            installed_version: "1.0-[FAKE]".into(),
            available_version: "2.0-[FAKE]".into(),
            available_build: "epic:2.0-[FAKE]".into(),
        })),
    )])));
    assert_eq!(app.updates.current.as_deref(), Some("epic-Alpha"));
    assert!(app.maintenance["epic-Alpha"].busy);
}

fn epic_install() -> slatty_core::install::Install {
    use slatty_core::install::{Install, Platform};
    use slatty_core::runner::Runner;
    Install {
        umu_id: None,
        game_id: "epic-Alpha".into(),
        title: "[FAKE] Alpha".into(),
        platform: Platform::Windows,
        path: "/games/Alpha".into(),
        client_id: None,
        runner: Runner::Umu {
            proton: "/proton/[FAKE]".into(),
            prefix: "/prefixes/epic-Alpha".into(),
        },
        isolated: true,
    }
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

/// An installed Epic game is played as a GOG one; GOG does not keep its play time, which comes
/// from the sessions played here.
#[test]
fn an_installed_epic_game_is_played() {
    let mut app = epic_app();
    app.installs.insert("epic-Alpha".into(), epic_install());
    assert!(app.is_installed("epic-Alpha"));
    app.playtime.insert(
        "epic-Alpha".into(),
        slatty_core::session::Playtime {
            seconds: 5400,
            last_played: 1_791_500_000,
        },
    );
    assert_eq!(app.played_seconds("epic-Alpha"), 5400);
    let _ = app.update(Message::Select("epic-Alpha".into()));
    let mut ui = render(&app);
    assert!(ui.click("Play").is_ok());
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Play(id) if id == "epic-Alpha"))
    );
}

/// An Epic game's cloud saves are Epic's: its page has their card, and they are checked while
/// signed in to Epic, without GOG.
#[test]
fn an_epic_games_cloud_saves_are_checked_with_epic() {
    use crate::cloud::CloudRequest;
    let mut app = epic_app();
    app.installs.insert("epic-Alpha".into(), epic_install());
    assert!(app.may_sync_cloud("epic-Alpha"));
    let _ = app.update(Message::Select("epic-Alpha".into()));
    assert!(render(&app).find("Cloud saves").is_ok());

    app.account = None;
    let _ = app.request_cloud("epic-Alpha".into(), CloudRequest::Check);
    assert!(app.cloud["epic-Alpha"].busy, "checked with Epic's account");

    app.cloud.clear();
    app.epic.account = None;
    let _ = app.request_cloud("epic-Alpha".into(), CloudRequest::Check);
    assert!(!app.cloud.contains_key("epic-Alpha"), "signed out of Epic");
}

/// An installed Epic game's settings, from its page: its Proton, controller and isolation, and
/// nothing of GOG's, which is not asked for its languages and DLC.
#[test]
fn an_installed_epic_games_settings_change_its_proton_and_isolation() {
    let mut app = epic_app();
    app.installs.insert("epic-Alpha".into(), epic_install());
    let _ = app.update(Message::Select("epic-Alpha".into()));
    let mut ui = render(&app);
    for x in (0..80).map(|i| SIZE.width - 4.0 - i as f32 * 4.0) {
        for y in (0..15).map(|i| 4.0 + i as f32 * 4.0) {
            ui.point_at(iced::Point::new(x, y));
            let _ = ui.simulate(iced_test::simulator::click());
        }
    }
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::OpenPanel(Panel::GameSettings))),
        "settings on its page"
    );
    let _ = app.update(Message::OpenPanel(Panel::GameSettings));
    assert!(
        app.maintenance
            .get("epic-Alpha")
            .is_none_or(|m| !m.busy && m.lines.is_empty()),
        "GOG not asked"
    );
    let mut ui = render(&app);
    assert!(ui.find("Used from the next launch.").is_ok(), "Proton");
    assert!(
        ui.find(
            "Turn on if the game does not recognize your controller. For UMU-Proton, GE-Proton \
             and Proton-CachyOS. Used from the next launch."
        )
        .is_ok()
    );
    assert!(
        ui.find(
            "The game sees its own folder and a home folder of its own, not your files. Used \
             from the next launch."
        )
        .is_ok(),
        "isolation"
    );
    assert!(ui.find("Game version").is_err(), "nothing of GOG's");
    assert!(ui.find("Language").is_err());
}

/// Its cover's settings button opens them over the library, as a GOG game's does.
#[test]
fn an_installed_epic_games_cover_opens_its_settings() {
    let mut app = epic_app();
    app.installs.insert("epic-Alpha".into(), epic_install());
    app.library.retain(|g| g.id == "epic-Alpha");
    app.lists = Default::default();
    let mut ui = render(&app);
    let grid = ui
        .find(iced_test::selector::id(crate::library::GRID))
        .unwrap()
        .bounds();
    for x in (0..30).map(|i| grid.x + 4.0 + i as f32 * 6.0) {
        for y in (0..50).map(|i| grid.y + 4.0 + i as f32 * 6.0) {
            ui.point_at(iced::Point::new(x, y));
            let _ = ui.simulate(iced_test::simulator::click());
        }
    }
    assert!(
        ui.into_messages().any(
            |m| matches!(m, Message::OpenDialog(id, Panel::GameSettings) if id == "epic-Alpha")
        )
    );
}

/// Downloads waiting for Epic say so, not GOG.
#[test]
fn downloads_waiting_for_epic_name_epic() {
    let mut app = epic_app();
    let _ = app.wait_for_gog("epic-Alpha", "[FAKE] network error".into());
    app.page = Page::Downloads;
    assert!(
        render(&app)
            .find(
                "Epic cannot be reached ([FAKE] network error). Downloads go on by themselves \
                 once it answers."
            )
            .is_ok()
    );
}
