//! Proton builds SlattyLauncher downloads and keeps itself, in `<data>/protons/<name>`, from the
//! GitHub releases of their projects. Each archive is checked against the SHA-512 sum its release
//! publishes before it is unpacked, and nothing in it is written out of its folder.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha512};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::Install;
use crate::installer::{leads_inside, linux::stays_inside};
use crate::paths::Dirs;
use crate::runner::Runner;

/// GitHub's API, where the releases are listed.
pub const API: &str = "https://api.github.com";

/// A project publishing Proton builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    GeProton,
    ProtonCachyOs,
    UmuProton,
}

impl Source {
    pub const ALL: [Source; 3] = [Source::GeProton, Source::ProtonCachyOs, Source::UmuProton];

    pub fn name(self) -> &'static str {
        match self {
            Source::GeProton => "GE-Proton",
            Source::ProtonCachyOs => "Proton-CachyOS",
            Source::UmuProton => "UMU-Proton",
        }
    }

    /// The project a downloaded build comes from, by the name of its folder.
    fn of(name: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|s| {
            let prefix = match s {
                Source::GeProton => "GE-Proton",
                Source::ProtonCachyOs => "proton-cachyos-",
                Source::UmuProton => "UMU-Proton-",
            };
            name.starts_with(prefix)
        })
    }

    fn repo(self) -> &'static str {
        match self {
            Source::GeProton => "GloriousEggroll/proton-ge-custom",
            Source::ProtonCachyOs => "CachyOS/proton-cachyos",
            Source::UmuProton => "Open-Wine-Components/umu-proton",
        }
    }

    /// The archive for an x86_64 computer among a release's files: CachyOS's x86-64-v3 build
    /// when the processor has those instructions (AVX2 and others).
    fn pick<'a>(self, names: &[&'a str], v3: bool) -> Option<&'a str> {
        let archives: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| archive_stem(n).is_some())
            .filter(|n| !n.contains("aarch64") && !n.contains("arm64"))
            .collect();
        let ends = |suffix: &str| archives.iter().copied().find(|n| n.ends_with(suffix));
        match self {
            Source::ProtonCachyOs if v3 => {
                ends("-x86_64_v3.tar.xz").or_else(|| ends("-x86_64.tar.xz"))
            }
            Source::ProtonCachyOs => ends("-x86_64.tar.xz"),
            Source::GeProton | Source::UmuProton => ends("-x86_64.tar.gz")
                .or_else(|| archives.iter().copied().find(|n| !n.contains("x86_64"))),
        }
    }
}

/// The newest build of a source, ready to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub source: Source,
    pub version: String,
    /// The folder it is unpacked in: the archive's name without its extension.
    pub name: String,
    pub size: u64,
    pub url: String,
    pub archive: String,
    pub checksum_url: String,
}

impl Release {
    pub fn path(&self, dirs: &Dirs) -> PathBuf {
        dir(dirs).join(&self.name)
    }
}

/// Where the downloaded builds are kept.
pub fn dir(dirs: &Dirs) -> PathBuf {
    dirs.data.join("protons")
}

/// The downloaded builds, by name, without the `-latest` links to them.
pub fn installed(dirs: &Dirs) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir(dirs))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        })
        .filter(|p| !p.is_symlink() && p.join("proton").is_file())
        .collect();
    found.sort();
    found
}

/// A link to the newest downloaded build of `source`: a game or the default set to it runs the
/// newest build, and an update changes it at once.
pub fn latest_link(dirs: &Dirs, source: Source) -> PathBuf {
    dir(dirs).join(format!("{}-latest", source.name()))
}

/// Where the `-latest` link of `source` leads, if it exists.
fn latest_target(dirs: &Dirs, source: Source) -> Option<PathBuf> {
    std::fs::read_link(latest_link(dirs, source))
        .ok()
        .map(|t| dir(dirs).join(t))
}

/// Points the `-latest` link of `source` at the build named `name`, replacing the old link in one
/// step.
fn point_latest(dirs: &Dirs, source: Source, name: &str) -> Result<()> {
    let link = latest_link(dirs, source);
    let temp = crate::fsutil::temp_sibling(&link);
    std::os::unix::fs::symlink(name, &temp)
        .map_err(|e| Error::io(format!("link {}", link.display()), e))?;
    std::fs::rename(&temp, &link).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        Error::io(format!("link {}", link.display()), e)
    })
}

/// Gives each project that has downloaded builds but no `-latest` link one, to the build unpacked
/// last (builds downloaded before the links existed).
pub fn link_newest(dirs: &Dirs) -> Result<()> {
    for source in Source::ALL {
        if std::fs::symlink_metadata(latest_link(dirs, source)).is_ok() {
            continue;
        }
        let newest = installed(dirs)
            .into_iter()
            .filter(|b| {
                Source::of(&b.file_name().unwrap_or_default().to_string_lossy()) == Some(source)
            })
            .max_by_key(|b| std::fs::metadata(b).and_then(|m| m.modified()).ok());
        if let Some(build) = newest {
            point_latest(
                dirs,
                source,
                &build.file_name().unwrap_or_default().to_string_lossy(),
            )?;
        }
    }
    Ok(())
}

/// The Proton builds games and the default are set to.
fn used(db: &Db) -> Result<Vec<PathBuf>> {
    let mut used: Vec<PathBuf> = Install::list(db)?
        .into_iter()
        .filter_map(|i| match i.runner {
            Runner::Umu { proton, .. } => Some(proton),
            _ => None,
        })
        .collect();
    used.extend(crate::settings::default_proton(db)?);
    Ok(used)
}

/// The projects a game or the default follows through their `-latest` link.
pub fn followed(db: &Db, dirs: &Dirs) -> Result<Vec<Source>> {
    let used = used(db)?;
    Ok(Source::ALL
        .into_iter()
        .filter(|s| used.contains(&latest_link(dirs, *s)))
        .collect())
}

/// Whether the followed builds are due for a check: both switches on, and a day passed since the
/// last one.
pub fn update_due(db: &Db, now: i64) -> Result<bool> {
    Ok(crate::settings::proton_downloads(db)?
        && crate::settings::proton_updates(db)?
        && crate::settings::proton_checked_at(db)?.is_none_or(|t| now - t >= 24 * 60 * 60))
}

/// Downloads the newest build of every followed project and points its `-latest` link at it. The
/// build it led to before is kept, to go back to; older ones no game uses are deleted. Returns the
/// builds installed.
pub async fn update(
    db: &Db,
    http: &Client,
    dirs: &Dirs,
    api: &str,
    progress: impl Fn(&str, Stage) + Send + Sync + 'static,
    cancel: &CancellationToken,
) -> Result<Vec<Release>> {
    check_allowed(db)?;
    let progress = std::sync::Arc::new(progress);
    let mut updated = Vec::new();
    for source in followed(db, dirs)? {
        let release = latest(http, api, source).await?;
        let previous = latest_target(dirs, source);
        if previous.as_ref() == Some(&release.path(dirs)) {
            continue;
        }
        let (report, name) = (progress.clone(), release.name.clone());
        install(http, dirs, &release, move |s| report(&name, s), cancel).await?;
        prune(db, dirs, source, &release.path(dirs), previous.as_deref())?;
        updated.push(release);
    }
    crate::settings::set_proton_checked_at(db, chrono::Utc::now().timestamp())?;
    Ok(updated)
}

/// Deletes the builds of `source` other than `newest` and `previous` that no game uses.
fn prune(
    db: &Db,
    dirs: &Dirs,
    source: Source,
    newest: &Path,
    previous: Option<&Path>,
) -> Result<()> {
    let _lock = lock(dirs)?;
    let used = used(db)?;
    for build in installed(dirs) {
        let name = build.file_name().unwrap_or_default().to_string_lossy();
        if Source::of(&name) == Some(source)
            && build != newest
            && Some(build.as_path()) != previous
            && !used.contains(&build)
        {
            std::fs::remove_dir_all(&build)
                .map_err(|e| Error::io(format!("delete {}", build.display()), e))?;
        }
    }
    Ok(())
}

/// Refuses unless Proton downloads are turned on: GitHub is contacted only then.
pub fn check_allowed(db: &Db) -> Result<()> {
    if crate::settings::proton_downloads(db)? {
        Ok(())
    } else {
        Err(Error::Refused(
            "Proton downloads from GitHub are off; turn them on in Settings → Runners".into(),
        ))
    }
}

/// The newest release of `source`, asked of GitHub's API at `api`.
pub async fn latest(http: &Client, api: &str, source: Source) -> Result<Release> {
    #[derive(Deserialize)]
    struct Raw {
        tag_name: String,
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        size: u64,
        browser_download_url: String,
    }
    const CONTEXT: &str = "listing Proton releases";
    let url = format!("{api}/repos/{}/releases/latest", source.repo());
    let req = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    let raw: Raw = match crate::http::json(req, CONTEXT).await {
        // GitHub answers 60 requests an hour to an address without an account: one shared with
        // others (a VPN) can run out.
        Err(Error::Http {
            status: 403 | 429, ..
        }) => {
            return Err(Error::Refused(
                "GitHub refuses more requests from this address for now; try again later".into(),
            ));
        }
        other => other?,
    };
    let names: Vec<&str> = raw.assets.iter().map(|a| a.name.as_str()).collect();
    let none = || {
        Error::NotFound(format!(
            "{} {} has no build for this computer",
            source.name(),
            raw.tag_name
        ))
    };
    let archive = source.pick(&names, x86_64_v3()).ok_or_else(none)?;
    let stem = archive_stem(archive).ok_or_else(none)?;
    let asset = |name: &str| raw.assets.iter().find(|a| a.name == name);
    let file = asset(archive).ok_or_else(none)?;
    let checksum = asset(&format!("{stem}.sha512sum")).ok_or_else(|| {
        Error::Refused(format!(
            "{archive} comes without a SHA-512 sum to check it against"
        ))
    })?;
    if !is_plain_name(stem) {
        return Err(Error::Refused(format!(
            "unexpected archive name `{archive}`"
        )));
    }
    Ok(Release {
        source,
        version: raw.tag_name.clone(),
        name: stem.to_string(),
        size: file.size,
        url: file.browser_download_url.clone(),
        archive: archive.to_string(),
        checksum_url: checksum.browser_download_url.clone(),
    })
}

/// Where a download stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Downloading { done: u64, total: u64 },
    Verifying,
    Unpacking { done: u64, total: u64 },
}

/// Downloads `release`, checks it against its SHA-512 sum and unpacks it, unless it is there
/// already, then points its project's `-latest` link at it. A download stopped half way goes on
/// from where it was.
pub async fn install(
    http: &Client,
    dirs: &Dirs,
    release: &Release,
    progress: impl Fn(Stage) + Send + Sync + 'static,
    cancel: &CancellationToken,
) -> Result<PathBuf> {
    let dest = release.path(dirs);
    if !dest.join("proton").is_file() {
        unpack_release(http, dirs, release, progress, cancel).await?;
    }
    point_latest(dirs, release.source, &release.name)?;
    Ok(dest)
}

async fn unpack_release(
    http: &Client,
    dirs: &Dirs,
    release: &Release,
    progress: impl Fn(Stage) + Send + Sync + 'static,
    cancel: &CancellationToken,
) -> Result<()> {
    let dest = release.path(dirs);
    let _lock = lock(dirs)?;
    let staging = dir(dirs).join(".staging");
    crate::paths::ensure_dir(&staging)?;
    let archive = staging.join(&release.archive);

    let raw =
        crate::http::bytes(http.get(&release.checksum_url), "downloading a Proton sum").await?;
    let expected = parse_sum(&raw, &release.archive)?;
    download(http, release, &archive, &progress, cancel).await?;

    progress(Stage::Verifying);
    let path = archive.clone();
    let sum = tokio::task::spawn_blocking(move || sha512_file(&path))
        .await
        .map_err(|e| Error::io("checking a Proton build", std::io::Error::other(e)))??;
    if sum != expected {
        let _ = std::fs::remove_file(&archive);
        return Err(Error::Refused(format!(
            "{} does not match the SHA-512 sum its release publishes; it was deleted",
            release.archive
        )));
    }

    let unpacked = staging.join(&release.name);
    let (from, to, cancel) = (archive.clone(), unpacked.clone(), cancel.clone());
    let progress = std::sync::Arc::new(progress);
    let report = progress.clone();
    tokio::task::spawn_blocking(move || {
        unpack(&from, &to, &cancel, &|done, total| {
            report(Stage::Unpacking { done, total })
        })
    })
    .await
    .map_err(|e| Error::io("unpacking a Proton build", std::io::Error::other(e)))??;
    if !unpacked.join("proton").is_file() {
        let _ = std::fs::remove_dir_all(&unpacked);
        return Err(Error::Refused(format!(
            "{} holds no `proton`",
            release.archive
        )));
    }
    std::fs::rename(&unpacked, &dest)
        .map_err(|e| Error::io(format!("move {}", dest.display()), e))?;
    let _ = std::fs::remove_file(&archive);
    Ok(())
}

/// Deletes a downloaded build, unless a game or the default Proton uses it.
pub fn remove(db: &Db, dirs: &Dirs, path: &Path) -> Result<()> {
    let root = dir(dirs);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    if path.parent() != Some(root.as_path())
        || !name.as_deref().is_some_and(is_plain_name)
        || path.is_symlink()
    {
        return Err(Error::Refused(format!(
            "{} is not a Proton build SlattyLauncher downloaded",
            path.display()
        )));
    }
    let _lock = lock(dirs)?;
    let newest_of = Source::ALL
        .into_iter()
        .find(|s| latest_target(dirs, *s).as_deref() == Some(path));
    if let Some(source) = newest_of
        && followed(db, dirs)?.contains(&source)
    {
        return Err(Error::Refused(format!(
            "{} is the newest {} build, which games set to {}-latest run; it goes once a newer \
             one is downloaded",
            name.unwrap_or_default(),
            source.name(),
            source.name()
        )));
    }
    let users: Vec<String> = Install::list(db)?
        .into_iter()
        .filter(|i| matches!(&i.runner, Runner::Umu { proton, .. } if proton == path))
        .map(|i| i.title)
        .collect();
    if !users.is_empty() {
        return Err(Error::Refused(format!(
            "{} runs {}; choose another Proton for it first",
            name.unwrap_or_default(),
            users.join(", ")
        )));
    }
    if crate::settings::default_proton(db)?.as_deref() == Some(path) {
        return Err(Error::Refused(format!(
            "{} is the default Proton; choose another one first",
            name.unwrap_or_default()
        )));
    }
    std::fs::remove_dir_all(path)
        .map_err(|e| Error::io(format!("delete {}", path.display()), e))?;
    // Nothing follows the link: it goes with the build it led to.
    if let Some(source) = newest_of {
        let link = latest_link(dirs, source);
        std::fs::remove_file(&link)
            .map_err(|e| Error::io(format!("delete {}", link.display()), e))?;
    }
    Ok(())
}

fn lock(dirs: &Dirs) -> Result<crate::lock::FileLock> {
    crate::lock::try_acquire(&dirs.locks().join("protons.lock"))?
        .ok_or_else(|| Error::Refused("another Proton download or removal is running".into()))
}

async fn download(
    http: &Client,
    release: &Release,
    archive: &Path,
    progress: &(impl Fn(Stage) + Send + Sync),
    cancel: &CancellationToken,
) -> Result<()> {
    const CONTEXT: &str = "downloading a Proton build";
    let total = release.size;
    let mut done = std::fs::metadata(archive).map(|m| m.len()).unwrap_or(0);
    if done > total {
        done = 0;
    }
    if done == total {
        return Ok(());
    }
    let mut req = http.get(&release.url);
    if done > 0 {
        req = req.header("Range", format!("bytes={done}-"));
    }
    let mut resp = crate::http::send(req, CONTEXT).await?;
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resumed {
        done = 0;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(archive)
        .await
        .map_err(|e| Error::io(format!("create {}", archive.display()), e))?;
    progress(Stage::Downloading { done, total });
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            chunk = resp.chunk() => chunk.map_err(|e| Error::network(CONTEXT, e))?,
        };
        let Some(chunk) = chunk else { break };
        done += chunk.len() as u64;
        if done > total {
            return Err(Error::Refused(format!(
                "{} is larger than its release says",
                release.archive
            )));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| Error::io(format!("write {}", archive.display()), e))?;
        progress(Stage::Downloading { done, total });
    }
    file.flush()
        .await
        .map_err(|e| Error::io(format!("write {}", archive.display()), e))?;
    if done != total {
        return Err(Error::io(CONTEXT, std::io::ErrorKind::UnexpectedEof.into()));
    }
    Ok(())
}

/// The sum for `archive` in a `.sha512sum` file (`<hex>  <name>`).
fn parse_sum(raw: &[u8], archive: &str) -> Result<String> {
    String::from_utf8_lossy(raw)
        .lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .find(|(_, name)| name.trim().trim_start_matches('*') == archive)
        .map(|(hex, _)| hex.to_ascii_lowercase())
        .filter(|hex| hex.len() == 128 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| Error::Refused(format!("no SHA-512 sum published for {archive}")))
}

fn sha512_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(crate::fsutil::hex(&hasher.finalize()))
}

/// The archive's name without `.tar.gz` or `.tar.xz`, for an archive this module can unpack.
fn archive_stem(name: &str) -> Option<&str> {
    name.strip_suffix(".tar.gz")
        .or_else(|| name.strip_suffix(".tar.xz"))
}

/// A single folder name, not hidden.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

#[cfg(target_arch = "x86_64")]
fn x86_64_v3() -> bool {
    use std::arch::is_x86_feature_detected as has;
    has!("avx2")
        && has!("bmi1")
        && has!("bmi2")
        && has!("f16c")
        && has!("fma")
        && has!("lzcnt")
        && has!("movbe")
        && has!("xsave")
}

#[cfg(not(target_arch = "x86_64"))]
fn x86_64_v3() -> bool {
    false
}

/// Counts what the archive's reader took, for progress.
struct Counting<R> {
    inner: R,
    read: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read
            .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(n)
    }
}

/// Unpacks a `.tar.gz` or `.tar.xz` holding one folder into `to`, without that first folder.
/// Paths that climb out, links that lead out (even through another link) and special files are
/// left out; files are never written through a link.
fn unpack(
    archive: &Path,
    to: &Path,
    cancel: &CancellationToken,
    progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let context = || format!("unpack {}", archive.display());
    if to.exists() {
        std::fs::remove_dir_all(to)
            .map_err(|e| Error::io(format!("delete {}", to.display()), e))?;
    }
    crate::paths::ensure_dir(to)?;
    let file = std::fs::File::open(archive).map_err(|e| Error::io(context(), e))?;
    let total = file.metadata().map(|m| m.len()).unwrap_or(0);
    let read = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counting = std::io::BufReader::new(Counting {
        inner: file,
        read: read.clone(),
    });
    let name = archive.to_string_lossy();
    let decoded: Box<dyn Read> = if name.ends_with(".tar.xz") {
        Box::new(lzma_rust2::XzReader::new(counting, true))
    } else {
        Box::new(flate2::read::GzDecoder::new(counting))
    };
    let mut tar = tar::Archive::new(decoded);
    let mut links = Vec::new();
    for entry in tar.entries().map_err(|e| Error::io(context(), e))? {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut entry = entry.map_err(|e| Error::io(context(), e))?;
        let raw = entry
            .path()
            .map_err(|e| Error::io(context(), e))?
            .into_owned();
        // Without the archive's own folder; what climbs out or starts at `/` is left out.
        let mut parts = raw.components().filter(|c| *c != Component::CurDir);
        parts.next();
        let rel: PathBuf = parts.collect();
        if rel.as_os_str().is_empty()
            || !rel.components().all(|c| matches!(c, Component::Normal(_)))
        {
            continue;
        }
        let dest = to.join(&rel);
        let parent = dest.parent().unwrap_or(to);
        let kind = entry.header().entry_type();
        if !kind.is_dir() && !kind.is_file() && !kind.is_symlink() && !kind.is_hard_link() {
            continue;
        }
        // Checked before the folder is made: a link made earlier could lead it out.
        if !leads_inside(to, parent) {
            continue;
        }
        crate::paths::ensure_dir(parent)?;
        // What is there already (an earlier entry of the same name) is replaced, never followed.
        if std::fs::symlink_metadata(&dest).is_ok_and(|m| !m.is_dir()) {
            std::fs::remove_file(&dest).map_err(|e| Error::io(context(), e))?;
        }
        if kind.is_dir() {
            crate::paths::ensure_dir(&dest)?;
        } else if kind.is_symlink() {
            let Some(target) = entry.link_name().map_err(|e| Error::io(context(), e))? else {
                continue;
            };
            if !stays_inside(&rel, &target) {
                continue;
            }
            std::os::unix::fs::symlink(&target, &dest).map_err(|e| Error::io(context(), e))?;
            links.push(dest);
        } else if kind.is_hard_link() {
            let Some(target) = entry.link_name().map_err(|e| Error::io(context(), e))? else {
                continue;
            };
            let mut target_parts = target.components().filter(|c| *c != Component::CurDir);
            target_parts.next();
            let target = to.join(target_parts.collect::<PathBuf>());
            if !leads_inside(to, &target)
                || !std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_file())
            {
                continue;
            }
            std::fs::hard_link(&target, &dest).map_err(|e| Error::io(context(), e))?;
        } else {
            use std::os::unix::fs::OpenOptionsExt;
            let mode = entry.header().mode().map_err(|e| Error::io(context(), e))? & 0o777;
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode | 0o600)
                .open(&dest)
                .map_err(|e| Error::io(format!("create {}", dest.display()), e))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| Error::io(context(), e))?;
        }
        progress(read.load(std::sync::atomic::Ordering::Relaxed), total);
    }
    // Checked again once all are made: a link can lead out through another one.
    for link in links {
        let Ok(target) = std::fs::read_link(&link) else {
            continue;
        };
        if !leads_inside(to, &link.parent().unwrap_or(to).join(&target)) {
            std::fs::remove_file(&link).map_err(|e| Error::io(context(), e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
