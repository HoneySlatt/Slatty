use super::*;
use crate::notify::{Kind, NotifyMsg, SHOWN_FOR};
use crate::play::{PlayMsg, PlayState};
use slatty_core::achievements::Unlock;
use slatty_core::play::PlayEvent;

fn unlocks(list: &[(&str, &str)]) -> Vec<Unlock> {
    list.iter()
        .map(|(key, name)| Unlock {
            key: key.to_string(),
            name: name.to_string(),
        })
        .collect()
}

/// A download says when it starts and when it is done, in a card in the window's corner; the card
/// goes after a few seconds, and a click opens the game's page.
#[test]
fn a_download_says_when_it_starts_and_ends_in_a_card() {
    let mut app = library_app();
    ready_to_install(&mut app, "5");
    let _ = app.update(Message::Install(InstallMsg::Start("5".into())));
    assert_eq!(
        toasted(&app),
        [("Download started".to_string(), "[FAKE] Game 5".to_string())]
    );
    let _ = app.update(Message::Install(InstallMsg::Done(
        "5".into(),
        Ok(Install {
            umu_id: None,
            game_id: "5".into(),
            title: "[FAKE] Game 5".into(),
            platform: Platform::Windows,
            path: "/games/Game 5".into(),
            client_id: None,
            runner: Runner::Native,
            isolated: false,
        }),
    )));
    assert_eq!(
        toasted(&app).last(),
        Some(&(
            "Download complete".to_string(),
            "[FAKE] Game 5 is ready to play.".to_string()
        ))
    );
    {
        let mut ui = render(&app);
        snapshot(&mut ui, "notifications");
        ui.click("Download complete").unwrap();
        for m in ui.into_messages().collect::<Vec<_>>() {
            let _ = app.update(m);
        }
    }
    assert_eq!(app.selected.as_deref(), Some("5"), "its page opens");
    assert_eq!(app.toasts.len(), 1, "the card clicked goes");

    let later = std::time::Instant::now() + SHOWN_FOR;
    let _ = app.update(Message::Notify(NotifyMsg::Expire(later)));
    assert!(app.toasts.is_empty(), "gone once its time is over");
}

/// A card is taken away by one wait of its own, not by a tick every half second while it shows,
/// each tick a view, a layout and a frame: 12 for a card shown its six seconds.
#[test]
fn a_card_goes_after_one_wait_of_its_own() {
    let mut app = library_app();
    let task = app.notify(Kind::Downloads, Some("5"), "Download complete", "[FAKE]");
    assert_eq!(task.units(), 1, "its wait");
    assert_eq!(app.toasts.len(), 1);
    let ticks = format!("{:?}", app.subscription());
    let none = format!("{:?}", {
        let mut quiet = library_app();
        quiet.toasts.clear();
        quiet.subscription()
    });
    assert_eq!(ticks, none, "no tick while it shows");
}

/// A long notification stays a small card: its title takes two lines at most, its text four.
#[test]
fn a_long_notification_stays_a_small_card() {
    let mut app = library_app();
    let title = "[FAKE] The very long title of a game ".repeat(5);
    let _ = app.notify(
        Kind::Updates,
        Some("3"),
        format!("{title}updated"),
        format!("{title}: {}", "a long reason given by GOG ".repeat(10)),
    );
    let mut ui = render(&app);
    let heading = ui.find(app.toasts[0].title.as_str()).unwrap().bounds();
    assert!(heading.height <= 2.0 * 15.0 * 1.3 + 0.5, "{heading:?}");
    let body = ui.find(app.toasts[0].body.as_str()).unwrap().bounds();
    assert!(body.height <= 4.0 * 13.0 * 1.3 + 0.5, "{body:?}");
}

/// The page a card opens goes back to the library where it was scrolled.
#[test]
fn a_game_opened_from_a_card_goes_back_to_where_the_library_was() {
    let (mut app, offset) = scrolled_library();
    let _ = app.notify(Kind::Downloads, Some("5"), "Download complete", "[FAKE]");
    let card = app.toasts[0].id;
    let _ = app.update(Message::Notify(NotifyMsg::Open(card)));
    assert_eq!(app.selected.as_deref(), Some("5"));
    let _ = app.update(Message::CloseDetail);
    assert_eq!(app.grid_view.map(|v| v.absolute_offset().y), Some(offset));
}

/// A card about a game the library no longer lists (signed out of GOG since its download ended)
/// opens nothing: the page it would select does not exist, and the dialogs over the library,
/// drawn only with no game selected, would no longer show.
#[test]
fn a_card_about_a_game_no_longer_listed_opens_nothing() {
    let mut app = library_app();
    let _ = app.notify(Kind::Downloads, Some("5"), "Download complete", "[FAKE]");
    let card = app.toasts[0].id;
    app.logged_out();
    let _ = app.update(Message::Notify(NotifyMsg::Open(card)));
    assert!(app.selected.is_none());
    assert!(app.toasts.is_empty(), "the card goes");
}

/// Achievements unlocked in a game are said once it ends, the only time the launcher learns them.
#[test]
fn achievements_unlocked_are_said_when_the_game_ends() {
    let mut app = library_app();
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "3".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Unlocked(
        unlocks(&[("first", "[FAKE] First Steps")]),
    ))));
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Unlocked(
        unlocks(&[("alpha", "[FAKE] Alpha"), ("beta", "[FAKE] Beta")]),
    ))));
    let _ = app.update(Message::Playing(PlayMsg::Event(
        PlayEvent::CloudUploadSkipped("the end of the session is uncertain".into()),
    )));
    assert_eq!(
        toasted(&app),
        [
            (
                "Achievement unlocked".to_string(),
                "[FAKE] First Steps · [FAKE] Game 3".to_string()
            ),
            (
                "2 achievements unlocked".to_string(),
                "[FAKE] Alpha, [FAKE] Beta · [FAKE] Game 3".to_string()
            ),
            (
                "Cloud saves not uploaded".to_string(),
                "[FAKE] Game 3: the end of the session is uncertain".to_string()
            ),
        ]
    );
}

/// Saves left out of the upload after a game (a conflict, a file refused or failing) are told like
/// an upload skipped as a whole; a clean upload is not.
#[test]
fn saves_left_out_of_the_upload_after_a_game_are_told() {
    use slatty_core::play::CloudSummary;
    let mut app = library_app();
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "3".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::CloudUploaded(
        CloudSummary {
            uploaded: 2,
            ..Default::default()
        },
    ))));
    assert!(app.toasts.is_empty(), "all uploaded");
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::CloudUploaded(
        CloudSummary {
            uploaded: 1,
            conflicts: vec!["saves/[FAKE] slot1.sav (BothChanged)".into()],
            ..Default::default()
        },
    ))));
    assert_eq!(
        toasted(&app),
        [(
            "Cloud saves not uploaded".to_string(),
            "[FAKE] Game 3: see Cloud saves on its page.".to_string()
        )]
    );
}

/// With Comet, an achievement is told as the game unlocks it, and not again once GOG has it; one
/// only GOG reports at the end is still told then.
#[test]
fn achievements_are_said_as_the_game_unlocks_them() {
    let mut app = library_app();
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "3".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Achieved(
        unlocks(&[("first", "[FAKE] First Steps")]),
    ))));
    assert_eq!(
        toasted(&app),
        [(
            "Achievement unlocked".to_string(),
            "[FAKE] First Steps · [FAKE] Game 3".to_string()
        )]
    );
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Unlocked(
        unlocks(&[("first", "[FAKE] First Steps")]),
    ))));
    assert_eq!(toasted(&app).len(), 1, "not told twice");
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Unlocked(
        unlocks(&[("first", "[FAKE] First Steps"), ("alpha", "[FAKE] Alpha")]),
    ))));
    assert_eq!(
        toasted(&app).last(),
        Some(&(
            "Achievement unlocked".to_string(),
            "[FAKE] Alpha · [FAKE] Game 3".to_string()
        ))
    );
    assert_eq!(
        app.play.as_ref().unwrap().log,
        [
            "Achievement unlocked: [FAKE] First Steps",
            "Achievements recorded on GOG: [FAKE] First Steps",
            "Achievements recorded on GOG: [FAKE] First Steps, [FAKE] Alpha",
        ]
    );
}

/// Comet asks GOG for names in the system's language, the launcher does not: an achievement told
/// in game under its French name is the same one GOG then reports under its English name.
#[test]
fn an_achievement_named_in_another_language_is_not_told_twice() {
    let mut app = library_app();
    let (stop, _rx) = tokio::sync::mpsc::unbounded_channel();
    app.play = Some(PlayState {
        game_id: "3".into(),
        log: Vec::new(),
        stop,
        running: true,
        told: Vec::new(),
    });
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Achieved(
        unlocks(&[("first", "[FAKE] Premiers pas")]),
    ))));
    let _ = app.update(Message::Playing(PlayMsg::Event(PlayEvent::Unlocked(
        unlocks(&[("first", "[FAKE] First Steps")]),
    ))));
    assert_eq!(
        toasted(&app),
        [(
            "Achievement unlocked".to_string(),
            "[FAKE] Premiers pas · [FAKE] Game 3".to_string()
        )]
    );
}

/// With the window in the background (a game in front), the desktop's notification is used: no
/// card, which no one would see.
#[test]
fn an_unfocused_window_leaves_it_to_the_desktop() {
    let mut app = library_app();
    let _ = app.update(Message::Notify(NotifyMsg::Focus(false)));
    let task = app.notify(Kind::Downloads, Some("5"), "Download complete", "[FAKE]");
    assert!(app.toasts.is_empty());
    assert!(task.units() > 0, "sent to the desktop");
    let _ = app.update(Message::Notify(NotifyMsg::Focus(true)));
    let _ = app.notify(Kind::Downloads, Some("5"), "Download complete", "[FAKE]");
    assert_eq!(app.toasts.len(), 1);
}

/// Each kind can be turned off, and all of them at once; the choice is kept.
#[test]
fn notifications_can_be_turned_off_kind_by_kind() {
    let mut app = library_app();
    let db = app.core.as_ref().unwrap().db.clone();
    let _ = app.update(Message::Notify(NotifyMsg::Switch(
        Some(Kind::Achievements),
        false,
    )));
    let told = app.notify(Kind::Achievements, None, "Achievement unlocked", "[FAKE]");
    assert!(app.toasts.is_empty() && told.units() == 0);
    let _ = app.notify(Kind::Downloads, None, "Download started", "[FAKE]");
    assert_eq!(app.toasts.len(), 1, "the others stay on");

    let _ = app.update(Message::Notify(NotifyMsg::Switch(None, false)));
    let _ = app.notify(Kind::Updates, None, "Proton updated", "[FAKE]");
    assert_eq!(app.toasts.len(), 1, "none at all");
    let kept = crate::notify::Switches::read(&db);
    assert!(!kept.all && !kept.kinds[Kind::Achievements as usize]);
    assert!(kept.kinds[Kind::Downloads as usize]);

    app.page = Page::Settings;
    app.window = Size::new(1440.0, 2400.0);
    let mut ui = render_window(&app);
    assert!(ui.find("Notifications").is_ok() && ui.find("Cloud saves").is_ok());
    snapshot(&mut ui, "settings-notifications");
}
