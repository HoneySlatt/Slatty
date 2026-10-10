//! Automatic game updates: installed games on their newest build are checked at start and every
//! six hours, and updated one after the other, never while a game runs or an install downloads.

use std::time::Duration;

use iced::Task;
use slatty_core::maintenance::{self, Change};

use crate::maintenance::MaintenanceMsg;
use crate::work::tokens;
use crate::{App, Message, err};

/// How often games are checked while the application stays open.
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Default)]
pub struct AutoUpdates {
    /// Settings → Installs, on until turned off.
    pub on: bool,
    pub checking: bool,
    /// Games with an update found, waiting their turn.
    pub waiting: Vec<String>,
    /// The game being updated.
    pub current: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UpdatesMsg {
    Toggle(bool),
    Check,
    /// Each game checked, with the versions of its update when there is one.
    Checked(Vec<(String, Result<Option<String>, String>)>),
}

impl App {
    pub fn update_updates(&mut self, msg: UpdatesMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            UpdatesMsg::Toggle(on) => {
                self.updates.on = on;
                if !on {
                    self.updates.waiting.clear();
                }
                if let Err(e) = slatty_core::settings::set_auto_update(&core.db, on) {
                    self.notify_error(e.to_string());
                }
                if on {
                    return self.update_updates(UpdatesMsg::Check);
                }
            }
            UpdatesMsg::Check => {
                if !self.updates.on || self.updates.checking || self.account.is_none() {
                    return Task::none();
                }
                self.updates.checking = true;
                return Task::perform(
                    async move {
                        let tokens = match tokens(&core).await {
                            Ok(t) => t,
                            Err(_) => return Vec::new(),
                        };
                        let ids = maintenance::auto_update_candidates(&core.db, &core.dirs)
                            .unwrap_or_default();
                        let mut found = Vec::new();
                        for id in ids {
                            // An unfinished change is finished at start already.
                            if maintenance::update_pending(&core.db, &id).is_ok_and(|p| p.is_some())
                            {
                                continue;
                            }
                            let check = maintenance::check_update(
                                &core.db, &core.dirs, &core.http, &tokens, &id,
                            )
                            .await
                            .map(|u| {
                                u.map(|u| {
                                    format!("{} → {}", u.installed_version, u.available_version)
                                })
                            })
                            .map_err(err);
                            found.push((id, check));
                        }
                        found
                    },
                    |found| Message::Updates(UpdatesMsg::Checked(found)),
                );
            }
            UpdatesMsg::Checked(found) => {
                self.updates.checking = false;
                for (id, check) in found {
                    // Up to date: nothing to say.
                    let Some(check) = check.transpose() else {
                        continue;
                    };
                    let view = self.maintenance.entry(id.clone()).or_default();
                    if view.busy {
                        continue;
                    }
                    match check {
                        Ok(what) => {
                            view.update_available = true;
                            view.lines = vec![format!(
                                "Update available: {what}. It is applied by itself."
                            )];
                            let updates = &mut self.updates;
                            if updates.current.as_ref() != Some(&id)
                                && !updates.waiting.contains(&id)
                            {
                                updates.waiting.push(id);
                            }
                        }
                        Err(e) => view.lines = vec![format!("Automatic update check failed: {e}")],
                    }
                }
            }
        }
        Task::none()
    }

    /// Starts the next waiting update when nothing holds it: no update, install, game or other
    /// work on a game running. Called after every message.
    pub fn next_update(&mut self) -> Task<Message> {
        let busy = self.updates.current.is_some()
            || self.updates.waiting.is_empty()
            || self.play.as_ref().is_some_and(|p| p.running)
            || self.installing().is_some()
            || self.auto_resume.is_some()
            || self.maintenance.values().any(|m| m.busy);
        if busy || !self.updates.on || self.account.is_none() {
            return Task::none();
        }
        let id = self.updates.waiting.remove(0);
        self.updates.current = Some(id.clone());
        self.update_maintenance(MaintenanceMsg::Apply(id, Change::Update))
    }

    /// An update ended, applied or not: the next one may start.
    pub fn update_ended(&mut self, game_id: &str, result: &Result<String, Option<String>>) {
        if self.updates.current.as_deref() != Some(game_id) {
            return;
        }
        self.updates.current = None;
        let title = self.title_of(game_id);
        match result {
            Ok(summary) => {
                self.notice = Some(crate::Notice {
                    error: false,
                    text: format!("{title} updated. {summary}"),
                });
            }
            Err(Some(e)) => self.notify_error(format!("{title} could not be updated: {e}")),
            // Paused: it waits until asked, like any paused update.
            Err(None) => {}
        }
    }
}
