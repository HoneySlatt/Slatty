use super::*;

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
