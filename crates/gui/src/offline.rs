//! Offline installers downloaded to keep: chosen in their drawer, then downloaded through the
//! download queue, one download at a time with the installs. Their place in the queue is the game
//! id after `ENTRY`; each waits as an offline job, so the queue outlives a restart.

use std::path::PathBuf;
use std::time::Instant;

use iced::Task;
use iced::futures::Stream;
use slatty_core::account::Account;
use slatty_core::installer::{Progress, QUEUED};
use slatty_core::offline::{self, Offer, OfflineJob};
use tokio_util::sync::CancellationToken;

use crate::install::Rate;
use crate::notify::Kind;
use crate::work::progress_stream;
use crate::{App, Core, Message, Panel, err};

pub const ENTRY: &str = "installers:";

/// The queue entry of a game's installers.
pub fn entry(game_id: &str) -> String {
    format!("{ENTRY}{game_id}")
}

/// The game whose installers a queue entry stands for; `None` for an install.
pub fn of_entry(entry: &str) -> Option<&str> {
    entry.strip_prefix(ENTRY)
}

/// What GOG offers for a game, and what is chosen in the drawer.
#[derive(Debug, Clone)]
pub struct Choice {
    pub offer: Offer,
    pub os: String,
    /// Never empty.
    pub languages: Vec<String>,
    pub dlcs: Vec<String>,
}

impl Choice {
    fn new(offer: Offer) -> Self {
        let os = offer.systems().first().copied().unwrap_or("windows");
        let mut choice = Choice {
            offer,
            os: String::new(),
            languages: vec!["en".into()],
            dlcs: Vec::new(),
        };
        choice.set_system(os);
        choice
    }

    /// Another system: the languages chosen it has, else English, else its first; and every DLC
    /// it has installers for.
    fn set_system(&mut self, os: &str) {
        let offered = self.offer.languages(os);
        self.languages.retain(|l| offered.contains(&l.as_str()));
        if self.languages.is_empty() {
            let first = if offered.contains(&"en") {
                Some("en")
            } else {
                offered.first().copied()
            };
            self.languages.extend(first.map(String::from));
        }
        self.dlcs = self.offer.dlcs_for(os).map(|d| d.id.clone()).collect();
        self.os = os.into();
    }

    /// Adds or takes away a language, but never the last one.
    fn toggle_language(&mut self, language: String) {
        match self.languages.iter().position(|l| *l == language) {
            Some(_) if self.languages.len() == 1 => {}
            Some(i) => {
                self.languages.remove(i);
            }
            None => self.languages.push(language),
        }
    }

    /// The download, as GOG's API rounds the sizes.
    pub fn size(&self) -> u64 {
        self.offer
            .chosen(&self.os, &self.languages, &self.dlcs)
            .iter()
            .map(|i| i.size())
            .sum()
    }
}

#[derive(Debug, Clone)]
pub enum OfflineView {
    Loading,
    Ready(Choice),
    Failed(String),
}

/// The installers downloading.
#[derive(Debug)]
pub struct OfflineRun {
    pub game_id: String,
    pub folder: PathBuf,
    pub progress: Progress,
    pub cancel: CancellationToken,
    pub rate: Rate,
}

#[derive(Debug, Clone)]
pub enum OfflineMsg {
    /// Asks GOG what it offers, for the drawer.
    Load(String),
    Offered(String, Result<Offer, String>),
    System(String, String),
    ToggleLanguage(String, String),
    ToggleDlc(String, String),
    /// Puts the choice made in the drawer in the queue.
    Download(String),
    /// Starts a download, its turn come.
    Start(String),
    Progress(String, Progress),
    /// `Err(None)` when paused; else the error, and whether GOG could not be reached.
    Done(String, Result<Vec<PathBuf>, Option<(String, bool)>>),
    Pause(String),
    Resume(String),
    Remove(String),
    Removed(String, Result<(), String>),
}

impl App {
    pub fn update_offline(&mut self, msg: OfflineMsg) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        match msg {
            OfflineMsg::Load(game_id) => {
                self.offline_views
                    .insert(game_id.clone(), OfflineView::Loading);
                let id = game_id.clone();
                return Task::perform(
                    async move {
                        let mut account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                        let tokens = account.tokens(&core.http).await.map_err(err)?.clone();
                        offline::offer(&core.http, &tokens, &id).await.map_err(err)
                    },
                    move |r| Message::Offline(OfflineMsg::Offered(game_id, r)),
                );
            }
            OfflineMsg::Offered(game_id, result) => {
                let view = match result {
                    Ok(offer) if offer.installers.is_empty() => OfflineView::Failed(
                        "GOG offers no installer of this game to download.".into(),
                    ),
                    Ok(offer) => OfflineView::Ready(Choice::new(offer)),
                    Err(e) => OfflineView::Failed(e),
                };
                self.offline_views.insert(game_id, view);
            }
            OfflineMsg::System(game_id, os) => {
                if let Some(OfflineView::Ready(c)) = self.offline_views.get_mut(&game_id) {
                    c.set_system(&os);
                }
            }
            OfflineMsg::ToggleLanguage(game_id, language) => {
                if let Some(OfflineView::Ready(c)) = self.offline_views.get_mut(&game_id) {
                    c.toggle_language(language);
                }
            }
            OfflineMsg::ToggleDlc(game_id, dlc) => {
                if let Some(OfflineView::Ready(c)) = self.offline_views.get_mut(&game_id) {
                    if let Some(i) = c.dlcs.iter().position(|d| *d == dlc) {
                        c.dlcs.remove(i);
                    } else {
                        c.dlcs.push(dlc);
                    }
                }
            }
            OfflineMsg::Download(game_id) => {
                let Some(OfflineView::Ready(c)) = self.offline_views.get(&game_id) else {
                    return Task::none();
                };
                let root = PathBuf::from(self.installers_root.trim());
                if !root.is_absolute() {
                    self.notify_error(
                        "The installers folder must be an absolute path (Settings → Installs)."
                            .into(),
                    );
                    return Task::none();
                }
                let job = OfflineJob {
                    game_id: game_id.clone(),
                    os: c.os.clone(),
                    languages: c.languages.clone(),
                    dlcs: c.dlcs.clone(),
                    folder: offline::folder_for(&root, &c.offer.title),
                    state: QUEUED.into(),
                };
                if let Err(e) = job.save(&core.db) {
                    self.notify_error(e.to_string());
                    return Task::none();
                }
                self.offline_stopped.retain(|(id, _)| *id != game_id);
                let entry = entry(&game_id);
                if !self.queue.contains(&entry) {
                    self.drag = None;
                    self.queue.push(entry);
                    self.save_queue();
                }
                if self.selected.as_deref() == Some(game_id.as_str())
                    && self.panel == Some(Panel::Installers)
                {
                    self.panel = None;
                }
                return self.start_next();
            }
            OfflineMsg::Start(game_id) => {
                let job = match OfflineJob::load(&core.db, &game_id) {
                    Ok(Some(job)) => job,
                    Ok(None) => return self.start_next(),
                    Err(e) => {
                        self.notify_error(e.to_string());
                        return self.start_next();
                    }
                };
                let cancel = CancellationToken::new();
                let started = self.notify(
                    Kind::Downloads,
                    Some(&game_id),
                    "Download started",
                    format!("The installers of {}", self.title_of(&game_id)),
                );
                self.offline_run = Some(OfflineRun {
                    game_id,
                    folder: job.folder.clone(),
                    progress: Progress::default(),
                    cancel: cancel.clone(),
                    rate: Rate::default(),
                });
                return Task::batch([Task::run(stream(core, job, cancel), |m| m), started]);
            }
            OfflineMsg::Progress(game_id, p) => {
                if let Some(run) = &mut self.offline_run
                    && run.game_id == game_id
                {
                    run.progress = p;
                    run.rate
                        .record(Instant::now(), p.bytes_done, p.network_bytes);
                }
            }
            OfflineMsg::Pause(game_id) => {
                if let Some(run) = &self.offline_run
                    && run.game_id == game_id
                {
                    run.cancel.cancel();
                }
            }
            OfflineMsg::Done(game_id, result) => {
                let Some(run) = self.offline_run.take_if(|r| r.game_id == game_id) else {
                    return Task::none();
                };
                let title = self.title_of(&game_id);
                match result {
                    Ok(files) => {
                        self.downloads_waiting = None;
                        self.waiting_told = false;
                        let done = self.notify(
                            Kind::Downloads,
                            Some(&game_id),
                            "Installers downloaded",
                            format!(
                                "{} file(s) of {title}, checked, in {}",
                                files.len(),
                                run.folder.display()
                            ),
                        );
                        return Task::batch([done, self.start_next()]);
                    }
                    // Paused: the queue waits too, as for an install.
                    Err(None) => self.offline_stopped.push((game_id, None)),
                    // GOG out of reach: it keeps its place, and the queue waits for GOG.
                    Err(Some((e, true))) => {
                        self.drag = None;
                        self.queue.insert(0, entry(&game_id));
                        self.save_queue();
                        return self.wait_for_gog(e);
                    }
                    Err(Some((e, false))) => {
                        let failed = self.notify(
                            Kind::Downloads,
                            Some(&game_id),
                            "Download failed",
                            format!("The installers of {title}: {e}"),
                        );
                        self.offline_stopped.push((game_id, Some(e)));
                        return Task::batch([failed, self.start_next()]);
                    }
                }
            }
            OfflineMsg::Resume(game_id) => {
                self.offline_stopped.retain(|(id, _)| *id != game_id);
                self.drag = None;
                self.queue.retain(|e| *e != entry(&game_id));
                self.queue.insert(0, entry(&game_id));
                self.save_queue();
                return self.start_next();
            }
            OfflineMsg::Remove(game_id) => {
                if self
                    .offline_run
                    .as_ref()
                    .is_some_and(|r| r.game_id == game_id)
                {
                    return Task::none();
                }
                self.offline_stopped.retain(|(id, _)| *id != game_id);
                self.drag = None;
                self.queue.retain(|e| *e != entry(&game_id));
                self.save_queue();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            offline::discard(&core.db, &core.dirs, &game_id).map_err(err)?;
                            Ok(game_id)
                        })
                        .await
                    },
                    |r| match r.map_err(err).and_then(|r| r) {
                        Ok(id) => Message::Offline(OfflineMsg::Removed(id, Ok(()))),
                        Err(e) => Message::Offline(OfflineMsg::Removed(String::new(), Err(e))),
                    },
                );
            }
            OfflineMsg::Removed(_, Ok(())) => return self.start_next(),
            OfflineMsg::Removed(_, Err(e)) => self.notify_error(e),
        }
        Task::none()
    }

    /// Whether something downloads from GOG: an install, or installers.
    pub fn downloading(&self) -> bool {
        self.installing().is_some() || self.offline_run.is_some()
    }
}

fn stream(core: Core, job: OfflineJob, cancel: CancellationToken) -> impl Stream<Item = Message> {
    let game_id = job.game_id.clone();
    let id = game_id.clone();
    progress_stream(
        async move |throttle| {
            let result = async {
                let mut account = Account::load(&core.db, &core.dirs).await?;
                let tokens = account.tokens(&core.http).await?.clone();
                offline::run(
                    &core.db,
                    &core.dirs,
                    &core.http,
                    &tokens,
                    &job,
                    &|p| throttle.report(p),
                    &cancel,
                )
                .await
            }
            .await
            .map_err(|e| match e {
                slatty_core::Error::Cancelled => None,
                e => Some((e.to_string(), e.is_temporary())),
            });
            Message::Offline(OfflineMsg::Done(game_id, result))
        },
        move |p| Message::Offline(OfflineMsg::Progress(id.clone(), p)),
    )
}
