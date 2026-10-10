//! Galaxy content system, generation 2. Formats and endpoints follow heroic-gogdl
//! (GPL-3.0, https://github.com/Heroic-Games-Launcher/heroic-gogdl).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::Read;

use flate2::read::ZlibDecoder;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::http;
use crate::paths::Dirs;

mod speeds;

const CONTENT_SYSTEM: &str = "https://content-system.gog.com";
const CDN: &str = "https://gog-cdn-fastly.gog.com";

#[derive(Debug, Clone, Deserialize)]
pub struct Build {
    pub build_id: String,
    pub version_name: String,
    pub link: String,
    pub branch: Option<String>,
    pub generation: u32,
    /// When GOG published the build, as GOG writes it (`2026-03-27T06:59:13+0000`).
    #[serde(default)]
    pub date_published: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub version: Option<u32>,
    pub base_product_id: String,
    pub client_id: Option<String>,
    pub install_directory: String,
    #[serde(default)]
    pub depots: Vec<Depot>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub products: Vec<Product>,
    #[serde(default)]
    pub script_interpreter: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depot {
    pub product_id: String,
    #[serde(default)]
    pub languages: Vec<String>,
    pub manifest: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub compressed_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub product_id: String,
    pub name: String,
    #[serde(default, rename = "temp_executable")]
    pub temp_executable: Option<String>,
    #[serde(default, rename = "temp_arguments")]
    pub temp_arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum DepotItem {
    DepotFile(DepotFile),
    DepotDirectory { path: String },
    DepotLink { path: String, target: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepotFile {
    pub path: String,
    #[serde(default)]
    pub chunks: Vec<Chunk>,
    #[serde(default)]
    pub flags: Vec<String>,
    pub md5: Option<String>,
    #[serde(rename = "sfcRef")]
    pub sfc_ref: Option<Value>,
    /// Product (game or DLC) whose depot lists this file; set after parsing.
    #[serde(skip)]
    pub product_id: String,
}

impl DepotFile {
    pub fn size(&self) -> u64 {
        self.chunks.iter().map(|c| c.size).sum()
    }

    pub fn is_support(&self) -> bool {
        self.flags.iter().any(|f| f == "support")
    }

    pub fn is_executable(&self) -> bool {
        self.flags.iter().any(|f| f == "executable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    pub md5: String,
    pub compressed_md5: String,
    pub size: u64,
    pub compressed_size: u64,
}

#[derive(Deserialize)]
struct DepotManifest {
    depot: DepotItems,
}

#[derive(Deserialize)]
struct DepotItems {
    items: Vec<DepotItem>,
}

/// `abcdef…` → `ab/cd/abcdef…`, the CDN layout for manifests and chunks.
pub fn galaxy_path(hash: &str) -> String {
    if hash.contains('/') || hash.len() < 4 || !hash.is_ascii() {
        hash.to_string()
    } else {
        format!("{}/{}/{hash}", &hash[0..2], &hash[2..4])
    }
}

pub fn decode_zlib_json<T: serde::de::DeserializeOwned>(
    raw: &[u8],
    context: &'static str,
) -> Result<T> {
    let mut text = Vec::new();
    match ZlibDecoder::new(raw).read_to_end(&mut text) {
        Ok(_) => serde_json::from_slice(&text),
        Err(_) => serde_json::from_slice(raw),
    }
    .map_err(|e| Error::parse(context, e))
}

pub fn parse_depot_items(raw: &[u8]) -> Result<Vec<DepotItem>> {
    Ok(decode_zlib_json::<DepotManifest>(raw, "depot manifest")?
        .depot
        .items)
}

/// Where installer data comes from; the real CDN or a test double.
pub trait ContentSource: Send + Sync {
    fn depot_manifest(&self, depot: &Depot) -> impl Future<Output = Result<Vec<DepotItem>>> + Send;
    /// Compressed chunk bytes, exactly as stored on the CDN.
    fn chunk(
        &self,
        product_id: &str,
        compressed_md5: &str,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

pub async fn builds(http: &Client, tokens: &Tokens, game_id: &str) -> Result<Vec<Build>> {
    #[derive(Deserialize)]
    struct Builds {
        #[serde(default)]
        items: Vec<Build>,
    }
    let url = format!("{CONTENT_SYSTEM}/products/{game_id}/os/windows/builds?generation=2");
    let builds: Builds = http::json(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "listing builds",
    )
    .await?;
    Ok(builds.items)
}

pub async fn meta(http: &Client, build: &Build) -> Result<Meta> {
    let raw = http::bytes(http.get(build.link.as_str()), "fetching build metadata").await?;
    decode_zlib_json(&raw, "build metadata")
}

#[derive(Debug, Clone, Deserialize)]
struct Endpoint {
    #[serde(default)]
    endpoint_name: String,
    url_format: String,
    parameters: serde_json::Map<String, Value>,
}

impl Endpoint {
    /// Name under which its speed is remembered.
    fn key(&self) -> String {
        if self.endpoint_name.is_empty() {
            self.parameters
                .get("base")
                .and_then(Value::as_str)
                .unwrap_or(&self.url_format)
                .to_string()
        } else {
            self.endpoint_name.clone()
        }
    }
}

/// CDN access for a game and its DLC. Each product has its own download links, fetched on first
/// use; links expire and are refreshed on 401/403, with the access token renewed when it has
/// expired too (a download can outlast it).
pub struct GogContent {
    http: Client,
    dirs: Dirs,
    tokens: tokio::sync::Mutex<Tokens>,
    endpoints: tokio::sync::RwLock<HashMap<String, Vec<Endpoint>>>,
}

impl GogContent {
    pub fn new(http: Client, tokens: Tokens, dirs: &Dirs) -> Self {
        Self {
            http,
            dirs: dirs.clone(),
            tokens: tokio::sync::Mutex::new(tokens),
            endpoints: tokio::sync::RwLock::new(HashMap::new()),
        }
    }

    async fn endpoints_for(&self, product_id: &str, refresh: bool) -> Result<Vec<Endpoint>> {
        if !refresh && let Some(e) = self.endpoints.read().await.get(product_id) {
            return Ok(e.clone());
        }
        let tokens = self.access().await?;
        let fresh = if product_id == REDIST {
            dependency_link(&self.http, &tokens).await?
        } else if let Some(id) = product_id.strip_suffix(PATCH_STORE) {
            secure_link(&self.http, &tokens, id, Some("/patches/store")).await?
        } else {
            secure_link(&self.http, &tokens, product_id, None).await?
        };
        self.endpoints
            .write()
            .await
            .insert(product_id.to_string(), fresh.clone());
        Ok(fresh)
    }

    /// Asks `endpoints[i]` for a chunk; when it is late, asks a copy from another endpoint and
    /// keeps the first answer. Returns the endpoint that answered, its result and its duration.
    async fn fetch_hedged(
        &self,
        endpoints: &[Endpoint],
        names: &[&str],
        i: usize,
        avoid: &[usize],
        compressed_md5: &str,
    ) -> (usize, Result<Vec<u8>>, std::time::Duration) {
        let started = std::time::Instant::now();
        let first = self.fetch(&endpoints[i], compressed_md5);
        tokio::pin!(first);
        let Some(wait) = endpoint_speeds().hedge_after(names[i]) else {
            return (i, first.await, started.elapsed());
        };
        tokio::select! {
            r = &mut first => return (i, r, started.elapsed()),
            () = tokio::time::sleep(wait) => {}
        }
        let Some(_slot) = CopySlot::take() else {
            return (i, first.await, started.elapsed());
        };
        let stalled: Vec<usize> = avoid.iter().copied().chain([i]).collect();
        let j = endpoint_speeds().pick(names, &stalled);
        let copy_started = std::time::Instant::now();
        let copy = self.fetch(&endpoints[j], compressed_md5);
        tokio::pin!(copy);
        // The first success wins; an error counts only once the other request has failed too.
        tokio::select! {
            r = &mut first => {
                if r.is_ok() {
                    endpoint_speeds().abandoned(names[j]);
                    return (i, r, started.elapsed());
                }
                let c = (&mut copy).await;
                if c.is_ok() {
                    endpoint_speeds().failed(names[i]);
                    (j, c, copy_started.elapsed())
                } else {
                    endpoint_speeds().failed(names[j]);
                    (i, r, started.elapsed())
                }
            }
            c = &mut copy => {
                if let Ok(b) = &c {
                    endpoint_speeds().outrun(names[i], b.len(), started.elapsed());
                    return (j, c, copy_started.elapsed());
                }
                endpoint_speeds().failed(names[j]);
                (i, (&mut first).await, started.elapsed())
            }
        }
    }

    async fn fetch(&self, endpoint: &Endpoint, compressed_md5: &str) -> Result<Vec<u8>> {
        let url = chunk_url(endpoint, compressed_md5);
        let resp = http::send(self.http.get(url), "downloading a chunk").await?;
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| Error::network("downloading a chunk", e))
    }

    async fn access(&self) -> Result<Tokens> {
        let mut tokens = self.tokens.lock().await;
        if tokens.is_expired() {
            *tokens = crate::account::fresh(&self.http, &self.dirs, &tokens, false).await?;
        }
        Ok(tokens.clone())
    }
}

/// At most one chunk copy in flight for the whole process: when the connection itself slows down,
/// every request turns late, and copying them all would only add load.
struct CopySlot;

static COPY_IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl CopySlot {
    fn take() -> Option<CopySlot> {
        use std::sync::atomic::Ordering;
        COPY_IN_FLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| CopySlot)
    }
}

impl Drop for CopySlot {
    fn drop(&mut self) {
        COPY_IN_FLIGHT.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Shared by every download of the process, so each operation does not measure again.
fn endpoint_speeds() -> std::sync::MutexGuard<'static, speeds::Speeds> {
    static SPEEDS: std::sync::LazyLock<std::sync::Mutex<speeds::Speeds>> =
        std::sync::LazyLock::new(Default::default);
    SPEEDS.lock().unwrap_or_else(|e| e.into_inner())
}

fn chunk_url(endpoint: &Endpoint, compressed_md5: &str) -> String {
    let mut params = endpoint.parameters.clone();
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    params.insert(
        "path".into(),
        Value::String(format!("{path}/{}", galaxy_path(compressed_md5))),
    );
    let mut url = endpoint.url_format.clone();
    for (k, v) in &params {
        let value = match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        url = url.replace(&format!("{{{k}}}"), &value);
    }
    url
}

async fn secure_link(
    http: &Client,
    tokens: &Tokens,
    product_id: &str,
    root: Option<&str>,
) -> Result<Vec<Endpoint>> {
    #[derive(Deserialize)]
    struct Links {
        urls: Vec<Endpoint>,
    }
    let mut url = format!(
        "{CONTENT_SYSTEM}/products/{product_id}/secure_link?_version=2&generation=2&path=/"
    );
    if let Some(root) = root {
        url.push_str(&format!("&root={root}"));
    }
    let links: Links = http::json(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "requesting download links",
    )
    .await?;
    if links.urls.is_empty() {
        return Err(Error::parse("requesting download links", "no endpoint"));
    }
    Ok(links.urls)
}

/// Pseudo product id for GOG's shared dependency store (redistributables, script interpreter).
pub const REDIST: &str = "redist";

const PATCH_STORE: &str = "#patches";

/// Pseudo product id for a product's patch store (binary deltas between builds).
pub fn patch_store(product_id: &str) -> String {
    format!("{product_id}{PATCH_STORE}")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub dependency_id: String,
    pub executable: DependencyExecutable,
    pub manifest: String,
    #[serde(default)]
    pub readable_name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub compressed_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DependencyExecutable {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub arguments: String,
}

impl Dependency {
    /// Installed once, outside the game folder (`__redist/…`); the others ship files into the game folder.
    pub fn is_shared(&self) -> bool {
        self.executable.path.starts_with("__redist")
    }

    pub fn depot(&self) -> Depot {
        Depot {
            product_id: REDIST.into(),
            languages: vec!["*".into()],
            manifest: self.manifest.clone(),
            size: self.size,
            compressed_size: self.compressed_size,
        }
    }
}

/// GOG's dependency repository (generation 2).
pub async fn dependencies(http: &Client, tokens: &Tokens) -> Result<Vec<Dependency>> {
    #[derive(Deserialize)]
    struct Repository {
        repository_manifest: String,
    }
    #[derive(Deserialize)]
    struct Manifest {
        depots: Vec<Dependency>,
    }
    let url = format!("{CONTENT_SYSTEM}/dependencies/repository?generation=2");
    let repo: Repository = http::json(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "reading the dependency repository",
    )
    .await?;
    let raw = http::bytes(
        http.get(repo.repository_manifest.as_str()),
        "reading the dependency list",
    )
    .await?;
    Ok(decode_zlib_json::<Manifest>(&raw, "dependency list")?.depots)
}

async fn dependency_link(http: &Client, tokens: &Tokens) -> Result<Vec<Endpoint>> {
    #[derive(Deserialize)]
    struct Link {
        url: String,
    }
    #[derive(Deserialize)]
    struct Links {
        urls: Vec<Link>,
    }
    let url =
        format!("{CONTENT_SYSTEM}/open_link?generation=2&_version=2&path=/dependencies/store/");
    let links: Links = http::json(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "requesting dependency links",
    )
    .await?;
    if links.urls.is_empty() {
        return Err(Error::parse("requesting dependency links", "no endpoint"));
    }
    Ok(links
        .urls
        .into_iter()
        .map(|l| {
            let mut parameters = serde_json::Map::new();
            parameters.insert(
                "base".into(),
                Value::String(l.url.trim_end_matches('/').to_string()),
            );
            parameters.insert("path".into(), Value::String(String::new()));
            Endpoint {
                endpoint_name: String::new(),
                url_format: "{base}{path}".into(),
                parameters,
            }
        })
        .collect())
}

/// Product ids (games and DLC) the account owns.
pub async fn owned_products(http: &Client, tokens: &Tokens) -> Result<HashSet<String>> {
    #[derive(Deserialize)]
    struct Owned {
        owned: Vec<Value>,
    }
    let req = http
        .get("https://embed.gog.com/user/data/games")
        .bearer_auth(tokens.access_token.expose());
    let owned: Owned = http::json(req, "listing owned products").await?;
    Ok(owned
        .owned
        .into_iter()
        .map(|v| match v {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .collect())
}

impl ContentSource for GogContent {
    async fn depot_manifest(&self, depot: &Depot) -> Result<Vec<DepotItem>> {
        let kind = if depot.product_id == REDIST {
            "dependencies/meta"
        } else {
            "meta"
        };
        let url = format!(
            "{CDN}/content-system/v2/{kind}/{}",
            galaxy_path(&depot.manifest)
        );
        let raw = http::bytes(self.http.get(url), "fetching a depot manifest").await?;
        parse_depot_items(&raw)
    }

    async fn chunk(&self, product_id: &str, compressed_md5: &str) -> Result<Vec<u8>> {
        let mut endpoints = self.endpoints_for(product_id, false).await?;
        let mut last = None;
        let mut failed_here = Vec::new();
        for attempt in 0..6 {
            let keys: Vec<String> = endpoints.iter().map(Endpoint::key).collect();
            let names: Vec<&str> = keys.iter().map(String::as_str).collect();
            let i = endpoint_speeds().pick(&names, &failed_here);
            let (served, result, took) = self
                .fetch_hedged(&endpoints, &names, i, &failed_here, compressed_md5)
                .await;
            match result {
                Ok(b) => {
                    tracing::debug!(
                        endpoint = names[served],
                        secs = took.as_secs_f64(),
                        hedged = served != i,
                        "chunk"
                    );
                    endpoint_speeds().record(names[served], b.len(), took);
                    return Ok(b);
                }
                Err(Error::Http {
                    status: 401 | 403, ..
                }) => {
                    endpoint_speeds().abandoned(names[served]);
                    endpoints = self.endpoints_for(product_id, true).await?;
                    last = Some(Error::Http {
                        context: "downloading a chunk",
                        status: 403,
                    });
                }
                Err(e) => {
                    endpoint_speeds().failed(names[served]);
                    failed_here.push(served);
                    last = Some(e);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500 * (attempt as u64 + 1))).await;
        }
        Err(last.unwrap_or(Error::Refused("no download endpoint".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galaxy_path_splits_hash() {
        assert_eq!(galaxy_path("abcdef0123"), "ab/cd/abcdef0123");
        assert_eq!(galaxy_path("ab/cd/abcdef"), "ab/cd/abcdef");
        assert_eq!(
            galaxy_path("aé123"),
            "aé123",
            "never cut inside a character"
        );
    }

    #[test]
    fn parses_depot_items() {
        let json = br#"{"depot":{"items":[
            {"type":"DepotFile","path":"bin\\game.exe","flags":["executable"],
             "chunks":[{"md5":"a","compressedMd5":"b","size":3,"compressedSize":5}]},
            {"type":"DepotDirectory","path":"saves"},
            {"type":"DepotLink","path":"x","target":"y"}]}}"#;
        let items = parse_depot_items(json).unwrap();
        assert!(matches!(&items[0], DepotItem::DepotFile(f) if f.is_executable() && f.size() == 3));
        assert!(matches!(&items[1], DepotItem::DepotDirectory { .. }));
        assert!(matches!(&items[2], DepotItem::DepotLink { .. }));
    }
}
