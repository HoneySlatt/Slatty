//! Launching a game and following its session.

use iced::Task;
use iced::futures::{SinkExt, Stream};
use slatty_core::play::{self, PlayEvent, PlayRequest};
use slatty_core::session;
use tokio::sync::mpsc::UnboundedSender;

use crate::notify::Kind;
use crate::{App, CloudRequest, Core, Message, err, ui};

pub struct PlayState {
    pub game_id: String,
    pub log: Vec<String>,
    pub stop: UnboundedSender<()>,
    pub running: bool,
    /// Keys of the achievements already told while the game ran, not told again once GOG has
    /// them.
    pub told: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum PlayMsg {
    Event(PlayEvent),
    Done(Result<(), String>),
}

impl App {
    /// No game is running and nothing else is working on this game's files or saves.
    pub fn can_play(&self, game_id: &str) -> bool {
        !self.play.as_ref().is_some_and(|p| p.running)
            && !self.maintenance.get(game_id).is_some_and(|m| m.busy)
            && !self.cloud.get(game_id).is_some_and(|c| c.busy)
    }

    pub fn start_game(&mut self, game_id: String) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        if !self.can_play(&game_id) {
            return Task::none();
        }
        // Read after start, from a folder that can be on a drive still waking up: Play waits for it.
        if self.installs.contains_key(&game_id) && !self.launch_options.contains_key(&game_id) {
            self.launch_reading = Some(game_id.clone());
            return self.read_launch_options(vec![game_id.clone()], Some(game_id));
        }
        // A game that can be started several ways asks which, once.
        if self
            .launch_options
            .get(&game_id)
            .is_some_and(|o| o.len() > 1)
            && !self.launch_choices.contains_key(&game_id)
        {
            self.launch_prompt = Some(game_id);
            return Task::none();
        }
        let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
        self.play = Some(PlayState {
            game_id: game_id.clone(),
            log: vec!["Preparing…".into()],
            stop: stop_tx,
            running: true,
            told: Vec::new(),
        });
        Task::run(play_stream(core, game_id, stop_rx), Message::Playing)
    }

    /// What a session tells by a notification: the achievements unlocked (as the game unlocks
    /// them when Comet runs, the others once GOG has them), and cloud saves needing attention or
    /// left out of the upload.
    fn play_notification(&mut self, game_id: &str, event: &PlayEvent) -> Task<Message> {
        let title = self.title_of(game_id);
        let names: Vec<String> = match (event, self.play.as_mut()) {
            (PlayEvent::Achieved(new), Some(p)) => {
                p.told.extend(new.iter().map(|u| u.key.clone()));
                new.iter().map(|u| u.name.clone()).collect()
            }
            (PlayEvent::Unlocked(new), Some(p)) => new
                .iter()
                .filter(|u| !p.told.contains(&u.key))
                .map(|u| u.name.clone())
                .collect(),
            _ => Vec::new(),
        };
        let (kind, heading, body) = match event {
            PlayEvent::Achieved(_) | PlayEvent::Unlocked(_) if names.is_empty() => {
                return Task::none();
            }
            PlayEvent::Achieved(_) | PlayEvent::Unlocked(_) if names.len() == 1 => (
                Kind::Achievements,
                "Achievement unlocked".to_string(),
                format!("{} · {title}", names[0]),
            ),
            PlayEvent::Achieved(_) | PlayEvent::Unlocked(_) => (
                Kind::Achievements,
                format!("{} achievements unlocked", names.len()),
                format!("{} · {title}", names.join(", ")),
            ),
            PlayEvent::Blocked(_) => (
                Kind::Cloud,
                "Cloud saves need attention".to_string(),
                format!("{title} was not started: see Cloud saves on its page."),
            ),
            PlayEvent::CloudUploadSkipped(why) => (
                Kind::Cloud,
                "Cloud saves not uploaded".to_string(),
                format!("{title}: {why}"),
            ),
            PlayEvent::CloudUploaded(summary)
                if !summary.is_clean() || !summary.unreachable.is_empty() =>
            {
                (
                    Kind::Cloud,
                    "Cloud saves not uploaded".to_string(),
                    format!("{title}: see Cloud saves on its page."),
                )
            }
            _ => return Task::none(),
        };
        self.notify(kind, Some(game_id), heading, body)
    }

    /// Keeps how a game is started, for its next launches.
    pub fn choose_launch(&mut self, game_id: &str, choice: String) {
        if let Some(core) = &self.core
            && let Err(e) = slatty_core::settings::set_launch_choice(&core.db, game_id, &choice)
        {
            self.notify_error(e.to_string());
            return;
        }
        self.launch_choices.insert(game_id.to_string(), choice);
    }

    pub fn stop_game(&self) {
        if let Some(p) = &self.play {
            let _ = p.stop.send(());
        }
    }

    pub fn on_play(&mut self, msg: PlayMsg) -> Task<Message> {
        let Some(p) = &mut self.play else {
            return Task::none();
        };
        let result = match msg {
            PlayMsg::Event(event) => {
                p.log.push(ui::format::describe_play_event(&event));
                let game = p.game_id.clone();
                return self.play_notification(&game, &event);
            }
            PlayMsg::Done(result) => result,
        };
        p.running = false;
        match result {
            Ok(()) => p.log.push("Done.".into()),
            Err(e) => p.log.push(format!("Failed: {e}")),
        }
        let game = p.game_id.clone();
        self.cloud.remove(&game);
        self.achievements.remove(&game);
        if let Some(core) = &self.core {
            let user = self.account.as_ref().map(|a| a.user_id.as_str());
            self.playtime = session::playtime(&core.db, user).unwrap_or_default();
        }
        let playtime = self.refresh_playtime(vec![game.clone()]);
        if self.selected.as_deref() == Some(game.as_str()) {
            return Task::batch([
                playtime,
                Task::done(Message::LoadAchievements(game.clone())),
                Task::done(Message::Cloud(game, CloudRequest::Check)),
            ]);
        }
        playtime
    }
}

fn play_stream(
    core: Core,
    game_id: String,
    stop: tokio::sync::mpsc::UnboundedReceiver<()>,
) -> impl Stream<Item = PlayMsg> {
    iced::stream::channel(64, async move |mut output| {
        let req = PlayRequest {
            game_id,
            cloud: true,
            comet: true,
            supervisor: session::supervisor_exe(),
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let run = async {
            play::play(
                &core.db,
                &core.dirs,
                &core.http,
                req,
                move |e| drop(tx.send(e)),
                stop,
            )
            .await
            .map_err(err)
        };
        let mut forward_out = output.clone();
        let forward = async move {
            while let Some(e) = rx.recv().await {
                let _ = forward_out.send(PlayMsg::Event(e)).await;
            }
        };
        let (result, ()) = tokio::join!(run, forward);
        let _ = output.send(PlayMsg::Done(result)).await;
    })
}
