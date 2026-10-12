//! Signing in through the browser, signing out, and the account's avatar.

use iced::Task;
use slatty_core::account::{Account, AccountInfo};
use slatty_core::library::{self, LibraryCache};
use slatty_core::overview;

use crate::{App, Message, Page, err};

impl App {
    /// Account results cannot refill cleared state, even after signing back into the same account.
    pub fn account_task(&self, task: Task<Message>) -> Task<Message> {
        let epoch = self.account_epoch;
        task.map(move |message| Message::AccountResult(epoch, Box::new(message)))
    }

    pub fn open_login_page(&mut self) {
        let url = slatty_core::auth::login_url();
        if slatty_core::auth::open_in_browser(&url).is_err() {
            self.notify_error(format!("Could not open a browser. Open: {url}"));
        }
    }

    pub fn submit_login(&mut self) -> Task<Message> {
        let (Some(core), false) = (self.core.clone(), self.login_busy) else {
            return Task::none();
        };
        let code = match slatty_core::auth::extract_code(&self.login_input) {
            Ok(c) => c,
            Err(e) => {
                self.notify_error(e.to_string());
                return Task::none();
            }
        };
        self.login_busy = true;
        Task::perform(
            async move {
                let account = Account::login(&core.http, &core.db, &core.dirs, &code)
                    .await
                    .map_err(err)?;
                let cache = library::load_cache(&core.dirs, &account.info.user_id).map_err(err)?;
                Ok((account.info, cache))
            },
            Message::LoggedIn,
        )
    }

    pub fn logged_in(
        &mut self,
        result: Result<(AccountInfo, Option<LibraryCache>), String>,
    ) -> Task<Message> {
        self.login_busy = false;
        self.login_input.clear();
        let (info, cache) = match result {
            Ok(r) => r,
            Err(e) => {
                self.notify_error(e);
                return Task::none();
            }
        };
        if let Some(core) = &self.core {
            self.overview = overview::load(&core.dirs, &info.user_id).unwrap_or_default();
            self.playtime =
                slatty_core::session::playtime(&core.db, Some(&info.user_id)).unwrap_or_default();
        }
        self.account_epoch += 1;
        // Asked again for this account, whose answer the old one's would never have been.
        self.offline_views.clear();
        // Its cache is its own again, work of before included.
        self.forgotten.retain(|id| *id != info.user_id);
        self.account = Some(info);
        self.login_open = false;
        let avatar = self.fetch_avatar();
        let updates = self.update_updates(crate::updates::UpdatesMsg::Check);
        Task::batch([
            avatar,
            updates,
            match cache {
                Some(c) => self.set_library(c),
                None => Task::done(Message::SyncLibrary),
            },
        ])
    }

    /// Signs out; `forget` also deletes what is kept of the account (`Account::sign_out`).
    pub fn logout(&mut self, forget: bool) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                Account::sign_out(&core.db, &core.dirs, forget)
                    .await
                    .map(|account| forget.then_some(account.user_id))
                    .map_err(err)
            },
            Message::LoggedOut,
        )
    }

    /// Deletes again what work for an account forgotten meanwhile wrote into its cache as it ended
    /// (`forget_data` deleted the cache before): a few files at most, the cache being gone.
    pub fn forget_again(&self) {
        let Some(core) = &self.core else {
            return;
        };
        for user_id in &self.forgotten {
            let cache = core.dirs.account_cache(user_id);
            if cache.exists() {
                let _ = std::fs::remove_dir_all(cache);
            }
        }
    }

    /// Forgets everything that belonged to the account. Installs and updates keep running.
    pub fn logged_out(&mut self) {
        self.account_epoch += 1;
        self.account = None;
        self.library_busy = false;
        self.overview_busy = false;
        self.avatar = None;
        self.achievements_game = None;
        self.library.clear();
        self.gog_titles.clear();
        self.sort_keys.clear();
        self.dialog = None;
        self.edit = None;
        self.context_menu = None;
        self.menu_for = None;
        self.launch_prompt = None;
        self.fetched_at = None;
        self.covers.clear();
        self.images.clear();
        self.images_requested.clear();
        self.forget_scaled();
        // Steam's games stay, without GOG's: their covers come back from Steam's files.
        self.merge_steam();
        self.overview.clear();
        // What was played signed out, games added by hand's whoever was signed in.
        self.playtime = self
            .core
            .as_ref()
            .and_then(|core| slatty_core::session::playtime(&core.db, None).ok())
            .unwrap_or_default();
        self.selected = None;
        self.panel = None;
        self.page = Page::Library;
        self.achievements.clear();
        self.pending_change = None;
        self.cloud.clear();
        // What GOG offers to download depends on the account (its DLC).
        self.offline_views.clear();
        // Found with the account signed out; its check's answer is dropped with its other work.
        self.updates.waiting.clear();
        self.updates.checking = false;
    }

    pub fn fetch_avatar(&self) -> Task<Message> {
        let (Some(core), Some(account)) = (self.core.clone(), self.account.as_ref()) else {
            return Task::none();
        };
        let user_id = account.user_id.clone();
        self.account_task(Task::perform(
            async move {
                slatty_core::account::avatar_url(&core.http, &user_id)
                    .await
                    .ok()
                    .flatten()
            },
            Message::Avatar,
        ))
    }
}
