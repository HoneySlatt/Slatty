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

/// GOG out of reach is told once, as downloads start waiting for it: not again at each retry,
/// every minute while it stays out of reach (a desktop notification each, the window behind).
#[test]
fn downloads_waiting_for_gog_are_told_once_not_at_each_retry() {
    use slatty_core::installer::{DOWNLOADING, InstallJob};
    let mut app = library_app();
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
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
    .save(&app.core.as_ref().unwrap().db)
    .unwrap();
    let unreachable = || crate::install::Failure("[FAKE] network error".into(), true);
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Err(Some(unreachable())),
    )));
    let told = |app: &App| {
        toasted(app)
            .iter()
            .filter(|(title, _)| title == "Downloads waiting for GOG")
            .count()
    };
    assert_eq!(told(&app), 1);
    for _ in 0..3 {
        let _ = app.update(Message::Install(InstallMsg::Retry));
        assert_eq!(app.auto_resume.as_deref(), Some("5"));
        let _ = app.update(Message::Install(InstallMsg::Unreachable(
            "5".into(),
            unreachable().0,
        )));
    }
    assert!(app.downloads_waiting.is_some());
    assert_eq!(told(&app), 1, "told once");
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
    let done = ui.find("0%").unwrap().bounds();
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
    use slatty_core::installer::{DOWNLOADING, FAILED, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
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
    // Nor tried again at the next start: only when asked.
    assert_eq!(InstallJob::load(&db, "5").unwrap().unwrap().state, FAILED);
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

/// Game 3, installed, started from its page.
fn play_3(app: &mut App) {
    app.launch_options.insert("3".into(), Vec::new());
    let _ = app.update(Message::Play("3".into()));
    assert!(app.play.as_ref().is_some_and(|p| p.running));
}

fn install_6_paused(app: &App) -> bool {
    matches!(app.install_views.get("6"), Some(InstallView::Running { cancel, .. }) if cancel.is_cancelled())
}

#[test]
fn a_game_pauses_the_download_running_until_it_closes() {
    use super::offline::{installing_6, paused_install_job};
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    app.queue = vec!["7".into()];
    play_3(&mut app);
    assert!(install_6_paused(&app));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(app.queue, ["6", "7"], "first in the queue");
    assert!(app.interrupted.is_empty(), "not held as paused");
    assert!(app.installing().is_none());
    assert_eq!(app.auto_resume, None, "nothing starts while the game runs");

    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    assert!(
        ui.find(
            "Paused while a game runs: the queue goes on once it closes. Download now starts one \
             anyway."
        )
        .is_ok()
    );
    snapshot(&mut ui, "downloads-while-playing");
    drop(ui);

    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert_eq!(
        app.auto_resume.as_deref(),
        Some("6"),
        "planned, then started"
    );
    assert_eq!(app.queue, ["7"]);
}

/// What the launcher paused goes on by itself, even while a download paused on request holds the
/// rest of the queue.
#[test]
fn a_download_the_launcher_paused_goes_on_beside_one_paused_on_request() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::{OfflineMsg, entry};
    // Paused for a game: it goes on once the game closes.
    let mut app = library_app();
    app.interrupted
        .push(("8".into(), crate::Interrupted::Paused));
    installing_6(&mut app);
    paused_install_job(&app);
    play_3(&mut app);
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(app.queue, ["6"]);
    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert_eq!(
        app.auto_resume.as_deref(),
        Some("6"),
        "planned, then started"
    );

    // Paused for Download now: it goes on once the download started is done.
    let mut app = library_app();
    app.interrupted
        .push(("8".into(), crate::Interrupted::Paused));
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6"]);
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Ok(Vec::new()),
    )));
    assert_eq!(app.auto_resume.as_deref(), Some("6"));

    // Cut off by a close: it goes on at the next start (`boot::downloads_from` puts it first).
    let mut app = library_app();
    app.interrupted
        .push(("8".into(), crate::Interrupted::Paused));
    queued_job(&app, "5");
    app.queue = vec![entry("5"), "7".into()];
    app.goes_on = vec![entry("5")];
    let _ = app.start_next();
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    // The rest of the queue still waits.
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Ok(Vec::new()),
    )));
    assert!(app.installing().is_none() && app.auto_resume.is_none());
    assert_eq!(app.queue, ["7"]);
}

/// Download now pressed while the download running pauses for a game starts its row once that one
/// stopped, the game running or not, the other going after it.
#[test]
fn download_now_while_a_download_pauses_for_a_game_starts_its_row() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::entry;
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    play_3(&mut app);
    queued_job(&app, "5");
    app.queue.push(entry("5"));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6"]);
}

/// A download the user is pausing stays paused: neither a game nor Download now takes it over.
#[test]
fn a_download_pausing_on_request_stays_paused() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::entry;
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    let _ = app.update(Message::Install(InstallMsg::Pause("6".into())));
    play_3(&mut app);
    assert!(app.preempted.is_none());
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(
        app.interrupted,
        [("6".to_string(), crate::Interrupted::Paused)]
    );
    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert!(app.installing().is_none() && app.auto_resume.is_none());

    // Download now meanwhile starts its download once the pause is done.
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.update(Message::Install(InstallMsg::Pause("6".into())));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    assert!(app.preempted.is_none());
    assert_eq!(app.queue, [entry("5")], "waits for the pause");
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(
        app.interrupted,
        [("6".to_string(), crate::Interrupted::Paused)]
    );
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
}

/// Download now's row taken out while the download running pauses for a game: the paused download
/// waits for the game to close, as it does without Download now.
#[test]
fn download_now_taken_out_while_a_download_pauses_for_a_game_starts_nothing() {
    use super::offline::queued_job;
    use crate::downloads::{DownloadsMsg, Preempt};
    use crate::offline::{OfflineMsg, entry};
    let mut app = library_app();
    queued_job(&app, "5");
    queued_job(&app, "8");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    app.queue = vec![entry("8")];
    play_3(&mut app);
    assert_eq!(app.preempted, Some((entry("5"), Preempt::Play)));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("8"))));
    let _ = app.update(Message::Downloads(DownloadsMsg::Remove(entry("8"))));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert!(
        app.offline_run.is_none(),
        "nothing starts while the game runs"
    );
    assert_eq!(app.queue, [entry("5")]);
    assert_eq!(app.goes_on, [entry("5")]);
    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
}

/// A download the launcher stopped, still in the saved queue after a restart, that GOG refuses to
/// plan for good at its turn waits among those stopped, with Resume and Discard, as one cut off
/// does.
#[test]
fn a_stopped_download_that_cannot_resume_after_a_restart_is_listed_as_failed() {
    use slatty_core::installer::{DOWNLOADING, FAILED, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    InstallJob {
        game_id: "6".into(),
        build_id: "1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 6".into(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    let waiting =
        crate::boot::downloads_from(vec!["6".into()], InstallJob::list(&db).unwrap(), &[]);
    app.queue = waiting.queue;
    app.goes_on = waiting.goes_on;
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("6"));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "6".into(),
        Err("[FAKE] DLC not owned".into()),
    )));
    assert_eq!(InstallJob::load(&db, "6").unwrap().unwrap().state, FAILED);
    assert!(app.queue.is_empty());
    assert_eq!(
        app.interrupted,
        [("6".to_string(), crate::Interrupted::Failed)]
    );
    assert!(app.goes_on.is_empty());
}

/// An install queued and never started, planned again at its turn while another download runs, is
/// queued again as never started: no pass past a download paused on request, and its first run
/// makes its folder.
#[test]
fn a_queued_install_planned_again_stays_never_started() {
    use super::offline::installing_6;
    use slatty_core::installer::{InstallJob, QUEUED};
    let mut app = library_app();
    installing_6(&mut app);
    ready_to_install(&mut app, "7");
    let _ = app.update(Message::Install(InstallMsg::Start("7".into())));
    assert_eq!(app.queue, ["7"]);
    // Its plan gone (a restart), planned again at its turn; another download started meanwhile.
    app.install_views.clear();
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("7"));
    installing_6(&mut app);
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "7".into(),
        Ok(crate::install::PlanInfo {
            title: "[FAKE] Game 7".into(),
            directory: "Game 7".into(),
            // Its queued job found: the plan says it can be taken up.
            resumable: true,
            ..fake_plan()
        }),
    )));
    assert_eq!(app.queue, ["7"]);
    let job = InstallJob::load(&app.core.as_ref().unwrap().db, "7")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, QUEUED);
    assert!(app.goes_on.is_empty());
}

/// A download the launcher stopped, saved as started in the queue, still goes on beside a download
/// paused on request when GOG could not be reached as it was planned to start: its pass lasts
/// until it runs.
#[test]
fn a_stopped_download_gog_could_not_plan_still_goes_on() {
    use slatty_core::installer::{DOWNLOADING, InstallJob};
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    InstallJob {
        game_id: "6".into(),
        build_id: "1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 6".into(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    let waiting =
        crate::boot::downloads_from(vec!["6".into()], InstallJob::list(&db).unwrap(), &[]);
    app.queue = waiting.queue;
    app.goes_on = waiting.goes_on;
    // Installers paused on request hold the rest of the queue.
    app.offline_stopped = vec![("8".into(), None)];
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("6"));
    let _ = app.update(Message::Install(InstallMsg::Unreachable(
        "6".into(),
        "[FAKE] network error".into(),
    )));
    assert_eq!(app.queue, ["6"]);
    let _ = app.update(Message::Install(InstallMsg::Retry));
    assert_eq!(app.auto_resume.as_deref(), Some("6"), "planned again");
    // Discarded meanwhile, it keeps no pass a later download of the game would get.
    let _ = app.update(Message::Install(InstallMsg::Discard("6".into())));
    assert!(app.goes_on.is_empty());
}

/// The download Download now started paused on request: the one it stopped goes on, as it does
/// beside any download paused on request.
#[test]
fn a_download_stopped_for_download_now_goes_on_when_the_one_started_is_paused() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::{OfflineMsg, entry};
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6"]);
    let _ = app.update(Message::Offline(OfflineMsg::Pause("5".into())));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert_eq!(app.offline_stopped, [("5".to_string(), None)]);
    assert_eq!(
        app.auto_resume.as_deref(),
        Some("6"),
        "the stopped one goes on"
    );
}

/// Download now pressed while a download pauses on request starts its row once that one is
/// paused, a game running or not, as it does when nothing downloads.
#[test]
fn download_now_while_a_download_pauses_on_request_starts_its_row_during_a_game() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::{OfflineMsg, entry};
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.update(Message::Install(InstallMsg::Pause("6".into())));
    play_3(&mut app);
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert_eq!(
        app.interrupted,
        [("6".to_string(), crate::Interrupted::Paused)]
    );
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));

    // Installers pausing on request, the same.
    let mut app = library_app();
    queued_job(&app, "5");
    queued_job(&app, "8");
    app.queue = vec![entry("5"), entry("8")];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Pause("5".into())));
    play_3(&mut app);
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("8"))));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert_eq!(app.offline_stopped, [("5".to_string(), None)]);
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "8"));
}

/// The download being planned to start gives way to Download now, as a download running does.
#[test]
fn download_now_goes_before_a_download_being_planned() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into(), "7".into()];
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow("7".into())));
    assert_eq!(
        app.auto_resume.as_deref(),
        Some("7"),
        "planned, then started"
    );
    assert_eq!(app.queue, ["5", "6"]);
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Queued(_))
    ));
}

#[test]
fn an_install_started_while_a_game_runs_waits_in_the_queue() {
    let mut app = library_app();
    play_3(&mut app);
    ready_to_install(&mut app, "7");
    app.panel = Some(Panel::Install);
    app.selected = Some("7".into());
    assert!(render(&app).find("Add to queue").is_ok());
    let _ = app.update(Message::Install(InstallMsg::Start("7".into())));
    assert!(app.installing().is_none());
    assert_eq!(app.queue, ["7"]);
}

/// A paused install resumed while another downloads waits in the queue saved as started: its
/// resume is checked as one (its folder still there, a game folder already published finished).
#[test]
fn a_paused_install_resumed_while_another_downloads_waits_as_started() {
    use super::offline::installing_6;
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    installing_6(&mut app);
    ready_to_install(&mut app, "7");
    if let Some(InstallView::Ready(info)) = app.install_views.get_mut("7") {
        info.resumable = true;
    }
    slatty_core::installer::InstallJob {
        game_id: "7".into(),
        build_id: "b1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 7".into(),
        state: slatty_core::installer::PAUSED.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Start("7".into())));
    assert_eq!(app.queue, ["7"]);
    let job = slatty_core::installer::InstallJob::load(&db, "7")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, slatty_core::installer::DOWNLOADING);
    assert_eq!(app.goes_on, ["7"]);
}

#[test]
fn download_now_starts_a_queued_install_while_a_game_runs() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    play_3(&mut app);
    for id in ["7", "8"] {
        ready_to_install(&mut app, id);
        let _ = app.update(Message::Install(InstallMsg::Start(id.into())));
    }
    assert_eq!(app.queue, ["7", "8"]);
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow("8".into())));
    assert_eq!(app.installing().map(|(id, _, _)| id), Some("8"));
    assert_eq!(app.queue, ["7"]);
}

/// A game started while the first of the queue was being planned to start: it waits, still first.
#[test]
fn a_download_planned_as_a_game_starts_keeps_its_place() {
    let mut app = library_app();
    app.queue = vec!["5".into(), "6".into()];
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    play_3(&mut app);
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(app.installing().is_none());
    assert_eq!(app.queue, ["5", "6"]);
    // And it starts first once the game closes.
    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert_eq!(app.installing().map(|(id, _, _)| id), Some("5"));
    assert_eq!(app.queue, ["6"]);

    // A download cut off waits among those stopped, and resumes once the game closes.
    let mut app = library_app();
    app.interrupted = vec![("5".into(), crate::Interrupted::Download)];
    let _ = app.start_next();
    assert_eq!(app.auto_resume.as_deref(), Some("5"));
    play_3(&mut app);
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    assert!(app.installing().is_none());
    assert!(app.queue.is_empty());
    let _ = app.update(Message::Playing(crate::play::PlayMsg::Done(Ok(()))));
    assert_eq!(
        app.auto_resume.as_deref(),
        Some("5"),
        "planned again, then started"
    );
}

#[test]
fn downloads_go_on_while_playing_once_turned_off() {
    use super::offline::installing_6;
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    let db = app.core.clone().unwrap().db;
    let _ = app.update(Message::Settings(SettingsMsg::PauseWhilePlaying(false)));
    assert!(!slatty_core::settings::pause_while_playing(&db).unwrap());
    installing_6(&mut app);
    play_3(&mut app);
    assert!(!install_6_paused(&app));
    assert!(app.preempted.is_none());
    // Turned on while the game runs: the download pauses then.
    let _ = app.update(Message::Settings(SettingsMsg::PauseWhilePlaying(true)));
    assert!(install_6_paused(&app));
}

#[test]
fn the_pause_while_playing_setting_shows_in_installs() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    let _ = app.update(Message::Settings(SettingsMsg::Show(Section::Installs)));
    assert!(render(&app).find("Pause downloads while playing").is_ok());
}

/// The download running takes a moment to stop: the tab says so at once.
#[test]
fn download_now_shows_at_once_what_it_is_doing() {
    use super::offline::{installing_6, paused_install_job, queued_job};
    use crate::downloads::DownloadsMsg;
    use crate::offline::entry;
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec!["7".into(), entry("5")];
    app.page = Page::Downloads;
    app.selected = None;
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    let mut ui = render(&app);
    snapshot(&mut ui, "download-now-pausing");
    assert!(ui.find("Pausing…").is_ok());
    assert!(ui.find("Starting…").is_ok());
    // The other row cannot ask the same meanwhile.
    ui.click("Download now").unwrap();
    assert!(ui.into_messages().next().is_none());

    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    let mut ui = render(&app);
    assert!(ui.find("Pausing…").is_err());
    assert!(ui.find("Starting…").is_err());
}

#[test]
fn a_game_starting_shows_the_download_pausing_for_it() {
    use super::offline::installing_6;
    let mut app = library_app();
    installing_6(&mut app);
    play_3(&mut app);
    app.page = Page::Downloads;
    app.selected = None;
    assert!(render(&app).find("Pausing while a game runs…").is_ok());
}
