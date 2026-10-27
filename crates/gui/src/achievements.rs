//! Achievements of a game: loading them, and manual changes once the user confirms.

use iced::Task;
use slatty_core::achievements::{self, Achievement};
use slatty_core::install::Install;
use slatty_core::library::LibraryGame;
use slatty_core::overview::{self, GameOverview};

use crate::work::tokens_for;
use crate::{App, Core, Loadable, Message, Page, Panel, err};

/// Manual achievement changes waiting for the user's confirmation.
#[derive(Debug, Clone)]
pub struct PendingChange {
    pub game_id: String,
    pub changes: Vec<AchievementChange>,
}

#[derive(Debug, Clone)]
pub struct AchievementChange {
    pub achievement_id: String,
    pub name: String,
    pub unlock: bool,
}

impl App {
    pub fn load_achievements(&mut self, game_id: String) -> Task<Message> {
        // A Steam game's are read from the Steam client's files, without any GOG account.
        if self.steam_game(&game_id).is_some() {
            // Read for the account whose games show, once they are read.
            let Some(account) = self.steam.account else {
                return Task::none();
            };
            self.achievements.insert(game_id.clone(), Loadable::Loading);
            let web = self
                .asks_steam_web()
                .then(|| {
                    let http = self.core.as_ref()?.http.clone();
                    Some((http, self.steam.web_allowed.clone()))
                })
                .flatten();
            return crate::steam::load_achievements(game_id, self.steam.images, account, web);
        }
        // An Epic game's are Epic's, read while signed in to it, without any GOG account.
        if slatty_core::store::epic_app(&game_id).is_some() {
            let (Some(core), Some(game)) = (self.core.clone(), self.epic_game(&game_id)) else {
                return Task::none();
            };
            if self.epic.account.is_none() {
                return Task::none();
            }
            self.achievements.insert(game_id.clone(), Loadable::Loading);
            return Task::perform(
                async move {
                    use slatty_core::epic::{achievements, auth, launch};
                    let tokens = auth::fresh(&core.http, &core.db, &core.dirs)
                        .await
                        .map_err(err)?;
                    let graphql = achievements::EpicGraphql {
                        http: &core.http,
                        token: &tokens.access_token,
                    };
                    let locale = launch::system_locale();
                    achievements::fetch(&graphql, &tokens.account_id, &game.namespace, &locale)
                        .await
                        .map_err(err)
                },
                move |r| Message::Achievements(game_id, r),
            );
        }
        // A game added by hand or emulated has none, and GOG is not told of it.
        if slatty_core::store::from_no_store(&game_id) {
            return Task::none();
        }
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            return Task::none();
        };
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let install = self.installs.get(&game_id).cloned();
        self.achievements.insert(game_id.clone(), Loadable::Loading);
        let id = game_id.clone();
        self.account_task(Task::perform(
            async move {
                let (user_id, client_id, token) =
                    achievement_access(&core, &user_id, &id, install).await?;
                achievements::fetch(&core.http, &user_id, &client_id, &token)
                    .await
                    .map_err(err)
            },
            move |r| Message::Achievements(game_id, r),
        ))
    }

    pub fn ask_achievement_change(&mut self, game_id: String, changes: Vec<AchievementChange>) {
        if !changes.is_empty() && self.manual_changes(&game_id) {
            self.pending_change = Some(PendingChange { game_id, changes });
        }
    }

    pub fn confirm_achievement_change(&mut self) -> Task<Message> {
        let Some(user_id) = self.account.as_ref().map(|a| a.user_id.clone()) else {
            return Task::none();
        };
        let (Some(core), Some(pending)) = (self.core.clone(), self.pending_change.take()) else {
            return Task::none();
        };
        let install = self.installs.get(&pending.game_id).cloned();
        let game_id = pending.game_id.clone();
        self.achievements.insert(game_id.clone(), Loadable::Loading);
        self.account_task(Task::perform(
            async move {
                let (user_id, client_id, token) =
                    achievement_access(&core, &user_id, &pending.game_id, install).await?;
                let mut failures = Vec::new();
                for change in &pending.changes {
                    if let Err(e) = achievements::set_unlocked(
                        &core.http,
                        &user_id,
                        &client_id,
                        &token,
                        &change.achievement_id,
                        change.unlock,
                    )
                    .await
                    {
                        failures.push(format!("{} : {e}", change.name));
                    }
                }
                let list = achievements::fetch(&core.http, &user_id, &client_id, &token)
                    .await
                    .map_err(err)?;
                Ok((list, failures))
            },
            move |r| Message::AchievementsChanged(game_id, r),
        ))
    }

    pub fn achievements_changed(
        &mut self,
        game_id: String,
        result: Result<(Vec<Achievement>, Vec<String>), String>,
    ) {
        match result {
            Ok((list, failures)) => {
                self.achievements_loaded(&game_id, &list);
                self.achievements.insert(game_id, Loadable::Ready(list));
                if !failures.is_empty() {
                    self.notify_error(format!("Failed: {}", failures.join(" ; ")));
                }
            }
            Err(e) => {
                self.achievements.insert(game_id, Loadable::Failed(e));
            }
        }
    }

    pub fn achievements_received(
        &mut self,
        id: String,
        result: Result<Vec<Achievement>, String>,
    ) -> Task<Message> {
        let task = match &result {
            Ok(list) => {
                self.achievements_loaded(&id, list);
                self.request_images(achievement_icons(list, self.shows_all_achievements(&id)))
            }
            Err(_) => Task::none(),
        };
        self.achievements.insert(
            id,
            match result {
                Ok(a) => Loadable::Ready(a),
                Err(e) => Loadable::Failed(e),
            },
        );
        task
    }

    pub fn shows_all_achievements(&self, game_id: &str) -> bool {
        let page = self.selected.is_none()
            && self.page == Page::Achievements
            && self.achievements_game.as_deref() == Some(game_id);
        let panel =
            self.panel == Some(Panel::Achievements) && self.selected.as_deref() == Some(game_id);
        page || panel
    }

    /// Full-page achievement list of one game, inside the Achievements tab.
    pub fn open_achievements(&mut self, id: String) -> Task<Message> {
        self.page = Page::Achievements;
        self.selected = None;
        self.panel = None;
        self.achievements_game = Some(id.clone());
        match self.achievements.get(&id) {
            Some(Loadable::Ready(list)) => self.request_images(achievement_icons(list, true)),
            Some(Loadable::Loading) => Task::none(),
            _ => Task::done(Message::LoadAchievements(id)),
        }
    }

    /// Keeps the cached overview in step with a freshly read achievement list; a Steam game's
    /// counts, and an Epic game's, are kept apart from GOG's.
    pub fn achievements_loaded(&mut self, game_id: &str, list: &[Achievement]) {
        if slatty_core::store::epic_app(game_id).is_some() {
            let counts = overview::counts(list);
            let kept = self.epic.achievements.get_or_insert_default();
            if kept.get(game_id) != Some(&counts) {
                kept.insert(game_id.to_string(), counts);
                self.save_epic_counts();
            }
            return;
        }
        if self.steam_game(game_id).is_some() {
            match overview::counts(list) {
                Some(counts) => self.steam.achievements.insert(game_id.to_string(), counts),
                None => self.steam.achievements.remove(game_id),
            };
            return;
        }
        let fresh = GameOverview {
            achievements: overview::counts(list),
            ..self.overview.get(game_id).copied().unwrap_or_default()
        };
        if self.overview.get(game_id) != Some(&fresh) {
            self.overview.insert(game_id.to_string(), fresh);
            self.save_overview();
        }
    }
}

/// Icons to show: the three latest unlocks on the game page, or all of them in the panel.
pub fn achievement_icons(list: &[Achievement], all: bool) -> Vec<String> {
    if all {
        list.iter()
            .map(|a| {
                if a.date_unlocked.is_some() {
                    a.image_url_unlocked.clone()
                } else {
                    a.image_url_locked.clone()
                }
            })
            .collect()
    } else {
        latest_unlocked(list)
            .iter()
            .map(|a| a.image_url_unlocked.clone())
            .collect()
    }
}

/// User id, Galaxy client id and game token for any owned game, installed or not.
async fn achievement_access(
    core: &Core,
    user_id: &str,
    game_id: &str,
    install: Option<Install>,
) -> Result<(String, String, slatty_core::secret::Secret), String> {
    let tokens = tokens_for(core, user_id).await?;
    let (client_id, token) = match &install {
        Some(i) => achievements::game_token(&core.http, &tokens, i).await,
        None => achievements::product_token(&core.http, &tokens, game_id).await,
    }
    .map_err(err)?;
    Ok((tokens.user_id, client_id, token))
}

/// Unlocked achievements, newest first.
pub fn latest_unlocked(list: &[Achievement]) -> Vec<&Achievement> {
    let mut done: Vec<&Achievement> = list.iter().filter(|a| a.date_unlocked.is_some()).collect();
    done.sort_by(|a, b| b.date_unlocked.cmp(&a.date_unlocked));
    done.truncate(3);
    done
}

impl App {
    /// Games with achievements and their unlocked and total counts, highest completion first; ties
    /// by title.
    pub fn games_by_achievements(&self) -> Vec<(&LibraryGame, usize, usize)> {
        let mut kept = self.lists.achievements.borrow_mut();
        let games = match &*kept {
            Some((len, games)) if *len == self.library.len() => games.clone(),
            _ => {
                let games: std::rc::Rc<[_]> = self.find_games_by_achievements().into();
                *kept = Some((self.library.len(), games.clone()));
                games
            }
        };
        games
            .iter()
            .map(|&(i, done, total)| (&self.library[i], done, total))
            .collect()
    }

    /// `games_by_achievements`, with the games as places in `library`.
    fn find_games_by_achievements(&self) -> Vec<(usize, usize, usize)> {
        let mut games: Vec<_> = self
            .library
            .iter()
            .enumerate()
            .filter_map(|(i, g)| {
                let (done, total) = self.achievement_counts(&g.id)?;
                Some((i, done, total, self.sort_name(g)))
            })
            .collect();
        // done / total compared as cross products, exact where floats could tie wrongly.
        games.sort_by(|a, b| (b.1 * a.2).cmp(&(a.1 * b.2)).then_with(|| a.3.cmp(&b.3)));
        games
            .into_iter()
            .map(|(i, done, total, _)| (i, done, total))
            .collect()
    }
}

/// Achievements from the most common to the rarest (share of players who have them); equal
/// shares keep GOG's order.
pub fn by_rarity(list: &[Achievement]) -> Vec<&Achievement> {
    let mut sorted: Vec<&Achievement> = list.iter().collect();
    sorted.sort_by(|a, b| b.rarity.total_cmp(&a.rarity));
    sorted
}
