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
        }
        self.account_epoch += 1;
        self.account = Some(info);
        let avatar = self.fetch_avatar();
        Task::batch([
            avatar,
            match cache {
                Some(c) => self.set_library(c),
                None => Task::done(Message::SyncLibrary),
            },
        ])
    }

    pub fn logout(&mut self) -> Task<Message> {
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        Task::perform(
            async move {
                let account = Account::load(&core.db, &core.dirs).await.map_err(err)?;
                account.logout(&core.db).await.map_err(err)
            },
            Message::LoggedOut,
        )
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
        self.dialog = None;
        self.edit = None;
        self.context_menu = None;
        self.menu_for = None;
        self.launch_prompt = None;
        self.fetched_at = None;
        self.covers.clear();
        self.images.clear();
        self.images_requested.clear();
        self.overview.clear();
        self.selected = None;
        self.panel = None;
        self.page = Page::Library;
        self.achievements.clear();
        self.pending_change = None;
        self.cloud.clear();
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
