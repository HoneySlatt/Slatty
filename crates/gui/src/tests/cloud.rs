use super::*;

/// A game not installed says it has no cloud saves only once GOG was asked: its play time alone,
/// read first, says nothing of them.
#[test]
fn a_game_not_installed_has_cloud_saves_until_gog_says_otherwise() {
    let mut app = library_app();
    let _ = app.update(Message::PlaytimesFetched(vec![("5".into(), 90)]));
    let _ = app.update(Message::Select("5".into()));
    settle(&mut app);
    {
        let mut ui = render(&app);
        assert!(ui.find("Not supported by this game").is_err());
        assert!(ui.find("Synced once installed").is_ok());
    }
    app.overview.get_mut("5").unwrap().checked = true;
    assert!(render(&app).find("Not supported by this game").is_ok());
}

/// Whether the game was started once (its prefix made) comes with the cloud check, made off the
/// interface thread: the page reads nothing of the prefix as it draws, which a network drive
/// waking up would freeze at every frame.
#[test]
fn the_cloud_card_reads_nothing_of_the_prefix_as_it_draws() {
    use crate::cloud::{CloudRequest, CloudResult, CloudStatus};
    let mut app = library_app();
    let _ = app.update(Message::Select("3".into()));
    settle(&mut app);
    let checked = |launched| CloudResult {
        lines: Vec::new(),
        locations: Vec::new(),
        conflicts: false,
        status: CloudStatus::Pending(2),
        summary: None,
        launched,
    };
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Ok(checked(true)),
    ));
    assert!(render(&app).find("2 file(s) to sync").is_ok());
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Ok(checked(false)),
    ));
    assert!(
        render(&app)
            .find("2 file(s) downloaded at first launch")
            .is_ok()
    );
    // A check that fails (offline after a session) leaves what was known of the game as it was:
    // started before, not a game whose saves come at its first launch.
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Ok(checked(true)),
    ));
    let _ = app.update(Message::CloudDone(
        "3".into(),
        CloudRequest::Check,
        Err("[FAKE] offline".into()),
    ));
    assert!(app.cloud["3"].launched);
}

/// GOG keeps no cloud saves of a Linux build: a check asked for one (as the end of a session asks,
/// its page open) asks GOG nothing and leaves Play available.
#[test]
fn a_linux_build_asks_gog_for_no_cloud_saves() {
    use crate::cloud::CloudRequest;
    let mut app = library_app();
    app.installs.get_mut("3").unwrap().platform = Platform::Linux;
    open(&mut app, "3", None);
    let _ = app.update(Message::Cloud("3".into(), CloudRequest::Check));
    assert!(!app.cloud.contains_key("3"));
    assert!(app.can_play("3"));
}

/// An install ending with its page open has its cloud saves checked at once, as opening the page
/// of an installed game does, rather than left "Not checked yet"; a Linux build's are not.
#[test]
fn an_install_ending_with_its_page_open_has_its_cloud_saves_checked() {
    let mut app = library_app();
    let installed = |app: &App, id: &str, platform| Install {
        game_id: id.into(),
        title: format!("[FAKE] Game {id}"),
        platform,
        ..app.installs["3"].clone()
    };
    let _ = app.update(Message::Select("5".into()));
    settle(&mut app);
    let windows = installed(&app, "5", Platform::Windows);
    let _ = app.update(Message::Install(InstallMsg::Done("5".into(), Ok(windows))));
    assert!(render(&app).find("Checking…").is_ok());

    let _ = app.update(Message::Select("6".into()));
    settle(&mut app);
    let linux = installed(&app, "6", Platform::Linux);
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Ok(linux))));
    assert!(!app.cloud.contains_key("6"));
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
        launched: false,
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
