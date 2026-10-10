//! Native Linux builds, from GOG's offline installers: a MojoSetup shell script followed by a zip
//! whose `data/noarch/` folder is the game. As in heroic-gogdl, only the game's files are read out
//! of the zip, each with its own ranged request: nothing else is downloaded and no copy of the
//! installer is kept.

use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use futures::StreamExt;
use futures::stream::BoxStream;
use reqwest::Client;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::zip::{self, ZipEntry};
use super::{Checked, Progress, RecordedFile, leads_inside, refuse_outside, safe_relative};
use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::http;
use crate::paths::Dirs;

/// Build ids of Linux installs start with this, followed by the installer's version: GOG offers
/// one version only, and an install or update job tells its platform by it.
pub const BUILD_PREFIX: &str = "linux:";
const GAME_DIR: &str = "data/noarch/";
const FILE_CONCURRENCY: usize = 8;
const DOWNLOAD_SUFFIX: &str = ".slatty-dl";
/// Downloaded bytes are inflated and written off the async threads in batches of this size.
const BATCH: usize = 1 << 20;

pub fn is_linux_build(build_id: &str) -> bool {
    build_id.starts_with(BUILD_PREFIX)
}

/// An installer GOG offers for a game or a DLC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installer {
    pub product_id: String,
    pub language: String,
    pub version: String,
    /// GOG's address that hands out a download link.
    pub downlink: String,
}

/// The Linux installers of a game and of its DLC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub title: String,
    pub installers: Vec<Installer>,
    /// Id, name and installers of each DLC that has a Linux installer.
    pub dlcs: Vec<(String, String, Vec<Installer>)>,
}

pub async fn offer(http: &Client, tokens: &Tokens, game_id: &str) -> Result<Offer> {
    let url = format!("https://api.gog.com/products/{game_id}?expand=downloads,expanded_dlcs");
    let raw = http::bytes(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "listing Linux installers",
    )
    .await?;
    parse_offer(&raw)
}

fn parse_offer(raw: &[u8]) -> Result<Offer> {
    #[derive(Deserialize)]
    struct Product {
        id: serde_json::Value,
        title: String,
        #[serde(default)]
        downloads: Downloads,
        #[serde(default)]
        expanded_dlcs: Vec<Product>,
    }
    #[derive(Deserialize, Default)]
    struct Downloads {
        #[serde(default)]
        installers: Vec<Raw>,
    }
    #[derive(Deserialize)]
    struct Raw {
        os: String,
        language: String,
        version: Option<String>,
        files: Vec<File>,
    }
    #[derive(Deserialize)]
    struct File {
        downlink: String,
    }
    let id = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let linux = |p: &Product| -> Result<Vec<Installer>> {
        p.downloads
            .installers
            .iter()
            .filter(|i| i.os == "linux")
            .map(|i| match i.files.as_slice() {
                [file] => Ok(Installer {
                    product_id: id(&p.id),
                    language: i.language.clone(),
                    version: i.version.clone().unwrap_or_default(),
                    downlink: file.downlink.clone(),
                }),
                _ => Err(Error::Unsupported(format!(
                    "the Linux installer of {} comes in several parts",
                    p.title
                ))),
            })
            .collect()
    };
    let product: Product =
        serde_json::from_slice(raw).map_err(|e| Error::parse("listing Linux installers", e))?;
    let mut dlcs = Vec::new();
    for d in &product.expanded_dlcs {
        let installers = linux(d)?;
        if !installers.is_empty() {
            dlcs.push((id(&d.id), d.title.clone(), installers));
        }
    }
    Ok(Offer {
        title: product.title.clone(),
        installers: linux(&product)?,
        dlcs,
    })
}

/// The installer in `language`, else in English, else the first one.
pub fn pick<'a>(installers: &'a [Installer], language: &str) -> Option<&'a Installer> {
    installers
        .iter()
        .find(|i| i.language.eq_ignore_ascii_case(language))
        .or_else(|| installers.iter().find(|i| i.language == "en"))
        .or(installers.first())
}

/// An installer and the game files in its zip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub installer: Installer,
    pub entries: Vec<ZipEntry>,
}

impl Part {
    pub fn download_size(&self) -> u64 {
        self.entries.iter().map(|e| e.compressed).sum()
    }

    pub fn disk_size(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }
}

/// Reads parts of installers.
pub trait Source: Send + Sync {
    /// Size of installer `part`.
    fn size(&self, part: usize) -> impl Future<Output = Result<u64>> + Send;
    /// Bytes `start..end` of installer `part`, as they arrive.
    fn open(
        &self,
        part: usize,
        start: u64,
        end: u64,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<Vec<u8>>>>> + Send;

    fn read(
        &self,
        part: usize,
        start: u64,
        end: u64,
    ) -> impl Future<Output = Result<Vec<u8>>> + Send {
        async move {
            let mut body = self.open(part, start, end).await?;
            let mut out = Vec::with_capacity((end - start) as usize);
            while let Some(bytes) = body.next().await {
                out.extend(bytes?);
            }
            Ok(out)
        }
    }
}

/// The game files of installer `part`: the entries under `data/noarch/`, read from its directory.
pub async fn read_entries<S: Source>(source: &S, part: usize) -> Result<Vec<ZipEntry>> {
    let size = source.size(part).await?;
    let start = size.saturating_sub(zip::TAIL);
    let tail = source.read(part, start, size).await?;
    let dir = zip::directory(&tail, size)?;
    let raw = source.read(part, dir.start, dir.start + dir.len).await?;
    Ok(zip::entries(&dir, &raw)?
        .into_iter()
        .filter(|e| e.name.starts_with(GAME_DIR) && e.name.len() > GAME_DIR.len())
        .collect())
}

/// GOG's installers, through download links asked for when first needed and again when refused.
pub struct GogInstallers {
    http: Client,
    tokens: tokio::sync::Mutex<Tokens>,
    /// To renew the session during a long download.
    dirs: Option<Dirs>,
    downlinks: Vec<String>,
    links: tokio::sync::Mutex<HashMap<usize, (String, u64)>>,
}

impl GogInstallers {
    pub fn new(http: Client, tokens: Tokens, dirs: Option<&Dirs>, downlinks: Vec<String>) -> Self {
        Self {
            http,
            tokens: tokio::sync::Mutex::new(tokens),
            dirs: dirs.cloned(),
            downlinks,
            links: Default::default(),
        }
    }

    /// The download link of `part` and the installer's size.
    async fn link(&self, part: usize, renew: bool) -> Result<(String, u64)> {
        if !renew && let Some(link) = self.links.lock().await.get(&part) {
            return Ok(link.clone());
        }
        let tokens = {
            let mut tokens = self.tokens.lock().await;
            if let Some(dirs) = &self.dirs
                && tokens.is_expired()
            {
                *tokens = crate::account::fresh(&self.http, dirs, &tokens, false).await?;
            }
            tokens.clone()
        };
        #[derive(Deserialize)]
        struct Downlink {
            downlink: String,
        }
        let api = self
            .downlinks
            .get(part)
            .ok_or_else(|| Error::NotFound(format!("installer {part}")))?;
        let link: Downlink = http::json(
            self.http.get(api).bearer_auth(tokens.access_token.expose()),
            "asking for a download link",
        )
        .await?;
        // The API's size is rounded; the CDN tells the exact one.
        let first = self.range(&link.downlink, 0, 1).await?;
        let size = first
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| Error::Parse {
                context: "asking for a download link",
                detail: "no installer size".into(),
            })?;
        let link = (link.downlink, size);
        self.links.lock().await.insert(part, link.clone());
        Ok(link)
    }

    async fn range(&self, url: &str, start: u64, end: u64) -> Result<reqwest::Response> {
        let resp = http::send(
            self.http
                .get(url)
                .header(reqwest::header::RANGE, format!("bytes={start}-{}", end - 1)),
            "downloading from a Linux installer",
        )
        .await?;
        // A server that ignores the range would send the whole installer.
        if resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(Error::Unsupported(
                "the download server does not serve parts of files".into(),
            ));
        }
        Ok(resp)
    }
}

impl Source for GogInstallers {
    async fn size(&self, part: usize) -> Result<u64> {
        Ok(self.link(part, false).await?.1)
    }

    async fn open(
        &self,
        part: usize,
        start: u64,
        end: u64,
    ) -> Result<BoxStream<'static, Result<Vec<u8>>>> {
        let (url, _) = self.link(part, false).await?;
        let resp = match self.range(&url, start, end).await {
            // An expired link.
            Err(Error::Http {
                status: 401 | 403 | 410,
                ..
            }) => {
                let (url, _) = self.link(part, true).await?;
                self.range(&url, start, end).await?
            }
            other => other?,
        };
        Ok(futures::stream::unfold(resp, |mut resp| async move {
            match resp.chunk().await {
                Ok(Some(bytes)) => Some((Ok(bytes.to_vec()), resp)),
                Ok(None) => None,
                Err(e) => Some((
                    Err(Error::network("downloading from a Linux installer", e)),
                    resp,
                )),
            }
        })
        .boxed())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxFile {
    pub path: PathBuf,
    pub entry: ZipEntry,
    /// Index of the installer it comes from.
    pub part: usize,
}

/// What the parts put in the game folder; a DLC's file replaces the game's at the same path.
#[derive(Debug, Default)]
pub struct LinuxSet {
    pub files: Vec<LinuxFile>,
    pub links: Vec<LinuxFile>,
    pub dirs: Vec<PathBuf>,
}

impl LinuxSet {
    pub fn new(parts: &[Part]) -> Result<Self> {
        let mut set = LinuxSet::default();
        let mut index: HashMap<PathBuf, (bool, usize)> = HashMap::new();
        for (part, p) in parts.iter().enumerate() {
            for e in &p.entries {
                let path = safe_relative(&e.name[GAME_DIR.len()..])?;
                if e.is_dir() {
                    set.dirs.push(path);
                    continue;
                }
                let file = LinuxFile {
                    path: path.clone(),
                    entry: e.clone(),
                    part,
                };
                let link = e.is_symlink();
                match index.get(&path) {
                    Some(&(true, i)) => set.links[i] = file,
                    Some(&(false, i)) => set.files[i] = file,
                    None => {
                        let list = if link { &mut set.links } else { &mut set.files };
                        index.insert(path, (link, list.len()));
                        list.push(file);
                    }
                }
            }
        }
        Ok(set)
    }

    pub fn disk_size(&self) -> u64 {
        self.files.iter().map(|f| f.entry.size).sum()
    }

    pub fn recorded_files(&self) -> Vec<RecordedFile> {
        self.files
            .iter()
            .chain(&self.links)
            .map(|f| RecordedFile {
                path: f.path.to_string_lossy().into_owned(),
                size: f.entry.size,
            })
            .collect()
    }
}

pub struct LinuxDownload<'a, S> {
    pub source: &'a S,
    pub cancel: CancellationToken,
    pub progress: &'a (dyn Fn(Progress) + Send + Sync),
    pub free_space: &'a (dyn Fn(&Path) -> Result<u64> + Send + Sync),
}

impl<S: Source> LinuxDownload<'_, S> {
    /// Fills `partial` (keeping verified files from an earlier attempt), then renames it to
    /// `target`. Nothing exists at `target` unless every file was verified. Returns the links
    /// that were left out because they pointed outside the game.
    pub async fn run(&self, set: &LinuxSet, partial: &Path, target: &Path) -> Result<usize> {
        if target.exists() {
            return Err(Error::Refused(format!(
                "{} already exists",
                target.display()
            )));
        }
        crate::paths::ensure_dir(partial)?;
        self.check_space(set, partial)?;
        let (_, skipped) = self.fill(set, partial, true).await?;
        std::fs::rename(partial, target).map_err(|e| {
            Error::io(
                format!("publish {} as {}", partial.display(), target.display()),
                e,
            )
        })?;
        fsutil::sync_filesystem(target)?;
        Ok(skipped)
    }

    /// Checks an installed game in place; with `repair`, downloads only the bad files again.
    pub async fn check_installed(
        &self,
        set: &LinuxSet,
        dir: &Path,
        repair: bool,
    ) -> Result<Checked> {
        if repair {
            self.check_space(set, dir)?;
        }
        Ok(self.fill(set, dir, repair).await?.0)
    }

    fn check_space(&self, set: &LinuxSet, dir: &Path) -> Result<()> {
        let present: u64 = set
            .files
            .iter()
            .filter_map(|f| {
                std::fs::metadata(dir.join(&f.path))
                    .ok()
                    .filter(|m| m.len() == f.entry.size)
            })
            .map(|m| m.len())
            .sum();
        let needed = set.disk_size().saturating_sub(present);
        let free = (self.free_space)(dir)?;
        if needed + needed / 50 > free {
            return Err(Error::Refused(format!(
                "not enough disk space: {} MiB needed, {} MiB free",
                needed >> 20,
                free >> 20
            )));
        }
        Ok(())
    }

    async fn fill(&self, set: &LinuxSet, dir: &Path, download: bool) -> Result<(Checked, usize)> {
        let bad = std::sync::Mutex::new(Vec::new());
        let total = Progress {
            files_total: set.files.len(),
            bytes_total: set.disk_size(),
            ..Default::default()
        };
        let files_done = AtomicUsize::new(0);
        let bytes_done = AtomicU64::new(0);
        let report = |files: usize, bytes: u64| {
            (self.progress)(Progress {
                files_done: files_done.fetch_add(files, Ordering::Relaxed) + files,
                bytes_done: bytes_done.fetch_add(bytes, Ordering::Relaxed) + bytes,
                ..total
            })
        };
        report(0, 0);
        let first_error: std::sync::Mutex<Option<Error>> = std::sync::Mutex::new(None);
        futures::stream::iter(0..set.files.len())
            .for_each_concurrent(FILE_CONCURRENCY, |i| {
                let file = &set.files[i];
                let dest = dir.join(&file.path);
                let (bad, first_error) = (&bad, &first_error);
                async move {
                    if self.cancel.is_cancelled() {
                        first_error.lock().unwrap().get_or_insert(Error::Cancelled);
                        return;
                    }
                    let result = async {
                        let verified = tokio::task::spawn_blocking({
                            let (dest, entry) = (dest.clone(), file.entry.clone());
                            move || verify_file(&dest, &entry)
                        })
                        .await
                        .expect("verify task panicked")?;
                        if verified {
                            report(1, file.entry.size);
                            return Ok(());
                        }
                        bad.lock().unwrap().push(file.path.clone());
                        if download {
                            refuse_outside(dir, &dest)?;
                            self.download_file(&dest, file, &|n| report(0, n)).await?;
                        }
                        report(1, if download { 0 } else { file.entry.size });
                        Ok::<(), Error>(())
                    }
                    .await;
                    if let Err(e) = result {
                        first_error.lock().unwrap().get_or_insert(e);
                    }
                }
            })
            .await;
        if let Some(e) = first_error.into_inner().unwrap() {
            return Err(e);
        }
        let mut skipped = 0;
        for link in &set.links {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match self.place_link(dir, link, download).await? {
                LinkState::Right => {}
                LinkState::Wrong => bad.lock().unwrap().push(link.path.clone()),
                LinkState::Outside => skipped += 1,
            }
        }
        // Checked again once all are made: a link can lead out through another one, whatever its
        // own path says.
        for link in &set.links {
            let dest = dir.join(&link.path);
            let Ok(target) = std::fs::read_link(&dest) else {
                continue;
            };
            if !leads_inside(dir, &dest.parent().unwrap_or(dir).join(&target)) {
                if download {
                    std::fs::remove_file(&dest)
                        .map_err(|e| Error::io(format!("delete {}", dest.display()), e))?;
                }
                bad.lock().unwrap().retain(|p| *p != link.path);
                skipped += 1;
            }
        }
        let mut bad = bad.into_inner().unwrap();
        if download {
            for d in &set.dirs {
                crate::paths::ensure_dir(&dir.join(d))?;
            }
            if !bad.is_empty() {
                fsutil::sync_filesystem(dir)?;
            }
        }
        bad.sort();
        Ok((
            Checked {
                bad,
                reused_bytes: 0,
            },
            skipped,
        ))
    }

    /// Checks a symbolic link and, with `download`, makes it right. A link out of the game folder
    /// is never made.
    async fn place_link(&self, dir: &Path, link: &LinuxFile, download: bool) -> Result<LinkState> {
        let raw = self.read_data(link).await?;
        let target = PathBuf::from(String::from_utf8_lossy(&raw).into_owned());
        let dest = dir.join(&link.path);
        if !stays_inside(&link.path, &target) {
            // Nor is an older copy of it kept.
            if download && std::fs::read_link(&dest).is_ok() {
                std::fs::remove_file(&dest)
                    .map_err(|e| Error::io(format!("delete {}", dest.display()), e))?;
            }
            return Ok(LinkState::Outside);
        }
        if std::fs::read_link(&dest).is_ok_and(|t| t == target) {
            return Ok(LinkState::Right);
        }
        if download {
            if let Some(parent) = dest.parent() {
                crate::paths::ensure_dir(parent)?;
            }
            if std::fs::symlink_metadata(&dest).is_ok_and(|m| !m.is_dir()) {
                std::fs::remove_file(&dest)
                    .map_err(|e| Error::io(format!("replace {}", dest.display()), e))?;
            }
            std::os::unix::fs::symlink(&target, &dest)
                .map_err(|e| Error::io(format!("link {}", dest.display()), e))?;
        }
        Ok(LinkState::Wrong)
    }

    /// Where the local header and data of `file` end at most in its installer.
    async fn end_of(&self, file: &LinuxFile) -> Result<u64> {
        let e = &file.entry;
        Ok((e.header + e.span()).min(self.source.size(file.part).await?))
    }

    /// The whole content of a small entry.
    async fn read_data(&self, file: &LinuxFile) -> Result<Vec<u8>> {
        let e = &file.entry;
        let raw = self
            .source
            .read(file.part, e.header, self.end_of(file).await?)
            .await?;
        let start = zip::local_header_len(&raw)?;
        let data = raw
            .get(start..start + e.compressed as usize)
            .ok_or_else(|| corrupted(&file.path))?;
        let mut out = Vec::new();
        if e.method == zip::DEFLATED {
            flate2::read::DeflateDecoder::new(data)
                .read_to_end(&mut out)
                .map_err(|_| corrupted(&file.path))?;
        } else {
            out = data.to_vec();
        }
        Ok(out)
    }

    /// Writes `file` at `dest` atomically, trying again when the connection fails on the way.
    async fn download_file(
        &self,
        dest: &Path,
        file: &LinuxFile,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> Result<()> {
        if let Some(parent) = dest.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let tmp = dest.with_file_name(format!(
            "{}{DOWNLOAD_SUFFIX}",
            dest.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ));
        // Bytes already counted, so a second attempt does not count them again.
        let counted = AtomicU64::new(0);
        let count = |written: u64| {
            let before = counted.fetch_max(written, Ordering::Relaxed);
            if written > before {
                on_bytes(written - before);
            }
        };
        let mut attempt = 0;
        loop {
            match self.write_file(&tmp, file, &count).await {
                Ok(()) => break,
                Err(Error::Cancelled) => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(Error::Cancelled);
                }
                Err(e) if attempt < 3 && retry(&e) => {
                    attempt += 1;
                    tracing::debug!("{}: {e}; trying again", file.path.display());
                    tokio::time::sleep(std::time::Duration::from_millis(500 * attempt)).await;
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e);
                }
            }
        }
        let mode = if file.entry.mode.unwrap_or(0) & 0o111 != 0 {
            0o755
        } else {
            0o644
        };
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode));
        std::fs::rename(&tmp, dest).map_err(|e| Error::io(format!("rename {}", tmp.display()), e))
    }

    async fn write_file(
        &self,
        tmp: &Path,
        file: &LinuxFile,
        count: &(dyn Fn(u64) + Sync),
    ) -> Result<()> {
        let e = &file.entry;
        let out = std::fs::File::create(tmp)
            .map_err(|err| Error::io(format!("create {}", tmp.display()), err))?;
        let sink = Sink {
            out: std::io::BufWriter::with_capacity(BATCH, out),
            crc: flate2::Crc::new(),
            written: 0,
        };
        let mut inflate = Some(if e.method == zip::DEFLATED {
            Inflate::Deflated(flate2::write::DeflateDecoder::new(sink))
        } else if e.method == zip::STORED {
            Inflate::Stored(sink)
        } else {
            return Err(Error::Unsupported(format!(
                "{} is compressed with method {}",
                file.path.display(),
                e.method
            )));
        });
        let end = self.end_of(file).await?;
        // An empty file is stored with no data to fetch.
        let mut body = if e.compressed > 0 {
            Some(self.source.open(file.part, e.header, end).await?)
        } else {
            None
        };
        let mut head: Vec<u8> = Vec::new();
        let mut header_len = None;
        let mut left = e.compressed;
        let mut batch = Vec::with_capacity(BATCH);
        while left > 0 {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let Some(bytes) = body
                .as_mut()
                .expect("opened when data is left")
                .next()
                .await
            else {
                return Err(closed());
            };
            let mut bytes = &bytes?[..];
            if header_len.is_none() {
                head.extend_from_slice(bytes);
                if head.len() < 30 {
                    continue;
                }
                let len = zip::local_header_len(&head)?;
                if head.len() < len {
                    continue;
                }
                header_len = Some(len);
                batch.extend_from_slice(&head[len..]);
                bytes = &[];
            }
            batch.extend_from_slice(bytes);
            if batch.len() as u64 > left {
                batch.truncate(left as usize);
            }
            if batch.len() >= BATCH || batch.len() as u64 == left {
                left -= batch.len() as u64;
                let mut i = inflate.take().expect("inflater put back");
                let data = std::mem::take(&mut batch);
                let (back, written) = tokio::task::spawn_blocking(move || {
                    let r = i.write_all(&data).map(|_| i.written());
                    (i, r)
                })
                .await
                .expect("inflate task panicked");
                inflate = Some(back);
                count(written.map_err(|_| corrupted(&file.path))?);
                batch = Vec::with_capacity(BATCH);
            }
        }
        drop(body);
        let sink =
            tokio::task::spawn_blocking(move || inflate.expect("inflater put back").finish())
                .await
                .expect("inflate task panicked")
                .map_err(|_| corrupted(&file.path))?;
        if sink.written != e.size || sink.crc.sum() != e.crc {
            return Err(corrupted(&file.path));
        }
        count(sink.written);
        Ok(())
    }
}

enum LinkState {
    Right,
    Wrong,
    Outside,
}

/// Whether a link at `path` (relative to the game folder) pointing to `target` stays inside it.
fn stays_inside(path: &Path, target: &Path) -> bool {
    let mut depth = path.components().count() as i64 - 1;
    for c in target.components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// The server stopped sending before the end of the range.
fn closed() -> Error {
    Error::io(
        "downloading from a Linux installer",
        std::io::ErrorKind::UnexpectedEof.into(),
    )
}

/// Worth another try: the connection, not the data.
fn retry(e: &Error) -> bool {
    match e {
        Error::Network { .. } => true,
        Error::Io { source, .. } => source.kind() == std::io::ErrorKind::UnexpectedEof,
        Error::Http { status, .. } => *status == 429 || *status >= 500,
        _ => false,
    }
}

fn corrupted(path: &Path) -> Error {
    Error::Refused(format!(
        "{} is corrupted in the installer download",
        path.display()
    ))
}

/// True when `path` already holds exactly this file.
pub fn verify_file(path: &Path, entry: &ZipEntry) -> Result<bool> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != entry.size {
        return Ok(false);
    }
    let mut f =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    let mut crc = flate2::Crc::new();
    let mut buf = vec![0; BATCH];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
    }
    Ok(crc.sum() == entry.crc)
}

/// Where inflated bytes go: the file, and their checksum.
struct Sink {
    out: std::io::BufWriter<std::fs::File>,
    crc: flate2::Crc,
    written: u64,
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.out.write_all(buf)?;
        self.crc.update(buf);
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.out.flush()
    }
}

enum Inflate {
    Stored(Sink),
    Deflated(flate2::write::DeflateDecoder<Sink>),
}

impl Inflate {
    fn write_all(&mut self, data: &[u8]) -> std::io::Result<()> {
        match self {
            Inflate::Stored(s) => s.write_all(data),
            Inflate::Deflated(d) => d.write_all(data),
        }
    }

    fn written(&self) -> u64 {
        match self {
            Inflate::Stored(s) => s.written,
            Inflate::Deflated(d) => d.get_ref().written,
        }
    }

    fn finish(self) -> std::io::Result<Sink> {
        let mut sink = match self {
            Inflate::Stored(s) => s,
            Inflate::Deflated(d) => d.finish()?,
        };
        sink.flush()?;
        Ok(sink)
    }
}

#[cfg(test)]
mod tests;
