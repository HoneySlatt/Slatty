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

/// Stopped installers asked for again from the drawer into the same folder are taken up as
/// started, as the command line's: their run checks that their folder is still there (a drive
/// unplugged) rather than make it again on the system disk.
#[test]
fn stopped_installers_asked_for_again_are_taken_up_as_started() {
    let stopped = |app: &mut App, state: &str| {
        open(app, "5", Some(Panel::Installers));
        offered(app);
        OfflineJob {
            game_id: "5".into(),
            os: "windows".into(),
            languages: vec!["en".into()],
            dlcs: Vec::new(),
            folder: "/tmp/[FAKE] installers/[FAKE] Game 5".into(),
            state: state.into(),
        }
        .save(&app.core.as_ref().unwrap().db)
        .unwrap();
        app.offline_stopped.push(("5".into(), None));
    };
    let state = |app: &App| {
        OfflineJob::load(&app.core.as_ref().unwrap().db, "5")
            .unwrap()
            .unwrap()
            .state
    };
    let mut app = library_app();
    stopped(&mut app, PAUSED);
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    assert_eq!(state(&app), DOWNLOADING);

    // While another download runs: it waits as started, after a restart too.
    let mut app = library_app();
    installing_6(&mut app);
    stopped(&mut app, FAILED);
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert_eq!(app.queue, [entry("5")]);
    assert_eq!(app.goes_on, [entry("5")]);
    assert_eq!(state(&app), DOWNLOADING);
    let jobs = OfflineJob::list(&app.core.as_ref().unwrap().db).unwrap();
    let waiting = crate::boot::downloads_from(app.queue.clone(), Vec::new(), &jobs);
    assert_eq!(waiting.goes_on, [entry("5")]);

    // Into another folder, it starts anew.
    let mut app = library_app();
    stopped(&mut app, PAUSED);
    app.installers_root = "/tmp/[FAKE] other".into();
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert_eq!(state(&app), QUEUED);
}

/// Two downloads the launcher stopped in turn, for Download now, both go on by themselves once
/// their rows are done, though a download paused on request holds the rest of the queue.
#[test]
fn every_download_the_launcher_stopped_goes_on() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    app.interrupted
        .push(("9".into(), crate::Interrupted::Paused));
    for id in ["5", "6", "7"] {
        queued_job(&app, id);
    }
    app.queue = vec![entry("5")];
    let _ = app.start_first();
    app.queue = vec![entry("6"), entry("7")];
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("6"))));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("7"))));
    let _ = app.update(Message::Offline(OfflineMsg::Done("6".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "7"));
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "7".into(),
        Ok(Vec::new()),
    )));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "6"));
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "6".into(),
        Ok(Vec::new()),
    )));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
}

/// GOG's offline installers name some languages with codes of their own ("jp", "br"): they show
/// by their names.
#[test]
fn the_drawer_names_languages_gog_gives_its_own_codes() {
    let mut app = library_app();
    open(&mut app, "5", Some(Panel::Installers));
    app.installers_root = "/tmp/[FAKE] installers".into();
    let offer = Offer {
        title: "[FAKE] Game 5".into(),
        installers: vec![
            installer("windows", "en", 1 << 30),
            installer("windows", "jp", 1 << 30),
            installer("windows", "br", 1 << 30),
        ],
        dlcs: Vec::new(),
    };
    let _ = app.update(Message::Offline(OfflineMsg::Offered("5".into(), Ok(offer))));
    let mut ui = render(&app);
    assert!(ui.find("Japanese").is_ok());
    assert!(ui.find("Portuguese (Brazil)").is_ok());
    assert!(ui.find("jp").is_err());
}

/// The command line downloading the same game's installers keeps its job: the drawer says so.
#[test]
fn installers_downloading_from_the_command_line_are_not_asked_again() {
    let mut app = library_app();
    let core = app.core.clone().unwrap();
    open(&mut app, "5", Some(Panel::Installers));
    offered(&mut app);
    let busy =
        slatty_core::lock::try_acquire(&core.dirs.locks().join("installers-5.lock")).unwrap();
    let _ = app.update(Message::Offline(OfflineMsg::Download("5".into())));
    assert!(OfflineJob::load(&core.db, "5").unwrap().is_none());
    assert!(app.queue.is_empty() && app.offline_run.is_none());
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    drop(busy);
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
    // Saved as started: its run checks that its folder is still there (a drive unplugged), and it
    // goes on by itself.
    assert_eq!(
        OfflineJob::load(&db, "5").unwrap().unwrap().state,
        DOWNLOADING
    );
    assert_eq!(app.goes_on, [entry("5")]);
    let waiting = crate::boot::downloads_from(
        app.queue.clone(),
        Vec::new(),
        &OfflineJob::list(&db).unwrap(),
    );
    assert_eq!(waiting.queue, [entry("5")]);
    assert!(waiting.offline_stopped.is_empty());
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

/// Closed while a row started with Download now downloaded: at the next start that row, cut off,
/// goes first, and the download it stopped keeps its place after it; both go on by themselves.
#[test]
fn start_up_keeps_a_download_the_launcher_stopped_at_its_place() {
    let install = |id: &str| InstallJob {
        game_id: id.into(),
        build_id: "1".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: format!("Game {id}"),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: None,
    };
    // 6 was downloading; 7, stopped for it, waited after it. Jobs come oldest first.
    let waiting =
        crate::boot::downloads_from(vec!["7".into()], vec![install("7"), install("6")], &[]);
    assert_eq!(waiting.queue, ["7"]);
    let cut_off: Vec<&str> = waiting
        .interrupted
        .iter()
        .map(|j| j.game_id.as_str())
        .collect();
    assert_eq!(cut_off, ["6"], "resumed before the queue");
    assert_eq!(waiting.goes_on, ["7"]);

    let job = |id: &str| OfflineJob {
        game_id: id.into(),
        os: "windows".into(),
        languages: vec!["en".into()],
        dlcs: Vec::new(),
        folder: PathBuf::new(),
        state: DOWNLOADING.into(),
    };
    let waiting = crate::boot::downloads_from(vec![entry("7")], Vec::new(), &[job("7"), job("6")]);
    assert_eq!(waiting.queue, [entry("6"), entry("7")]);
    assert_eq!(waiting.goes_on, [entry("6"), entry("7")]);
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
    let waiting = crate::boot::downloads_from(
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
        waiting.queue,
        [entry("5"), entry("7"), "6".into()],
        "a paused one is not in the queue"
    );
    // 10, queued but out of the saved order, was taken out to start and stopped before it began.
    let stopped = waiting.offline_stopped;
    assert_eq!(stopped.len(), 3);
    assert_eq!(stopped[0], ("8".to_string(), None));
    assert!(stopped[1].0 == "9" && stopped[1].1.is_some());
    assert!(stopped[2].0 == "10" && stopped[2].1.is_some());
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

/// The row Download now asked for during a game starts though another came first meanwhile, also
/// when the download it paused fails, or is cut off by GOG, rather than pauses.
#[test]
fn download_now_during_a_game_starts_its_row_when_the_download_fails_or_is_cut_off() {
    use crate::downloads::DownloadsMsg;
    for (error, temporary) in [("[FAKE] disk error", false), ("[FAKE] network error", true)] {
        let mut app = library_app();
        queued_job(&app, "5");
        queued_job(&app, "8");
        app.queue = vec![entry("5")];
        let _ = app.start_next();
        paused_job(&mut app, "9");
        app.queue = vec![entry("8")];
        app.launch_options.insert("3".into(), Vec::new());
        let _ = app.update(Message::Play("3".into()));
        let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("8"))));
        let _ = app.update(Message::Offline(OfflineMsg::Resume("9".into())));
        let _ = app.update(Message::Offline(OfflineMsg::Done(
            "5".into(),
            Err(Some((error.into(), temporary))),
        )));
        if temporary {
            let _ = app.update(Message::Install(crate::install::InstallMsg::Retry));
        }
        assert!(
            app.offline_run.as_ref().is_some_and(|r| r.game_id == "8"),
            "{error}"
        );
    }
}

/// Installers cut off because GOG could not be reached go on first once it answers, beside a
/// download paused on request, as they would after a restart; cut off before their run began
/// (saved as never started), they wait as never started.
#[test]
fn installers_cut_off_by_the_network_go_on_beside_a_paused_download() {
    for started in [true, false] {
        let mut app = library_app();
        app.interrupted
            .push(("6".into(), crate::Interrupted::Paused));
        queued_job(&app, "5");
        app.queue = vec![entry("5")];
        let _ = app.start_first();
        assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
        // As the run leaves it once its folder is made.
        if started {
            let db = &app.core.as_ref().unwrap().db;
            let job = OfflineJob::load(db, "5").unwrap().unwrap();
            OfflineJob {
                state: DOWNLOADING.into(),
                ..job
            }
            .save(db)
            .unwrap();
        }
        let _ = app.update(Message::Offline(OfflineMsg::Done(
            "5".into(),
            Err(Some(("[FAKE] network error".into(), true))),
        )));
        assert_eq!(app.queue, [entry("5")]);
        let _ = app.update(Message::Install(crate::install::InstallMsg::Retry));
        let run = app.offline_run.as_ref().is_some_and(|r| r.game_id == "5");
        assert_eq!(run, started);
        let jobs = OfflineJob::list(&app.core.as_ref().unwrap().db).unwrap();
        let waiting = crate::boot::downloads_from(app.queue.clone(), Vec::new(), &jobs);
        if !started {
            assert_eq!(app.goes_on, waiting.goes_on, "as after a restart");
        }
    }
}

/// The row Download now asked for during a game starts once the download it paused stopped, though
/// another row was put first meanwhile; the stopped download goes right after it.
#[test]
fn download_now_during_a_game_starts_its_row_though_another_came_first() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    queued_job(&app, "5");
    queued_job(&app, "8");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    paused_job(&mut app, "9");
    app.queue = vec![entry("8")];
    app.launch_options.insert("3".into(), Vec::new());
    let _ = app.update(Message::Play("3".into()));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("8"))));
    let _ = app.update(Message::Offline(OfflineMsg::Resume("9".into())));
    assert_eq!(app.queue, [entry("9"), entry("8")]);
    // The row said to be starting is the one asked for, on both screens.
    let rows = app.download_rows();
    let eight = rows.iter().find(|r| r.game_id == "8").unwrap();
    assert_eq!(eight.status, "Starting…");
    app.page = Page::Downloads;
    app.selected = None;
    let mut ui = render(&app);
    let mut middle = |wanted: &str| {
        use iced_test::selector::Candidate;
        ui.find(|c: Candidate<'_>| match c {
            Candidate::Text {
                content, bounds, ..
            } if content == wanted => Some(bounds.center_y()),
            _ => None,
        })
        .unwrap()
    };
    let starting = middle("Starting…");
    let row = middle("[FAKE] Game 8 · offline installers");
    assert!((starting - row).abs() < 1.0, "{starting} and {row}");
    drop(ui);
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "8"));
    assert_eq!(app.queue, [entry("5"), entry("9")]);
}

/// Installers stopped before their run began (their folder could not be made, the keyring locked)
/// are resumed as never started: their run makes the folder rather than take it for a drive gone.
#[test]
fn installers_that_never_started_are_resumed_as_never_started() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Err(Some(("[FAKE] permission denied".into(), false))),
    )));
    let _ = app.update(Message::Offline(OfflineMsg::Resume("5".into())));
    let job = OfflineJob::load(&app.core.as_ref().unwrap().db, "5")
        .unwrap()
        .unwrap();
    assert_eq!(job.state, QUEUED);
    assert!(app.offline_run.as_ref().is_some_and(|r| r.game_id == "5"));
}

/// Installers resumed before their run began, while another download runs, wait as never started:
/// no pass past a download paused on request, as after a restart.
#[test]
fn installers_resumed_before_they_began_wait_as_never_started() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Err(Some(("[FAKE] permission denied".into(), false))),
    )));
    installing_6(&mut app);
    let _ = app.update(Message::Offline(OfflineMsg::Resume("5".into())));
    assert_eq!(app.queue, [entry("5")]);
    let jobs = OfflineJob::list(&app.core.as_ref().unwrap().db).unwrap();
    let waiting = crate::boot::downloads_from(app.queue.clone(), Vec::new(), &jobs);
    assert!(app.goes_on.is_empty());
    assert_eq!(app.goes_on, waiting.goes_on);
}

/// Installers stopped before their run began (their folder could not be made, a drive not there)
/// stay listed as stopped after a restart, rather than start again by themselves: the drive may
/// still be missing, its empty mount point then filled.
#[test]
fn installers_stopped_before_they_began_stay_stopped_after_a_restart() {
    let mut app = library_app();
    queued_job(&app, "5");
    app.queue = vec![entry("5")];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Done(
        "5".into(),
        Err(Some(("[FAKE] permission denied".into(), false))),
    )));
    assert!(app.queue.is_empty());
    let jobs = OfflineJob::list(&app.core.as_ref().unwrap().db).unwrap();
    let waiting = crate::boot::downloads_from(app.queue.clone(), Vec::new(), &jobs);
    assert!(waiting.queue.is_empty());
    assert!(matches!(&waiting.offline_stopped[..], [(id, Some(_))] if id == "5"));
}

#[test]
fn installers_removed_before_download_now_started_them_wait_for_a_pause_once_queued_again() {
    use crate::downloads::DownloadsMsg;
    let mut app = library_app();
    queued_job(&app, "5");
    queued_job(&app, "8");
    app.queue = vec![entry("5"), entry("8")];
    let _ = app.start_next();
    let _ = app.update(Message::Offline(OfflineMsg::Pause("5".into())));
    let _ = app.update(Message::Downloads(DownloadsMsg::StartNow(entry("8"))));
    let _ = app.update(Message::Downloads(DownloadsMsg::Remove(entry("8"))));
    let _ = app.update(Message::Offline(OfflineMsg::Done("5".into(), Err(None))));
    assert_eq!(app.offline_stopped, [("5".to_string(), None)]);

    queued_job(&app, "8");
    app.queue = vec![entry("8")];
    let _ = app.start_next();
    assert!(
        app.offline_run.is_none(),
        "the paused download holds the queue"
    );
}
