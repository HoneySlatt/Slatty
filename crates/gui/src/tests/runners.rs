use slatty_core::protons::{Release, Source, Stage};

use super::*;
use crate::runners::RunnersMsg;

fn release(app: &App) -> Release {
    let dirs = &app.core.as_ref().unwrap().dirs;
    let release = Release {
        source: Source::GeProton,
        version: "GE-Proton11-7".into(),
        name: "GE-Proton11-7-x86_64".into(),
        size: 563_784_602,
        url: "https://example.invalid/GE-Proton11-7-x86_64.tar.gz".into(),
        archive: "GE-Proton11-7-x86_64.tar.gz".into(),
        checksum_url: "https://example.invalid/GE-Proton11-7-x86_64.sha512sum".into(),
    };
    assert!(!release.path(dirs).exists());
    release
}

fn runners_page() -> App {
    let mut app = library_app();
    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    app
}

#[test]
fn github_is_offered_only_once_proton_downloads_are_turned_on() {
    let mut app = runners_page();
    let db = app.core.as_ref().unwrap().db.clone();
    assert!(!app.runners.downloads, "off until turned on");
    assert!(render_window(&app).find("Proton downloads").is_ok());
    assert!(render_window(&app).find("Check GitHub").is_err());
    // Asked for anyway (a message left from before), GitHub is not contacted.
    let _ = app.update(Message::Runners(RunnersMsg::Check));
    assert!(!app.runners.checking);

    let _ = app.update(Message::Runners(RunnersMsg::Downloads(true)));
    assert!(slatty_core::settings::proton_downloads(&db).unwrap());
    let mut ui = render_window(&app);
    assert!(ui.find("Check GitHub").is_ok());
    assert!(ui.find("Not checked yet.").is_ok());
    snapshot(&mut ui, "settings-runners");
    drop(ui);

    let _ = app.update(Message::Runners(RunnersMsg::Downloads(false)));
    assert!(!slatty_core::settings::proton_downloads(&db).unwrap());
}

#[test]
fn the_newest_builds_can_be_installed_and_a_download_followed_and_stopped() {
    let mut app = runners_page();
    let _ = app.update(Message::Runners(RunnersMsg::Downloads(true)));
    let release = release(&app);
    let _ = app.update(Message::Runners(RunnersMsg::Checked(vec![
        (Source::GeProton, Ok(release.clone())),
        (
            Source::ProtonCachyOs,
            Err("GitHub refuses more requests from this address for now; try again later".into()),
        ),
    ])));
    {
        let mut ui = render_window(&app);
        assert!(ui.find("GE-Proton11-7-x86_64 · 537.7 MiB").is_ok());
        assert!(ui.find("Install").is_ok());
        assert!(
            ui.find("GitHub refuses more requests from this address for now; try again later")
                .is_ok()
        );
    }

    let _ = app.update(Message::Runners(RunnersMsg::Install(release.clone())));
    assert!(app.runners.installing.is_some());
    let _ = app.update(Message::Runners(RunnersMsg::Progress(Stage::Downloading {
        done: 50,
        total: 100,
    })));
    {
        let mut ui = render_window(&app);
        assert!(ui.find("Downloading 50 %").is_ok());
        assert!(ui.find("Stop").is_ok());
    }
    let cancel = app.runners.installing.as_ref().unwrap().2.clone();
    let _ = app.update(Message::Runners(RunnersMsg::Cancel));
    assert!(cancel.is_cancelled());
    // Stopped: no error shown.
    let _ = app.update(Message::Runners(RunnersMsg::Installed(Err(None))));
    assert!(app.runners.installing.is_none());
    assert!(app.notice.is_none());

    // Once downloaded, it is listed, in the Proton menus too.
    let path = release.path(&app.core.as_ref().unwrap().dirs);
    let _ = app.update(Message::Runners(RunnersMsg::Listed(
        vec![path.clone()],
        vec![path.clone()],
    )));
    assert_eq!(app.proton_choices, std::slice::from_ref(&path));
    let mut ui = render_window(&app);
    assert!(ui.find("Installed").is_ok());
    assert!(ui.find("Delete").is_ok());
}

#[test]
fn a_build_that_cannot_be_deleted_says_why() {
    let mut app = runners_page();
    let _ = app.update(Message::Runners(RunnersMsg::Removed(Err(
        "GE-Proton11-7-x86_64 runs [FAKE] Game 1; choose another Proton for it first".into(),
    ))));
    assert!(
        app.notice
            .as_ref()
            .is_some_and(|n| n.error && n.text.contains("[FAKE] Game 1"))
    );
}
