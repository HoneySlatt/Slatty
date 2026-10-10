//! The download queue: installs chosen while another one downloads, started one after the other
//! in an order that can be changed. Each waits as an install job in the `queued` state, so the
//! queue outlives a restart; its order is kept in the settings.

use std::path::PathBuf;

use iced::Task;
use slatty_core::installer::{InstallJob, QUEUED};

use crate::install::{InstallMsg, InstallView};
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
                self.queue.retain(|id| *id != game_id);
                self.save_queue();
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
        }
        Task::none()
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
        };
        if let Err(e) = job.save(&core.db) {
            self.install_views.insert(game_id, InstallView::Ready(info));
            self.notify_error(e.to_string());
            return Task::none();
        }
        self.install_views
            .insert(game_id.clone(), InstallView::Queued(info));
        self.forget_interrupted(&game_id);
        self.queue.push(game_id.clone());
        self.save_queue();
        self.dialog.take_if(|(id, _)| *id == game_id);
        if self.selected.as_deref() == Some(game_id.as_str()) && self.panel == Some(Panel::Install)
        {
            self.panel = None;
        }
        Task::none()
    }

    /// Starts the first install of the queue, unless one is downloading.
    pub fn start_next(&mut self) -> Task<Message> {
        if self.installing().is_some() || self.queue.is_empty() {
            return Task::none();
        }
        let game_id = self.queue.remove(0);
        self.save_queue();
        match self.install_views.remove(&game_id) {
            Some(InstallView::Queued(info)) => {
                self.install_views
                    .insert(game_id.clone(), InstallView::Ready(info));
                self.update_install(InstallMsg::Start(game_id))
            }
            // Not planned yet (just after start-up): planned, then started.
            _ => {
                self.auto_resume = Some(game_id.clone());
                self.update_install(InstallMsg::Prepare(game_id, None))
            }
        }
    }

    fn save_queue(&mut self) {
        if let Some(core) = &self.core
            && let Err(e) = slatty_core::settings::set_download_queue(&core.db, &self.queue)
        {
            self.notify_error(e.to_string());
        }
    }
}
