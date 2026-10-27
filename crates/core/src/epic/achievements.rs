//! An Epic game's achievements, read as Epic's launcher reads them: through Epic's GraphQL API,
//! the game's definitions by its sandbox (its namespace), and the account's progress in it.
//! legendary does not read them; the queries come from woctezuma's notes on that API
//! (gist 8ca464a276b15d7dfad475fd6b6cbee9) and trophyhunter (`lib/epic/game-achievements.ts`,
//! MIT). Games unlock their achievements themselves, through Epic's online services, once signed in.

use std::collections::HashMap;
use std::future::Future;

use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::achievements::Achievement;
use crate::error::{Error, Result};
use crate::paths::Dirs;
use crate::secret::Secret;
use crate::{fsutil, http};

/// Unlocked and total achievements of the account's games as last read, by game id; none for a
/// game without any.
pub type Counts = HashMap<String, Option<(usize, usize)>>;

fn counts_file(dirs: &Dirs, account_id: &str) -> Result<std::path::PathBuf> {
    Ok(super::library::cache_folder(dirs, account_id)?.join("achievements.json"))
}

/// The counts kept for the account; none before any was read.
pub fn load_counts(dirs: &Dirs, account_id: &str) -> Result<Counts> {
    let path = counts_file(dirs, account_id)?;
    match std::fs::read(&path) {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| Error::parse("Epic's achievements", e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Counts::new()),
        Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
    }
}

pub fn save_counts(dirs: &Dirs, account_id: &str, counts: &Counts) -> Result<()> {
    let json = serde_json::to_vec(counts).map_err(|e| Error::parse("Epic's achievements", e))?;
    fsutil::write_atomic(&counts_file(dirs, account_id)?, &json)
}

/// Where the launcher's store sends its queries; the second answers when the first does not.
const ENDPOINTS: [&str; 2] = [
    "https://launcher.store.epicgames.com/graphql",
    "https://store.epicgames.com/graphql",
];
/// The launcher's store names itself so; the user agent of Epic's other services
/// (`super::USER_AGENT`) is sent to a captcha there.
const STORE_USER_AGENT: &str = "EpicGamesLauncher/14.0.8-22004686+++Portal+Release-Live";

const DEFINITIONS: &str = "query ProductAchievements($sandboxId: String!, $locale: String!) { \
    Achievement { productAchievementsRecordBySandbox(sandboxId: $sandboxId, locale: $locale) { \
    achievements { achievement { name hidden unlockedDisplayName lockedDisplayName \
    unlockedDescription lockedDescription unlockedIconLink lockedIconLink rarity { percent } } } \
    } } }";

const PROGRESS: &str = "query PlayerAchievement($epicAccountId: String!, $sandboxId: String!) { \
    PlayerAchievement { playerAchievementGameRecordsBySandbox(epicAccountId: $epicAccountId, \
    sandboxId: $sandboxId) { records { playerAchievements { playerAchievement { \
    achievementName unlocked unlockDate } } } } } }";

/// Epic's GraphQL API, or a test double: the `data` of a query's answer.
pub trait Graphql: Send + Sync {
    fn query(
        &self,
        query: &'static str,
        variables: Value,
    ) -> impl Future<Output = Result<Value>> + Send;
}

/// Epic's GraphQL API, with the account's token.
pub struct EpicGraphql<'a> {
    pub http: &'a Client,
    pub token: &'a Secret,
}

impl Graphql for EpicGraphql<'_> {
    async fn query(&self, query: &'static str, variables: Value) -> Result<Value> {
        let context = "reading Epic's achievements";
        let mut last = None;
        for url in ENDPOINTS {
            let req = self
                .http
                .post(url)
                .bearer_auth(self.token.expose())
                .header(reqwest::header::USER_AGENT, STORE_USER_AGENT)
                .json(&json!({ "query": query, "variables": variables }));
            let answer = async {
                let resp = req.send().await.map_err(|e| Error::network(context, e))?;
                let status = resp.status();
                let raw = http::read_up_to(resp, context, http::MAX_BODY).await?;
                // A query Epic refuses (400) says why in its answer.
                if status == reqwest::StatusCode::BAD_REQUEST
                    && let Some(why) = refused(&raw)
                {
                    return Err(Error::parse(context, why));
                }
                if !status.is_success() {
                    return super::library::session_checked(Err(Error::Http {
                        context,
                        status: status.as_u16(),
                    }));
                }
                serde_json::from_slice::<Value>(&raw).map_err(|e| Error::parse(context, e))
            }
            .await;
            match answer {
                Ok(mut answer) => {
                    return match answer.get_mut("data").map(Value::take) {
                        Some(data) if !data.is_null() => Ok(data),
                        _ => Err(Error::parse(context, "Epic answered without data")),
                    };
                }
                Err(e @ Error::EpicSessionRejected) => return Err(e),
                Err(e) => last = Some(e),
            }
        }
        Err(last.expect("an address was tried"))
    }
}

/// The messages of a GraphQL answer's `errors`.
fn refused(raw: &[u8]) -> Option<String> {
    let answer: Value = serde_json::from_slice(raw).ok()?;
    let messages: Vec<&str> = answer["errors"]
        .as_array()?
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect();
    (!messages.is_empty()).then(|| messages.join("; "))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Definition {
    name: String,
    #[serde(default)]
    hidden: bool,
    #[serde(default)]
    unlocked_display_name: String,
    #[serde(default)]
    locked_display_name: String,
    #[serde(default)]
    unlocked_description: String,
    #[serde(default)]
    locked_description: String,
    #[serde(default)]
    unlocked_icon_link: String,
    #[serde(default)]
    locked_icon_link: String,
    rarity: Option<Rarity>,
}

#[derive(Deserialize)]
struct Rarity {
    percent: Option<f32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    achievement_name: String,
    #[serde(default)]
    unlocked: bool,
    unlock_date: Option<String>,
}

/// The achievements of the game whose sandbox is `namespace`, for `account_id`, in Epic's order,
/// their texts in `locale`. None when the game has none.
pub async fn fetch<G: Graphql>(
    graphql: &G,
    account_id: &str,
    namespace: &str,
    locale: &str,
) -> Result<Vec<Achievement>> {
    let definitions = graphql
        .query(
            DEFINITIONS,
            json!({ "sandboxId": namespace, "locale": locale }),
        )
        .await?;
    let definitions: Vec<Definition> = items(
        &definitions["Achievement"]["productAchievementsRecordBySandbox"]["achievements"],
        "achievement",
    )?;
    if definitions.is_empty() {
        return Ok(Vec::new());
    }
    let progress = graphql
        .query(
            PROGRESS,
            json!({ "epicAccountId": account_id, "sandboxId": namespace }),
        )
        .await?;
    let records =
        &progress["PlayerAchievement"]["playerAchievementGameRecordsBySandbox"]["records"];
    let mut unlocked: HashMap<String, Option<String>> = HashMap::new();
    for record in records.as_array().into_iter().flatten() {
        let list: Vec<Progress> = items(&record["playerAchievements"], "playerAchievement")?;
        for p in list.into_iter().filter(|p| p.unlocked) {
            unlocked.insert(p.achievement_name, p.unlock_date);
        }
    }
    Ok(definitions
        .into_iter()
        .map(|d| {
            let pick = |a: String, b: String| if a.is_empty() { b } else { a };
            Achievement {
                achievement_id: d.name.clone(),
                achievement_key: d.name.clone(),
                date_unlocked: unlocked.get(&d.name).map(|at| date(at.as_deref())),
                name: pick(d.unlocked_display_name, d.locked_display_name),
                description: pick(d.unlocked_description, d.locked_description),
                visible: !d.hidden,
                rarity: d.rarity.and_then(|r| r.percent).unwrap_or(-1.0),
                image_url_unlocked: d.unlocked_icon_link,
                image_url_locked: d.locked_icon_link,
            }
        })
        .collect())
}

/// The `field` of each element of `list`, an absent or null list being empty.
fn items<T: for<'de> Deserialize<'de>>(list: &Value, field: &str) -> Result<Vec<T>> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            serde_json::from_value(e[field].clone())
                .map_err(|e| Error::parse("Epic's achievements", e))
        })
        .collect()
}

/// An unlock date as Slatty keeps them (`%Y-%m-%dT%H:%M:%S+0000`, as GOG writes them), from
/// Epic's (RFC 3339); empty when Epic gives none it can read.
fn date(at: Option<&str>) -> String {
    at.and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| {
            t.with_timezone(&chrono::Utc)
                .format("%Y-%m-%dT%H:%M:%S+0000")
                .to_string()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
