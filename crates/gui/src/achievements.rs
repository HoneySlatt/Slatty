//! Achievements of a game: loading them, and manual changes once the user confirms.

use iced::Task;
use slatty_core::achievements::{self, Achievement};
use slatty_core::install::Install;
use slatty_core::library::LibraryGame;
use slatty_core::overview::{self, GameOverview};

use crate::work::tokens;
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
        let Some(core) = self.core.clone() else {
            return Task::none();
        };
        let install = self.installs.get(&game_id).cloned();
        self.achievements.insert(game_id.clone(), Loadable::Loading);
        let id = game_id.clone();
        Task::perform(
            async move {
                let (user_id, client_id, token) = achievement_access(&core, &id, install).await?;
                achievements::fetch(&core.http, &user_id, &client_id, &token)
                    .await
                    .map_err(err)
            },
            move |r| Message::Achievements(game_id, r),
        )
    }

    pub fn ask_achievement_change(&mut self, game_id: String, changes: Vec<AchievementChange>) {
        if !changes.is_empty() {
            self.pending_change = Some(PendingChange { game_id, changes });
        }
    }

    pub fn confirm_achievement_change(&mut self) -> Task<Message> {
        let (Some(core), Some(pending)) = (self.core.clone(), self.pending_change.take()) else {
            return Task::none();
        };
        let install = self.installs.get(&pending.game_id).cloned();
        let game_id = pending.game_id.clone();
        self.achievements.insert(game_id.clone(), Loadable::Loading);
        Task::perform(
            async move {
                let (user_id, client_id, token) =
                    achievement_access(&core, &pending.game_id, install).await?;
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
        )
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

    /// Keeps the cached overview in step with a freshly read achievement list.
    pub fn achievements_loaded(&mut self, game_id: &str, list: &[Achievement]) {
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
    game_id: &str,
    install: Option<Install>,
) -> Result<(String, String, slatty_core::secret::Secret), String> {
    let tokens = tokens(core).await?;
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
        let mut games: Vec<(&LibraryGame, usize, usize)> = self
            .library
            .iter()
            .filter_map(|g| {
                let (done, total) = self.overview.get(&g.id)?.achievements?;
                Some((g, done, total))
            })
            .collect();
        // done / total compared as cross products, exact where floats could tie wrongly.
        games.sort_by(|a, b| {
            (b.1 * a.2)
                .cmp(&(a.1 * b.2))
                .then_with(|| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()))
        });
        games
    }
}

/// Achievements from the most common to the rarest (share of players who have them); equal
/// shares keep GOG's order.
pub fn by_rarity(list: &[Achievement]) -> Vec<&Achievement> {
    let mut sorted: Vec<&Achievement> = list.iter().collect();
    sorted.sort_by(|a, b| b.rarity.total_cmp(&a.rarity));
    sorted
}
