use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;

use crate::auth::{self, Tokens};
use crate::cloud::locations::game_client;
use crate::error::{Error, Result};
use crate::http;
use crate::install::Install;
use crate::secret::Secret;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Achievement {
    pub achievement_id: String,
    pub achievement_key: String,
    pub name: String,
    pub description: String,
    pub visible: bool,
    pub date_unlocked: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    items: Vec<Achievement>,
}

/// Access token for a game's own Galaxy client, as the game itself would obtain.
pub async fn game_token(
    http: &Client,
    tokens: &Tokens,
    install: &Install,
) -> Result<(String, Secret)> {
    let (client_id, token) = product_token(http, tokens, &install.game_id).await?;
    if install.client_id.as_deref().is_some_and(|c| c != client_id) {
        return Err(Error::parse(
            "fetching build metadata",
            "client id differs from the installed game",
        ));
    }
    Ok((client_id, token))
}

/// Same as [`game_token`] for any owned product, installed or not.
pub async fn product_token(
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
) -> Result<(String, Secret)> {
    let (client_id, secret) = game_client(http, tokens, game_id).await?;
    let token = auth::game_access_token(http, tokens, &client_id, &secret).await?;
    Ok((client_id, token))
}

pub async fn fetch(
    http: &Client,
    user_id: &str,
    client_id: &str,
    token: &Secret,
) -> Result<Vec<Achievement>> {
    let url = format!("https://gameplay.gog.com/clients/{client_id}/users/{user_id}/achievements");
    let page: Page = http::json(
        http.get(url).bearer_auth(token.expose()),
        "fetching achievements",
    )
    .await?;
    Ok(page.items)
}

/// Unlocks an achievement now, or clears it, outside any game.
/// GOG shows the result on the profile exactly like an unlock made in game.
pub async fn set_unlocked(
    http: &Client,
    user_id: &str,
    client_id: &str,
    token: &Secret,
    achievement_id: &str,
    unlocked: bool,
) -> Result<()> {
    let url = format!(
        "https://gameplay.gog.com/clients/{client_id}/users/{user_id}/achievements/{achievement_id}"
    );
    let date = unlocked.then(|| Utc::now().format("%Y-%m-%dT%H:%M:%S+0000").to_string());
    let req = http
        .post(url)
        .bearer_auth(token.expose())
        .json(&serde_json::json!({ "date_unlocked": date }));
    let context = if unlocked {
        "unlocking an achievement"
    } else {
        "clearing an achievement"
    };
    http::send(req, context).await.map(drop)
}

/// Finds an achievement by key, numeric id or exact name (case-insensitive).
pub fn select<'a>(list: &'a [Achievement], query: &str) -> Option<&'a Achievement> {
    list.iter()
        .find(|a| a.achievement_key == query || a.achievement_id == query)
        .or_else(|| {
            let q = query.to_lowercase();
            list.iter().find(|a| a.name.to_lowercase() == q)
        })
}

/// Achievements unlocked in `after` that were locked in `before`.
pub fn newly_unlocked<'a>(
    before: &[Achievement],
    after: &'a [Achievement],
) -> Vec<&'a Achievement> {
    after
        .iter()
        .filter(|a| a.date_unlocked.is_some())
        .filter(|a| {
            !before
                .iter()
                .any(|b| b.achievement_key == a.achievement_key && b.date_unlocked.is_some())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(key: &str, unlocked: bool) -> Achievement {
        Achievement {
            achievement_id: format!("id-{key}"),
            achievement_key: key.into(),
            name: key.into(),
            description: String::new(),
            visible: true,
            date_unlocked: unlocked.then(|| "2026-10-08T20:00:00+0000".into()),
        }
    }

    #[test]
    fn detects_only_new_unlocks() {
        let before = [a("x", true), a("y", false), a("z", false)];
        let after = [a("x", true), a("y", true), a("z", false)];
        let new: Vec<_> = newly_unlocked(&before, &after)
            .iter()
            .map(|a| a.achievement_key.as_str())
            .collect();
        assert_eq!(new, ["y"]);
    }

    #[test]
    fn selects_by_key_id_or_name() {
        let list = [a("Bookworm", false), a("lethal_key", true)];
        assert_eq!(
            select(&list, "Bookworm").unwrap().achievement_key,
            "Bookworm"
        );
        assert_eq!(
            select(&list, "id-lethal_key").unwrap().achievement_key,
            "lethal_key"
        );
        assert_eq!(
            select(&list, "bookworm").unwrap().achievement_key,
            "Bookworm"
        );
        assert!(select(&list, "nope").is_none());
    }

    #[test]
    fn parses_gameplay_page() {
        let page: Page = serde_json::from_str(
            r#"{"total_count":1,"limit":1000,"page_token":"0","items":[{"achievement_id":"1","achievement_key":"k",
            "name":"N","description":"D","image_url_locked":"","image_url_unlocked":"","visible":true,
            "date_unlocked":null,"rarity":1.5,"rarity_level_description":"","rarity_level_slug":""}],"achievements_mode":"all_visible"}"#,
        )
        .unwrap();
        assert_eq!(page.items[0].achievement_id, "1");
    }
}
