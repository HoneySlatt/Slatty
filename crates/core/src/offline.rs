//! GOG's offline installers, downloaded whole to keep: the setup programs GOG's site offers for
//! Windows, Linux and macOS, with those of the DLC. As lgogdownloader does, each file is checked
//! against the MD5 sums GOG keeps for it, in the XML its download link names, and takes its name
//! only once checked.

use std::future::Future;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use chrono::Utc;
use futures::StreamExt;
use md5::{Digest, Md5};
use reqwest::Client;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::installer::linux::{GogInstallers, Source as Bytes};
use crate::installer::{DOWNLOADING, FAILED, PAUSED, Progress, QUEUED, folder_name, free_space};
use crate::paths::Dirs;
use crate::{fsutil, http};

/// The systems GOG makes installers for, in the order they are offered.
pub const SYSTEMS: [&str; 3] = ["windows", "linux", "mac"];
const DOWNLOAD_SUFFIX: &str = ".slatty-dl";
/// Times a file's download is tried in a row after its connection dropped, the first after this
/// wait, the next after twice as long: a dropped connection often comes back within seconds.
const ATTEMPTS: u32 = 3;
const FIRST_WAIT: std::time::Duration = if cfg!(test) {
    std::time::Duration::from_millis(10)
} else {
    std::time::Duration::from_secs(1)
};

/// A file of an installer: large ones come in several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// As GOG's API rounds it.
    pub size: u64,
    /// GOG's address that hands out a download link.
    pub downlink: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installer {
    pub os: String,
    pub language: String,
    pub version: String,
    pub files: Vec<File>,
}

impl Installer {
    pub fn size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dlc {
    pub id: String,
    pub title: String,
    pub installers: Vec<Installer>,
}

/// The installers of a game and of its DLC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub title: String,
    pub installers: Vec<Installer>,
    /// Only the DLC that have installers: GOG lists none for those the account does not own.
    pub dlcs: Vec<Dlc>,
}

impl Offer {
    /// The systems the game has installers for.
    pub fn systems(&self) -> Vec<&'static str> {
        SYSTEMS
            .into_iter()
            .filter(|s| self.installers.iter().any(|i| i.os == *s))
            .collect()
    }

    /// The languages of the game's installers for `os`.
    pub fn languages(&self, os: &str) -> Vec<&str> {
        let mut languages = Vec::new();
        for i in self.installers.iter().filter(|i| i.os == os) {
            if !languages.contains(&i.language.as_str()) {
                languages.push(i.language.as_str());
            }
        }
        languages
    }

    /// The DLC with an installer for `os`.
    pub fn dlcs_for<'a>(&'a self, os: &'a str) -> impl Iterator<Item = &'a Dlc> {
        self.dlcs
            .iter()
            .filter(move |d| d.installers.iter().any(|i| i.os == os))
    }

    /// The installers to download: the game's for `os` in each of `languages`, and those of each
    /// DLC in `dlcs`, in the languages it has.
    pub fn chosen(&self, os: &str, languages: &[String], dlcs: &[String]) -> Vec<&Installer> {
        let game = pick(&self.installers, os, languages);
        if game.is_empty() {
            return game;
        }
        game.into_iter()
            .chain(
                self.dlcs
                    .iter()
                    .filter(|d| dlcs.contains(&d.id))
                    .flat_map(|d| pick(&d.installers, os, languages)),
            )
            .collect()
    }
}

/// The installers for `os` in `languages`; when it has none of them, the English one, else the
/// first.
fn pick<'a>(installers: &'a [Installer], os: &str, languages: &[String]) -> Vec<&'a Installer> {
    let of_os = || installers.iter().filter(|i| i.os == os);
    let wanted: Vec<&Installer> = of_os()
        .filter(|i| languages.iter().any(|l| i.language.eq_ignore_ascii_case(l)))
        .collect();
    if !wanted.is_empty() {
        return wanted;
    }
    of_os()
        .find(|i| i.language == "en")
        .or_else(|| of_os().next())
        .into_iter()
        .collect()
}

pub async fn offer(http: &Client, tokens: &Tokens, game_id: &str) -> Result<Offer> {
    crate::paths::check_game_id(game_id)?;
    let url = format!("https://api.gog.com/products/{game_id}?expand=downloads,expanded_dlcs");
    let raw = http::bytes(
        http.get(url).bearer_auth(tokens.access_token.expose()),
        "listing installers",
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
        files: Vec<RawFile>,
    }
    #[derive(Deserialize)]
    struct RawFile {
        #[serde(default)]
        size: u64,
        downlink: String,
    }
    let installers = |p: &Product| -> Vec<Installer> {
        p.downloads
            .installers
            .iter()
            .filter(|i| !i.files.is_empty())
            .map(|i| Installer {
                os: i.os.clone(),
                language: i.language.clone(),
                version: i.version.clone().unwrap_or_default(),
                files: i
                    .files
                    .iter()
                    .map(|f| File {
                        size: f.size,
                        downlink: f.downlink.clone(),
                    })
                    .collect(),
            })
            .collect()
    };
    let product: Product =
        serde_json::from_slice(raw).map_err(|e| Error::parse("listing installers", e))?;
    Ok(Offer {
        title: product.title.clone(),
        installers: installers(&product),
        dlcs: product
            .expanded_dlcs
            .iter()
            .map(|d| Dlc {
                id: match &d.id {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                },
                title: d.title.clone(),
                installers: installers(d),
            })
            .filter(|d| !d.installers.is_empty())
            .collect(),
    })
}

/// The folder a game's installers go into, under `root`: named after its title, or its id when
/// nothing of the title is left for a name.
pub fn folder_for(root: &Path, title: &str, game_id: &str) -> PathBuf {
    match folder_name(title) {
        name if name.is_empty() => root.join(game_id),
        name => root.join(name),
    }
}

/// Installers chosen for download, kept until they are all downloaded so that they resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflineJob {
    pub game_id: String,
    pub os: String,
    /// Kept in the `language` column, separated by commas.
    pub languages: Vec<String>,
    pub dlcs: Vec<String>,
    /// The game's own folder, where its installers go.
    pub folder: PathBuf,
    /// One of `installer::{QUEUED, DOWNLOADING, PAUSED, FAILED}`.
    pub state: String,
}

impl OfflineJob {
    pub fn save(&self, db: &Db) -> Result<()> {
        crate::paths::check_game_id(&self.game_id)?;
        db.conn().execute(
            "INSERT INTO offline_jobs (game_id, os, language, dlcs, folder, state, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(game_id) DO UPDATE SET os = excluded.os, language = excluded.language,
                dlcs = excluded.dlcs, folder = excluded.folder, state = excluded.state,
                updated_at = excluded.updated_at",
            params![
                self.game_id,
                self.os,
                self.languages.join(","),
                self.dlcs.join(","),
                self.folder.to_string_lossy(),
                self.state,
                Utc::now().timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn load(db: &Db, game_id: &str) -> Result<Option<OfflineJob>> {
        Ok(db
            .conn()
            .query_row(
                "SELECT game_id, os, language, dlcs, folder, state FROM offline_jobs
                 WHERE game_id = ?1",
                [game_id],
                row,
            )
            .optional()?)
    }

    /// Every job, the oldest first.
    pub fn list(db: &Db) -> Result<Vec<OfflineJob>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT game_id, os, language, dlcs, folder, state FROM offline_jobs
             ORDER BY updated_at",
        )?;
        Ok(stmt.query_map([], row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Forgets the job; what it downloaded stays.
    pub fn delete(db: &Db, game_id: &str) -> Result<()> {
        db.conn()
            .execute("DELETE FROM offline_jobs WHERE game_id = ?1", [game_id])?;
        Ok(())
    }
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<OfflineJob> {
    let list = |s: String| {
        s.split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    };
    Ok(OfflineJob {
        game_id: r.get(0)?,
        os: r.get(1)?,
        languages: list(r.get(2)?),
        dlcs: list(r.get(3)?),
        folder: PathBuf::from(r.get::<_, String>(4)?),
        state: r.get(5)?,
    })
}

/// Downloads `job`'s installers into its folder, and returns the files. The job is forgotten once
/// they are all there; otherwise it is kept paused (`Error::Cancelled`), failed, or downloading
/// when cut off on the way, to be taken up again.
pub async fn run(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    job: &OfflineJob,
    progress: &(dyn Fn(Progress) + Sync),
    cancel: &CancellationToken,
) -> Result<Vec<PathBuf>> {
    let _busy = lock(dirs, &job.game_id)?;
    let mut job = job.clone();
    // The installers folder a started download was going into is gone, its drive unplugged or its
    // share not mounted: starting again would fill the empty mount point instead.
    let root = job.folder.parent().unwrap_or(&job.folder);
    if job.state != QUEUED && !root.is_dir() {
        let missing = Error::NotFound(format!(
            "{} is missing: is its drive mounted?",
            root.display()
        ));
        job.state = FAILED.into();
        job.save(db)?;
        return Err(missing);
    }
    job.state = DOWNLOADING.into();
    job.save(db)?;
    let result = async {
        let offer = offer(http, tokens, &job.game_id).await?;
        let chosen = offer.chosen(&job.os, &job.languages, &job.dlcs);
        if chosen.is_empty() {
            return Err(Error::NotFound(format!(
                "GOG offers no {} installer of {}",
                job.os, offer.title
            )));
        }
        let downlinks = chosen
            .iter()
            .flat_map(|i| &i.files)
            .map(|f| f.downlink.clone())
            .collect();
        let source = GogInstallers::new(http.clone(), tokens.clone(), Some(dirs), downlinks);
        download(&source, &job.folder, &job.game_id, progress, cancel).await
    }
    .await;
    match &result {
        Ok(_) => OfflineJob::delete(db, &job.game_id)?,
        Err(Error::Cancelled) => {
            job.state = PAUSED.into();
            job.save(db)?;
        }
        Err(e) if e.is_temporary() => {}
        Err(_) => {
            job.state = FAILED.into();
            job.save(db)?;
        }
    }
    result
}

/// Forgets `game_id`'s job and deletes what it left unfinished, the staged files in its folder;
/// the installers already downloaded stay.
pub fn discard(db: &Db, dirs: &Dirs, game_id: &str) -> Result<()> {
    let _busy = lock(dirs, game_id)?;
    let Some(job) = OfflineJob::load(db, game_id)? else {
        return Ok(());
    };
    remove_staged(&job.folder, game_id)?;
    // Only when nothing else is in it.
    let _ = std::fs::remove_dir(&job.folder);
    OfflineJob::delete(db, game_id)
}

/// Deletes what `game_id`'s downloads left unfinished in `folder`; never another game's, whose
/// title may give the same folder.
fn remove_staged(folder: &Path, game_id: &str) -> Result<()> {
    let ours = format!(".{game_id}{DOWNLOAD_SUFFIX}");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Ok(());
    };
    for e in entries.flatten() {
        let staged = e.file_name().to_str().is_some_and(|n| n.ends_with(&ours));
        if staged && e.file_type().is_ok_and(|t| !t.is_dir()) {
            std::fs::remove_file(e.path())
                .map_err(|err| Error::io(format!("delete {}", e.path().display()), err))?;
        }
    }
    Ok(())
}

/// Held while a game's installers download or are discarded, from any process.
fn lock(dirs: &Dirs, game_id: &str) -> Result<crate::lock::FileLock> {
    crate::paths::check_game_id(game_id)?;
    crate::lock::try_acquire(&dirs.locks().join(format!("installers-{game_id}.lock")))?
        .ok_or_else(|| Error::Refused("the installers of this game are downloading".into()))
}

/// Files to download whole: their bytes, names and GOG's sums.
pub trait Source: Bytes {
    fn parts(&self) -> usize;
    /// The name `part` is served under, as it comes (percent-encoded).
    fn name(&self, part: usize) -> impl Future<Output = Result<String>> + Send;
    /// GOG's checksum XML for `part`, when it keeps one.
    fn checksums(&self, part: usize) -> impl Future<Output = Result<Option<Vec<u8>>>> + Send;
}

impl Source for GogInstallers {
    fn parts(&self) -> usize {
        GogInstallers::parts(self)
    }

    async fn name(&self, part: usize) -> Result<String> {
        Ok(self.link(part, false).await?.name)
    }

    async fn checksums(&self, part: usize) -> Result<Option<Vec<u8>>> {
        self.checksum_file(part).await
    }
}

/// Downloads every file of `source` into `folder`, each staged under a name of its own (its
/// game's id in it), checked, then renamed. A file already there at its size is kept; one there
/// at another size is not replaced.
pub async fn download<S: Source>(
    source: &S,
    folder: &Path,
    game_id: &str,
    progress: &(dyn Fn(Progress) + Sync),
    cancel: &CancellationToken,
) -> Result<Vec<PathBuf>> {
    let partial_of = |name: &str| folder.join(format!("{name}.{game_id}{DOWNLOAD_SUFFIX}"));
    let mut files: Vec<(usize, String, u64, Option<Checksums>)> = Vec::new();
    for part in 0..source.parts() {
        let name = file_name(&source.name(part).await?)?;
        let size = source.size(part).await?;
        // GOG lists an installer under each language it speaks: one file, downloaded once.
        match files.iter().find(|(_, n, _, _)| *n == name) {
            Some((_, _, s, _)) if *s == size => continue,
            Some(_) => {
                return Err(Error::parse(
                    "listing installers",
                    format!("two files are named {name}"),
                ));
            }
            None => {}
        }
        // Read with the link, before any bytes: their signed address does not last, and sums that
        // cannot be read stop the download before it fetches gigabytes for nothing.
        let sums = if there(&folder.join(&name), size)?.is_some() {
            None
        } else {
            match source.checksums(part).await? {
                Some(xml) => Some(parse_checksums(&xml)?),
                None => None,
            }
        };
        if sums
            .as_ref()
            .is_some_and(|s| s.chunks.iter().any(|c| c.to >= size))
        {
            return Err(Error::parse(
                "reading an installer's checksums",
                "a chunk past the end of the file",
            ));
        }
        files.push((part, name, size, sums));
    }
    crate::paths::ensure_dir(folder)?;
    let mut p = Progress {
        files_total: files.len(),
        bytes_total: files.iter().map(|(_, _, size, _)| size).sum(),
        ..Progress::default()
    };
    let mut present = 0;
    for (_, name, size, _) in &files {
        if let Some(have) = there(&folder.join(name), *size)? {
            present += have;
        } else if let Ok(m) = std::fs::symlink_metadata(partial_of(name)) {
            present += m.len().min(*size);
        }
    }
    let needed = p.bytes_total - present;
    let free = free_space(folder)?;
    if needed > free {
        return Err(Error::Refused(format!(
            "not enough disk space: {} needed, {} free",
            crate::units::size(needed),
            crate::units::size(free)
        )));
    }
    let mut done = Vec::new();
    for (part, name, size, sums) in files {
        let target = folder.join(&name);
        if there(&target, size)?.is_none() {
            let partial = partial_of(&name);
            fetch(source, part, size, &partial, &mut p, progress, cancel).await?;
            if let Some(sums) = sums {
                check(source, part, &partial, &sums, cancel).await?;
            }
            std::fs::rename(&partial, &target)
                .map_err(|e| Error::io(format!("rename {}", partial.display()), e))?;
        } else {
            p.bytes_done += size;
        }
        p.files_done += 1;
        progress(p);
        done.push(target);
    }
    // All there: what is left unfinished is an installer GOG has since replaced.
    let _ = remove_staged(folder, game_id);
    Ok(done)
}

/// `Some(size)` when `path` is the file at its size, already downloaded; `None` when it is not
/// there. Anything else in its place is not overwritten.
fn there(path: &Path, size: u64) -> Result<Option<u64>> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.len() == size => Ok(Some(size)),
        Ok(_) => Err(Error::Refused(format!(
            "{} is already there and is not this installer",
            path.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
    }
}

/// Fills `partial` up to `size`, from where an earlier attempt left it.
#[allow(clippy::too_many_arguments)]
async fn fetch<S: Source>(
    source: &S,
    part: usize,
    size: u64,
    partial: &Path,
    p: &mut Progress,
    progress: &(dyn Fn(Progress) + Sync),
    cancel: &CancellationToken,
) -> Result<()> {
    let io = |what: &str, e| Error::io(format!("{what} {}", partial.display()), e);
    // Never through a link left in its place.
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(partial)
        .map_err(|e| io("open", e))?;
    let mut have = file.metadata().map_err(|e| io("read", e))?.len();
    if have > size {
        file.set_len(0).map_err(|e| io("empty", e))?;
        have = 0;
    }
    file.seek(SeekFrom::Start(have))
        .map_err(|e| io("seek", e))?;
    p.bytes_done += have;
    progress(*p);
    let mut attempt = 1;
    while have < size {
        let before = have;
        let got = async {
            let mut body = source.open(part, have, size).await?;
            loop {
                let piece = tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(Error::Cancelled),
                    piece = body.next() => piece,
                };
                let Some(piece) = piece else {
                    return Ok(());
                };
                let piece = piece?;
                let piece = &piece[..piece.len().min((size - have) as usize)];
                // Pieces are small: written as they come, the file system caches them.
                file.write_all(piece).map_err(|e| io("write", e))?;
                have += piece.len() as u64;
                p.bytes_done += piece.len() as u64;
                p.network_bytes += piece.len() as u64;
                progress(*p);
                if have == size {
                    return Ok(());
                }
            }
        }
        .await;
        // A connection that brought something leaves every try for the next drop.
        if have > before {
            attempt = 1;
        }
        let again = match got {
            Ok(()) if have == size => false,
            // Cut short, or dropped on the way: taken up where it stopped.
            Ok(()) if attempt < ATTEMPTS => true,
            Err(e) if e.is_temporary() && attempt < ATTEMPTS => true,
            Ok(()) => {
                return Err(Error::io(
                    "downloading an installer",
                    std::io::ErrorKind::UnexpectedEof.into(),
                ));
            }
            Err(e) => return Err(e),
        };
        if again {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                _ = tokio::time::sleep(FIRST_WAIT * attempt) => {}
            }
            attempt += 1;
        }
    }
    file.sync_all().map_err(|e| io("sync", e))
}

/// GOG's MD5 sums of a file: of the whole, and of each part.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Checksums {
    md5: String,
    chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Chunk {
    from: u64,
    /// The part's last byte.
    to: u64,
    md5: String,
}

/// Reads GOG's checksum XML: `<file md5="…" …>` holding `<chunk from="…" to="…"
/// method="md5">…</chunk>` elements.
fn parse_checksums(xml: &[u8]) -> Result<Checksums> {
    let bad = |what: &str| Error::parse("reading an installer's checksums", what);
    let xml = std::str::from_utf8(xml).map_err(|_| bad("not text"))?;
    let start = xml.find("<file ").ok_or_else(|| bad("no file"))?;
    let tag = &xml[start..start + xml[start..].find('>').ok_or_else(|| bad("no file"))?];
    let md5 = attribute(tag, "md5")
        .filter(|m| is_md5(m))
        .ok_or_else(|| bad("no MD5"))?;
    let mut chunks = Vec::new();
    for piece in xml[start..].split("<chunk ").skip(1) {
        let (tag, rest) = piece.split_once('>').ok_or_else(|| bad("a chunk"))?;
        let (sum, _) = rest.split_once("</chunk>").ok_or_else(|| bad("a chunk"))?;
        if attribute(tag, "method") != Some("md5") {
            continue;
        }
        let number = |name| attribute(tag, name).and_then(|v| v.parse::<u64>().ok());
        // In order, one after the other.
        let after = chunks.last().map(|c: &Chunk| c.to + 1);
        match (number("from"), number("to"), sum.trim()) {
            (Some(from), Some(to), sum)
                if from <= to && after.is_none_or(|a| from >= a) && is_md5(sum) =>
            {
                chunks.push(Chunk {
                    from,
                    to,
                    md5: sum.to_ascii_lowercase(),
                })
            }
            _ => return Err(bad("a chunk")),
        }
    }
    Ok(Checksums {
        md5: md5.to_ascii_lowercase(),
        chunks,
    })
}

/// The value of `name="…"` in `tag`.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    Some(&tag[at..at + tag[at..].find('"')?])
}

fn is_md5(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Checks `partial` against GOG's sums. Parts found damaged are downloaded again, once; a file
/// still wrong after that is deleted.
async fn check<S: Source>(
    source: &S,
    part: usize,
    partial: &Path,
    sums: &Checksums,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut bad = damaged(partial, sums, cancel).await?;
    if bad.as_ref().is_some_and(|b| !b.is_empty()) {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(partial)
            .map_err(|e| Error::io(format!("open {}", partial.display()), e))?;
        for c in bad.unwrap_or_default() {
            let data = source.read(part, c.from, c.to + 1).await?;
            if data.len() as u64 != c.to + 1 - c.from {
                return Err(Error::io(
                    "downloading an installer",
                    std::io::ErrorKind::UnexpectedEof.into(),
                ));
            }
            file.write_all_at(&data, c.from)
                .map_err(|e| Error::io(format!("write {}", partial.display()), e))?;
        }
        file.sync_all()
            .map_err(|e| Error::io(format!("sync {}", partial.display()), e))?;
        bad = damaged(partial, sums, cancel).await?;
    }
    if bad.is_some() {
        let _ = std::fs::remove_file(partial);
        return Err(Error::parse(
            "downloading an installer",
            "the file does not match GOG's checksum; download it again",
        ));
    }
    Ok(())
}

/// `None` when `path` matches `sums`; else the parts that do not, empty when only the whole is
/// wrong. The parts are summed only when the whole is wrong. Read at the lowest priority; a pause
/// stops it between reads.
async fn damaged(
    path: &Path,
    sums: &Checksums,
    cancel: &CancellationToken,
) -> Result<Option<Vec<Chunk>>> {
    let (path, sums, cancel) = (path.to_path_buf(), sums.clone(), cancel.clone());
    fsutil::in_background(move || {
        let mut whole = Md5::new();
        read_through(&path, &cancel, |piece| whole.update(piece))?;
        if fsutil::hex(&whole.finalize()) == sums.md5 {
            return Ok(None);
        }
        let mut bad = Vec::new();
        let mut at = 0u64;
        // The chunks follow each other from the start; bytes between or after them count only in
        // the whole.
        let mut chunks = sums.chunks.iter().peekable();
        let mut current: Option<(&Chunk, Md5)> = None;
        read_through(&path, &cancel, |mut piece| {
            while !piece.is_empty() {
                if current.is_none()
                    && let Some(c) = chunks.next_if(|c| c.from <= at)
                {
                    current = Some((c, Md5::new()));
                }
                let take = match &mut current {
                    Some((c, md5)) => {
                        let take = piece.len().min((c.to + 1 - at) as usize);
                        md5.update(&piece[..take]);
                        take
                    }
                    None => match chunks.peek() {
                        Some(c) => piece.len().min((c.from - at) as usize),
                        None => piece.len(),
                    },
                };
                at += take as u64;
                piece = &piece[take..];
                if let Some((c, _)) = &current
                    && at == c.to + 1
                {
                    let (c, md5) = current.take().expect("a chunk");
                    if fsutil::hex(&md5.finalize()) != c.md5 {
                        bad.push(c.clone());
                    }
                }
            }
        })?;
        Ok(Some(bad))
    })
    .await
}

/// Gives `each` the bytes of `path` in order, until its end or `cancel`.
fn read_through(
    path: &Path,
    cancel: &CancellationToken,
    mut each: impl FnMut(&[u8]),
) -> Result<()> {
    let read = |e| Error::io(format!("read {}", path.display()), e);
    let mut file = std::fs::File::open(path).map_err(read)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let n = file.read(&mut buf).map_err(read)?;
        if n == 0 {
            return Ok(());
        }
        each(&buf[..n]);
    }
}

/// A file's name as the CDN serves it: decoded, and only a name, never a path.
fn file_name(raw: &str) -> Result<String> {
    let mut bytes = Vec::with_capacity(raw.len());
    let mut it = raw.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let (Some(high), Some(low)) = (it.next(), it.next()) else {
                return Err(bad_name(raw));
            };
            let hex = std::str::from_utf8(&[high, low])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            bytes.push(hex.ok_or_else(|| bad_name(raw))?);
        } else {
            bytes.push(b);
        }
    }
    let name = String::from_utf8(bytes).map_err(|_| bad_name(raw))?;
    let fine = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
        && !name.ends_with(DOWNLOAD_SUFFIX);
    if fine { Ok(name) } else { Err(bad_name(raw)) }
}

fn bad_name(raw: &str) -> Error {
    Error::parse(
        "downloading an installer",
        format!("{raw:?} is not a file name"),
    )
}

#[cfg(test)]
mod tests;
