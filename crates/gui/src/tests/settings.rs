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
    // Tests tell times in the same time zone wherever they run (Paris in summer, UTC+2).
    assert!(
        ui.find("14 owned · Last refreshed 09/10/2026 00:53")
            .is_ok()
    );
    // And show their throwaway folder under one name, whatever its real one this run.
    assert!(ui.find("/tmp/slatty-test/config/theme.toml").is_ok());
    snapshot(&mut ui, "settings");
}

#[test]
fn forgetting_the_account_asks_first() {
    let mut app = library_app();
    app.page = Page::Settings;
    let mut ui = render(&app);
    ui.click("Forget account").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    let mut ui = render(&app);
    assert!(ui.find("Forget testeur?").is_ok());
    snapshot(&mut ui, "forget-account");
    ui.click("Cancel").unwrap();
    for m in ui.into_messages() {
        let _ = app.update(m);
    }
    assert!(!app.forget_confirm);
    assert!(app.account.is_some(), "nothing forgotten");

    let _ = app.update(Message::AskForget);
    app.go_back();
    assert!(!app.forget_confirm, "Escape closes it too");
}

/// Work for an account forgotten meanwhile (a library sync, a cover) writes into its cache as it
/// ends, after the cache was deleted: that is deleted again as its result comes, too late to be
/// shown. Once signed in again, the account's cache is its own again.
#[test]
fn work_ending_after_its_account_was_forgotten_leaves_nothing_of_it() {
    let mut app = library_app();
    let dirs = app.core.clone().unwrap().dirs;
    let cache = dirs.account_cache("77");
    let late = |app: &mut App, epoch| {
        std::fs::create_dir_all(cache.join("covers")).unwrap();
        std::fs::write(cache.join("covers/1.jpg"), b"[FAKE]").unwrap();
        let ended = Message::Image("https://images.gog.example/1.jpg".into(), None);
        let _ = app.update(Message::AccountResult(epoch, Box::new(ended)));
    };
    let epoch = app.account_epoch;
    // Written by a result handled while the sign-out ran, after its data was deleted.
    std::fs::create_dir_all(cache.join("covers")).unwrap();
    std::fs::write(cache.join("library.json"), b"[FAKE]").unwrap();
    let _ = app.update(Message::LoggedOut(Ok(Some("77".into()))));
    assert!(!cache.exists(), "deleted as the sign-out ends");
    late(&mut app, epoch);
    assert!(!cache.exists(), "deleted again");

    let account = AccountInfo {
        user_id: "77".into(),
        username: "[FAKE]".into(),
    };
    let _ = app.update(Message::LoggedIn(Ok((account, None))));
    late(&mut app, epoch);
    assert!(cache.join("covers/1.jpg").exists(), "its own again");
    std::fs::remove_dir_all(cache).unwrap();
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
    // A config folder of its own: the theme file it writes would show in other tests' snapshots.
    let core = app.core.as_mut().unwrap();
    core.dirs = Dirs::under(&core.dirs.config.join("theme-test"));
    let path = crate::theme::file(&core.dirs.config);
    let _ = std::fs::remove_file(&path);
    // Tall enough to show Appearance below Installs, Runners and Emulators.
    app.window = Size::new(1440.0, 2400.0);
    {
        let mut ui = render_window(&app);
        ui.click("Create theme file").unwrap();
        for m in ui.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("[colors]") && written.contains("accent = \"#c4b1fa\""));
    let mut ui = render_window(&app);
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
        snapshot(&mut ui, "settings-cover-size");
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

/// At the smallest window, notice shown or not, every part of the side list can be reached.
#[test]
fn the_side_list_of_settings_fits_the_smallest_window() {
    use crate::settings::{Section, SettingsMsg};
    let mut app = library_app();
    app.page = Page::Settings;
    app.window = Size::new(800.0, 560.0);
    for notice in [false, true] {
        app.notice = notice.then(|| crate::Notice {
            error: true,
            text: "[FAKE] Library not refreshed: GOG did not answer.".into(),
        });
        let mut ui = Simulator::with_size(settings(), app.window, app.view());
        if notice {
            snapshot(&mut ui, "settings-small-notice");
        }
        let about = ui.find("About").unwrap().bounds();
        assert!(
            about.height > 0.0 && about.y + about.height <= app.window.height,
            "notice {notice}: {about:?}"
        );
        ui.click("About").unwrap();
        assert!(
            ui.into_messages()
                .any(|m| matches!(m, Message::Settings(SettingsMsg::Show(Section::About)))),
            "notice {notice}"
        );
    }
}

/// About gives what the GPL asks the interface to show: the copyright, the licence, that there is
/// no warranty and where the source is, and the licences of what Slatty includes.
#[test]
fn about_gives_the_licences_and_the_source() {
    let mut app = library_app();
    app.page = Page::Settings;
    let mut ui = render(&app);
    for shown in [
        format!(
            "Slatty {} · Copyright © 2026 HoneySlatt",
            env!("CARGO_PKG_VERSION")
        ),
        "Free software under the GNU General Public License, version 3 or later, with no \
         warranty. Source code and licence: https://github.com/HoneySlatt/Slatty"
            .into(),
        "Includes the Geist font (SIL Open Font License 1.1), the Lucide icons (ISC; some from \
         Feather, MIT) and the image sharpening of AMD FidelityFX FSR 1 (MIT)."
            .into(),
        "Achievements in game go through Comet and its GalaxyCommunication service (Apache-2.0)."
            .into(),
    ] {
        assert!(ui.find(shown.as_str()).is_ok(), "{shown}");
    }
}

/// On a very wide window, settings keep lines short enough to read.
#[test]
fn settings_keep_a_readable_width_on_a_wide_window() {
    let mut app = library_app();
    app.page = Page::Settings;
    app.window = Size::new(3840.0, 1080.0);
    let mut ui = Simulator::with_size(settings(), app.window, app.view());
    snapshot(&mut ui, "settings-wide");
    let log_out = ui.find("Log out").unwrap().bounds();
    assert!(log_out.x + log_out.width < 1700.0, "{log_out:?}");
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
        Section::Steam,
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
    assert_eq!(scrolled(&mut app, 500.0), Section::Steam);

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
    assert!(
        !app.umu_lookup,
        "umu's database online: off until turned on"
    );
    assert!(app.report_playtime, "on until turned off");
    assert!(
        !app.game_achievements && !slatty_core::settings::game_achievements(&db).unwrap(),
        "Comet, which hands the access token to any local program: off until turned on"
    );
    let _ = app.update(Message::Settings(SettingsMsg::GameAchievements(true)));
    assert!(app.game_achievements && slatty_core::settings::game_achievements(&db).unwrap());
    let _ = app.update(Message::Settings(SettingsMsg::UmuLookup(true)));
    assert!(app.umu_lookup && slatty_core::settings::umu_lookup(&db).unwrap());
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

/// Where the user decides, the switches say what they share, and the note names every host.
#[test]
fn the_privacy_page_says_what_comet_exposes_and_every_host_reached() {
    let mut app = library_app();
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 1900.0);
    let texts = texts(&mut Simulator::with_size(
        settings(),
        app.window,
        app.view(),
    ))
    .join(" ");
    for said in [
        "any program on this computer can get your GOG access token",
        "download what they install",
        "repo.steampowered.com",
        "SECURITY.md",
    ] {
        assert!(texts.contains(said), "{said}");
    }
}

#[test]
fn new_windows_games_are_isolated_until_turned_off() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(app.isolate_new_games);
    app.window = Size::new(1440.0, 1900.0);
    assert!(
        Simulator::with_size(settings(), app.window, app.view())
            .find("Isolate new Windows games")
            .is_ok()
    );
    let _ = app.update(Message::Settings(SettingsMsg::IsolateNewGames(false)));
    assert!(!app.isolate_new_games);
    assert!(!slatty_core::settings::isolate_new_games(&db).unwrap());
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
