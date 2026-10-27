//! Automatic game updates: installed games on their newest build are checked at start and every
//! six hours, and updated one after the other, never while a game runs or an install downloads;
//! one being applied when a game starts pauses, and goes on once the game closes.

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
    /// The game whose update paused for a game played: it goes on first once the game closes.
    pub held: Option<String>,
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
                if !self.updates.on || self.updates.checking || !self.signed_in_somewhere() {
                    return Task::none();
                }
                self.updates.checking = true;
                let (gog, epic) = (self.account.is_some(), self.epic.account.is_some());
                return self.account_task(Task::perform(
                    async move {
                        let tokens = match gog {
                            true => tokens(&core).await.ok(),
                            false => None,
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
                            // Each store's games while signed in to it.
                            let check = if slatty_core::store::epic_app(&id).is_some() {
                                if !epic {
                                    continue;
                                }
                                slatty_core::epic::install::check_update(
                                    &core.db, &core.dirs, &core.http, &id,
                                )
                                .await
                            } else if let Some(tokens) = &tokens {
                                maintenance::check_update(
                                    &core.db, &core.dirs, &core.http, tokens, &id,
                                )
                                .await
                            } else {
                                continue;
                            };
                            found.push((id, check.map_err(err)));
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

    /// Pauses the update being applied when a game starts, as the download running is: it goes on
    /// once the game closes. One the user is pausing stays paused.
    pub fn hold_update(&mut self) -> Task<Message> {
        let Some(id) = self.updates.current.clone() else {
            return Task::none();
        };
        let cancel = self.maintenance.get(&id).and_then(|v| v.cancel.as_ref());
        if self.updates.held.is_some() || cancel.is_none_or(|c| c.is_cancelled()) {
            return Task::none();
        }
        self.updates.held = Some(id.clone());
        self.update_maintenance(MaintenanceMsg::Pause(id))
    }

    /// Starts the next waiting update when nothing holds it: no update, download (an install or
    /// offline installers), game (a Steam game too, or Play pressed, waiting to start it), cloud
    /// sync or other work on a game running. One paused for a game goes first. Called after every
    /// message.
    pub fn next_update(&mut self) -> Task<Message> {
        let busy = self.updates.current.is_some()
            || self.play.as_ref().is_some_and(|p| p.running)
            || (self.steam.on && !self.steam.running.is_empty())
            || self.launch_reading.is_some()
            || self.launch_prompt.is_some()
            || self.downloading()
            || self.auto_resume.is_some()
            || self.maintenance.values().any(|m| m.busy)
            || self.cloud.values().any(|c| c.busy);
        if busy || !self.signed_in_somewhere() {
            return Task::none();
        }
        // Begun already, the game cannot start until it is finished: it goes on even if
        // automatic updates were turned off meanwhile.
        if let Some(id) = self.updates.held.take() {
            self.updates.current = Some(id.clone());
            return self.update_maintenance(MaintenanceMsg::Apply(id, Change::Update));
        }
        if self.updates.waiting.is_empty() || !self.updates.on {
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

    /// Signed in to GOG or to Epic, whose games' updates are looked for.
    fn signed_in_somewhere(&self) -> bool {
        self.account.is_some() || self.epic.account.is_some()
    }

    /// An update ended, applied or not: the next one may start. Said by a notification, as any
    /// update (`MaintenanceMsg::Updated`).
    pub fn update_ended(&mut self, game_id: &str, result: &Result<String, Option<String>>) {
        if self.updates.current.as_deref() != Some(game_id) {
            return;
        }
        self.updates.current = None;
        let held = self.updates.held.as_deref() == Some(game_id);
        match result {
            // Paused for a game: it goes on once the game closes (`next_update`), and at the next
            // start if Slatty is closed meanwhile.
            Err(None) if held => {
                if let Some(core) = self.core.clone()
                    && let Err(e) = maintenance::resume_later(&core.db, &core.dirs, game_id)
                {
                    self.notify_error(e.to_string());
                }
            }
            // Paused: it waits until asked, like any paused update, and the others wait for the
            // next check rather than start in its place.
            Err(None) => self.updates.waiting.clear(),
            _ => {
                self.updates.held.take_if(|h| h == game_id);
            }
        }
    }
}
