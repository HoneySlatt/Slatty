use super::*;

#[test]
fn uninstall_requires_an_explicit_choice() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let mut ui = render(&app);
    ui.click("Uninstall…").unwrap();
    let messages: Vec<Message> = ui.into_messages().collect();
    assert!(
        matches!(messages.as_slice(), [Message::Maintenance(MaintenanceMsg::AskUninstall(id))] if id == "3")
    );
    for m in messages {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    snapshot(&mut ui, "uninstall-confirm");
    ui.click("Uninstall, keep the prefix").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, false)) if id == "3")
    ));
    let mut ui = render(&app);
    ui.click("Also delete the prefix (backed up)").unwrap();
    assert!(ui.into_messages().any(
        |m| matches!(m, Message::Maintenance(MaintenanceMsg::Uninstall(id, true)) if id == "3")
    ));
}

#[test]
fn update_button_appears_only_when_an_update_exists() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    assert!(render(&app).find("Update now").is_err());
    let _ = app.update(Message::Maintenance(MaintenanceMsg::UpdateChecked(
        "3".into(),
        Ok(Some("1.0 → 1.1".into())),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Update available: 1.0 → 1.1").is_ok());
    snapshot(&mut ui, "update-available");
    ui.click("Update now").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Maintenance(MaintenanceMsg::Apply(id, slatty_core::maintenance::Change::Update)) if id == "3"))
    );
}

#[test]
fn an_update_shows_its_progress_and_can_be_paused() {
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::Manage));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Apply(
        "3".into(),
        slatty_core::maintenance::Change::Update,
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Progress(
        "3".into(),
        slatty_core::installer::Progress {
            bytes_done: 1 << 30,
            bytes_total: 4 << 30,
            files_done: 3,
            files_total: 10,
        },
    )));
    let mut ui = render(&app);
    assert!(ui.find("1.00 GiB / 4.00 GiB · files 3/10").is_ok());
    snapshot(&mut ui, "update-progress");
    ui.click("Pause").unwrap();
    for m in ui.into_messages().collect::<Vec<_>>() {
        let _ = app.update(m);
    }
    assert!(app.maintenance["3"].cancel.as_ref().unwrap().is_cancelled());

    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Err(None),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Paused. Apply it again to resume.").is_ok());
    assert!(ui.find("Pause").is_err());
}

#[test]
fn game_settings_panel_applies_dlc_changes_only_when_something_changed() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: vec!["en-US".into(), "fr-FR".into()],
        chosen_language: "en-US".into(),
        dlcs: vec![fake_dlc("21", "[FAKE] Owned DLC", true, true)],
        chosen_dlcs: vec!["21".into()],
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content),
    )));
    let mut ui = render(&app);
    let _ = ui.click("Apply DLC changes");
    assert!(
        ui.into_messages().next().is_none(),
        "no change, button disabled"
    );

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ToggleContentDlc(
        "3".into(),
        "21".into(),
    )));
    let mut ui = render(&app);
    snapshot(&mut ui, "game-settings");
    ui.click("Apply DLC changes").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Dlcs(d))) if id == "3" && d.is_empty()
    )));
}

#[test]
fn interrupted_work_is_listed_with_a_way_to_finish_it() {
    use crate::Interrupted;
    let mut app = library_app();
    app.interrupted = vec![
        ("5".into(), Interrupted::Download),
        ("3".into(), Interrupted::Update),
    ];
    let mut ui = render(&app);
    assert!(
        ui.find("Download interrupted. It resumes where it stopped.")
            .is_ok()
    );
    assert!(
        ui.find("Update interrupted. The game cannot start until it is finished.")
            .is_ok()
    );
    snapshot(&mut ui, "interrupted");
    ui.click("Resume").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenDialog(id, Panel::Install) if id == "5"))
    );
    let mut ui = render(&app);
    ui.click("Finish update").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, slatty_core::maintenance::Change::Update)) if id == "3"
    )));

    let _ = app.update(Message::Install(InstallMsg::Discarded("5".into(), Ok(()))));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Ok("[FAKE] done".into()),
    )));
    assert!(app.interrupted.is_empty());

    let _ = app.update(Message::Maintenance(MaintenanceMsg::Updated(
        "3".into(),
        Err(Some("[FAKE] network down before anything changed".into())),
    )));
    let _ = app.update(Message::Install(InstallMsg::Done(
        "8".into(),
        Err(Some("[FAKE] refused before downloading".into())),
    )));
    assert!(
        app.interrupted.is_empty(),
        "failures that left nothing unfinished are not listed"
    );

    slatty_core::installer::InstallJob {
        game_id: "7".into(),
        build_id: "b".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 7".into(),
        state: "paused".into(),
        dlcs: vec![],
    }
    .save(&app.core.as_ref().unwrap().db)
    .unwrap();
    let _ = app.update(Message::Install(InstallMsg::Done("7".into(), Err(None))));
    assert_eq!(app.interrupted, [("7".to_string(), Interrupted::Paused)]);
}

#[test]
fn an_installed_game_can_change_its_proton() {
    let mut app = library_app();
    let core = app.core.clone().unwrap();
    app.installs["3"].save(&core.db).unwrap();
    let other = std::env::temp_dir().join(format!("slatty-gui-proton-{}", std::process::id()));
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("proton"), b"").unwrap();
    app.proton_choices = vec!["/proton/GE-Proton".into(), other.clone()];
    open(&mut app, "3", Some(Panel::GameSettings));
    {
        let mut ui = render(&app);
        assert!(ui.find("Used from the next launch.").is_ok());
        snapshot(&mut ui, "game-settings-proton");
    }

    let _ = app.update(Message::Settings(crate::settings::SettingsMsg::GameProton(
        "3".into(),
        crate::settings::ProtonChoice(other.clone()),
    )));
    let saved = slatty_core::install::Install::get(&core.db, "3")
        .unwrap()
        .unwrap();
    for install in [&app.installs["3"], &saved] {
        assert_eq!(
            install.runner,
            Runner::Umu {
                proton: other.clone(),
                prefix: "/prefixes/jeu3".into()
            }
        );
    }
    std::fs::remove_dir_all(other).unwrap();
}

#[test]
fn game_settings_show_the_language_and_switch_it_when_gog_offers_others() {
    use crate::maintenance::ContentInfo;
    use slatty_core::maintenance::Change;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = |languages: &[&str]| ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: languages.iter().map(|l| l.to_string()).collect(),
        chosen_language: "en-US".into(),
        dlcs: Vec::new(),
        chosen_dlcs: Vec::new(),
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(&["en-US"])),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("English").is_ok(), "named, not en-US");
        assert!(ui.find("Switch language").is_err(), "nothing to switch to");
        snapshot(&mut ui, "game-settings-one-language");
    }

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(&["en-US", "fr-FR"])),
    )));
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ChooseLanguage(
        "3".into(),
        "fr-FR".into(),
    )));
    let mut ui = render(&app);
    ui.click("Switch language").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Maintenance(MaintenanceMsg::Apply(id, Change::Language(l))) if id == "3" && l == "fr-FR"
    )));
}

#[test]
fn game_settings_list_only_owned_dlc() {
    use crate::maintenance::ContentInfo;
    let mut app = library_app();
    open(&mut app, "3", Some(Panel::GameSettings));
    let content = |dlcs| ContentInfo {
        build_id: "b1".into(),
        chosen_build: "b1".into(),
        versions: Vec::new(),
        language: "en-US".into(),
        languages: vec!["en-US".into()],
        chosen_language: "en-US".into(),
        dlcs,
        chosen_dlcs: vec!["21".into()],
    };
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(vec![
            fake_dlc("21", "[FAKE] Owned DLC", true, true),
            fake_dlc("22", "[FAKE] Other DLC", false, false),
        ])),
    )));
    {
        let mut ui = render(&app);
        assert!(ui.find("[FAKE] Owned DLC (2.00 GiB)").is_ok());
        assert!(ui.find("[FAKE] Other DLC (2.00 GiB)").is_err());
    }

    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(content(vec![fake_dlc(
            "22",
            "[FAKE] Other DLC",
            false,
            false,
        )])),
    )));
    assert!(
        render(&app)
            .find("You own none of this game's DLC.")
            .is_ok()
    );
}

#[test]
fn a_cut_off_update_resumes_by_itself_at_start_up() {
    use crate::Interrupted;
    let mut app = library_app();
    app.installs.insert(
        "4".into(),
        Install {
            game_id: "4".into(),
            ..app.installs["3"].clone()
        },
    );
    app.interrupted = vec![
        ("3".into(), Interrupted::Update),
        ("4".into(), Interrupted::UpdatePaused),
    ];
    let _ = app.resume_interrupted();
    assert!(app.maintenance.get("3").is_some_and(|m| m.busy));
    assert!(
        !app.maintenance.get("4").is_some_and(|m| m.busy),
        "paused on request: waits for Finish update"
    );
    let mut ui = render(&app);
    assert!(
        ui.find("Update paused. The game cannot start until it is finished.")
            .is_ok()
    );
    assert!(ui.find("Finishing the update…").is_ok());
}

#[test]
fn game_settings_open_over_the_library_too() {
    use crate::maintenance::ContentInfo;
    let mut app = library_app();
    let _ = app.update(Message::OpenDialog("3".into(), Panel::GameSettings));
    assert!(
        app.maintenance.get("3").is_some_and(|m| m.busy),
        "reading languages and DLC"
    );
    let _ = app.update(Message::Maintenance(MaintenanceMsg::ContentLoaded(
        "3".into(),
        Ok(ContentInfo {
            build_id: "b1".into(),
            chosen_build: "b1".into(),
            versions: Vec::new(),
            language: "en-US".into(),
            languages: vec!["en-US".into()],
            chosen_language: "en-US".into(),
            dlcs: Vec::new(),
            chosen_dlcs: Vec::new(),
        }),
    )));
    let mut ui = render(&app);
    assert!(ui.find("Game settings").is_ok() && ui.find("Used from the next launch.").is_ok());
    snapshot(&mut ui, "settings-dialog");
    drop(ui);
    assert_eq!(app.selected, None, "still on the library");
    let _ = app.update(Message::CloseDialog);
    assert_eq!(app.dialog, None);
}
