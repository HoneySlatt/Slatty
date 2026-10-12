//! The download queue: installs and offline installers chosen while another download runs, started
//! one after the other in an order that can be changed. Each waits as an install or offline job in
//! the `queued` state, so the queue outlives a restart; its order is kept in the settings.

use std::path::PathBuf;

use iced::Task;
use slatty_core::installer::{InstallJob, QUEUED};

use crate::install::{InstallMsg, InstallView};
use crate::offline::{OfflineMsg, of_entry};
use crate::{App, Message, Panel};

/// Height of a queue row and the space between two, which tell a row from a pointer position.
pub const ROW_HEIGHT: f32 = 70.0;
pub const ROW_SPACING: f32 = 8.0;

#[derive(Debug, Clone)]
pub enum DownloadsMsg {
    /// Takes an install out of the queue and forgets what was chosen for it.
    Remove(String),
    /// A row's handle was pressed: the row follows the pointer until released.
    Grab(usize),
    /// Where the pointer is over the queue, from its top.
    Drag(f32),
    Drop,
    /// Starts a queued download now: the one running is paused and goes back to the queue, right
    /// after it, to go on once it is done.
    StartNow(String),
}

/// Why the launcher paused the download running, which then goes back to the queue rather than
/// waiting to be resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preempt {
    /// To make way for a download started from the queue.
    StartNow,
    /// While a game runs (Settings → Installs): first in the queue, it goes on once the game
    /// closes.
    Play,
}

/// A queue row being moved, from its place to where it would land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    pub from: usize,
    pub to: usize,
}

impl App {
    pub fn update_downloads(&mut self, msg: DownloadsMsg) -> Task<Message> {
        match msg {
            DownloadsMsg::Remove(game_id) => {
                if let Some(id) = of_entry(&game_id) {
                    return self.update_offline(OfflineMsg::Remove(id.to_owned()));
                }
                self.drag = None;
                self.queue.retain(|id| *id != game_id);
                self.save_queue();
                self.starting_now.take_if(|id| *id == game_id);
                self.install_views.remove(&game_id);
                return self.update_install(InstallMsg::Discard(game_id));
            }
            DownloadsMsg::Grab(i) if i < self.queue.len() => {
                self.drag = Some(Drag { from: i, to: i });
            }
            DownloadsMsg::Grab(_) => {}
            DownloadsMsg::Drag(y) => {
                let last = self.queue.len().saturating_sub(1);
                if let Some(d) = &mut self.drag {
                    d.to = ((y / (ROW_HEIGHT + ROW_SPACING)).max(0.0) as usize).min(last);
                }
            }
            DownloadsMsg::Drop => {
                if let Some(d) = self.drag.take()
                    && d.from != d.to
                {
                    let id = self.queue.remove(d.from);
                    self.queue.insert(d.to, id);
                    self.save_queue();
                }
            }
            DownloadsMsg::StartNow(entry) => {
                let Some(i) = self.queue.iter().position(|e| *e == entry) else {
                    return Task::none();
                };
                self.drag = None;
                let entry = self.queue.remove(i);
                self.starting_now = Some(entry.clone());
                self.queue.insert(0, entry);
                self.save_queue();
                if let Some(running) = self.running_entry() {
                    self.preempted = Some((running.clone(), Preempt::StartNow));
                    return self.pause_entry(&running);
                }
                // One being planned to start goes first.
                if self.auto_resume.is_none() {
                    return self.start_first();
                }
            }
        }
        Task::none()
    }

    /// Whether downloads wait for the game running to close (Settings → Installs).
    pub fn held_for_play(&self) -> bool {
        self.pause_while_playing && self.play.as_ref().is_some_and(|p| p.running)
    }

    /// Whether a download started now would wait in the queue.
    pub fn downloads_held(&self) -> bool {
        self.downloading() || self.held_for_play()
    }

    /// Pauses the download running while a game runs; it goes back first in the queue.
    pub fn hold_for_play(&mut self) -> Task<Message> {
        if !self.held_for_play() || self.preempted.is_some() {
            return Task::none();
        }
        let Some(running) = self.running_entry() else {
            return Task::none();
        };
        self.preempted = Some((running.clone(), Preempt::Play));
        self.pause_entry(&running)
    }

    /// The queue entry of the download running.
    fn running_entry(&self) -> Option<String> {
        self.installing()
            .map(|(id, _, _)| id.to_string())
            .or_else(|| {
                let run = self.offline_run.as_ref()?;
                Some(crate::offline::entry(&run.game_id))
            })
    }

    fn pause_entry(&mut self, entry: &str) -> Task<Message> {
        match of_entry(entry) {
            Some(id) => self.update_offline(OfflineMsg::Pause(id.to_owned())),
            None => self.update_install(InstallMsg::Pause(entry.to_owned())),
        }
    }

    /// A download paused by the launcher (`Preempt`), once stopped: queued again, after the one it
    /// made way for, which then starts. `None` when the launcher did not pause it.
    pub(crate) fn requeue_preempted(&mut self, entry: &str) -> Option<Task<Message>> {
        let (_, why) = self.preempted.take_if(|(e, _)| e == entry)?;
        let core = self.core.clone()?;
        let saved = match of_entry(entry) {
            Some(id) => slatty_core::offline::OfflineJob::load(&core.db, id).and_then(|job| {
                job.map_or(Ok(()), |job| {
                    slatty_core::offline::OfflineJob {
                        state: QUEUED.into(),
                        ..job
                    }
                    .save(&core.db)
                })
            }),
            // Under the game's lock, as when it was queued.
            None => slatty_core::lock::game(&core.dirs, entry).and_then(|_busy| {
                InstallJob::load(&core.db, entry)?.map_or(Ok(()), |job| {
                    InstallJob {
                        state: QUEUED.into(),
                        ..job
                    }
                    .save(&core.db)
                })
            }),
        };
        if let Err(e) = saved {
            self.notify_error(e.to_string());
            return None;
        }
        // No longer running, before the next one starts.
        match of_entry(entry) {
            Some(id) => self.offline_stopped.retain(|(g, _)| g != id),
            None => {
                self.forget_interrupted(entry);
                self.install_views.remove(entry);
            }
        }
        self.drag = None;
        let at = match why {
            Preempt::StartNow => 1.min(self.queue.len()),
            Preempt::Play => 0,
        };
        self.queue.insert(at, entry.to_owned());
        self.save_queue();
        let next = match why {
            Preempt::StartNow => self.start_first(),
            // The game may have closed before the download stopped.
            Preempt::Play => self.start_next(),
        };
        // An install is planned again, for its size in the queue.
        let plan = match of_entry(entry) {
            Some(_) => Task::none(),
            None => self.update_install(InstallMsg::Prepare(entry.to_owned(), None)),
        };
        Some(Task::batch([next, plan]))
    }

    /// The queue as shown: while a row is moved, as it would be once dropped.
    pub fn queue_order(&self) -> Vec<&str> {
        let mut order: Vec<&str> = self.queue.iter().map(String::as_str).collect();
        if let Some(d) = self.drag {
            let id = order.remove(d.from);
            order.insert(d.to, id);
        }
        order
    }

    /// Puts an install chosen in its panel at the end of the queue.
    pub fn enqueue(&mut self, game_id: String) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let Some(InstallView::Ready(info)) = self.install_views.remove(&game_id) else {
            return Task::none();
        };
        let job = InstallJob {
            game_id: game_id.clone(),
            build_id: info.build_id.clone(),
            language: info.language.clone(),
            root: PathBuf::from(info.root.trim()),
            directory: info.directory.clone(),
            state: QUEUED.into(),
            dlcs: info
                .dlcs
                .iter()
                .filter(|d| d.selected)
                .map(|d| d.id.clone())
                .collect(),
            proton: info.proton.clone(),
        };
        // Decided now, as the build is chosen: by its turn, GOG may have published a newer one.
        // An interrupted install keeps the decision made when it started. Written under the game's
        // lock: the command line may be installing or updating it, its own job not to be replaced.
        let saved = slatty_core::lock::game(&core.dirs, &game_id).and_then(|_busy| {
            job.save(&core.db)?;
            if info.resumable {
                return Ok(());
            }
            slatty_core::settings::set_held_back(&core.db, &game_id, !info.newest)
        });
        if let Err(e) = saved {
            self.install_views.insert(game_id, InstallView::Ready(info));
            self.notify_error(e.to_string());
            return Task::none();
        }
        self.install_views
            .insert(game_id.clone(), InstallView::Queued(info));
        self.forget_interrupted(&game_id);
        self.drag = None;
        self.queue.push(game_id.clone());
        self.save_queue();
        self.dialog.take_if(|(id, _)| *id == game_id);
        if self.selected.as_deref() == Some(game_id.as_str()) && self.panel == Some(Panel::Install)
        {
            self.panel = None;
        }
        Task::none()
    }

    /// Starts what downloads next, unless something downloads or a game runs while downloads wait
    /// for it: a download cut off (closed window, crash) first, then the first install of the
    /// queue. A download paused on request holds the queue until it is resumed or discarded.
    pub fn start_next(&mut self) -> Task<Message> {
        // One being planned to start is as good as downloading. Waiting for GOG, the queue is
        // tried again by itself (`InstallMsg::Retry`).
        if self.downloads_held() || self.auto_resume.is_some() || self.downloads_waiting.is_some() {
            return Task::none();
        }
        let waiting = |kind| {
            self.interrupted
                .iter()
                .find(|(_, k)| *k == kind)
                .map(|(id, _)| id.clone())
        };
        if let Some(game_id) = waiting(crate::Interrupted::Download) {
            self.auto_resume = Some(game_id.clone());
            // Planned already (its panel asked): it starts once planned.
            if matches!(
                self.install_views.get(&game_id),
                Some(InstallView::Planning)
            ) {
                return Task::none();
            }
            return self.update_install(InstallMsg::Prepare(game_id, None));
        }
        let paused = waiting(crate::Interrupted::Paused).is_some()
            || self
                .offline_stopped
                .iter()
                .any(|(_, failed)| failed.is_none());
        if paused {
            return Task::none();
        }
        self.start_first()
    }

    /// Starts the first download of the queue, whatever waits beside it.
    fn start_first(&mut self) -> Task<Message> {
        if self.queue.is_empty() {
            return Task::none();
        }
        self.drag = None;
        let game_id = self.queue.remove(0);
        self.save_queue();
        if let Some(id) = of_entry(&game_id) {
            return self.update_offline(OfflineMsg::Start(id.to_owned()));
        }
        match self.install_views.remove(&game_id) {
            Some(InstallView::Queued(info)) => {
                self.install_views
                    .insert(game_id.clone(), InstallView::Ready(info));
                self.update_install(InstallMsg::Start(game_id))
            }
            // Planned again (its choices changed): it starts once planned.
            Some(InstallView::Planning) => {
                self.install_views
                    .insert(game_id.clone(), InstallView::Planning);
                self.auto_resume = Some(game_id);
                Task::none()
            }
            // Not planned yet (just after start-up): planned, then started.
            _ => {
                self.auto_resume = Some(game_id.clone());
                self.update_install(InstallMsg::Prepare(game_id, None))
            }
        }
    }

    pub(crate) fn save_queue(&mut self) {
        if let Some(core) = &self.core
            && let Err(e) = slatty_core::settings::set_download_queue(&core.db, &self.queue)
        {
            self.notify_error(e.to_string());
        }
    }
}
