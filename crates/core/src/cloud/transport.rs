use std::future::Future;
use std::io::Write;

use chrono::{DateTime, SecondsFormat, Utc};
use flate2::Compression;
use flate2::write::GzEncoder;
use md5::{Digest, Md5};
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use crate::error::{Error, Result};
use crate::fsutil;
use crate::http;
use crate::secret::Secret;

/// Remote entries carrying this hash are ignored, as gogdl does; their meaning is unverified.
pub const IGNORED_REMOTE_HASH: &str = "aadd86936a80ee8a369579c3926f1b3c";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub hash: String,
}

pub trait CloudTransport: Send + Sync {
    fn list(&self) -> impl Future<Output = Result<Vec<RemoteEntry>>> + Send;
    fn download(&self, name: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
    fn upload(
        &self,
        name: &str,
        data: &[u8],
        modified: DateTime<Utc>,
    ) -> impl Future<Output = Result<()>> + Send;
    fn delete(&self, name: &str) -> impl Future<Output = Result<()>> + Send;
}

/// Same identification gogdl uses for cloud storage; whether GOG requires it is unverified.
const GALAXY_USER_AGENT: &str = "GOGGalaxyCommunicationService/2.0.13.27 (Windows_32bit) dont_sync_marker/true installation_source/gog";

pub struct GogCloud {
    http: Client,
    base: Url,
    token: Secret,
}

impl GogCloud {
    pub fn new(http: Client, user_id: &str, client_id: &str, token: Secret) -> Result<Self> {
        let base = Url::parse(&format!(
            "https://cloudstorage.gog.com/v1/{user_id}/{client_id}"
        ))
        .map_err(|e| Error::parse("cloud storage URL", e))?;
        Ok(Self { http, base, token })
    }

    fn url(&self, name: &str) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("https URL has segments")
            .extend(name.split('/'));
        url
    }

    fn request(&self, method: reqwest::Method, url: Url) -> reqwest::RequestBuilder {
        self.http
            .request(method, url)
            .bearer_auth(self.token.expose())
            .header("User-Agent", GALAXY_USER_AGENT)
            .header("X-Object-Meta-User-Agent", GALAXY_USER_AGENT)
    }
}

#[derive(Deserialize)]
struct ListedFile {
    name: String,
    hash: String,
}

impl CloudTransport for GogCloud {
    async fn list(&self) -> Result<Vec<RemoteEntry>> {
        let req = self
            .request(reqwest::Method::GET, self.base.clone())
            .header("Accept", "application/json");
        match http::json::<Vec<ListedFile>>(req, "listing cloud saves").await {
            Ok(files) => Ok(files
                .into_iter()
                .map(|f| RemoteEntry {
                    name: f.name,
                    hash: f.hash,
                })
                .collect()),
            Err(Error::Http { status: 404, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    async fn download(&self, name: &str) -> Result<Vec<u8>> {
        let resp = http::send(
            self.request(reqwest::Method::GET, self.url(name)),
            "downloading a cloud save",
        )
        .await?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| Error::network("downloading a cloud save", e))?;
        Ok(bytes.to_vec())
    }

    async fn upload(&self, name: &str, data: &[u8], modified: DateTime<Utc>) -> Result<()> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::new(6));
        gz.write_all(data)
            .map_err(|e| Error::io("compressing a save", e))?;
        let compressed = gz
            .finish()
            .map_err(|e| Error::io("compressing a save", e))?;
        let req = self
            .request(reqwest::Method::PUT, self.url(name))
            .header(
                "X-Object-Meta-LocalLastModified",
                modified.to_rfc3339_opts(SecondsFormat::Secs, false),
            )
            .header("Etag", fsutil::hex(&Md5::digest(&compressed)))
            .header("Content-Encoding", "gzip")
            .body(compressed);
        http::send(req, "uploading a cloud save").await.map(drop)
    }

    async fn delete(&self, name: &str) -> Result<()> {
        http::send(
            self.request(reqwest::Method::DELETE, self.url(name)),
            "deleting a cloud save",
        )
        .await
        .map(drop)
    }
}

#[cfg(test)]
pub mod memory {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    type Files = BTreeMap<String, Vec<u8>>;
    type Mutation = Box<dyn FnOnce(&mut Files) + Send>;

    /// In-memory stand-in for GOG cloud storage, with fault injection.
    #[derive(Default)]
    pub struct MemoryCloud {
        pub files: Mutex<Files>,
        pub offline: AtomicBool,
        pub fail_downloads: AtomicBool,
        lists: AtomicUsize,
        after_list: Mutex<Vec<(usize, Mutation)>>,
        pub uploads: AtomicUsize,
        pub downloads: AtomicUsize,
    }

    pub fn hash(data: &[u8]) -> String {
        fsutil::hex(&Md5::digest(data))
    }

    impl MemoryCloud {
        pub fn put(&self, name: &str, data: &[u8]) {
            self.files
                .lock()
                .unwrap()
                .insert(name.into(), data.to_vec());
        }

        pub fn get(&self, name: &str) -> Option<Vec<u8>> {
            self.files.lock().unwrap().get(name).cloned()
        }

        pub fn list_count(&self) -> usize {
            self.lists.load(Ordering::SeqCst)
        }

        /// Applies `f` right after the `n`-th listing (1-based), simulating another machine.
        pub fn after_list(&self, n: usize, f: impl FnOnce(&mut Files) + Send + 'static) {
            self.after_list.lock().unwrap().push((n, Box::new(f)));
        }

        fn check_online(&self) -> Result<()> {
            if self.offline.load(Ordering::SeqCst) {
                Err(Error::Http {
                    context: "memory cloud (offline)",
                    status: 503,
                })
            } else {
                Ok(())
            }
        }
    }

    impl CloudTransport for MemoryCloud {
        async fn list(&self) -> Result<Vec<RemoteEntry>> {
            self.check_online()?;
            let n = self.lists.fetch_add(1, Ordering::SeqCst) + 1;
            let mut files = self.files.lock().unwrap();
            let listing = files
                .iter()
                .map(|(k, v)| RemoteEntry {
                    name: k.clone(),
                    hash: hash(v),
                })
                .collect();
            let mut pending = self.after_list.lock().unwrap();
            let (due, rest): (Vec<_>, Vec<_>) = pending.drain(..).partition(|(i, _)| *i == n);
            *pending = rest;
            for (_, f) in due {
                f(&mut files);
            }
            Ok(listing)
        }

        async fn download(&self, name: &str) -> Result<Vec<u8>> {
            self.check_online()?;
            if self.fail_downloads.load(Ordering::SeqCst) {
                return Err(Error::Http {
                    context: "memory cloud download",
                    status: 500,
                });
            }
            self.downloads.fetch_add(1, Ordering::SeqCst);
            self.get(name).ok_or(Error::Http {
                context: "memory cloud download",
                status: 404,
            })
        }

        async fn upload(&self, name: &str, data: &[u8], _: DateTime<Utc>) -> Result<()> {
            self.check_online()?;
            self.uploads.fetch_add(1, Ordering::SeqCst);
            self.put(name, data);
            Ok(())
        }

        async fn delete(&self, name: &str) -> Result<()> {
            self.check_online()?;
            self.files.lock().unwrap().remove(name);
            Ok(())
        }
    }
}
