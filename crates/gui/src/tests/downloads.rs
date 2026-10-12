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

/// A game GOG could not be reached for, back at the head of the queue, cancels a drag: the row held
/// would no longer be the one dropped.
#[test]
fn a_game_back_at_the_head_of_the_queue_cancels_the_drag() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into(), "7".into()];
    app.auto_resume = Some("4".into());
    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(2)));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drag(0.0)));
    let _ = app.update(Message::Install(InstallMsg::Unreachable(
        "4".into(),
        "[FAKE] network error".into(),
    )));
    let _ = app.update(Message::Downloads(DownloadsMsg::Drop));
    assert_eq!(app.queue, ["4", "5", "6", "7"]);
}

/// A game the command line is installing or updating is not queued over it: its job (folder,
/// build, an update half applied) is left as it is.
#[test]
fn a_game_busy_elsewhere_is_not_queued_over() {
    use slatty_core::installer::InstallJob;
    let mut app = library_app();
    let dirs = slatty_core::paths::Dirs::under(&super::root().join("queue-busy"));
    app.core.as_mut().unwrap().dirs = dirs.clone();
    let db = app.core.as_ref().unwrap().db.clone();
    ready_to_install(&mut app, "5");
    let busy = slatty_core::lock::game(&dirs, "5").unwrap();
    let _ = app.enqueue("5".into());
    assert!(app.queue.is_empty());
    assert!(
        InstallJob::load(&db, "5").unwrap().is_none(),
        "nothing written"
    );
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    assert!(
        matches!(app.install_views.get("5"), Some(InstallView::Ready(_))),
        "still ready to queue once free"
    );
    drop(busy);
    let _ = app.enqueue("5".into());
    assert_eq!(app.queue, ["5"]);
}

/// Offline at start, the queue waits for GOG, every game in its place, rather than each failing in
/// turn; it is tried again by itself, or at once with Try now.
#[test]
fn the_queue_waits_for_gog_rather_than_fail_game_after_game() {
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into(), "7".into()];
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    let _ = app.update(Message::Install(InstallMsg::Unreachable(
        "5".into(),
        "[FAKE] network error".into(),
    )));
    assert_eq!(app.queue, ["5", "6", "7"], "each in its place");
    assert!(app.auto_resume.is_none());
    assert!(app.install_views.is_empty(), "none failed");
    let _ = app.start_next();
    assert!(app.auto_resume.is_none(), "the queue waits");

    app.page = Page::Downloads;
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(
            ui.find(
                "GOG cannot be reached ([FAKE] network error). Downloads go on by themselves once \
                 it answers."
            )
            .is_ok()
        );
        snapshot(&mut ui, "downloads-waiting");
        ui.click("Try now").unwrap();
        ui.into_messages().collect()
    };
    for m in messages {
        let _ = app.update(m);
    }
    assert_eq!(app.auto_resume.as_deref(), Some("5"), "tried again");
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.queue, ["6", "7"]);
}

/// The next install, being planned again (its choices changed), starts once planned rather than
/// being planned a second time; a cut-off download discarded meanwhile does not hold the queue.
#[test]
fn a_game_being_planned_is_not_planned_twice_to_start() {
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into()];
    app.install_views.insert("5".into(), InstallView::Planning);
    assert_eq!(app.start_next().units(), 0, "planned once");
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));

    let mut app = library_app();
    app.interrupted = vec![("5".into(), crate::Interrupted::Download)];
    app.queue = vec!["6".into()];
    // Its discard runs.
    app.install_views.insert("5".into(), InstallView::Planning);
    let _ = app.start_next();
    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    assert_eq!(app.auto_resume.as_deref(), Some("6"), "the queue goes on");
}

/// A cut-off download being planned to resume by itself, then discarded: its plan answering
/// before the discard ends does not start it; and a download started meanwhile keeps its view,
/// its Pause and Cancel with it.
#[test]
fn a_download_discarded_while_planned_to_resume_is_not_started_by_its_plan() {
    let mut app = library_app();
    app.interrupted = vec![("5".into(), crate::Interrupted::Download)];
    app.install_views.insert("5".into(), InstallView::Planning);
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    let _ = app.update(Message::Install(InstallMsg::Discard("5".into())));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(
        !matches!(
            app.install_views.get("5"),
            Some(InstallView::Running { .. })
        ),
        "not started"
    );
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    let _ = app.update(Message::Install(InstallMsg::Discarded(
        "5".into(),
        Err("[FAKE] busy".into()),
    )));
    assert!(
        matches!(
            app.install_views.get("5"),
            Some(InstallView::Running { .. })
        ),
        "still shown"
    );
}

/// A download cut off because GOG stopped answering is left to resume, and holds the queue until
/// GOG answers again, rather than the next install failing the same way at once.
#[test]
fn a_download_cut_off_by_the_network_waits_and_resumes_first() {
    use slatty_core::installer::{DOWNLOADING, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    for id in ["5", "6"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    // What the core leaves behind when GOG cannot be reached.
    InstallJob {
        game_id: "5".into(),
        build_id: "b1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 5".into(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Err(Some(crate::install::Failure(
            "[FAKE] network error".into(),
            true,
        ))),
    )));
    assert!(
        matches!(app.install_views.get("6"), Some(InstallView::Queued(_))),
        "the next install waits"
    );
    assert_eq!(app.queue, ["6"]);
    assert!(app.auto_resume.is_none(), "not tried again at once");
    assert!(app.downloads_waiting.is_some());
    let _ = app.update(Message::Install(InstallMsg::Retry));
    assert_eq!(app.auto_resume.as_deref(), Some("5"), "resumed first");
}

/// Each message rebuilds the page: moving the pointer over the queue sends none, unless a row is
/// being moved.
#[test]
fn the_pointer_over_the_queue_sends_nothing_until_a_row_is_moved() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into()];
    app.page = Page::Downloads;
    let moves = |app: &App| {
        let mut ui = render(app);
        let row = ui.find("[FAKE] Game 6").unwrap().bounds().center();
        for x in [row.x, row.x + 5.0] {
            let position = iced::Point::new(x, row.y);
            ui.point_at(position);
            let _ = ui.simulate([iced::Event::Mouse(iced::mouse::Event::CursorMoved {
                position,
            })]);
        }
        ui.into_messages()
            .filter(|m| matches!(m, Message::Downloads(DownloadsMsg::Drag(_))))
            .count()
    };
    assert_eq!(moves(&app), 0);
    let _ = app.update(Message::Downloads(DownloadsMsg::Grab(1)));
    assert!(moves(&app) > 0, "the row follows the pointer");
}

/// Whether a queued game follows updates is decided as it is queued: by its turn GOG may have
/// published a newer build, which would make the build chosen look older than it was.
#[test]
fn a_queued_install_follows_updates_as_chosen() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    for (id, newest) in [("5", true), ("6", true), ("7", false)] {
        ready_to_install(&mut app, id);
        if let Some(InstallView::Ready(info)) = app.install_views.get_mut(id) {
            info.newest = newest;
        }
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    assert_eq!(app.queue, ["6", "7"]);
    assert!(!slatty_core::settings::held_back(&db, "6").unwrap());
    assert!(slatty_core::settings::held_back(&db, "7").unwrap());
}

/// On a very wide window a download's controls stay near its title.
#[test]
fn downloads_keep_a_readable_width_on_a_wide_window() {
    let mut app = library_app();
    for id in ["5", "6"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    app.page = Page::Downloads;
    app.window = Size::new(3840.0, 1080.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    snapshot(&mut ui, "downloads-wide");
    // The share done, at the end of the progress bar.
    let done = ui.find("0 %").unwrap().bounds();
    assert!(done.x + done.width < 1700.0, "{done:?}");
}

/// At the smallest window, a long title stays inside its row, two lines at most: queued, in Steam
/// and completed alike.
#[test]
fn a_long_title_stays_inside_its_download_row() {
    use crate::downloads::ROW_HEIGHT;
    use iced_test::selector::Candidate;
    use slatty_core::steam::local::{Installed, Update};
    let long = |what: &str| format!("[FAKE] {what} {}", "with a very long title ".repeat(8));
    let mut app = library_app();
    app.window = Size::new(800.0, 560.0);
    app.page = Page::Downloads;
    app.library[4].title = long("Queued");
    app.queue = vec!["5".into()];
    app.library[5].title = long("Completed");
    app.completed = vec![("6".into(), 1 << 30)];
    app.steam.on = true;
    app.steam.games = vec![LibraryGame {
        title: long("In Steam"),
        ..fake_game("steam-620", "")
    }];
    app.steam.installed.insert(
        "steam-620".into(),
        Installed {
            app_id: 620,
            name: "[FAKE] 620".into(),
            dir: "/games/steamapps/common/620".into(),
            size: 0,
            ready: false,
            update: Some(Update {
                done: 1_000,
                total: 4_000,
                stage_total: 0,
                started: true,
                paused: false,
            }),
        },
    );
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    let (mut rows, mut titles) = (Vec::new(), Vec::new());
    let _ = ui.find(|candidate: Candidate<'_>| -> Option<()> {
        match candidate {
            Candidate::Container { bounds, .. } if bounds.height == ROW_HEIGHT => rows.push(bounds),
            Candidate::Text {
                content, bounds, ..
            } if content.contains("with a very long title") => titles.push(bounds),
            _ => {}
        }
        None
    });
    assert_eq!(titles.len(), 3, "{titles:?}");
    for title in titles {
        assert!(title.height <= 2.0 * 16.0 * 1.3 + 0.5, "{title:?}");
        assert!(rows.iter().any(|row| title.is_within(row)), "{title:?}");
    }
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
        assert!(
            ui.find("Listing files…").is_ok(),
            "no size before the files are listed"
        );
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
            network_bytes: 3 << 30,
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

/// A cut-off download GOG refuses to plan for good (a DLC no longer owned) waits for Resume or
/// Discard rather than being tried again and again, and does not hold the queue meanwhile.
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
        Err("[FAKE] DLC not owned".into()),
    )));
    assert_eq!(app.interrupted, [("5".to_string(), Interrupted::Failed)]);
    assert!(app.queue.is_empty(), "the queue goes on");
    assert_eq!(app.auto_resume.as_deref(), Some("6"));
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
        proton: None,
    }
    .save(&db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Err(Some(crate::install::Failure(
            "not enough disk space".into(),
            false,
        ))),
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

/// A download stopped by something else than GOG out of reach fails, even when its job still looks
/// cut off (the command line installing the game holds it): waiting for GOG would start it again
/// every minute, each time checking every file already downloaded.
#[test]
fn a_download_stopped_by_another_error_fails_rather_than_wait_for_gog() {
    use slatty_core::installer::{DOWNLOADING, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    for id in ["5", "6"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    InstallJob {
        game_id: "5".into(),
        build_id: "b1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 5".into(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Err(Some(crate::install::Failure(
            "[FAKE] another operation on this game is running".into(),
            false,
        ))),
    )));
    assert!(app.downloads_waiting.is_none(), "GOG is not waited for");
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
        "the queue goes on"
    );
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    app.interrupted.retain(|(id, _)| id == "5");
    let _ = app.start_next();
    assert!(app.auto_resume.is_none(), "not started again by itself");
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

/// The Proton build chosen for a queued install is kept with it, for a start after a restart, when
/// the default may be another.
#[test]
fn a_queued_install_keeps_the_proton_chosen_for_it() {
    use slatty_core::installer::InstallJob;
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    ready_to_install(&mut app, "6");
    if let Some(InstallView::Ready(info)) = app.install_views.get_mut("6") {
        info.proton = Some("/proton/[FAKE]-Other".into());
    }
    let _ = app.update(Message::Install(InstallMsg::Start("6".into())));
    assert_eq!(app.queue, ["6"]);
    assert_eq!(
        InstallJob::load(&db, "6").unwrap().unwrap().proton,
        Some(PathBuf::from("/proton/[FAKE]-Other"))
    );
}

/// After a restart with no default Proton, a queued Windows install cannot start: the ones after
/// it still do.
#[test]
fn an_install_that_cannot_start_does_not_hold_up_the_queue() {
    let mut app = library_app();
    for (id, proton) in [("6", None), ("7", Some("/proton/GE-Proton".into()))] {
        let info = crate::install::PlanInfo {
            title: format!("[FAKE] Game {id}"),
            directory: format!("Game {id}"),
            proton,
            ..fake_plan()
        };
        app.install_views
            .insert(id.into(), InstallView::Queued(info));
        app.queue.push(id.into());
    }
    let _ = app.start_next();
    assert!(matches!(
        app.install_views.get("7"),
        Some(InstallView::Running { .. })
    ));
    assert!(app.notice.is_some(), "says why the other did not start");
}
