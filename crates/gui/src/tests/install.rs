use super::*;

#[test]
fn install_needs_a_proton_choice_then_starts() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            proton: None,
            ..fake_plan()
        }),
    );
    let mut ui = render(&app);
    let _ = ui.click("Start install");
    assert!(
        !ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(_))))
    );

    let _ = app.update(Message::Install(InstallMsg::Proton(
        "5".into(),
        crate::settings::ProtonChoice("/proton/GE-Proton".into()),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "install-ready");
    ui.click("Start install").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Start(id)) if id == "5"))
    );
    // Once started, the panel gives way to the game page, which shows the download.
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.panel, None);
    let mut ui = render(&app);
    snapshot(&mut ui, "download-on-game-page");
}

#[test]
fn install_folder_can_be_typed_or_browsed_but_must_be_absolute() {
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    open(&mut app, "5", Some(Panel::Install));
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(ui.find("Game folder: /games/Game 5").is_ok());
    ui.click("Browse").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Browse(id)) if id == "5"))
    );

    let _ = app.update_install(InstallMsg::Browsed("5".into(), Some("/mnt/ssd".into())));
    assert!(render(&app).find("Game folder: /mnt/ssd/Game 5").is_ok());
    let _ = app.update_install(InstallMsg::Browsed("5".into(), None));
    assert!(render(&app).find("Game folder: /mnt/ssd/Game 5").is_ok());

    let _ = app.update_install(InstallMsg::RootInput("5".into(), "games".into()));
    let _ = app.update_install(InstallMsg::Start("5".into()));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Ready(_))
    ));
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
}

#[test]
fn a_resumed_install_keeps_its_folder() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    );
    let _ = app.update_install(InstallMsg::RootInput("5".into(), "/elsewhere".into()));
    let mut ui = render(&app);
    assert!(
        ui.find("/games/Game 5 (an interrupted install resumes where it started)")
            .is_ok()
    );
    assert!(ui.find("Browse").is_err());
}

#[test]
fn running_install_shows_progress_and_can_pause() {
    let mut app = library_app();
    // 20 MiB in two seconds.
    let mut rate = crate::install::Rate::default();
    let start = std::time::Instant::now();
    rate.record(start, 0);
    rate.record(start + std::time::Duration::from_secs(2), 20 << 20);
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 3,
                files_total: 10,
                bytes_done: 1 << 30,
                bytes_total: 4 << 30,
            },
            cancel: tokio_util::sync::CancellationToken::new(),
            cancelling: crate::install::Cancelling::No,
            rate,
        },
    );
    assert!(render(&app).find("Downloading [FAKE] Game 5").is_ok());
    // The game page shows the download; no panel holds the window.
    let _ = app.update(Message::Select("5".into()));
    let _ = app.update(Message::OpenPanel(Panel::Install));
    assert_eq!(app.panel, None);
    settle(&mut app);
    let mut ui = render(&app);
    assert!(ui.find("1.00 GiB / 4.00 GiB · 10.0 MiB/s").is_ok());
    assert!(ui.find("25 %").is_ok(), "at the end of the bar");
    snapshot(&mut ui, "install-running");
    ui.click("Pause").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Install(InstallMsg::Pause(id)) if id == "5"))
    );
}

#[test]
fn owned_dlc_can_be_deselected_before_install_and_sizes_follow() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.proton = Some("/proton/GE-Proton".into());
    app.install_views
        .insert("5".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(ui.find("4.00 GiB").is_ok() && ui.find("7.00 GiB").is_ok());
    assert!(ui.find("100.00 GiB").is_ok(), "free space");
    assert!(
        ui.find("[FAKE] Other DLC (2.00 GiB)").is_err(),
        "not owned: not listed"
    );
    snapshot(&mut ui, "install-dlc");
    ui.click("[FAKE] Owned DLC (2.00 GiB)").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Install(InstallMsg::ToggleDlc(g, d))] if g == "5" && d == "21")
    );
    for m in messages {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    assert!(ui.find("3.00 GiB").is_ok() && ui.find("5.00 GiB").is_ok());
}

#[test]
fn cancelling_a_download_asks_then_deletes_it() {
    use crate::install::Cancelling;
    let mut app = library_app();
    open(&mut app, "5", None);
    let cancel = tokio_util::sync::CancellationToken::new();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: slatty_core::installer::Progress {
                files_done: 3,
                files_total: 10,
                bytes_done: 3 << 30,
                bytes_total: 8 << 30,
            },
            cancel: cancel.clone(),
            cancelling: Cancelling::No,
            rate: Default::default(),
        },
    );
    let cancelling = |app: &App| match app.install_views.get("5") {
        Some(InstallView::Running { cancelling, .. }) => Some(*cancelling),
        _ => None,
    };
    let clicked = |app: &App, label: &str| -> Vec<Message> {
        let mut ui = render(app);
        ui.click(label).unwrap();
        ui.into_messages().collect()
    };

    for m in clicked(&app, "Cancel") {
        let _ = app.update(m);
    }
    assert_eq!(cancelling(&app), Some(Cancelling::Asked));
    {
        let mut ui = render(&app);
        assert!(
            ui.find(
                "Cancel the download of [FAKE] Game 5 and delete the 3.00 GiB downloaded so far?"
            )
            .is_ok()
        );
        snapshot(&mut ui, "install-cancel-confirm");
    }
    for m in clicked(&app, "Keep downloading") {
        let _ = app.update(m);
    }
    assert_eq!(cancelling(&app), Some(Cancelling::No));
    assert!(!cancel.is_cancelled(), "asking stops nothing");

    let _ = app.update(Message::Install(InstallMsg::AskCancel("5".into())));
    for m in clicked(&app, "Cancel download") {
        let _ = app.update(m);
    }
    assert!(cancel.is_cancelled());
    assert_eq!(cancelling(&app), Some(Cancelling::Confirmed));

    // Once stopped, the partial download is deleted rather than offered for resuming.
    let _ = app.update(Message::Install(InstallMsg::Done("5".into(), Err(None))));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    assert!(!app.install_views.contains_key("5"));
    assert_eq!(app.panel, None);
    assert!(app.interrupted.is_empty());
}

#[test]
fn a_build_without_language_packs_says_the_game_chooses() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            language: "*".into(),
            languages: Vec::new(),
            ..fake_plan()
        }),
    );
    let mut ui = render(&app);
    assert!(
        ui.find("Language: one download holds every language; choose it in the game.")
            .is_ok()
    );
}

#[test]
fn download_speed_follows_the_last_seconds() {
    use std::time::Duration;
    let mut rate = crate::install::Rate::default();
    let start = std::time::Instant::now();
    let at = |s: u64| start + Duration::from_millis(s * 250);
    rate.record(at(0), 0);
    assert_eq!(rate.per_second(), None, "not before a second");
    rate.record(at(2), 1 << 20);
    assert_eq!(rate.per_second(), None);
    rate.record(at(4), 2 << 20);
    assert_eq!(rate.per_second(), Some((2 << 20) as f64));
    // A fast start no longer counts once it is more than three seconds old.
    for i in 5..=40 {
        rate.record(at(i), (2 << 20) + (i - 4) * (256 << 10));
    }
    assert_eq!(rate.per_second(), Some((1 << 20) as f64));
}

#[test]
fn install_panel_shows_the_free_space_and_warns_when_short() {
    use crate::ui::format::human_size;
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    let plan = fake_plan();
    app.install_views
        .insert("5".into(), InstallView::Ready(plan.clone()));
    assert!(render(&app).find("100.00 GiB").is_ok());

    // Another folder: its drive is measured again, and an answer for the old one is ignored.
    let _ = app.update(Message::Install(InstallMsg::RootInput(
        "5".into(),
        "/small".into(),
    )));
    let _ = app.update(Message::Install(InstallMsg::FreeSpace(
        "5".into(),
        "/games".into(),
        Some(100 << 30),
    )));
    assert!(render(&app).find("—").is_ok(), "unknown yet");
    let _ = app.update(Message::Install(InstallMsg::FreeSpace(
        "5".into(),
        "/small".into(),
        Some(1 << 30),
    )));
    let mut ui = render(&app);
    assert!(
        ui.find(format!(
            "Not enough space on this drive: 1.00 GiB free, {} needed.",
            human_size(plan.total_disk())
        ))
        .is_ok()
    );
    snapshot(&mut ui, "install-short-of-space");
}

#[test]
fn a_cut_off_download_resumes_by_itself_at_start_up() {
    use crate::Interrupted;
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    app.interrupted = vec![
        ("6".into(), Interrupted::Paused),
        ("5".into(), Interrupted::Download),
    ];
    let _ = app.resume_interrupted();
    assert_eq!(app.auto_resume.as_deref(), Some("5"), "not the paused one");
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(crate::install::PlanInfo {
            resumable: true,
            ..fake_plan()
        }),
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Running { .. })
    ));
    assert_eq!(app.interrupted, [("6".to_string(), Interrupted::Paused)]);
    assert!(app.installing().is_some());

    // Offline at start-up: it stays listed, and the reason is shown.
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    app.interrupted = vec![("5".into(), Interrupted::Download)];
    let _ = app.resume_interrupted();
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Err("network error".into()),
    )));
    assert!(app.installing().is_none());
    assert_eq!(app.interrupted.len(), 1);
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
}

#[test]
fn versions_are_named_with_their_date_and_the_latest_marked() {
    let labels: Vec<String> = fake_versions().into_iter().map(|v| v.label).collect();
    assert_eq!(labels, ["1.1 · 27 Mar 2026 (latest)", "1.0 · 28 Aug 2025"]);
}

#[test]
fn another_version_can_be_chosen_before_install_and_after() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Install));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            build_id: "b2".into(),
            versions: fake_versions(),
            ..fake_plan()
        }),
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Game version").is_ok());
        snapshot(&mut ui, "install-version");
    }
    let _ = app.update(Message::Install(InstallMsg::Version(
        "5".into(),
        "b1".into(),
    )));
    assert!(
        matches!(app.install_views.get("5"), Some(InstallView::Planning)),
        "planned again for that version"
    );

    // Installed: switching asks for that build.
    open(&mut app, "3", Some(Panel::GameSettings));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(ContentInfo {
            build_id: "b2".into(),
            chosen_build: "b2".into(),
            versions: fake_versions(),
            language: "en-US".into(),
            languages: vec!["en-US".into()],
            chosen_language: "en-US".into(),
            dlcs: Vec::new(),
            chosen_dlcs: Vec::new(),
        }),
    )));
    {
        let mut ui = render(&app);
        let _ = ui.click("Switch version");
        assert!(ui.into_messages().next().is_none(), "nothing chosen yet");
    }
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ChooseVersion(
        "3".into(),
        "b1".into(),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "game-settings-version");
    ui.click("Switch version").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Build(b))) if id == "3" && b == "b1"
    )));
}

#[test]
fn a_second_install_says_why_it_waits() {
    let mut app = library_app();
    app.install_views.insert(
        "5".into(),
        InstallView::Running {
            title: "[FAKE] Game 5".into(),
            folder: "/games/Game 5".into(),
            progress: Default::default(),
            cancel: tokio_util::sync::CancellationToken::new(),
            cancelling: crate::install::Cancelling::No,
            rate: Default::default(),
        },
    );
    open(&mut app, "6", Some(Panel::Install));
    app.install_views
        .insert("6".into(), InstallView::Ready(fake_plan()));
    let mut ui = render(&app);
    assert!(
        ui.find("[FAKE] Game 5 is downloading; this one can start once it is done.")
            .is_ok()
    );
    let _ = ui.click("Start install");
    assert!(ui.into_messages().next().is_none());
}

#[test]
fn installing_from_the_library_stays_on_the_library() {
    let mut app = library_app();
    app.proton = Some("/proton/GE-Proton".into());
    let _ = app.update(Message::OpenDialog("5".into(), Panel::Install));
    assert_eq!(app.dialog, Some(("5".to_string(), Panel::Install)));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    let _ = app.update(Message::Install(InstallMsg::Planned(
        "5".into(),
        Ok(fake_plan()),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("Start install").is_ok() && ui.find("Proton").is_ok());
        snapshot(&mut ui, "install-dialog");
    }
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert_eq!(app.dialog, None, "the dialog gives way once started");
    assert_eq!(app.selected, None, "still on the library");
    assert!(
        render(&app).find("Downloading [FAKE] Game 5").is_ok(),
        "its banner"
    );

    // Escape closes the dialog without leaving the library.
    let _ = app.update(Message::OpenDialog("6".into(), Panel::Install));
    app.go_back();
    assert_eq!(app.dialog, None);
}

#[test]
fn a_linux_build_can_be_chosen_and_needs_no_proton() {
    let mut app = library_app();
    app.library.iter_mut().find(|g| g.id == "5").unwrap().os =
        vec!["windows".into(), "linux".into(), "osx".into()];
    assert_eq!(app.platforms("5"), vec![Platform::Windows, Platform::Linux]);
    assert_eq!(app.platforms("3"), vec![Platform::Windows]);
    open(&mut app, "5", Some(Panel::Install));
    let both = vec![Platform::Windows, Platform::Linux];
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            platforms: both.clone(),
            proton: None,
            ..fake_plan()
        }),
    );
    {
        let mut ui = render(&app);
        assert!(ui.find("Proton").is_ok());
        assert!(ui.find("Choose a Proton build to start.").is_ok());
    }
    // Another platform is planned again.
    let _ = app.update(Message::Install(InstallMsg::Platform(
        "5".into(),
        Platform::Linux,
    )));
    assert!(matches!(
        app.install_views.get("5"),
        Some(InstallView::Planning)
    ));
    app.install_views.insert(
        "5".into(),
        InstallView::Ready(crate::install::PlanInfo {
            platform: Platform::Linux,
            platforms: both,
            proton: None,
            ..fake_plan()
        }),
    );
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        assert!(ui.find("Proton").is_err(), "a Linux build runs without");
        assert!(ui.find("Game version").is_err());
        assert!(
            ui.find(
                "GOG keeps no cloud saves for Linux builds, and they do not report achievements."
            )
            .is_ok()
        );
        snapshot(&mut ui, "install-linux");
        ui.click("Start install").unwrap();
        ui.into_messages().collect()
    };
    assert!(
        matches!(&messages[..], [Message::Install(InstallMsg::Start(id))] if id == "5"),
        "{messages:?}"
    );
}
