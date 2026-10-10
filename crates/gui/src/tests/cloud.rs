use super::*;

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
