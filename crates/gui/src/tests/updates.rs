use super::*;
use crate::play::{PlayMsg, PlayState};
use crate::updates::UpdatesMsg;

/// Games 3 and 5 have an update; 4 is up to date and 6 could not be checked.
fn found() -> Message {
    let update = |from: &str, to: &str, build: &str| {
        Ok(Some(slatty_core::maintenance::UpdateCheck {
            installed_version: from.into(),
            available_version: to.into(),
            available_build: build.into(),
        }))
    };
    Message::Updates(UpdatesMsg::Checked(vec![
        ("3".into(), update("1.0", "1.1", "b31")),
        ("4".into(), Ok(None)),
        ("5".into(), update("2.0", "2.1", "b51")),
        ("6".into(), Err("GOG did not answer".into())),
    ]))
}

/// A game busy elsewhere when its turn comes (played from the command line, or from before the
/// interface was restarted) is left for the next check, and one updated elsewhere since its check
/// is not updated again: neither shows an error.
#[test]
fn a_game_busy_or_updated_elsewhere_is_passed_over_without_an_error() {
    let mut app = auto_app();
    // Folders of its own: the record it writes would show in other tests.
    let dirs = slatty_core::paths::Dirs::under(&super::root().join("updated-elsewhere"));
    app.core.as_mut().unwrap().dirs = dirs.clone();
    let _running = slatty_core::lock::game(&dirs, "3").unwrap();
    let mut record = slatty_core::installer::InstallRecord {
        build_id: "b51".into(),
        version: "2.1".into(),
        language: "en-US".into(),
        path: None,
        dlcs: Vec::new(),
        setup_build: None,
        setup_failures: 0,
        files: Vec::new(),
    };
    record.save(&dirs, "5").unwrap();
    let _ = app.update(found());
    assert!(app.updates.current.is_none(), "{:?}", app.updates.current);
    assert!(app.updates.waiting.is_empty());
    assert!(
        app.notice.is_none(),
        "{:?}",
        app.notice.as_ref().map(|n| &n.text)
    );
    assert!(!app.maintenance["5"].update_available, "no longer said");

    // Still behind once the game is free: found again, and updated.
    drop(_running);
    record.build_id = "b50".into();
    record.save(&dirs, "5").unwrap();
    let _ = app.update(found());
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

fn auto_app() -> App {
    let mut app = library_app();
    app.updates.on = true;
    app
}

#[test]
fn games_found_with_an_update_are_updated_one_after_the_other() {
    let mut app = auto_app();
    let _ = app.update(found());
    assert_eq!(app.updates.current.as_deref(), Some("3"));
    assert_eq!(app.updates.waiting, ["5"]);
    assert!(app.maintenance["3"].busy, "applied as Update now would");
    assert!(app.maintenance["6"].lines[0].contains("GOG did not answer"));
    assert!(!app.maintenance.contains_key("4"));

    // Its page shows it updating instead of Play.
    open(&mut app, "3", None);
    assert!(render(&app).find("Updating 0%").is_ok());
    app.selected = None;
    app.page = Page::Downloads;
    {
        let mut ui = render(&app);
        assert!(ui.find("Updates · 2").is_ok());
        assert!(ui.find("[FAKE] Game 5").is_ok() && ui.find("Waiting").is_ok());
        snapshot(&mut ui, "downloads-updates");
    }

    // Done: said so, and the next one starts.
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Ok("Now at 1.1: 2 file(s) downloaded, 0 removed.".into()),
    )));
    assert_eq!(
        toasted(&app).last(),
        Some(&(
            "[FAKE] Game 3 updated".to_string(),
            "Now at 1.1: 2 file(s) downloaded, 0 removed.".to_string()
        ))
    );
    assert_eq!(app.updates.current.as_deref(), Some("5"));
    assert!(app.updates.waiting.is_empty());

    // Paused: it waits until asked, and nothing else starts in its place.
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "5".into(),
        Err(None),
    )));
    assert!(app.updates.current.is_none());
}

#[test]
fn updates_wait_while_a_game_runs_and_start_once_it_ends() {
    let mut app = auto_app();
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "4".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = app.update(found());
    assert!(app.updates.current.is_none());
    assert_eq!(app.updates.waiting, ["3", "5"]);
    let _ = app.update(Message::Playing(PlayMsg::Done(Ok(()))));
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

/// Play pressed while its game's folder is read, or while it asks how to start: an update found
/// meanwhile waits, rather than starting on that game and leaving the press without effect.
#[test]
fn updates_wait_for_a_play_pressed_before_they_start() {
    let mut app = auto_app();
    let _ = app.update(Message::Play("3".into()));
    let _ = app.update(found());
    assert!(app.updates.current.is_none());
    let _ = app.update(Message::LaunchOptions(Vec::new(), Some("3".into())));
    assert!(app.play.as_ref().is_some_and(|p| p.running), "it starts");
    game_ends(&mut app);
    assert_eq!(app.updates.current.as_deref(), Some("3"));

    let mut app = auto_app();
    app.launch_options.insert(
        "3".into(),
        vec!["[FAKE] Game 3".into(), "Configuration Tool".into()],
    );
    let _ = app.update(Message::Play("3".into()));
    let _ = app.update(found());
    assert!(app.updates.current.is_none(), "asking how to start");
    let _ = app.update(Message::CancelLaunch);
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

fn playing(app: &mut App) {
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "4".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
}

fn game_ends(app: &mut App) {
    let _ = app.update(Message::Playing(PlayMsg::Done(Ok(()))));
}

#[test]
fn a_waiting_game_the_user_changes_is_not_updated_after_all() {
    use slatty_core::maintenance::Change;
    let mut app = auto_app();
    playing(&mut app);
    let _ = app.update(found());
    // Game 3 moved to an older version, game 5 uninstalled, while they waited.
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Apply(
        "3".into(),
        Change::Build("old".into()),
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Ok("Now at 1.0.".into()),
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Uninstalled(
        "5".into(),
        Ok(Default::default()),
    )));
    assert!(app.updates.waiting.is_empty(), "{:?}", app.updates.waiting);
    game_ends(&mut app);
    assert!(app.updates.current.is_none());
}

#[test]
fn a_game_held_back_or_paused_since_its_check_is_not_updated() {
    let mut app = auto_app();
    let db = app.core.as_ref().unwrap().db.clone();
    playing(&mut app);
    let _ = app.update(found());
    // Chosen elsewhere (the command line, another window) once the check had answered.
    slatty_core::settings::set_held_back(&db, "3", true).unwrap();
    game_ends(&mut app);
    assert_eq!(app.updates.current.as_deref(), Some("5"));
    assert!(!app.maintenance["3"].busy);
}

#[test]
fn pausing_the_update_running_starts_no_other() {
    let mut app = auto_app();
    let _ = app.update(found());
    assert_eq!(app.updates.current.as_deref(), Some("3"));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Err(None),
    )));
    assert!(app.updates.current.is_none());
    assert!(
        !app.maintenance["5"].busy,
        "game 5 waits for the next check"
    );
}

#[test]
fn updates_wait_for_a_cloud_sync() {
    let mut app = auto_app();
    app.cloud.entry("4".into()).or_default().busy = true;
    let _ = app.update(found());
    assert!(app.updates.current.is_none());
    app.cloud.get_mut("4").unwrap().busy = false;
    let _ = app.update(Message::DismissNotice);
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

#[test]
fn updates_wait_while_offline_installers_download() {
    use crate::offline::{OfflineMsg, entry};
    let mut app = auto_app();
    super::offline::queued_job(&app, "8");
    app.queue = vec![entry("8")];
    let _ = app.start_next();
    assert!(app.offline_run.is_some());
    let _ = app.update(found());
    assert!(app.updates.current.is_none());
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "8".into(),
        Ok(Vec::new()),
    )));
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

#[test]
fn updates_found_belong_to_the_account_and_the_switch_that_found_them() {
    // Turned off while the check ran: its answer is not applied.
    let mut app = auto_app();
    app.updates.on = false;
    let _ = app.update(found());
    assert!(app.updates.waiting.is_empty() && app.updates.current.is_none());
    // Signed out: nothing found for that account is left to update under the next one.
    let mut app = auto_app();
    playing(&mut app);
    let _ = app.update(found());
    app.updates.checking = true;
    app.logged_out();
    assert!(app.updates.waiting.is_empty());
    assert!(!app.updates.checking, "the next sign-in checks again");
    // Signing in checks at once, rather than six hours later.
    let _ = app.logged_in(Ok((
        slatty_core::account::AccountInfo {
            user_id: "0".into(),
            username: "testeur".into(),
        },
        None,
    )));
    assert!(app.updates.checking);
}

#[test]
fn automatic_updates_are_turned_off_in_settings() {
    let mut app = auto_app();
    let db = app.core.as_ref().unwrap().db.clone();
    app.page = Page::Settings;
    assert!(render(&app).find("Update games automatically").is_ok());
    // A check asked once off finds nothing to do.
    let _ = app.update(Message::Updates(UpdatesMsg::Toggle(false)));
    assert!(!slatty_core::settings::auto_update(&db).unwrap());
    let _ = app.update(Message::Updates(UpdatesMsg::Check));
    assert!(!app.updates.checking);
    // Found while on, then turned off before its turn: not applied.
    let mut busy = library_app();
    busy.updates.on = true;
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    busy.play = Some(PlayState {
        game_id: "4".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = busy.update(found());
    let _ = busy.update(Message::Updates(UpdatesMsg::Toggle(false)));
    assert!(busy.updates.waiting.is_empty());
    let _ = busy.update(Message::Playing(PlayMsg::Done(Ok(()))));
    assert!(busy.updates.current.is_none());
}
