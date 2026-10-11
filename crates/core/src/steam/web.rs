//! Steam's Web API (api.steampowered.com), once the user turns it on and gives their own key
//! (`settings::steam_web_api`): the account's games the client has not described yet
//! (`GetOwnedGames`), and the achievements of a game the client keeps none of
//! (`GetPlayerAchievements`, `GetSchemaForGame`, `GetGlobalAchievementPercentagesForApp`). The
//! key goes in the address, as the API asks: network errors never carry the address
//! (`Error::network`), and the key stays in the keyring.

use std::collections::HashMap;

use reqwest::Client;
use serde::Deserialize;

use crate::achievements::Achievement;
use crate::error::Result;
use crate::http;
use crate::secret::Secret;

/// Name of the API key in the keyring.
pub const KEY: &str = "steam-web-api";
/// Where a user creates their key.
pub const KEY_PAGE: &str = "https://steamcommunity.com/dev/apikey";
const API: &str = "https://api.steampowered.com";
/// Where Steam's image servers keep a game's achievement icons.
const ICONS: &str = "https://shared.steamstatic.com/community_assets/images/apps/";

pub fn key_page() -> url::Url {
    url::Url::parse(KEY_PAGE).expect("a valid address")
}

/// A game of the account, as the Web API lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedGame {
    pub app_id: u32,
    pub name: String,
    pub minutes: u64,
    /// Seconds since 1970; 0 when never.
    pub last_played: i64,
}

/// The account's games, free ones played included; none when its game details are private to
/// the key.
pub async fn owned_games(http: &Client, key: &Secret, steam_id: u64) -> Result<Vec<OwnedGame>> {
    owned_games_at(http, API, key, steam_id).await
}

async fn owned_games_at(
    http: &Client,
    api: &str,
    key: &Secret,
    steam_id: u64,
) -> Result<Vec<OwnedGame>> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        response: Games,
    }
    #[derive(Deserialize, Default)]
    struct Games {
        #[serde(default)]
        games: Vec<Game>,
    }
    #[derive(Deserialize)]
    struct Game {
        appid: u32,
        #[serde(default)]
        name: String,
        #[serde(default)]
        playtime_forever: u64,
        #[serde(default)]
        rtime_last_played: i64,
    }
    let req = http
        .get(format!("{api}/IPlayerService/GetOwnedGames/v1/"))
        .query(&[
            ("key", key.expose()),
            ("steamid", &steam_id.to_string()),
            ("include_appinfo", "1"),
            ("include_played_free_games", "1"),
            ("format", "json"),
        ]);
    let answer: Answer = http::json(req, "listing the Steam account's games").await?;
    Ok(answer
        .response
        .games
        .into_iter()
        .filter(|g| !g.name.trim().is_empty())
        .map(|g| OwnedGame {
            app_id: g.appid,
            name: g.name.trim().to_string(),
            minutes: g.playtime_forever,
            last_played: g.rtime_last_played.max(0),
        })
        .collect())
}

/// The achievements of `app_id` for the account, with how common each is; with `icons`, each names
/// its icons on Steam's image servers. None when the game has no achievements.
pub async fn achievements(
    http: &Client,
    key: &Secret,
    steam_id: u64,
    app_id: u32,
    icons: bool,
) -> Result<Vec<Achievement>> {
    achievements_at(http, API, key, steam_id, app_id, icons).await
}

async fn achievements_at(
    http: &Client,
    api: &str,
    key: &Secret,
    steam_id: u64,
    app_id: u32,
    icons: bool,
) -> Result<Vec<Achievement>> {
    #[derive(Deserialize)]
    struct Schema {
        #[serde(default)]
        game: SchemaGame,
    }
    #[derive(Deserialize, Default)]
    struct SchemaGame {
        #[serde(default, rename = "availableGameStats")]
        stats: SchemaStats,
    }
    #[derive(Deserialize, Default)]
    struct SchemaStats {
        #[serde(default)]
        achievements: Vec<Defined>,
    }
    #[derive(Deserialize)]
    struct Defined {
        name: String,
        #[serde(default, rename = "displayName")]
        display_name: String,
        #[serde(default)]
        description: String,
        #[serde(default)]
        hidden: u8,
        #[serde(default)]
        icon: String,
        #[serde(default)]
        icongray: String,
    }
    #[derive(Deserialize)]
    struct Player {
        playerstats: PlayerStats,
    }
    #[derive(Deserialize)]
    struct PlayerStats {
        #[serde(default)]
        achievements: Vec<Unlocked>,
    }
    #[derive(Deserialize)]
    struct Unlocked {
        apiname: String,
        #[serde(default)]
        achieved: u8,
        #[serde(default)]
        unlocktime: i64,
    }
    let app = app_id.to_string();
    let schema = http
        .get(format!("{api}/ISteamUserStats/GetSchemaForGame/v2/"))
        .query(&[("key", key.expose()), ("appid", &app), ("l", "english")]);
    let schema: Schema = http::json(schema, "reading a Steam game's achievements").await?;
    let defined = schema.game.stats.achievements;
    if defined.is_empty() {
        return Ok(Vec::new());
    }
    let player = http
        .get(format!("{api}/ISteamUserStats/GetPlayerAchievements/v1/"))
        .query(&[
            ("key", key.expose()),
            ("steamid", &steam_id.to_string()),
            ("appid", &app),
        ]);
    let player: Player = http::json(player, "reading the Steam account's achievements").await?;
    let unlocked: HashMap<String, i64> = player
        .playerstats
        .achievements
        .into_iter()
        .filter(|a| a.achieved != 0)
        .map(|a| (a.apiname, a.unlocktime))
        .collect();
    // How common each is: a nice-to-have, which needs no key.
    let rarity = rarity_at(http, api, app_id).await.unwrap_or_default();
    let icon = |url: &str| {
        let file = url.rsplit('/').next().unwrap_or_default();
        if icons && safe_name(file) {
            format!("{ICONS}{app_id}/{file}")
        } else {
            String::new()
        }
    };
    Ok(defined
        .into_iter()
        .map(|a| Achievement {
            achievement_id: a.name.clone(),
            achievement_key: a.name.clone(),
            name: a.display_name,
            description: a.description,
            visible: a.hidden == 0,
            date_unlocked: unlocked.get(&a.name).map(|t| date(*t)),
            rarity: rarity.get(&a.name).copied().unwrap_or(-1.0),
            image_url_unlocked: icon(&a.icon),
            image_url_locked: icon(&a.icongray),
        })
        .collect())
}

/// How common each achievement of a game is, in percent, by key.
async fn rarity_at(http: &Client, api: &str, app_id: u32) -> Result<HashMap<String, f32>> {
    #[derive(Deserialize)]
    struct Answer {
        achievementpercentages: Percentages,
    }
    #[derive(Deserialize)]
    struct Percentages {
        #[serde(default)]
        achievements: Vec<Share>,
    }
    #[derive(Deserialize)]
    struct Share {
        name: String,
        /// A number, or a number written as text in newer answers.
        percent: serde_json::Value,
    }
    let req = http
        .get(format!(
            "{api}/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/"
        ))
        .query(&[("gameid", app_id.to_string())]);
    let answer: Answer = http::json(req, "reading how common Steam achievements are").await?;
    Ok(answer
        .achievementpercentages
        .achievements
        .into_iter()
        .filter_map(|a| {
            let percent = match &a.percent {
                serde_json::Value::Number(n) => n.as_f64()?,
                serde_json::Value::String(s) => s.parse().ok()?,
                _ => return None,
            };
            (0.0..=100.0)
                .contains(&percent)
                .then_some((a.name, percent as f32))
        })
        .collect())
}

/// An icon's file name, fit for an address: letters, digits and `._-`.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// A date as GOG writes the time an achievement was unlocked.
fn date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .filter(|_| seconds > 0)
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S+0000").to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::error::Error;

    /// Answers each request whose path starts with one of `routes` with its status and body, in
    /// any order, `count` requests in all; hands back the requests received.
    async fn serve(
        routes: Vec<(&'static str, &'static str, &'static str)>,
        count: usize,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut seen = Vec::new();
            for _ in 0..count {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request.split(' ').nth(1).unwrap_or_default().to_string();
                let (status, body) = routes
                    .iter()
                    .find(|(prefix, ..)| path.starts_with(prefix))
                    .map_or(("404 Not Found", "{}"), |(_, s, b)| (*s, *b));
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                     connection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
                seen.push(request);
            }
            seen
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn the_accounts_games_come_with_the_key_in_the_address() {
        let (api, requests) = serve(
            vec![(
                "/IPlayerService/GetOwnedGames/v1/",
                "200 OK",
                r#"{"response":{"game_count":3,"games":[
                    {"appid":70,"name":" [FAKE] Life ","playtime_forever":754,"rtime_last_played":1760000000},
                    {"appid":80,"name":"","playtime_forever":1},
                    {"appid":90,"name":"[FAKE] Never played"}]}}"#,
            )],
            1,
        )
        .await;
        let key = Secret::new("FAKEKEY0123");
        let games = owned_games_at(&http::client().unwrap(), &api, &key, 76561198000000002)
            .await
            .unwrap();
        assert_eq!(
            games,
            [
                OwnedGame {
                    app_id: 70,
                    name: "[FAKE] Life".into(),
                    minutes: 754,
                    last_played: 1_760_000_000
                },
                OwnedGame {
                    app_id: 90,
                    name: "[FAKE] Never played".into(),
                    minutes: 0,
                    last_played: 0
                }
            ]
        );
        let request = &requests.await.unwrap()[0];
        let first = request.lines().next().unwrap();
        assert!(first.contains("key=FAKEKEY0123") && first.contains("steamid=76561198000000002"));

        // A private profile answers an empty response.
        let (api, _) = serve(vec![("/IPlayerService", "200 OK", r#"{"response":{}}"#)], 1).await;
        let games = owned_games_at(&http::client().unwrap(), &api, &key, 1).await;
        assert_eq!(games.unwrap(), []);
    }

    #[tokio::test]
    async fn achievements_come_from_the_schema_and_the_accounts_unlocks() {
        let (api, requests) = serve(
            vec![
                (
                    "/ISteamUserStats/GetSchemaForGame/v2/",
                    "200 OK",
                    r#"{"game":{"gameName":"[FAKE] Life","availableGameStats":{"achievements":[
                        {"name":"ACH_WAKE","displayName":"[FAKE] Wake up","hidden":0,"description":"[FAKE] Up",
                         "icon":"https://steamcdn-a.akamaihd.net/steamcommunity/public/images/apps/70/abc.jpg",
                         "icongray":"https://elsewhere.example/../../x y.jpg"},
                        {"name":"ACH_SECRET","displayName":"[FAKE] Secret","hidden":1,"description":""}]}}}"#,
                ),
                (
                    "/ISteamUserStats/GetPlayerAchievements/v1/",
                    "200 OK",
                    r#"{"playerstats":{"steamID":"1","success":true,"achievements":[
                        {"apiname":"ACH_WAKE","achieved":1,"unlocktime":1760000000},
                        {"apiname":"ACH_SECRET","achieved":0,"unlocktime":0}]}}"#,
                ),
                (
                    "/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/",
                    "200 OK",
                    r#"{"achievementpercentages":{"achievements":[
                        {"name":"ACH_WAKE","percent":"61.5"},{"name":"ACH_SECRET","percent":300}]}}"#,
                ),
            ],
            3,
        )
        .await;
        let key = Secret::new("FAKEKEY0123");
        let list = achievements_at(&http::client().unwrap(), &api, &key, 1, 70, true)
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        let (wake, secret) = (&list[0], &list[1]);
        assert_eq!(wake.name, "[FAKE] Wake up");
        assert_eq!(
            wake.date_unlocked.as_deref(),
            Some("2025-10-09T08:53:20+0000")
        );
        assert_eq!(wake.rarity, 61.5);
        assert_eq!(
            wake.image_url_unlocked,
            "https://shared.steamstatic.com/community_assets/images/apps/70/abc.jpg",
            "on Steam's image servers, whatever host the API names"
        );
        assert!(wake.image_url_locked.is_empty(), "not a plain file name");
        assert!(!secret.visible && secret.date_unlocked.is_none());
        assert!(secret.rarity < 0.0, "more than all players: not trusted");
        let requests = requests.await.unwrap();
        let global = requests
            .iter()
            .find(|r| r.contains("GetGlobalAchievementPercentagesForApp"))
            .unwrap();
        assert!(!global.contains("FAKEKEY0123"), "asked without the key");
    }

    #[tokio::test]
    async fn a_refused_key_is_an_error_without_the_address() {
        let (api, _) = serve(vec![("/", "403 Forbidden", "<html>Forbidden</html>")], 1).await;
        let key = Secret::new("FAKEKEY0123");
        let e = owned_games_at(&http::client().unwrap(), &api, &key, 1)
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Http { status: 403, .. }), "{e:?}");
        assert!(!format!("{e} {e:?}").contains("FAKEKEY0123"));
    }
}
