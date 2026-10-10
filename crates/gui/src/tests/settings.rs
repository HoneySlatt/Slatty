use super::*;

#[test]
fn settings_page_holds_account_library_and_install_options() {
    let mut app = library_app();
    app.page = Page::Settings;
    let mut ui = render(&app);
    for label in [
        "testeur",
        "Log out",
        "Refresh library",
        "Default installation path",
        "Default Proton",
    ] {
        assert!(ui.find(label).is_ok(), "{label}");
    }
    snapshot(&mut ui, "settings");
}

#[test]
fn default_installation_path_is_browsed_or_typed_without_a_save_button() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let db = app.core.clone().unwrap().db;
    let saved = || slatty_core::settings::library_root(&db).unwrap();
    {
        let mut ui = render(&app);
        assert!(ui.find("Save").is_err());
        ui.click("Browse").unwrap();
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::Settings(SettingsMsg::BrowseRoot)))
        );
    }
    let _ = app.update(Message::Settings(SettingsMsg::BrowsedRoot(Some(
        "/games/picked".into(),
    ))));
    assert_eq!(app.library_root, "/games/picked");
    assert_eq!(saved(), PathBuf::from("/games/picked"));

    let _ = app.update(Message::Settings(SettingsMsg::RootInput("games".into())));
    assert_eq!(
        saved(),
        PathBuf::from("/games/picked"),
        "not absolute: kept"
    );
    assert!(app.notice.is_none(), "no error while typing");
    let _ = app.update(Message::Settings(SettingsMsg::SaveRoot));
    assert!(app.notice.is_some(), "Enter says why");
    let _ = app.update(Message::Settings(SettingsMsg::RootInput(
        "/games/typed".into(),
    )));
    assert_eq!(saved(), PathBuf::from("/games/typed"));
}

#[test]
fn the_theme_file_can_be_created_from_settings() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let path = crate::theme::file(&app.core.as_ref().unwrap().dirs.config);
    let _ = std::fs::remove_file(&path);
    {
        let mut ui = render(&app);
        ui.click("Create theme file").unwrap();
        for m in ui.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("[colors]") && written.contains("accent = \"#c4b1fa\""));
    let mut ui = render(&app);
    assert!(ui.find("Edit").is_ok() && ui.find("Reload").is_ok());
    snapshot(&mut ui, "settings-appearance");
    drop(ui);
    // Reloading the defaults just written changes nothing for the other tests.
    let _ = app.update(Message::Settings(SettingsMsg::ReloadTheme));
    assert!(app.notice.as_ref().is_some_and(|n| !n.error));
}

#[test]
fn the_interface_font_is_chosen_in_settings() {
    use crate::settings::{FontChoice, SettingsMsg};
    let mut app = library_app();
    let _ = app.update(Message::ShowPage(Page::Settings));
    assert!(!app.font_families.is_empty(), "listed when Settings opens");
    assert!(render(&app).find("Font").is_ok());
    // The default leaves the font every other test draws with.
    let _ = app.update(Message::Settings(SettingsMsg::Font(FontChoice::Default)));
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(slatty_core::settings::interface_font(&db).unwrap(), None);
    assert!(app.notice.is_none());
}

#[test]
fn cover_size_is_set_in_settings_and_kept() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    let _ = app.update(Message::ShowPage(Page::Settings));
    {
        let mut ui = render(&app);
        assert!(ui.find("Cover size").is_ok());
        snapshot(&mut ui, "settings-appearance");
    }
    let _ = app.update(Message::Settings(SettingsMsg::CoverSize(
        crate::settings::CoverSize(130),
    )));
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(
        slatty_core::settings::cover_width(&db).unwrap(),
        Some(195.0)
    );
    assert_eq!(app.card_width, 195.0);
    // A size saved by the former slider shows as the nearest one offered.
    assert_eq!(crate::settings::CoverSize::of(200.0).0, 130);
}

#[test]
fn the_default_platform_is_kept() {
    use crate::settings::{PlatformChoice, SettingsMsg};
    let mut app = library_app();
    let _ = app.update(Message::Settings(SettingsMsg::Platform(PlatformChoice(
        Platform::Linux,
    ))));
    assert_eq!(app.default_platform, Platform::Linux);
    let db = app.core.as_ref().unwrap().db.clone();
    assert_eq!(
        slatty_core::settings::default_platform(&db).unwrap(),
        Platform::Linux
    );
}

#[test]
fn the_side_list_of_settings_brings_a_part_to_the_top() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    assert_eq!(app.settings_view.section, Section::Account);
    let messages: Vec<Message> = {
        let mut ui = render(&app);
        for s in Section::ALL {
            assert!(ui.find(s.title()).is_ok(), "{}", s.title());
        }
        ui.click("Installs").unwrap();
        ui.into_messages().collect()
    };
    assert!(
        matches!(
            &messages[..],
            [Message::Settings(SettingsMsg::Show(Section::Installs))]
        ),
        "{messages:?}"
    );
    for m in messages {
        let _ = app.update(m);
    }
    assert_eq!(app.settings_view.section, Section::Installs);

    // In a narrow window each name goes above its setting.
    app.window = Size::new(940.0, 1000.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    assert!(ui.find("Default installation path").is_ok());
    snapshot(&mut ui, "settings-narrow");
}

#[test]
fn the_side_list_of_settings_follows_the_scroll() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    let scrolled = |app: &mut App, offset: f32| {
        let _ = app.update(Message::Settings(SettingsMsg::Scrolled {
            offset,
            max: 900.0,
        }));
        app.settings_view.section
    };
    // Measured first; the parts start 0, 200, 450, 800 and 1100 below the top.
    assert_eq!(scrolled(&mut app, 300.0), Section::Account);
    for (s, y) in Section::ALL
        .into_iter()
        .zip([0.0, 200.0, 450.0, 800.0, 1100.0])
    {
        let _ = app.update(Message::Settings(SettingsMsg::Measured(s, y)));
    }
    assert_eq!(scrolled(&mut app, 0.0), Section::Account);
    assert_eq!(scrolled(&mut app, 210.0), Section::Library);
    assert_eq!(
        scrolled(&mut app, 440.0),
        Section::Installs,
        "nearly at the top"
    );
    assert_eq!(scrolled(&mut app, 900.0), Section::About, "at the end");

    // An entry chosen stays highlighted while the page goes where it asked, even when its part
    // cannot reach the top; scrolling away hands over to the page again.
    let _ = app.update(Message::Settings(SettingsMsg::Show(Section::Appearance)));
    let _ = app.update(Message::Settings(SettingsMsg::ScrollTo(Some(800.0))));
    assert_eq!(scrolled(&mut app, 300.0), Section::Library);
    let _ = app.update(Message::Settings(SettingsMsg::Show(Section::Appearance)));
    let _ = app.update(Message::Settings(SettingsMsg::ScrollTo(Some(1100.0))));
    assert_eq!(scrolled(&mut app, 900.0), Section::Appearance);
    assert_eq!(scrolled(&mut app, 500.0), Section::Installs);

    // A new window size measures again.
    let _ = app.update(Message::WindowResized(Size::new(1000.0, 800.0)));
    assert!(app.settings_view.tops.iter().all(Option::is_none));
}

#[test]
fn what_leaves_the_computer_can_be_turned_off() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(app.umu_lookup && app.report_playtime, "on until turned off");
    let _ = app.update(Message::Settings(SettingsMsg::UmuLookup(false)));
    let _ = app.update(Message::Settings(SettingsMsg::ReportPlaytime(false)));
    let _ = app.update(Message::Settings(SettingsMsg::GameAchievements(false)));
    assert!(!app.game_achievements);
    assert!(!slatty_core::settings::game_achievements(&db).unwrap());
    assert!(!app.umu_lookup && !app.report_playtime);
    assert!(!slatty_core::settings::umu_lookup(&db).unwrap());
    assert!(!slatty_core::settings::report_playtime(&db).unwrap());
    app.window = Size::new(1440.0, 1900.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    assert!(ui.find("Play time on GOG").is_ok());
    snapshot(&mut ui, "settings-privacy");
}

#[test]
fn steamgriddb_is_off_until_turned_on_and_its_key_is_never_shown() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    assert!(!app.steamgriddb);
    {
        let mut ui = render(&app);
        assert!(ui.find("SteamGridDB").is_ok());
        assert!(ui.find("API key").is_err(), "no key asked while off");
    }
    let _ = app.update(Message::Settings(SettingsMsg::SteamGridDb(true)));
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(slatty_core::settings::steamgriddb(&db).unwrap(), "kept");
    let _ = app.update(Message::Settings(SettingsMsg::SteamGridDbKey(Ok(false))));
    {
        let mut ui = render(&app);
        assert!(ui.find("API key").is_ok());
        assert!(ui.find("Create a key on steamgriddb.com").is_ok());
    }
    let _ = app.update(Message::Settings(SettingsMsg::SteamGridDbKeyInput(
        "[FAKE]-key".into(),
    )));
    assert!(
        render(&app).find("[FAKE]-key").is_err(),
        "typed into a secure field"
    );
    // Saving hands the key to the keyring and clears the field.
    let _ = app.update(Message::Settings(SettingsMsg::SaveSteamGridDbKey));
    assert!(app.steamgriddb_key_input.is_empty());
    let _ = app.update(Message::Settings(SettingsMsg::SteamGridDbKey(Ok(true))));
    let mut ui = render(&app);
    assert!(ui.find("Saved in the system keyring").is_ok());
    assert!(ui.find("Remove").is_ok());
}
