use std::sync::Arc;

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::http;
use crate::paths::Dirs;

const CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetadataSource {
    Gamesdb,
    Product,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryGame {
    pub id: String,
    pub title: String,
    pub cover: Option<String>,
    pub icon: Option<String>,
    /// Wide key art, shown behind the game page.
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    pub os: Vec<String>,
    pub metadata: MetadataSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryCache {
    pub user_id: String,
    pub fetched_at: i64,
    pub games: Vec<LibraryGame>,
}

#[derive(Deserialize)]
struct ReleasesPage {
    items: Vec<Release>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    platform_id: String,
    external_id: String,
    certificate: Option<String>,
}

pub async fn fetch(http: &Client, tokens: &Tokens) -> Result<Vec<LibraryGame>> {
    let releases = fetch_releases(http, tokens).await?;
    let token: Arc<str> = tokens.access_token.expose().into();
    let limit = Arc::new(Semaphore::new(CONCURRENCY));
    let mut tasks = JoinSet::new();
    for release in releases {
        let (http, token, limit) = (http.clone(), token.clone(), limit.clone());
        tasks.spawn(async move {
            let _permit = limit.acquire_owned().await.expect("semaphore never closed");
            describe(&http, &token, release).await
        });
    }
    let mut games = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        if let Some(game) = joined.expect("library task panicked")? {
            games.push(game);
        }
    }
    games.sort_by_cached_key(|g| g.title.to_lowercase());
    Ok(games)
}

async fn fetch_releases(http: &Client, tokens: &Tokens) -> Result<Vec<Release>> {
    let url = format!(
        "https://galaxy-library.gog.com/users/{}/releases",
        tokens.user_id
    );
    let mut releases = Vec::new();
    let mut page_token: Option<String> = None;
    loop {
        let mut req = http.get(&url).bearer_auth(tokens.access_token.expose());
        if let Some(t) = &page_token {
            req = req.query(&[("page_token", t)]);
        }
        let page: ReleasesPage = http::json(req, "fetching the library").await?;
        releases.extend(page.items.into_iter().filter(|r| r.platform_id == "gog"));
        match page.next_page_token.filter(|t| !t.is_empty()) {
            Some(t) => page_token = Some(t),
            None => return Ok(releases),
        }
    }
}

async fn describe(http: &Client, token: &str, release: Release) -> Result<Option<LibraryGame>> {
    let url = format!(
        "https://gamesdb.gog.com/platforms/gog/external_releases/{}",
        release.external_id
    );
    let mut req = http.get(url).bearer_auth(token);
    if let Some(cert) = &release.certificate {
        req = req.header("X-GOG-Library-Cert", cert);
    }
    match http::json::<Value>(req, "fetching game metadata").await {
        Ok(v) => Ok(from_gamesdb(&release.external_id, &v)),
        Err(Error::Http { .. } | Error::Parse { .. }) => {
            Ok(Some(from_product(http, &release.external_id).await))
        }
        Err(e) => Err(e),
    }
}

fn from_gamesdb(id: &str, v: &Value) -> Option<LibraryGame> {
    let kind = v["type"].as_str().unwrap_or_default();
    if !matches!(kind, "game" | "mod") || v["game"]["visible_in_library"] == Value::Bool(false) {
        return None;
    }
    let title = v["title"]["*"]
        .as_str()
        .or_else(|| v["game"]["title"]["*"].as_str())
        .unwrap_or(id)
        .trim()
        .to_string();
    let image = |key: &str, ext: &str| {
        v["game"][key]["url_format"]
            .as_str()
            .map(|f| f.replace("{formatter}", "").replace("{ext}", ext))
    };
    let os = v["supported_operating_systems"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o["slug"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    Some(LibraryGame {
        id: id.to_string(),
        title,
        cover: image("vertical_cover", "jpg").or_else(|| image("cover", "jpg")),
        icon: image("square_icon", "png").or_else(|| image("icon", "png")),
        background: image("background", "jpg"),
        logo: image("logo", "png"),
        os,
        metadata: MetadataSource::Gamesdb,
    })
}

async fn from_product(http: &Client, id: &str) -> LibraryGame {
    let req = http.get(format!("https://api.gog.com/products/{id}"));
    let title = http::json::<Value>(req, "fetching product data")
        .await
        .ok()
        .and_then(|v| v["title"].as_str().map(String::from));
    LibraryGame {
        id: id.to_string(),
        metadata: if title.is_some() {
            MetadataSource::Product
        } else {
            MetadataSource::Missing
        },
        title: title.unwrap_or_else(|| format!("GOG product {id}")),
        cover: None,
        icon: None,
        background: None,
        logo: None,
        os: Vec::new(),
    }
}

/// Cover image bytes, served from the per-account disk cache when present.
pub async fn cover(
    http: &Client,
    dirs: &Dirs,
    user_id: &str,
    game: &LibraryGame,
) -> Result<Option<Vec<u8>>> {
    let Some(url) = &game.cover else {
        return Ok(None);
    };
    let path = dirs
        .account_cache(user_id)
        .join("covers")
        .join(format!("{}.img", game.id));
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(Some(bytes));
    }
    let bytes = http::bytes(http.get(url), "downloading a cover").await?;
    fsutil::write_atomic(&path, &bytes)?;
    Ok(Some(bytes))
}

/// Where an image downloaded from `url` is cached for the account.
pub fn image_path(dirs: &Dirs, user_id: &str, url: &str) -> std::path::PathBuf {
    dirs.account_cache(user_id)
        .join("images")
        .join(fsutil::sha256_hex(url.as_bytes()))
}

/// Bytes of any GOG image (key art, logo, achievement icon), cached per account by URL.
pub async fn image(http: &Client, dirs: &Dirs, user_id: &str, url: &str) -> Result<Vec<u8>> {
    let path = image_path(dirs, user_id, url);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        return Ok(bytes);
    }
    let bytes = http::bytes(http.get(url), "downloading an image").await?;
    fsutil::write_atomic(&path, &bytes)?;
    Ok(bytes)
}

fn cache_file(dirs: &Dirs, user_id: &str) -> std::path::PathBuf {
    dirs.account_cache(user_id).join("library.json")
}

pub fn save_cache(dirs: &Dirs, user_id: &str, games: Vec<LibraryGame>) -> Result<LibraryCache> {
    let cache = LibraryCache {
        user_id: user_id.to_string(),
        fetched_at: Utc::now().timestamp(),
        games,
    };
    let json = serde_json::to_vec_pretty(&cache).map_err(|e| Error::parse("library cache", e))?;
    fsutil::write_atomic(&cache_file(dirs, user_id), &json)?;
    Ok(cache)
}

pub fn load_cache(dirs: &Dirs, user_id: &str) -> Result<Option<LibraryCache>> {
    let path = cache_file(dirs, user_id);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)
            .ok()
            .filter(|c: &LibraryCache| c.user_id == user_id)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_gamesdb_release() {
        let v = json!({
            "type": "game",
            "title": {"*": " Tomb Raider "},
            "supported_operating_systems": [{"slug": "windows"}],
            "game": {
                "visible_in_library": true,
                "vertical_cover": {"url_format": "https://img/abc{formatter}.{ext}"}
            }
        });
        let g = from_gamesdb("1724969043", &v).unwrap();
        assert_eq!(g.title, "Tomb Raider");
        assert_eq!(g.cover.as_deref(), Some("https://img/abc.jpg"));
        assert_eq!(g.os, vec!["windows"]);
    }

    #[test]
    fn skips_dlc_and_hidden_entries() {
        assert!(from_gamesdb("1", &json!({"type": "dlc", "game": {}})).is_none());
        assert!(
            from_gamesdb(
                "1",
                &json!({"type": "game", "game": {"visible_in_library": false}})
            )
            .is_none()
        );
    }

    #[test]
    fn cache_is_scoped_to_its_account() {
        let dirs =
            Dirs::under(&std::env::temp_dir().join(format!("slatty-lib-{}", std::process::id())));
        save_cache(&dirs, "111", vec![]).unwrap();
        assert!(load_cache(&dirs, "111").unwrap().is_some());
        assert!(load_cache(&dirs, "222").unwrap().is_none());
        std::fs::remove_dir_all(dirs.cache.parent().unwrap()).unwrap();
    }
}
