//! Automatic game updates: installed games on their newest build are checked at start and every
//! six hours, and updated one after the other, never while a game runs or an install downloads.

use std::time::Duration;

use iced::Task;
use slatty_core::installer::InstallRecord;
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
    /// The build each was found to update to.
    pub builds: std::collections::HashMap<String, String>,
    /// The game being updated.
    pub current: Option<String>,
}

#[derive(Debug, Clone)]
pub enum UpdatesMsg {
    Toggle(bool),
    Check,
    /// Each game checked, with its update when there is one.
    Checked(Vec<(String, Result<Option<maintenance::UpdateCheck>, String>)>),
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
                return self.account_task(Task::perform(
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
                            .map_err(err);
                            found.push((id, check));
                        }
                        found
                    },
                    |found| Message::Updates(UpdatesMsg::Checked(found)),
                ));
            }
            UpdatesMsg::Checked(found) => {
                self.updates.checking = false;
                // Turned off while the check ran: what it found is not applied.
                if !self.updates.on {
                    return Task::none();
                }
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
                        Ok(found) => {
                            view.update_available = true;
                            view.lines = vec![format!(
                                "Update available: {} → {}. It is applied by itself.",
                                found.installed_version, found.available_version
                            )];
                            let updates = &mut self.updates;
                            updates.builds.insert(id.clone(), found.available_build);
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

    /// Starts the next waiting update when nothing holds it: no update, install, game (or Play
    /// pressed, waiting to start it), cloud sync or other work on a game running. Called after
    /// every message.
    pub fn next_update(&mut self) -> Task<Message> {
        let busy = self.updates.current.is_some()
            || self.updates.waiting.is_empty()
            || self.play.as_ref().is_some_and(|p| p.running)
            || self.launch_reading.is_some()
            || self.launch_prompt.is_some()
            || self.installing().is_some()
            || self.auto_resume.is_some()
            || self.maintenance.values().any(|m| m.busy)
            || self.cloud.values().any(|c| c.busy);
        if busy || !self.updates.on || self.account.is_none() {
            return Task::none();
        }
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        // Since its check, a game may have been moved to an older version or had an update paused,
        // here or from the command line: those choices stand. One busy elsewhere (played from the
        // command line, or since before the interface was restarted) waits for the next check, and
        // one updated elsewhere is not updated again: no error for either.
        while !self.updates.waiting.is_empty() {
            let id = self.updates.waiting.remove(0);
            let held = slatty_core::settings::held_back(&core.db, &id).unwrap_or(true);
            let pending = maintenance::update_pending(&core.db, &id).map_or(true, |p| p.is_some());
            let busy = slatty_core::lock::game(&core.dirs, &id).is_err();
            let found = self.updates.builds.remove(&id);
            let updated = InstallRecord::load(&core.dirs, &id)
                .ok()
                .flatten()
                .is_some_and(|r| Some(&r.build_id) == found.as_ref());
            if updated && let Some(view) = self.maintenance.get_mut(&id) {
                view.update_available = false;
                view.lines.clear();
            }
            if !held && !pending && !busy && !updated {
                self.updates.current = Some(id.clone());
                return self.update_maintenance(MaintenanceMsg::Apply(id, Change::Update));
            }
        }
        Task::none()
    }

    /// An update ended, applied or not: the next one may start. Said by a notification, as any
    /// update (`MaintenanceMsg::Updated`).
    pub fn update_ended(&mut self, game_id: &str, result: &Result<String, Option<String>>) {
        if self.updates.current.as_deref() != Some(game_id) {
            return;
        }
        self.updates.current = None;
        // Paused: it waits until asked, like any paused update, and the others wait for the next
        // check rather than start in its place.
        if let Err(None) = result {
            self.updates.waiting.clear();
        }
    }
}
