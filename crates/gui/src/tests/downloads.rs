use super::*;

#[test]
fn a_queue_change_cancels_the_drag_before_the_next_render() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into()];
    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(1)));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drag(0.0)));
    let _ = app.start_next();
    assert_eq!(app.queue_order(), vec!["6"]);
    let _ = app.update(Message::Downloads(DownloadsMsg::Drop));
    assert_eq!(app.queue, vec!["6"]);

    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(0)));
    let _ = app.update(Message::Downloads(DownloadsMsg::Remove("6".into())));
    assert!(app.queue_order().is_empty());
    let _ = app.update(Message::Downloads(DownloadsMsg::Drop));
    assert!(app.queue.is_empty());
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
        isolated: true,
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

/// After a restart the queue is planned again before it starts: the queue going on meanwhile (a
/// download discarded) does not take a second install out of it, which would never start.
#[test]
fn an_install_being_planned_to_start_holds_the_queue() {
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into()];
    let _ = app.start_next();
    let _ = app.start_next();
    assert_eq!(app.queue, ["6"], "one install starts at a time");
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.queue, ["6"]);
}

/// A download that fails (a full disk, a folder it cannot write to) waits for Resume or Discard:
/// started again at once, it would fail again, and again, asking GOG for its plan each time. The
/// queue goes on meanwhile.
#[test]
fn a_failed_download_is_not_started_again_by_itself() {
    use slatty_core::installer::{FAILED, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    for id in ["5", "6"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    // What the core leaves behind when the download fails.
    InstallJob {
        game_id: "5".into(),
        build_id: "b1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 5".into(),
        state: FAILED.into(),
        dlcs: Vec::new(),
    }
    .save(&db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Err(Some("not enough disk space".into())),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Failed(_))
    ));
    assert_eq!(
        app.interrupted,
        [("5".to_string(), crate::Interrupted::Failed)]
    );
    assert!(
        matches!(
            app.install_views.get("6"),
            Some(InstallView::Running { .. })
        ),
        "the queue goes on, not the failed download"
    );
    // Nor at the next start: only a download cut off resumes by itself.
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    app.interrupted.retain(|(id, _)| id == "5");
    let _ = app.start_next();
    assert!(app.auto_resume.is_none());
    {
        let mut ui = render(&app);
        assert!(
            ui.find("Download failed. It resumes where it stopped.")
                .is_ok()
        );
    }
}

/// While a game downloads, the install dialog of another one puts it in the queue, rather than
/// waiting with its button greyed out until the download is done.
#[test]
fn an_install_chosen_during_a_download_goes_to_the_queue_from_its_dialog() {
    let mut app = library_app();
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    ready_to_install(&mut app, "6");
    let _ = app.update(Message::OpenDialog("6".into(), Panel::Install));
    let mut ui = render(&app);
    assert!(
        ui.find("[FAKE] Game 5 is downloading; this one waits in the queue until it is done.")
            .is_ok()
    );
    ui.click("Add to queue").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert_eq!(app.queue, ["6"]);
    assert!(app.dialog.is_none(), "the dialog closes once queued");
}
