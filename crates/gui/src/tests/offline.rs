use slatty_core::installer::{DOWNLOADING, FAILED, InstallJob, PAUSED, QUEUED};
use slatty_core::offline::{Dlc, File, Installer, Offer, OfflineJob};

use super::*;
use crate::offline::{OfflineMsg, OfflineView, entry};

fn installer(os: &str, language: &str, size: u64) -> Installer {
    Installer {
        os: os.into(),
        language: language.into(),
        version: "1.0".into(),
        files: vec![File {
            size,
            downlink: "https://api.gog.com/[FAKE]".into(),
        }],
    }
}

/// Windows installers in English and French, a Linux one in English, and a DLC for Windows only.
fn fake_offer() -> Offer {
    Offer {
        title: "[FAKE] Game 5".into(),
        installers: vec![
            installer("windows", "en", 3 << 30),
            installer("windows", "fr", 3 << 30),
            installer("linux", "en", 2 << 30),
        ],
        dlcs: vec![Dlc {
            id: "50".into(),
            title: "[FAKE] Extra".into(),
            installers: vec![installer("windows", "en", 1 << 30)],
        }],
    }
}

fn offered(app: &mut App) {
    app.installers_root = "/tmp/[FAKE] installers".into();
    let _ = app.update(Message::Offline(OfflineMsg::Offered(
        "5".into(),
        Ok(fake_offer()),
    )));
}

fn choice(app: &App) -> &crate::offline::Choice {
    match app.offline_views.get("5") {
        Some(OfflineView::Ready(c)) => c,
        other => panic!("no choice: {other:?}"),
    }
}

pub(super) fn queued_job(app: &App, game_id: &str) {
    OfflineJob {
        game_id: game_id.into(),
        os: "windows".into(),
        languages: vec!["en".into()],
        dlcs: Vec::new(),
        folder: "/tmp/[FAKE] installers/[FAKE] Game".into(),
        state: QUEUED.into(),
    }
    .save(&app.core.as_ref().unwrap().db)
    .unwrap();
}

/// An install of game 6, downloading.
pub(super) fn installing_6(app: &mut App) {
    app.install_views.insert(
        "6".into(),
        InstallView::Running {
            title: "[FAKE] Game 6".into(),
            folder: "/games/Game 6".into(),
            progress: Default::default(),
            cancel: Default::default(),
            cancelling: crate::install::Cancelling::No,
            rate: Default::default(),
        },
    );
}

#[test]
fn a_gog_game_not_installed_offers_its_installers_from_manage() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Manage));
    let mut ui = render(&app);
    assert!(ui.find("Verify files").is_err());
    ui.click("Download installers…").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::OpenPanel(Panel::Installers)))
    );
    // Installed, among its other tools.
    open(&mut app, "3", Some(Panel::Manage));
    let mut ui = render(&app);
    assert!(ui.find("Verify files").is_ok());
    assert!(ui.find("Download installers…").is_ok());
}

#[test]
fn the_drawer_offers_each_system_with_its_languages_and_dlc() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Installers));
    assert!(render(&app).find("Asking GOG for its installers…").is_ok());
    offered(&mut app);
    let mut ui = render(&app);
    snapshot(&mut ui, "installers");
    assert!(ui.find("[FAKE] Extra").is_ok());
    assert!(
        ui.find("/tmp/[FAKE] installers/[FAKE] Game 5").is_ok(),
        "a folder of the game's own"
    );
    assert!(ui.find("Download").is_ok());
    // Each language GOG offers, to tick.
    ui.click("French").unwrap();
    assert!(ui.into_messages().any(|m| matches!(
        m,
        Message::Offline(OfflineMsg::ToggleLanguage(id, l)) if id == "5" && l == "fr"
    )));
    let c = choice(&app);
    assert_eq!(
        (c.os.as_str(), c.languages.as_slice()),
        ("windows", &["en".to_string()][..])
    );
    assert_eq!(c.dlcs, ["50"]);
    assert_eq!(c.size(), 4 << 30);

    let toggle = |app: &mut App, l: &str| {
        let _ = app.update(Message::Offline(OfflineMsg::ToggleLanguage(
            "5".into(),
            l.into(),
        )));
    };
    toggle(&mut app, "fr");
    assert_eq!(choice(&app).languages, ["en", "fr"]);
    assert_eq!(
        choice(&app).size(),
        7 << 30,
        "the game in both, the DLC in English"
    );
    toggle(&mut app, "en");
    let _ = app.update(Message::Offline(OfflineMsg::ToggleDlc(
        "5".into(),
        "50".into(),
    )));
    assert_eq!(choice(&app).size(), 3 << 30);
    // The last language stays.
    toggle(&mut app, "fr");
    assert_eq!(choice(&app).languages, ["fr"]);
    // Linux has no French installer, and no DLC.
    let _ = app.update(Message::Offline(OfflineMsg::System(
        "5".into(),
        "linux".into(),
    )));
    let c = choice(&app);
    assert_eq!(
        (c.os.as_str(), c.languages.as_slice()),
        ("linux", &["en".to_string()][..])
    );
    assert!(c.dlcs.is_empty());
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Extra").is_err());
    // One language only: named, not offered as a choice.
    assert!(ui.find("French").is_err());
    assert!(
        ui.find(
            "GOG offers these installers in this language only. Many hold every language of the \
             game, chosen in its own options."
        )
        .is_ok()
    );
    snapshot(&mut ui, "installers-one-language");
}

#[test]
fn installers_wait_in_the_queue_behind_a_running_install() {
    let mut app = library_app();
    installing_6(&mut app);
    open(&mut app, "5", Some(Panel::Installers));
    offered(&mut app);
    assert!(render(&app).find("Add to queue").is_ok());
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert_eq!(app.queue, [entry("5")]);
    assert!(app.offline_run.is_none());
    assert_eq!(app.panel, None);
    let job = OfflineJob::load(&app.core.as_ref().unwrap().db, "5")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, QUEUED);
    assert_eq!(job.dlcs, ["50"]);
    assert_eq!(
        job.folder,
        PathBuf::from("/tmp/[FAKE] installers/[FAKE] Game 5")
    );
    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    snapshot(&mut ui, "installers-queued");
    assert!(ui.find("[FAKE] Game 5 · offline installers").is_ok());
}

/// What GOG offered an account (its DLC among them) goes with it: the next account's drawer asks
/// again.
#[test]
fn the_installers_offered_go_with_the_account() {
    let mut app = library_app();
    offered(&mut app);
    app.logged_out();
    assert!(app.offline_views.is_empty());
}

/// Download, as the drawer says when nothing downloads, starts beside a download paused on request
/// and before the queue it holds.
#[test]
fn installers_download_at_once_beside_a_paused_install() {
    let mut app = library_app();
    app.interrupted
        .push(("6".into(), crate::Interrupted::Paused));
    app.queue = vec!["7".into()];
    open(&mut app, "5", Some(Panel::Installers));
    offered(&mut app);
    assert!(render(&app).find("Download").is_ok());
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["7"]);
}

#[test]
fn an_install_waits_while_installers_download() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert!(app.queue.is_empty());
    assert!(app.downloading());

    ready_to_install(&mut app, "6");
    let _ = app.update(Message::Install(InstallMsg::Start("6".into())));
    assert!(app.installing().is_none());
    assert_eq!(app.queue, ["6"]);

    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    assert!(ui.find("[FAKE] Game 5").is_ok());
    snapshot(&mut ui, "installers-downloading");
    drop(ui);
    let _ = app.update(Message::CloseRequested);
    assert!(
        render(&app)
            .find(
                "Downloading the installers of [FAKE] Game 5 (0%). It stops here and resumes \
                 where it stopped when SlattyLauncher starts again."
            )
            .is_ok()
    );
}

#[test]
fn paused_installers_hold_the_queue_until_resumed() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5"), "6".into()];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert!(app.offline_run.is_none());
    assert_eq!(app.offline_stopped, [("5".to_string(), None)]);
    assert_eq!(app.queue, ["6"], "the queue waits");

    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    ui.click("Resume").unwrap();
    assert!(
        ui.into_messages()
            .any(|m| matches!(m, Message::Offline(OfflineMsg::Resume(id)) if id == "5"))
    );
    let _ = app.update(Message::Offline(OfflineMsg::Resume("5".into())));
    assert!(app.offline_stopped.is_empty());
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6"]);
}

/// Paused installers, as the core leaves them.
fn paused_job(app: &mut App, game_id: &str) {
    let db = app.core.as_ref().unwrap().db.clone();
    queued_job(app, game_id);
    let job = OfflineJob::load(&db, game_id).unwrap().unwrap();
    OfflineJob {
        state: PAUSED.into(),
        ..job
    }
    .save(&db)
    .unwrap();
    app.offline_stopped.push((game_id.into(), None));
}

#[test]
fn resumed_installers_start_beside_a_paused_install_and_stay_resumed() {
    let mut app = library_app();
    paused_job(&mut app, "5");
    app.interrupted
        .push(("6".into(), crate::Interrupted::Paused));
    let _ = app.update(Message::Offline(OfflineMsg::Resume("5".into())));
    assert!(app.offline_stopped.is_empty());
    assert!(
        app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"),
        "the installers asked for start"
    );

    // Resumed while an install runs: first in the queue, and still there after a restart.
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    paused_job(&mut app, "5");
    installing_6(&mut app);
    let _ = app.update(Message::Offline(OfflineMsg::Resume("5".into())));
    assert!(app.offline_run.is_none());
    assert_eq!(app.queue, [entry("5")]);
    let (queue, stopped) = crate::boot::downloads_from(
        app.queue.clone(),
        Vec::new(),
        &OfflineJob::list(&db).unwrap(),
    );
    assert_eq!(queue, [entry("5")]);
    assert!(stopped.is_empty());
}

#[test]
fn installers_out_of_reach_of_gog_keep_their_place() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5"), "6".into()];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Err(Some(("[FAKE] network error".into(), true))),
    )));
    assert_eq!(app.queue, [entry("5"), "6".into()]);
    assert!(app.downloads_waiting.is_some());
    assert!(app.offline_stopped.is_empty());
}

#[test]
fn failed_installers_let_the_queue_go_on() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5"), "6".into()];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Err(Some(("[FAKE] refused".into(), false))),
    )));
    assert_eq!(
        app.offline_stopped,
        [("5".to_string(), Some("[FAKE] refused".to_string()))]
    );
    assert!(app.queue.is_empty(), "game 6 is planned to start");
    assert_eq!(app.auto_resume.as_deref(), Some("6"));
}

#[test]
fn installers_taken_out_of_the_queue_are_forgotten() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    installing_6(&mut app);
    app.queue = vec![entry("5"), "7".into()];
    app.offline_stopped = vec![("8".into(), None)];
    let _ = app.update(Message::Downloads(DownloadsMsg::Remove(entry("5"))));
    assert_eq!(app.queue, ["7"]);
    let _ = app.update(Message::Offline(OfflineMsg::Remove("8".into())));
    assert!(app.offline_stopped.is_empty());
}

#[test]
fn start_up_resumes_installers_cut_off_first_and_keeps_the_stopped_ones_apart() {
    let job = |id: &str, state: &str| OfflineJob {
        game_id: id.into(),
        os: "windows".into(),
        languages: vec!["en".into()],
        dlcs: Vec::new(),
        folder: PathBuf::new(),
        state: state.into(),
    };
    let install = InstallJob {
        game_id: "6".into(),
        build_id: "1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 6".into(),
        state: QUEUED.into(),
        dlcs: Vec::new(),
        proton: None,
    };
    let (queue, stopped) = crate::boot::downloads_from(
        vec![entry("7"), "6".into(), entry("8")],
        vec![install],
        &[
            job("5", DOWNLOADING),
            job("7", QUEUED),
            job("8", PAUSED),
            job("9", FAILED),
            job("10", QUEUED),
        ],
    );
    assert_eq!(
        queue,
        [entry("5"), entry("7"), "6".into(), entry("10")],
        "a paused one is not in the queue"
    );
    assert_eq!(stopped.len(), 2);
    assert_eq!(stopped[0], ("8".to_string(), None));
    assert!(stopped[1].0 == "9" && stopped[1].1.is_some());
}

#[test]
fn the_installers_folder_is_chosen_in_settings() {
    use crate::settings::SettingsMsg;
    let mut app = library_app();
    app.page = Page::Settings;
    let _ = app.update(Message::Settings(SettingsMsg::Show(
        crate::settings::Section::Installs,
    )));
    let db = app.core.clone().unwrap().db;
    assert!(render(&app).find("Installers folder").is_ok());
    let _ = app.update(Message::Settings(SettingsMsg::BrowsedInstallersRoot(Some(
        "/tmp/[FAKE] picked".into(),
    ))));
    assert_eq!(app.installers_root, "/tmp/[FAKE] picked");
    assert_eq!(
        slatty_core::settings::installers_root(&db).unwrap(),
        PathBuf::from("/tmp/[FAKE] picked")
    );
    let _ = app.update(Message::Settings(SettingsMsg::InstallersRootInput(
        "relative".into(),
    )));
    assert_eq!(
        slatty_core::settings::installers_root(&db).unwrap(),
        PathBuf::from("/tmp/[FAKE] picked"),
        "not absolute: kept"
    );
}

/// The install of game 6 paused, as the core leaves its job.
pub(super) fn paused_install_job(app: &App) {
    InstallJob {
        game_id: "6".into(),
        build_id: "1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game 6".into(),
        state: PAUSED.into(),
        dlcs: Vec::new(),
        proton: None,
    }
    .save(&app.core.as_ref().unwrap().db)
    .unwrap();
}

#[test]
fn a_queued_download_started_now_queues_the_running_install_after_it() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    installing_6(&mut app);
    paused_install_job(&app);
    queued_job(&app, "5");
    app.queue = vec!["7".into(), entry("5")];
    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    snapshot(&mut ui, "queue-download-now");
    drop(ui);

    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    assert_eq!(app.queue, [entry("5"), "7".into()]);
    assert!(
        matches!(app.install_views.get("6"), Some(InstallView::Running { cancel, .. }) if cancel.is_cancelled()),
        "the install is paused"
    );
    assert!(app.offline_run.is_none(), "not before it stopped");

    let _ = app.update(Message::Install(InstallMsg::Done("6".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6", "7"], "right after the one started");
    assert!(app.interrupted.is_empty(), "not held as paused");
    assert!(app.preempted.is_none());
    // Saved as started: its resume is checked as one (its folder still there, a game folder already
    // published finished), and it goes on first after a restart.
    let job = InstallJob::load(&app.core.as_ref().unwrap().db, "6")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, DOWNLOADING);
}

#[test]
fn an_install_started_now_queues_the_running_installers_after_it() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    ready_to_install(&mut app, "7");
    let _ = app.update(Message::Install(InstallMsg::Start("7".into())));
    assert_eq!(app.queue, ["7"]);

    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow("7".into())));
    assert!(
        app.offline_run
            .as_ref()
            .is_some_and(|r| r.cancel.is_cancelled())
    );
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert!(app.installing().is_some_and(|(id, _, _)| id == "7"));
    assert_eq!(app.queue, [entry("5")]);
    assert!(app.offline_stopped.is_empty(), "not held as paused");
    let job = OfflineJob::load(&app.core.as_ref().unwrap().db, "5")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, DOWNLOADING, "saved as started");
}

#[test]
fn a_download_started_now_with_nothing_running_starts_at_once() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec!["6".into(), entry("5")];
    // Even a paused one, which holds the queue otherwise.
    app.offline_stopped = vec![("8".into(), None)];
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("5"))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(app.queue, ["6"]);
}

#[test]
fn a_download_the_user_pauses_still_holds_the_queue() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5"), "6".into()];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Pause("5".into())));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert_eq!(app.offline_stopped, [("5".to_string(), None)]);
    assert_eq!(app.queue, ["6"]);
}
