//! The account's Epic games: the library service lists what it owns, Epic's catalog describes each
//! (title, images, whether it is an add-on), as legendary reads them (`EPCAPI.get_library_items`,
//! `get_game_info`, `LegendaryCore.get_non_asset_library_items`).

use std::collections::HashMap;
use std::path::PathBuf;

use futures::{StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil;
use crate::http;
use crate::library::{LibraryGame, MetadataSource};
use crate::paths::Dirs;
use crate::secret::Secret;
use crate::store;

const LIBRARY_URL: &str =
    "https://library-service.live.use1a.on.epicgames.com/library/api/public/items";
const CATALOG_HOST: &str = "https://catalog-public-service-prod06.ol.epicgames.com";
/// Pages of the library read at most: a cursor that never ends stops there.
const MAX_PAGES: usize = 200;
/// Catalog requests at once.
const AT_ONCE: usize = 8;

/// An Epic game of the account, as kept in its cache.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EpicGame {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub title: String,
    pub cover: Option<String>,
    pub background: Option<String>,
    pub logo: Option<String>,
    /// The systems Epic's catalog names for it, as GOG's are (`windows`, `osx`).
    pub os: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpicLibrary {
    pub account_id: String,
    pub display_name: String,
    pub fetched_at: i64,
    pub games: Vec<EpicGame>,
    /// The catalog items the catalog described as add-ons or mods, not asked about again.
    #[serde(default)]
    pub left_out: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct LibraryPage {
    #[serde(default)]
    pub records: Vec<Record>,
    #[serde(rename = "responseMetadata", default)]
    pub metadata: PageMetadata,
}

#[derive(Debug, Default, Deserialize)]
pub struct PageMetadata {
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub namespace: String,
    pub catalog_item_id: String,
    pub app_name: Option<String>,
    pub sandbox_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub key_images: Vec<KeyImage>,
    /// The game an add-on belongs to: none for a game.
    pub main_game_item: Option<serde_json::Value>,
    #[serde(default)]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub release_info: Vec<ReleaseInfo>,
    /// Among others `FolderName`, the folder the game installs into, and `CanRunOffline`.
    #[serde(default)]
    pub custom_attributes: HashMap<String, CustomAttribute>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomAttribute {
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyImage {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub platform: Vec<String>,
}

/// Where the library and the catalog come from: Epic's services, or a test double.
pub trait Source: Send + Sync {
    fn library_page(
        &self,
        cursor: Option<String>,
    ) -> impl Future<Output = Result<LibraryPage>> + Send;
    /// The catalog's description of an item, none when it has none.
    fn catalog_item(
        &self,
        namespace: String,
        catalog_item_id: String,
    ) -> impl Future<Output = Result<Option<CatalogItem>>> + Send;
}

/// Epic's own services, with the account's access token.
pub struct Epic<'a> {
    pub http: &'a Client,
    pub token: &'a Secret,
}

impl Epic<'_> {
    pub(crate) fn get(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .bearer_auth(self.token.expose())
            .header(reqwest::header::USER_AGENT, super::USER_AGENT)
    }
}

impl Source for Epic<'_> {
    async fn library_page(&self, cursor: Option<String>) -> Result<LibraryPage> {
        let mut query = vec![("includeMetadata", "true".to_string())];
        query.extend(cursor.map(|c| ("cursor", c)));
        let req = self.get(LIBRARY_URL).query(&query);
        session_checked(http::json(req, "reading the Epic library").await)
    }

    async fn catalog_item(
        &self,
        namespace: String,
        catalog_item_id: String,
    ) -> Result<Option<CatalogItem>> {
        let url = format!("{CATALOG_HOST}/catalog/api/shared/namespace/{namespace}/bulk/items");
        let req = self.get(&url).query(&[
            ("id", catalog_item_id.as_str()),
            ("includeDLCDetails", "true"),
            ("includeMainGameDetails", "true"),
            ("country", "US"),
            ("locale", "en"),
        ]);
        let mut items: HashMap<String, CatalogItem> =
            session_checked(http::json(req, "reading Epic's catalog").await)?;
        Ok(items.remove(&catalog_item_id))
    }
}

/// A token Epic no longer accepts is a session to sign in to again.
pub(crate) fn session_checked<T>(result: Result<T>) -> Result<T> {
    match result {
        Err(Error::Http { status: 401, .. }) => Err(Error::EpicSessionRejected),
        other => other,
    }
}

/// The games of the account, and the catalog items left out as add-ons or mods. Those `known`
/// already described keep their description, and those it left out stay out: the catalog is asked
/// only about new ones. Add-ons, mods, Unreal Engine's marketplace and private sandboxes (games in
/// development) are left out, as legendary does.
pub async fn fetch(
    source: &impl Source,
    known: &[EpicGame],
    left_out: &[String],
) -> Result<(Vec<EpicGame>, Vec<String>)> {
    let mut records = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_PAGES {
        let page = source.library_page(cursor).await?;
        records.extend(page.records);
        cursor = page.metadata.next_cursor.filter(|c| !c.is_empty());
        if cursor.is_none() {
            break;
        }
    }
    let mut seen = std::collections::HashSet::new();
    let owned: Vec<(String, Record)> = records
        .into_iter()
        .filter(|r| r.namespace != "ue" && r.sandbox_type.as_deref() != Some("PRIVATE"))
        .filter_map(|r| {
            let app = r.app_name.clone()?;
            if app == "1" || !store::is_epic_app_name(&app) {
                if app != "1" {
                    tracing::warn!("an Epic app name that cannot name files was left out");
                }
                return None;
            }
            seen.insert(app.clone()).then_some((app, r))
        })
        .collect();
    let was_left_out: std::collections::HashSet<&str> =
        left_out.iter().map(String::as_str).collect();
    let owned: Vec<(String, Record)> = owned
        .into_iter()
        .filter(|(_, r)| !was_left_out.contains(r.catalog_item_id.as_str()))
        .collect();
    let known: HashMap<(&str, &str), &EpicGame> = known
        .iter()
        .map(|g| ((g.app_name.as_str(), g.catalog_item_id.as_str()), g))
        .collect();
    // None for an item the catalog has no description of this time: asked about again next time.
    let found: Vec<Option<(String, Option<EpicGame>)>> =
        futures::stream::iter(owned.into_iter().map(|(app, r)| {
            let known = known
                .get(&(app.as_str(), r.catalog_item_id.as_str()))
                .map(|g| (*g).clone());
            async move {
                let id = r.catalog_item_id.clone();
                if let Some(game) = known {
                    return Ok(Some((id, Some(game))));
                }
                let item = source
                    .catalog_item(r.namespace.clone(), r.catalog_item_id.clone())
                    .await?;
                Ok::<_, Error>(item.map(|item| (id, game(app, r, item))))
            }
        }))
        .buffer_unordered(AT_ONCE)
        .try_collect()
        .await?;
    let mut games = Vec::new();
    let mut left_out: Vec<String> = was_left_out.into_iter().map(String::from).collect();
    for (id, game) in found.into_iter().flatten() {
        match game {
            Some(game) => games.push(game),
            None => left_out.push(id),
        }
    }
    left_out.sort();
    left_out.dedup();
    games.sort_by(|a, b| a.title.cmp(&b.title).then(a.app_name.cmp(&b.app_name)));
    Ok((games, left_out))
}

/// A game of the library as the catalog describes it: none for an add-on or a mod.
fn game(app_name: String, record: Record, item: CatalogItem) -> Option<EpicGame> {
    if item.main_game_item.is_some() || item.categories.iter().any(|c| c.path == "mods") {
        return None;
    }
    let image = |kinds: &[&str]| {
        kinds.iter().find_map(|kind| {
            item.key_images
                .iter()
                .find(|i| i.kind == *kind && i.url.starts_with("https://"))
                .map(|i| i.url.clone())
        })
    };
    let platforms: Vec<&str> = item
        .release_info
        .iter()
        .flat_map(|r| r.platform.iter().map(String::as_str))
        .collect();
    let os = [("Windows", "windows"), ("Mac", "osx")]
        .into_iter()
        .filter(|(epic, _)| platforms.contains(epic))
        .map(|(_, os)| os.to_string())
        .collect();
    Some(EpicGame {
        title: if item.title.is_empty() {
            app_name.clone()
        } else {
            item.title.clone()
        },
        cover: image(&["DieselGameBoxTall", "OfferImageTall", "Thumbnail"]),
        background: image(&["DieselGameBox", "OfferImageWide"]),
        logo: image(&["DieselGameBoxLogo"]),
        os,
        app_name,
        namespace: record.namespace,
        catalog_item_id: record.catalog_item_id,
    })
}

/// A game as the library shows it.
pub fn library_game(game: &EpicGame) -> LibraryGame {
    LibraryGame {
        id: store::epic_id(&game.app_name),
        title: game.title.clone(),
        cover: game.cover.clone(),
        icon: None,
        background: game.background.clone(),
        logo: game.logo.clone(),
        os: game.os.clone(),
        metadata: MetadataSource::Product,
        steam: None,
    }
}

/// The account's folder in the cache: its library, its covers.
fn cache_folder(dirs: &Dirs, account_id: &str) -> Result<PathBuf> {
    super::auth::check_account_id(account_id)?;
    Ok(dirs.cache.join("epic").join(account_id))
}

pub fn load_cache(dirs: &Dirs, account_id: &str) -> Result<Option<EpicLibrary>> {
    let path = cache_folder(dirs, account_id)?.join("library.json");
    match std::fs::read(&path) {
        Ok(raw) => Ok(serde_json::from_slice(&raw).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
    }
}

pub fn save_cache(dirs: &Dirs, library: &EpicLibrary) -> Result<()> {
    let path = cache_folder(dirs, &library.account_id)?.join("library.json");
    let json = serde_json::to_vec(library).map_err(|e| Error::parse("the Epic library", e))?;
    fsutil::write_atomic(&path, &json)
}

/// Reads the account's library again, from its cache's descriptions and the catalog for new
/// games, and keeps it in the cache.
pub async fn refresh(
    http: &Client,
    dirs: &Dirs,
    tokens: &super::auth::Tokens,
) -> Result<EpicLibrary> {
    let (known, left_out) = load_cache(dirs, &tokens.account_id)?
        .map(|l| (l.games, l.left_out))
        .unwrap_or_default();
    let source = Epic {
        http,
        token: &tokens.access_token,
    };
    let (games, left_out) = fetch(&source, &known, &left_out).await?;
    let library = EpicLibrary {
        account_id: tokens.account_id.clone(),
        display_name: tokens.display_name.clone(),
        fetched_at: chrono::Utc::now().timestamp(),
        games,
        left_out,
    };
    save_cache(dirs, &library)?;
    Ok(library)
}

/// The covers already in the account's cache, by game id.
pub fn cached_covers(dirs: &Dirs, account_id: &str) -> Result<HashMap<String, PathBuf>> {
    Ok(crate::library::covers_in(
        &cache_folder(dirs, account_id)?.join("covers"),
    ))
}

/// The file of a game's cover in the account's cache, downloaded from Epic's image servers when
/// missing.
pub async fn cover(
    http: &Client,
    dirs: &Dirs,
    account_id: &str,
    game: &EpicGame,
) -> Result<Option<PathBuf>> {
    let Some(url) = &game.cover else {
        return Ok(None);
    };
    let folder = cache_folder(dirs, account_id)?.join("covers");
    let id = store::epic_id(&game.app_name);
    crate::library::cached(http, folder, id, url, "downloading an Epic cover")
        .await
        .map(Some)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// The library in pages, and the catalog, as Epic's services answer; the catalog requests
    /// are counted.
    struct Fake {
        pages: Vec<LibraryPage>,
        catalog: HashMap<String, CatalogItem>,
        asked: Mutex<Vec<String>>,
    }

    impl Source for Fake {
        async fn library_page(&self, cursor: Option<String>) -> Result<LibraryPage> {
            let at: usize = cursor.map_or(0, |c| c.parse().unwrap());
            let page = &self.pages[at];
            Ok(LibraryPage {
                records: page.records.clone(),
                metadata: PageMetadata {
                    next_cursor: page.metadata.next_cursor.clone(),
                },
            })
        }

        async fn catalog_item(&self, _: String, id: String) -> Result<Option<CatalogItem>> {
            self.asked.lock().unwrap().push(id.clone());
            Ok(self.catalog.get(&id).cloned())
        }
    }

    fn record(app: Option<&str>, namespace: &str, id: &str) -> Record {
        Record {
            namespace: namespace.into(),
            catalog_item_id: id.into(),
            app_name: app.map(String::from),
            sandbox_type: Some("PUBLIC".into()),
        }
    }

    fn item(title: &str) -> CatalogItem {
        serde_json::from_value(serde_json::json!({
            "title": title,
            "keyImages": [
                {"type": "DieselGameBox", "url": "https://cdn1.epicgames.example/wide.jpg"},
                {"type": "DieselGameBoxTall", "url": "https://cdn1.epicgames.example/tall.jpg"},
                {"type": "DieselGameBoxLogo", "url": "http://cdn1.epicgames.example/logo.png"},
            ],
            "releaseInfo": [{"platform": ["Windows"]}],
        }))
        .unwrap()
    }

    fn fake() -> Fake {
        let mut addon = item("[FAKE] Add-on");
        addon.main_game_item = Some(serde_json::json!({"id": "c1"}));
        let mut private = record(Some("Private"), "n5", "c5");
        private.sandbox_type = Some("PRIVATE".into());
        Fake {
            pages: vec![
                LibraryPage {
                    records: vec![
                        record(Some("Beta"), "n1", "c1"),
                        record(Some("Addon"), "n1", "c2"),
                        record(None, "n3", "c3"),
                    ],
                    metadata: PageMetadata {
                        next_cursor: Some("1".into()),
                    },
                },
                LibraryPage {
                    records: vec![
                        record(Some("Alpha"), "n4", "c4"),
                        record(Some("Undescribed"), "n9", "c9"),
                        record(Some("Beta"), "n1", "c1"),
                        record(Some("Engine"), "ue", "c6"),
                        record(Some("../x"), "n7", "c7"),
                        private,
                    ],
                    metadata: PageMetadata::default(),
                },
            ],
            catalog: [
                ("c1", item("[FAKE] Beta")),
                ("c2", addon),
                ("c4", item("[FAKE] Alpha")),
                ("c6", item("[FAKE] Engine")),
                ("c7", item("[FAKE] Odd")),
            ]
            .into_iter()
            .map(|(id, item)| (id.to_string(), item))
            .collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// Every page is read; add-ons, Unreal Engine's marketplace, private sandboxes, records
    /// without an app and app names unsafe in a file name are left out, and a game listed twice
    /// shows once.
    #[tokio::test]
    async fn the_library_lists_the_accounts_games_alone() {
        let source = fake();
        let (games, left_out) = fetch(&source, &[], &[]).await.unwrap();
        assert_eq!(left_out, ["c2"]);
        let apps: Vec<&str> = games.iter().map(|g| g.app_name.as_str()).collect();
        assert_eq!(apps, ["Alpha", "Beta"], "sorted by title");
        let beta = &games[1];
        assert_eq!(beta.title, "[FAKE] Beta");
        assert_eq!(beta.namespace, "n1");
        assert_eq!(beta.os, ["windows"]);
        assert_eq!(
            beta.cover.as_deref(),
            Some("https://cdn1.epicgames.example/tall.jpg")
        );
        assert_eq!(
            beta.background.as_deref(),
            Some("https://cdn1.epicgames.example/wide.jpg")
        );
        assert_eq!(beta.logo, None, "only images over HTTPS");
        let mut asked = source.asked.lock().unwrap().clone();
        asked.sort();
        assert_eq!(
            asked,
            ["c1", "c2", "c4", "c9"],
            "never about what is left out"
        );
    }

    /// The catalog is asked only about items the cache neither described nor left out: an item it
    /// had no description of is not shown, and asked about again.
    #[tokio::test]
    async fn items_already_described_are_not_asked_about_again() {
        let source = fake();
        let (first, left_out) = fetch(&source, &[], &[]).await.unwrap();
        source.asked.lock().unwrap().clear();
        let again = fetch(&source, &first, &left_out).await.unwrap();
        assert_eq!(again, (first, left_out));
        assert_eq!(
            *source.asked.lock().unwrap(),
            ["c9"],
            "the item the catalog did not describe, asked about again"
        );
    }

    #[test]
    fn a_game_shows_under_its_epic_id() {
        let game = game(
            "Alpha".into(),
            record(Some("Alpha"), "n4", "c4"),
            item("[FAKE] Alpha"),
        )
        .unwrap();
        let shown = library_game(&game);
        assert_eq!(shown.id, "epic-Alpha");
        assert_eq!(store::of(&shown.id).unwrap(), store::Store::Epic);
    }

    #[test]
    fn the_cache_keeps_the_library_of_its_account() {
        let root = std::env::temp_dir().join(format!("slatty-epic-cache-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let library = EpicLibrary {
            account_id: "0123456789abcdef0123456789abcdef".into(),
            display_name: "[FAKE] player".into(),
            fetched_at: 1,
            games: vec![
                game(
                    "Alpha".into(),
                    record(Some("Alpha"), "n4", "c4"),
                    item("[FAKE] Alpha"),
                )
                .unwrap(),
            ],
            left_out: Vec::new(),
        };
        save_cache(&dirs, &library).unwrap();
        let read = load_cache(&dirs, &library.account_id).unwrap().unwrap();
        assert_eq!(read.games, library.games);
        assert!(load_cache(&dirs, "../../etc").is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
