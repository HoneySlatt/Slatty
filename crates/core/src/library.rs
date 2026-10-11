use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

#[derive(Debug, Deserialize)]
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
    collect_releases(|page_token| {
        let mut req = http.get(&url).bearer_auth(tokens.access_token.expose());
        if let Some(t) = &page_token {
            req = req.query(&[("page_token", t)]);
        }
        http::json(req, "fetching the library")
    })
    .await
}

/// Far more pages than a library of 10,000 games takes.
const MAX_PAGES: usize = 10_000;

/// Every GOG release of the listing, page after page as `page` reads them. A page already read,
/// or more pages than any library has, stops it with an error rather than have it read forever.
async fn collect_releases<F: Future<Output = Result<ReleasesPage>>>(
    mut page: impl FnMut(Option<String>) -> F,
) -> Result<Vec<Release>> {
    let mut releases = Vec::new();
    let mut page_token: Option<String> = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        if seen.len() >= MAX_PAGES {
            return Err(Error::parse(
                "fetching the library",
                "GOG gave more pages than any library has",
            ));
        }
        let read = page(page_token.clone()).await?;
        releases.extend(read.items.into_iter().filter(|r| r.platform_id == "gog"));
        match read.next_page_token.filter(|t| !t.is_empty()) {
            Some(t) if !seen.insert(t.clone()) => {
                return Err(Error::parse(
                    "fetching the library",
                    "GOG gave a page already read",
                ));
            }
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

/// Why GOG offers nothing to install for a product it lists as owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotInstallable {
    /// A bundle: its games are owned, and listed, on their own.
    Pack,
    Other,
}

/// `None` when GOG offers something to install for the product, from its public product data.
pub async fn not_installable(http: &Client, id: &str) -> Result<Option<NotInstallable>> {
    let req = http.get(format!("https://api.gog.com/products/{id}"));
    Ok(installability(
        &http::json::<Value>(req, "fetching product data").await?,
    ))
}

fn installability(product: &Value) -> Option<NotInstallable> {
    if product["is_installable"].as_bool() != Some(false) {
        return None;
    }
    Some(match product["game_type"].as_str() {
        Some("pack") => NotInstallable::Pack,
        _ => NotInstallable::Other,
    })
}

/// The file of a game's cover in the per-account cache, downloaded when missing.
pub async fn cover(
    http: &Client,
    dirs: &Dirs,
    user_id: &str,
    game: &LibraryGame,
) -> Result<Option<PathBuf>> {
    let Some(url) = &game.cover else {
        return Ok(None);
    };
    crate::paths::check_game_id(&game.id)?;
    let folder = dirs.account_cache(user_id).join("covers");
    cached(http, folder, game.id.clone(), url, "downloading a cover")
        .await
        .map(Some)
}

/// The file of any GOG image (key art, logo, achievement icon) in the per-account cache, kept by
/// URL, downloaded when missing.
pub async fn image(http: &Client, dirs: &Dirs, user_id: &str, url: &str) -> Result<PathBuf> {
    let folder = dirs.account_cache(user_id).join("images");
    let name = fsutil::sha256_hex(url.as_bytes());
    cached(http, folder, name, url, "downloading an image").await
}

/// `<folder>/<name>.<format>`, downloaded from `url` when missing. Named after its format, which
/// image decoders go by, so the interface shows an image from its file rather than keep its bytes.
async fn cached(
    http: &Client,
    folder: PathBuf,
    name: String,
    url: &str,
    context: &'static str,
) -> Result<PathBuf> {
    let (dir, stem) = (folder.clone(), name.clone());
    if let Some(path) = tokio::task::spawn_blocking(move || cached_file(&dir, &stem))
        .await
        .expect("cache task panicked")
    {
        return Ok(path);
    }
    let bytes = http::bytes(http.get(url), context).await?;
    let ext = crate::custom::image_extension(&bytes)
        .ok_or_else(|| Error::parse(context, "not an image"))?;
    let path = folder.join(format!("{name}.{ext}"));
    fsutil::write_atomic(&path, &bytes)?;
    Ok(path)
}

const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "png", "webp", "gif", "bmp"];

/// The covers already in the account's cache, by game id, in one pass over the folder rather than
/// a few checks per game. Covers an earlier version kept without their format in their name are
/// left to `cover`, which renames them.
pub fn cached_covers(dirs: &Dirs, user_id: &str) -> HashMap<String, PathBuf> {
    let folder = dirs.account_cache(user_id).join("covers");
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| !t.is_dir()))
        .filter_map(|e| {
            let path = e.path();
            if !IMAGE_EXTENSIONS.contains(&path.extension()?.to_str()?) {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_string();
            crate::paths::check_game_id(&id).ok()?;
            Some((id, path))
        })
        .collect()
}

/// An image already in the cache. One an earlier version kept without its format in its name
/// (`<id>.img` for a cover, the bare hash for an image) is renamed after it, or dropped when it is
/// not an image.
fn cached_file(folder: &Path, name: &str) -> Option<PathBuf> {
    if let Some(path) = IMAGE_EXTENSIONS
        .iter()
        .map(|ext| folder.join(format!("{name}.{ext}")))
        .find(|p| p.is_file())
    {
        return Some(path);
    }
    let old = [folder.join(format!("{name}.img")), folder.join(name)]
        .into_iter()
        .find(|p| p.is_file())?;
    let mut head = [0u8; 16];
    let read = std::fs::File::open(&old).and_then(|mut f| std::io::Read::read(&mut f, &mut head));
    let ext = crate::custom::image_extension(&head[..read.ok()?]);
    let Some(ext) = ext else {
        let _ = std::fs::remove_file(&old);
        return None;
    };
    let path = folder.join(format!("{name}.{ext}"));
    std::fs::rename(&old, &path).ok()?;
    Some(path)
}

fn cache_file(dirs: &Dirs, user_id: &str) -> std::path::PathBuf {
    dirs.account_cache(user_id).join("library.json")
}

/// Saves the library just fetched. A game GOG's database failed to describe this time (an outage,
/// a session expiring during the sync) keeps the description an earlier sync got: its bare entry
/// has no cover and no systems, which would hide its Linux build.
pub fn save_cache(dirs: &Dirs, user_id: &str, mut games: Vec<LibraryGame>) -> Result<LibraryCache> {
    let rank = |m: MetadataSource| match m {
        MetadataSource::Gamesdb => 2,
        MetadataSource::Product => 1,
        MetadataSource::Missing => 0,
    };
    if games.iter().any(|g| g.metadata != MetadataSource::Gamesdb)
        && let Ok(Some(before)) = load_cache(dirs, user_id)
    {
        let mut before: HashMap<String, LibraryGame> = before
            .games
            .into_iter()
            .map(|g| (g.id.clone(), g))
            .collect();
        for game in &mut games {
            if let Some(known) = before.remove(&game.id)
                && rank(known.metadata) > rank(game.metadata)
            {
                *game = known;
            }
        }
        games.sort_by_cached_key(|g| g.title.to_lowercase());
    }
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
    fn tells_products_with_nothing_to_install() {
        let product = |installable, kind| json!({"is_installable": installable, "game_type": kind});
        assert_eq!(installability(&product(true, "game")), None);
        assert_eq!(
            installability(&product(false, "pack")),
            Some(NotInstallable::Pack)
        );
        assert_eq!(
            installability(&product(false, "game")),
            Some(NotInstallable::Other)
        );
        assert_eq!(installability(&json!({})), None, "installable unless said");
    }

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

    fn game_with_cover(id: &str, url: String) -> LibraryGame {
        LibraryGame {
            id: id.into(),
            title: "[FAKE] Game".into(),
            cover: Some(url),
            icon: None,
            background: None,
            logo: None,
            os: Vec::new(),
            metadata: MetadataSource::Gamesdb,
        }
    }

    #[tokio::test]
    async fn covers_are_kept_as_files_named_after_their_format() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dirs = Dirs::under(
            &std::env::temp_dir().join(format!("slatty-covers-{}", std::process::id())),
        );
        let folder = dirs.account_cache("111").join("covers");
        std::fs::create_dir_all(&folder).unwrap();
        let http = http::client().unwrap();
        // Nothing listens there: a cover found in the cache is never downloaded.
        let offline = "http://127.0.0.1:9/cover.jpg".to_string();

        // Cached by an earlier version: renamed after its format, without a download.
        std::fs::write(folder.join("1.img"), [0xFF, 0xD8, 0xFF, 0xE0, 1, 2]).unwrap();
        let path = cover(&http, &dirs, "111", &game_with_cover("1", offline.clone()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(path, folder.join("1.jpg"));
        assert!(path.is_file() && !folder.join("1.img").exists());
        let game = game_with_cover("1", offline.clone());
        let again = cover(&http, &dirs, "111", &game).await.unwrap();
        assert_eq!(again, Some(path));

        // Not an image: dropped, and asked for again.
        std::fs::write(folder.join("2.img"), b"<html>").unwrap();
        assert!(
            cover(&http, &dirs, "111", &game_with_cover("2", offline))
                .await
                .is_err()
        );
        assert!(!folder.join("2.img").exists());

        // A game id that would lead out of the cache folder is refused before anything is read.
        let escaping = game_with_cover("../covers/1", "http://127.0.0.1:9/c.jpg".into());
        assert!(matches!(
            cover(&http, &dirs, "111", &escaping).await,
            Err(crate::Error::Refused(_))
        ));

        // Downloaded: named after what it is, whatever its address says.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/cover.jpg", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = socket.read(&mut [0u8; 4096]).await;
            let png = b"\x89PNG\r\n\x1a\n";
            let head = format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", png.len());
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(png).await.unwrap();
        });
        let path = cover(&http, &dirs, "111", &game_with_cover("3", url))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(path, folder.join("3.png"));

        // Read all at once at start: covers named after their format, nothing else.
        std::fs::write(folder.join("4.img"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        std::fs::write(folder.join(".5.jpg.slatty-tmp-1-0"), b"").unwrap();
        std::fs::create_dir_all(folder.join("6.jpg")).unwrap();
        let found = cached_covers(&dirs, "111");
        assert_eq!(
            found,
            HashMap::from([
                ("1".to_string(), folder.join("1.jpg")),
                ("3".to_string(), folder.join("3.png"))
            ])
        );

        // Other images are kept by the hash of their address, which an earlier version used alone.
        let images = dirs.account_cache("111").join("images");
        let art = "http://127.0.0.1:9/art.jpg";
        let hash = fsutil::sha256_hex(art.as_bytes());
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join(&hash), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        let path = image(&http, &dirs, "111", art).await.unwrap();
        assert_eq!(path, images.join(format!("{hash}.jpg")));
        assert!(!images.join(&hash).exists());
        std::fs::remove_dir_all(dirs.cache.parent().unwrap()).unwrap();
    }

    /// GOG's game database failing for a while (an outage, a session expiring during a long sync)
    /// leaves bare entries: they do not replace what an earlier sync described.
    #[test]
    fn a_game_described_before_keeps_its_description_when_gog_fails_to_give_it() {
        let dirs = Dirs::under(
            &std::env::temp_dir().join(format!("slatty-lib-described-{}", std::process::id())),
        );
        let game = |id: &str, title: &str, metadata| LibraryGame {
            id: id.into(),
            title: title.into(),
            cover: (metadata == MetadataSource::Gamesdb).then(|| "https://images/[FAKE]".into()),
            icon: None,
            background: None,
            logo: None,
            os: if metadata == MetadataSource::Gamesdb {
                vec!["linux".into()]
            } else {
                Vec::new()
            },
            metadata,
        };
        save_cache(
            &dirs,
            "111",
            vec![game("1", "[FAKE] Described", MetadataSource::Gamesdb)],
        )
        .unwrap();
        let cache = save_cache(
            &dirs,
            "111",
            vec![
                game("1", "GOG product 1", MetadataSource::Missing),
                game("2", "GOG product 2", MetadataSource::Missing),
            ],
        )
        .unwrap();
        assert_eq!(cache.games[0].title, "[FAKE] Described");
        assert_eq!(cache.games[0].os, ["linux"]);
        assert!(cache.games[0].cover.is_some());
        assert_eq!(cache.games[1].title, "GOG product 2", "new: kept as it is");
        let saved = load_cache(&dirs, "111").unwrap().unwrap();
        assert_eq!(saved.games[0].metadata, MetadataSource::Gamesdb);
        std::fs::remove_dir_all(dirs.cache.parent().unwrap()).unwrap();
    }

    /// GOG handing back a page already read would have the sync ask for it forever: it stops with
    /// an error instead, and the library cached is kept.
    #[tokio::test]
    async fn a_library_listing_that_repeats_itself_stops() {
        let page = |next: Option<&str>| ReleasesPage {
            items: vec![Release {
                platform_id: "gog".into(),
                external_id: "1".into(),
                certificate: None,
            }],
            next_page_token: next.map(String::from),
        };
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let result = collect_releases(|token| {
            let n = asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(n < 10, "asked again and again");
            // "a", then "b", then "a" again.
            let next = match token.as_deref() {
                None => Some("a"),
                Some("a") => Some("b"),
                _ => Some("a"),
            };
            std::future::ready(Ok(page(next)))
        })
        .await;
        assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");

        let pages =
            collect_releases(|token| std::future::ready(Ok(page(token.is_none().then_some("a")))))
                .await
                .unwrap();
        assert_eq!(pages.len(), 2, "every page of a listing that ends");
    }

    #[tokio::test]
    async fn a_library_listing_that_never_ends_stops() {
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let result = collect_releases(|_| {
            let n = asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            assert!(n <= MAX_PAGES, "asked again and again");
            std::future::ready(Ok(ReleasesPage {
                items: Vec::new(),
                next_page_token: Some(n.to_string()),
            }))
        })
        .await;
        assert!(matches!(result, Err(Error::Parse { .. })), "{result:?}");
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
