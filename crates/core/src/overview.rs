//! What GOG offers for each owned game (achievements, cloud saves), gathered in the background
//! and cached per account so the library can be filtered and summarised offline.

use std::collections::HashMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::achievements;
use crate::auth::{self, Tokens};
use crate::cloud::locations;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::paths::Dirs;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameOverview {
    /// Unlocked and total achievements; `None` when the game has none.
    pub achievements: Option<(usize, usize)>,
    pub cloud_saves: bool,
}

impl GameOverview {
    pub fn from_achievements(list: &[achievements::Achievement], cloud_saves: bool) -> Self {
        GameOverview {
            achievements: (!list.is_empty()).then(|| {
                (
                    list.iter().filter(|a| a.date_unlocked.is_some()).count(),
                    list.len(),
                )
            }),
            cloud_saves,
        }
    }
}

/// Reads achievements and cloud save support of one game. A game without a Galaxy build has
/// neither.
pub async fn fetch(http: &Client, tokens: &Tokens, game_id: &str) -> Result<GameOverview> {
    let (client_id, secret) = match locations::game_client(http, tokens, game_id).await {
        Ok(c) => c,
        Err(Error::Unsupported(_)) => return Ok(GameOverview::default()),
        Err(e) => return Err(e),
    };
    let cloud_saves = locations::fetch(http, &client_id).await?.is_some();
    let token = auth::game_access_token(http, tokens, &client_id, &secret).await?;
    let list = achievements::fetch(http, &tokens.user_id, &client_id, &token).await?;
    Ok(GameOverview::from_achievements(&list, cloud_saves))
}

fn cache_file(dirs: &Dirs, user_id: &str) -> std::path::PathBuf {
    dirs.account_cache(user_id).join("overview.json")
}

pub fn load(dirs: &Dirs, user_id: &str) -> Result<HashMap<String, GameOverview>> {
    match std::fs::read(cache_file(dirs, user_id)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| Error::parse("game overview", e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(Error::io("read game overview", e)),
    }
}

pub fn save(dirs: &Dirs, user_id: &str, all: &HashMap<String, GameOverview>) -> Result<()> {
    let json = serde_json::to_vec(all).map_err(|e| Error::parse("game overview", e))?;
    fsutil::write_atomic(&cache_file(dirs, user_id), &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_round_trips_and_starts_empty() {
        let root = std::env::temp_dir().join(format!("slatty-overview-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        assert!(load(&dirs, "0").unwrap().is_empty());
        let all = HashMap::from([(
            "1".to_string(),
            GameOverview {
                achievements: Some((2, 5)),
                cloud_saves: true,
            },
        )]);
        save(&dirs, "0", &all).unwrap();
        assert_eq!(load(&dirs, "0").unwrap(), all);
        std::fs::remove_dir_all(root).unwrap();
    }
}
