//! Galaxy content system, generation 2. Formats and endpoints follow heroic-gogdl
//! (GPL-3.0, https://github.com/Heroic-Games-Launcher/heroic-gogdl).

use std::future::Future;
use std::io::Read;

use flate2::read::ZlibDecoder;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::http;

const CONTENT_SYSTEM: &str = "https://content-system.gog.com";
const CDN: &str = "https://gog-cdn-fastly.gog.com";

#[derive(Debug, Clone, Deserialize)]
pub struct Build {
    pub build_id: String,
    pub version_name: String,
    pub link: String,
    pub branch: Option<String>,
    pub generation: u32,
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
    if hash.contains('/') || hash.len() < 4 {
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
    fn depot_manifest(&self, manifest: &str)
    -> impl Future<Output = Result<Vec<DepotItem>>> + Send;
    /// Compressed chunk bytes, exactly as stored on the CDN.
    fn chunk(&self, compressed_md5: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
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
    let resp = http::send(http.get(build.link.as_str()), "fetching build metadata").await?;
    let raw = resp
        .bytes()
        .await
        .map_err(|e| Error::network("fetching build metadata", e))?;
    decode_zlib_json(&raw, "build metadata")
}

#[derive(Debug, Clone, Deserialize)]
struct Endpoint {
    url_format: String,
    parameters: serde_json::Map<String, Value>,
}

/// CDN access for one product. Secure links expire; they are refreshed on 401/403.
pub struct GogContent {
    http: Client,
    tokens: Tokens,
    product_id: String,
    endpoints: tokio::sync::RwLock<Vec<Endpoint>>,
}

impl GogContent {
    pub async fn new(http: Client, tokens: Tokens, product_id: &str) -> Result<Self> {
        let endpoints = secure_link(&http, &tokens, product_id).await?;
        Ok(Self {
            http,
            tokens,
            product_id: product_id.to_string(),
            endpoints: tokio::sync::RwLock::new(endpoints),
        })
    }

    async fn chunk_url(&self, compressed_md5: &str, index: usize) -> Option<String> {
        let endpoints = self.endpoints.read().await;
        let e = endpoints.get(index)?;
        let mut params = e.parameters.clone();
        let path = params
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        params.insert(
            "path".into(),
            Value::String(format!("{path}/{}", galaxy_path(compressed_md5))),
        );
        let mut url = e.url_format.clone();
        for (k, v) in &params {
            let value = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            url = url.replace(&format!("{{{k}}}"), &value);
        }
        Some(url)
    }
}

async fn secure_link(http: &Client, tokens: &Tokens, product_id: &str) -> Result<Vec<Endpoint>> {
    #[derive(Deserialize)]
    struct Links {
        urls: Vec<Endpoint>,
    }
    let url = format!(
        "{CONTENT_SYSTEM}/products/{product_id}/secure_link?_version=2&generation=2&path=/"
    );
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

impl ContentSource for GogContent {
    async fn depot_manifest(&self, manifest: &str) -> Result<Vec<DepotItem>> {
        let url = format!("{CDN}/content-system/v2/meta/{}", galaxy_path(manifest));
        let resp = http::send(self.http.get(url), "fetching a depot manifest").await?;
        let raw = resp
            .bytes()
            .await
            .map_err(|e| Error::network("fetching a depot manifest", e))?;
        parse_depot_items(&raw)
    }

    async fn chunk(&self, compressed_md5: &str) -> Result<Vec<u8>> {
        let mut last = None;
        for attempt in 0..6 {
            let count = self.endpoints.read().await.len();
            let Some(url) = self.chunk_url(compressed_md5, attempt % count.max(1)).await else {
                break;
            };
            match http::send(self.http.get(url), "downloading a chunk").await {
                Ok(resp) => match resp.bytes().await {
                    Ok(b) => return Ok(b.to_vec()),
                    Err(e) => last = Some(Error::network("downloading a chunk", e)),
                },
                Err(Error::Http {
                    status: 401 | 403, ..
                }) => {
                    *self.endpoints.write().await =
                        secure_link(&self.http, &self.tokens, &self.product_id).await?;
                    last = Some(Error::Http {
                        context: "downloading a chunk",
                        status: 403,
                    });
                }
                Err(e) => last = Some(e),
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
