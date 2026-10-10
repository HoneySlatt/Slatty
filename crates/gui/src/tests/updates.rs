use super::*;
use crate::play::{PlayMsg, PlayState};
use crate::updates::UpdatesMsg;

/// Games 3 and 5 have an update; 4 is up to date and 6 could not be checked.
fn found() -> Message {
    Message::Updates(UpdatesMsg::Checked(vec![
        ("3".into(), Ok(Some("1.0 → 1.1".into()))),
        ("4".into(), Ok(None)),
        ("5".into(), Ok(Some("2.0 → 2.1".into()))),
        ("6".into(), Err("GOG did not answer".into())),
    ]))
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
    assert!(render(&app).find("Updating 0 %").is_ok());
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
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|n| !n.error && n.text.starts_with("[FAKE] Game 3 updated."))
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
    });
    let _ = app.update(found());
    assert!(app.updates.current.is_none());
    assert_eq!(app.updates.waiting, ["3", "5"]);
    let _ = app.update(Message::Playing(PlayMsg::Done(Ok(()))));
    assert_eq!(app.updates.current.as_deref(), Some("3"));
}

fn playing(app: &mut App) {
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "4".into(),
        log: Vec::new(),
        stop,
        running: true,
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
        Ok(Vec::new()),
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
    });
    let _ = busy.update(found());
    let _ = busy.update(Message::Updates(UpdatesMsg::Toggle(false)));
    assert!(busy.updates.waiting.is_empty());
    let _ = busy.update(Message::Playing(PlayMsg::Done(Ok(()))));
    assert!(busy.updates.current.is_none());
}
