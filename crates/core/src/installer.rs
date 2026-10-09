use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use chrono::Utc;
use flate2::read::ZlibDecoder;
use futures::StreamExt;
use md5::{Digest, Md5};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::galaxy::{self, Build, ContentSource, Depot, DepotFile, DepotItem, Meta};
use crate::install::{Install, Platform};
use crate::paths::Dirs;
use crate::runner::Runner;

const FILE_CONCURRENCY: usize = 4;
const CHUNK_CONCURRENCY: usize = 2;
const PARTIAL_SUFFIX: &str = ".slatty-partial";
const DOWNLOAD_SUFFIX: &str = ".slatty-dl";

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub game_id: String,
    pub title: String,
    pub build: Build,
    pub meta: Meta,
    pub language: String,
    pub languages: Vec<String>,
    pub depots: Vec<Depot>,
    pub dlcs: Vec<DlcChoice>,
    /// Resolved from GOG's dependency repository by `plan_for`.
    pub dependencies: Vec<galaxy::Dependency>,
    pub download_size: u64,
    pub disk_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlcChoice {
    pub id: String,
    pub name: String,
    pub owned: bool,
    pub selected: bool,
    /// For the chosen language.
    pub download_size: u64,
    pub disk_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlcSelection {
    AllOwned,
    Only(Vec<String>),
}

impl InstallPlan {
    pub fn selected_dlcs(&self) -> Vec<String> {
        self.dlcs
            .iter()
            .filter(|d| d.selected)
            .map(|d| d.id.clone())
            .collect()
    }

    /// Selects the depots of the base game and of the chosen owned DLC for one language.
    pub fn new(
        game_id: &str,
        build: Build,
        meta: Meta,
        language: Option<&str>,
        owned: &HashSet<String>,
        selection: &DlcSelection,
    ) -> Result<Self> {
        if build.generation != 2 || meta.version == Some(1) {
            return Err(Error::Unsupported(
                "only generation 2 (Galaxy) builds are supported".into(),
            ));
        }
        if meta.base_product_id != game_id {
            return Err(Error::Unsupported(format!(
                "build belongs to product {}",
                meta.base_product_id
            )));
        }
        let base: Vec<&Depot> = meta
            .depots
            .iter()
            .filter(|d| d.product_id == game_id)
            .collect();
        let mut languages: Vec<String> = Vec::new();
        for l in base.iter().flat_map(|d| &d.languages).filter(|l| *l != "*") {
            if !languages.iter().any(|x| x.eq_ignore_ascii_case(l)) {
                languages.push(l.clone());
            }
        }
        let language = match language {
            Some(wanted) => languages
                .iter()
                .find(|l| l.eq_ignore_ascii_case(wanted))
                .cloned()
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "language `{wanted}` not offered; available: {}",
                        languages.join(", ")
                    ))
                })?,
            None => ["en-US", "en", "English"]
                .iter()
                .find_map(|p| languages.iter().find(|l| l.eq_ignore_ascii_case(p)))
                .or(languages.first())
                .cloned()
                .unwrap_or_else(|| "*".into()),
        };
        let speaks = |d: &&Depot| {
            d.languages
                .iter()
                .any(|l| l == "*" || l.eq_ignore_ascii_case(&language))
        };
        if !base.iter().any(speaks) {
            return Err(Error::NotFound("no depot for this language".into()));
        }
        let mut dlcs: Vec<DlcChoice> = meta
            .products
            .iter()
            .filter(|p| p.product_id != game_id)
            .filter(|p| meta.depots.iter().any(|d| d.product_id == p.product_id))
            .map(|p| {
                let depots = || {
                    meta.depots
                        .iter()
                        .filter(|d| d.product_id == p.product_id)
                        .filter(speaks)
                };
                DlcChoice {
                    id: p.product_id.clone(),
                    name: p.name.clone(),
                    owned: owned.contains(&p.product_id),
                    selected: false,
                    download_size: depots().map(|d| d.compressed_size).sum(),
                    disk_size: depots().map(|d| d.size).sum(),
                }
            })
            .collect();
        for dlc in &mut dlcs {
            dlc.selected = dlc.owned
                && match selection {
                    DlcSelection::AllOwned => true,
                    DlcSelection::Only(ids) => ids.contains(&dlc.id),
                };
        }
        if let DlcSelection::Only(ids) = selection
            && let Some(missing) = ids
                .iter()
                .find(|id| !dlcs.iter().any(|d| d.selected && &d.id == *id))
        {
            return Err(Error::Refused(format!(
                "DLC {missing} is not owned or not part of this game"
            )));
        }
        let depots: Vec<Depot> = meta
            .depots
            .iter()
            .filter(|d| {
                d.product_id == game_id || dlcs.iter().any(|c| c.selected && c.id == d.product_id)
            })
            .filter(speaks)
            .cloned()
            .collect();
        let title = meta
            .products
            .iter()
            .find(|p| p.product_id == game_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| meta.install_directory.clone());
        Ok(Self {
            game_id: game_id.to_string(),
            title,
            download_size: depots.iter().map(|d| d.compressed_size).sum(),
            disk_size: depots.iter().map(|d| d.size).sum(),
            build,
            meta,
            language,
            languages,
            depots,
            dlcs,
            dependencies: Vec::new(),
        })
    }

    pub fn directory_name(&self) -> Result<String> {
        let name = self.meta.install_directory.trim();
        match Path::new(name).components().collect::<Vec<_>>().as_slice() {
            [Component::Normal(_)] => Ok(name.to_string()),
            _ => Err(Error::Refused(format!("unsafe install directory `{name}`"))),
        }
    }
}

/// Files to write, keyed by lowercase path: Windows treats differently cased paths as one file.
#[derive(Debug, Default)]
pub struct FileSet {
    pub files: Vec<(PathBuf, DepotFile)>,
    pub dirs: Vec<PathBuf>,
    /// GOG installer support files (scripts, icons), relative to the game's support folder as
    /// `<product id>/<path>`. They never go into the game folder.
    pub support: Vec<(PathBuf, DepotFile)>,
    pub skipped_links: usize,
}

impl FileSet {
    pub fn disk_size(&self) -> u64 {
        self.files.iter().map(|(_, f)| f.size()).sum()
    }

    pub fn support_set(&self) -> FileSet {
        FileSet {
            files: self.support.clone(),
            ..Default::default()
        }
    }
}

pub fn safe_relative(raw: &str) -> Result<PathBuf> {
    let parts: Vec<&str> = raw
        .split(['\\', '/'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() || raw.starts_with('/') || raw.starts_with('\\') || raw.contains(':') {
        return Err(Error::Refused(format!("unsafe path in manifest: `{raw}`")));
    }
    let mut out = PathBuf::new();
    for p in parts {
        if !matches!(
            Path::new(p).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        ) {
            return Err(Error::Refused(format!("unsafe path in manifest: `{raw}`")));
        }
        out.push(p);
    }
    Ok(out)
}

pub async fn collect_files<S: ContentSource>(source: &S, depots: &[Depot]) -> Result<FileSet> {
    let mut set = FileSet::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut casing: HashMap<String, PathBuf> = HashMap::new();
    for depot in depots {
        for item in source.depot_manifest(depot).await? {
            match item {
                DepotItem::DepotFile(mut f) => {
                    f.product_id = depot.product_id.clone();
                    if f.is_support() {
                        let rel = safe_relative(&depot.product_id)?.join(safe_relative(&f.path)?);
                        set.support.push((rel, f));
                        continue;
                    }
                    if f.chunks.is_empty() && f.sfc_ref.is_some() {
                        return Err(Error::Unsupported(format!(
                            "`{}` is stored in a small-files container",
                            f.path
                        )));
                    }
                    let rel = safe_relative(&f.path)?;
                    let key = rel.to_string_lossy().to_lowercase();
                    let rel = casing.entry(key.clone()).or_insert(rel).clone();
                    match index.get(&key) {
                        Some(&i) => set.files[i] = (rel, f),
                        None => {
                            index.insert(key, set.files.len());
                            set.files.push((rel, f));
                        }
                    }
                }
                DepotItem::DepotDirectory { path } => set.dirs.push(safe_relative(&path)?),
                DepotItem::DepotLink { .. } => set.skipped_links += 1,
            }
        }
    }
    Ok(set)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// Result of checking files in place.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Checked {
    /// Files that were missing or wrong (and, when repairing, were replaced).
    pub bad: Vec<PathBuf>,
    /// Bytes of replaced files copied from their previous version instead of downloaded.
    pub reused_bytes: u64,
}

pub struct Download<'a, S> {
    pub source: &'a S,
    pub cancel: CancellationToken,
    pub progress: &'a (dyn Fn(Progress) + Send + Sync),
    pub free_space: &'a (dyn Fn(&Path) -> Result<u64> + Send + Sync),
}

impl<S: ContentSource> Download<'_, S> {
    /// Fills `partial` (reusing verified files from an earlier attempt), then renames it to `target`.
    /// Nothing exists at `target` unless every file was verified.
    pub async fn run(&self, set: &FileSet, partial: &Path, target: &Path) -> Result<()> {
        if target.exists() {
            return Err(Error::Refused(format!(
                "{} already exists",
                target.display()
            )));
        }
        crate::paths::ensure_dir(partial)?;
        self.check_space(set, partial)?;
        self.fill(set, partial, true).await?;
        for dir in &set.dirs {
            crate::paths::ensure_dir(&partial.join(dir))?;
        }
        std::fs::rename(partial, target).map_err(|e| {
            Error::io(
                format!("publish {} as {}", partial.display(), target.display()),
                e,
            )
        })
    }

    /// Checks an installed game in place; with `repair`, re-downloads only the bad files.
    pub async fn check_installed(
        &self,
        set: &FileSet,
        dir: &Path,
        repair: bool,
    ) -> Result<Checked> {
        if repair {
            self.check_space(set, dir)?;
        }
        let checked = self.fill(set, dir, repair).await?;
        if repair {
            for d in &set.dirs {
                crate::paths::ensure_dir(&dir.join(d))?;
            }
        }
        Ok(checked)
    }

    fn check_space(&self, set: &FileSet, dir: &Path) -> Result<()> {
        let present: u64 = set
            .files
            .iter()
            .filter_map(|(rel, f)| {
                std::fs::metadata(dir.join(rel))
                    .ok()
                    .filter(|m| m.len() == f.size())
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

    /// Verifies every file under `dir`; returns those that were missing or wrong.
    /// With `download`, they are fetched again (atomically replaced).
    async fn fill(&self, set: &FileSet, dir: &Path, download: bool) -> Result<Checked> {
        let bad: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
        let reused = AtomicU64::new(0);
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

        // A failed file does not stop the others, so a retry has less to fetch; cancelling stops all.
        let first_error: std::sync::Mutex<Option<Error>> = std::sync::Mutex::new(None);
        // Indices rather than borrowed items keep the future provably Send (rustc HRTB limitation).
        futures::stream::iter(0..set.files.len())
            .for_each_concurrent(FILE_CONCURRENCY, |i| {
                let (rel, file) = &set.files[i];
                let dest = dir.join(rel);
                let first_error = &first_error;
                let bad = &bad;
                let reused = &reused;
                async move {
                    if self.cancel.is_cancelled() {
                        first_error.lock().unwrap().get_or_insert(Error::Cancelled);
                        return;
                    }
                    let result = async {
                        let verified = tokio::task::spawn_blocking({
                            let (dest, file) = (dest.clone(), file.clone());
                            move || verify_file(&dest, &file)
                        })
                        .await
                        .expect("verify task panicked")?;
                        if verified {
                            report(1, file.size());
                            return Ok(());
                        }
                        bad.lock().unwrap().push(rel.clone());
                        if download {
                            let local = self.download_file(&dest, file, &|n| report(0, n)).await?;
                            reused.fetch_add(local, Ordering::Relaxed);
                        }
                        report(1, if download { 0 } else { file.size() });
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
        let mut bad = bad.into_inner().unwrap();
        bad.sort();
        Ok(Checked {
            bad,
            reused_bytes: reused.into_inner(),
        })
    }

    /// Writes `file` at `dest` atomically; returns the bytes taken from the file already there.
    async fn download_file(
        &self,
        dest: &Path,
        file: &DepotFile,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> Result<u64> {
        if let Some(parent) = dest.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let tmp = dest.with_file_name(format!(
            "{}{DOWNLOAD_SUFFIX}",
            dest.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        ));
        let local = tokio::task::spawn_blocking({
            let (dest, file) = (dest.to_path_buf(), file.clone());
            move || local_chunks(&dest, &file)
        })
        .await
        .expect("scan task panicked")?;
        let mut out = std::fs::File::create(&tmp)
            .map_err(|e| Error::io(format!("create {}", tmp.display()), e))?;
        let mut chunks = futures::stream::iter(0..file.chunks.len())
            .map(|i| {
                let c = file.chunks[i].clone();
                let product = file.product_id.clone();
                let local = local.clone();
                async move {
                    if self.cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    if let Some(local) = local {
                        let c = c.clone();
                        let data = tokio::task::spawn_blocking(move || local.read(&c))
                            .await
                            .expect("chunk task panicked");
                        if let Some(data) = data {
                            return Ok((data, true));
                        }
                    }
                    let raw = self.source.chunk(&product, &c.compressed_md5).await?;
                    tokio::task::spawn_blocking(move || unpack_chunk(&raw, &c))
                        .await
                        .expect("chunk task panicked")
                        .map(|data| (data, false))
                }
            })
            .buffered(CHUNK_CONCURRENCY);
        let mut reused = 0;
        while let Some(data) = chunks.next().await {
            let (data, local) = data.inspect_err(|_| drop(std::fs::remove_file(&tmp)))?;
            out.write_all(&data)
                .map_err(|e| Error::io(format!("write {}", tmp.display()), e))?;
            if local {
                reused += data.len() as u64;
            }
            on_bytes(data.len() as u64);
        }
        out.sync_all()
            .map_err(|e| Error::io(format!("sync {}", tmp.display()), e))?;
        drop(out);
        if file.is_executable() {
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
        }
        std::fs::rename(&tmp, dest)
            .map_err(|e| Error::io(format!("rename {}", tmp.display()), e))?;
        Ok(reused)
    }
}

fn unpack_chunk(raw: &[u8], c: &crate::galaxy::Chunk) -> Result<Vec<u8>> {
    if fsutil::hex(&Md5::digest(raw)) != c.compressed_md5 {
        return Err(Error::Refused(format!(
            "chunk {} is corrupted (compressed checksum)",
            c.compressed_md5
        )));
    }
    let mut data = Vec::with_capacity(c.size as usize);
    ZlibDecoder::new(raw)
        .read_to_end(&mut data)
        .map_err(|e| Error::io(format!("decompress chunk {}", c.compressed_md5), e))?;
    if data.len() as u64 != c.size || fsutil::hex(&Md5::digest(&data)) != c.md5 {
        return Err(Error::Refused(format!(
            "chunk {} is corrupted (content checksum)",
            c.compressed_md5
        )));
    }
    Ok(data)
}

/// Chunks of `file` already present in the file at its destination (an older version, a damaged
/// copy), found by hashing that file at the new chunk offsets. Unchanged regions of a changed file
/// are then copied locally instead of downloaded.
struct LocalChunks {
    file: std::fs::File,
    /// Content MD5 → offset in `file`.
    offsets: HashMap<String, u64>,
}

impl LocalChunks {
    /// Reads a chunk back, checking it again: the file may have changed since it was scanned.
    fn read(&self, c: &crate::galaxy::Chunk) -> Option<Vec<u8>> {
        let offset = *self.offsets.get(&c.md5)?;
        let mut data = vec![0; c.size as usize];
        self.file.read_exact_at(&mut data, offset).ok()?;
        (fsutil::hex(&Md5::digest(&data)) == c.md5).then_some(data)
    }
}

fn local_chunks(path: &Path, file: &DepotFile) -> Result<Option<std::sync::Arc<LocalChunks>>> {
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file()) {
        return Ok(None);
    }
    let mut f =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    let mut offsets = HashMap::new();
    let mut offset = 0;
    let mut buf = Vec::new();
    for c in &file.chunks {
        buf.resize(c.size as usize, 0);
        if f.read_exact(&mut buf).is_err() {
            break;
        }
        offsets
            .entry(fsutil::hex(&Md5::digest(&buf)))
            .or_insert(offset);
        offset += c.size;
    }
    let wanted: HashSet<&str> = file.chunks.iter().map(|c| c.md5.as_str()).collect();
    offsets.retain(|md5, _| wanted.contains(md5.as_str()));
    Ok((!offsets.is_empty()).then(|| std::sync::Arc::new(LocalChunks { file: f, offsets })))
}

/// True when `path` already holds exactly this file, checked chunk by chunk.
pub fn verify_file(path: &Path, file: &DepotFile) -> Result<bool> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != file.size() {
        return Ok(false);
    }
    let mut f =
        std::fs::File::open(path).map_err(|e| Error::io(format!("open {}", path.display()), e))?;
    let mut buf = Vec::new();
    for c in &file.chunks {
        buf.resize(c.size as usize, 0);
        f.read_exact(&mut buf)
            .map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        if fsutil::hex(&Md5::digest(&buf)) != c.md5 {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn partial_dir(root: &Path, directory: &str) -> PathBuf {
    root.join(format!(".{directory}{PARTIAL_SUFFIX}"))
}

pub fn free_space(path: &Path) -> Result<u64> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() {
            break;
        }
    }
    let st = nix::sys::statvfs::statvfs(&probe)
        .map_err(|e| Error::io(format!("statvfs {}", probe.display()), e.into()))?;
    Ok(st.blocks_available() as u64 * st.fragment_size() as u64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallJob {
    pub game_id: String,
    pub build_id: String,
    pub language: String,
    pub root: PathBuf,
    pub directory: String,
    pub state: String,
    pub dlcs: Vec<String>,
}

impl InstallJob {
    pub fn save(&self, db: &Db) -> Result<()> {
        db.conn().execute(
            "INSERT INTO install_jobs (game_id, build_id, language, root, directory, state, updated_at, dlcs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(game_id) DO UPDATE SET build_id = excluded.build_id, language = excluded.language,
                root = excluded.root, directory = excluded.directory, state = excluded.state,
                updated_at = excluded.updated_at, dlcs = excluded.dlcs",
            params![
                self.game_id,
                self.build_id,
                self.language,
                self.root.to_string_lossy(),
                self.directory,
                self.state,
                Utc::now().timestamp(),
                self.dlcs.join(",")
            ],
        )?;
        Ok(())
    }

    pub fn load(db: &Db, game_id: &str) -> Result<Option<InstallJob>> {
        Ok(db
            .conn()
            .query_row(
                "SELECT game_id, build_id, language, root, directory, state, dlcs FROM install_jobs WHERE game_id = ?1",
                [game_id],
                |r| {
                    Ok(InstallJob {
                        game_id: r.get(0)?,
                        build_id: r.get(1)?,
                        language: r.get(2)?,
                        root: PathBuf::from(r.get::<_, String>(3)?),
                        directory: r.get(4)?,
                        state: r.get(5)?,
                        dlcs: r
                            .get::<_, String>(6)?
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                            .collect(),
                    })
                },
            )
            .optional()?)
    }

    pub fn list(db: &Db) -> Result<Vec<InstallJob>> {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT game_id FROM install_jobs ORDER BY updated_at")?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        drop(conn);
        ids.iter()
            .filter_map(|id| Self::load(db, id).transpose())
            .collect()
    }

    pub fn delete(db: &Db, game_id: &str) -> Result<()> {
        db.conn()
            .execute("DELETE FROM install_jobs WHERE game_id = ?1", [game_id])?;
        Ok(())
    }
}

/// Abandons an unfinished install: deletes its hidden partial folder and its job. An unfinished
/// update is refused here, since its files are the installed game itself.
pub fn discard(db: &Db, dirs: &Dirs, game_id: &str) -> Result<Option<PathBuf>> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let Some(job) = InstallJob::load(db, game_id)? else {
        return Ok(None);
    };
    if Install::get(db, game_id)?.is_some() {
        return Err(Error::Refused(format!(
            "{game_id} is installed; an unfinished update must be completed, not discarded"
        )));
    }
    let partial = partial_dir(&job.root, &job.directory);
    if partial.exists() {
        std::fs::remove_dir_all(&partial)
            .map_err(|e| Error::io(format!("delete {}", partial.display()), e))?;
    }
    InstallJob::delete(db, game_id)?;
    Ok(Some(partial))
}

/// Public build by default; a pinned build id (from an interrupted job) must still exist.
pub async fn plan_for(
    http: &reqwest::Client,
    tokens: &Tokens,
    game_id: &str,
    language: Option<&str>,
    build_id: Option<&str>,
    dlcs: &DlcSelection,
) -> Result<InstallPlan> {
    let builds = galaxy::builds(http, tokens, game_id).await?;
    let build = match build_id {
        Some(id) => builds.iter().find(|b| b.build_id == id).ok_or_else(|| {
            Error::NotFound(format!(
                "build {id} is no longer offered; restart the install"
            ))
        })?,
        None => builds
            .iter()
            .find(|b| b.branch.is_none())
            .or(builds.first())
            .ok_or_else(|| Error::Unsupported("no Windows Galaxy build for this game".into()))?,
    }
    .clone();
    let meta = galaxy::meta(http, &build).await?;
    let has_dlc = meta.products.iter().any(|p| p.product_id != game_id);
    let owned = if has_dlc {
        galaxy::owned_products(http, tokens).await?
    } else {
        HashSet::new()
    };
    let mut plan = InstallPlan::new(game_id, build, meta, language, &owned, dlcs)?;
    if !plan.meta.dependencies.is_empty() {
        let repository = galaxy::dependencies(http, tokens).await?;
        plan.dependencies = plan
            .meta
            .dependencies
            .iter()
            .filter_map(|id| repository.iter().find(|d| &d.dependency_id == id).cloned())
            .collect();
        for dep in plan.dependencies.iter().filter(|d| !d.is_shared()) {
            plan.depots.push(dep.depot());
            plan.download_size += dep.compressed_size;
            plan.disk_size += dep.size;
        }
    }
    Ok(plan)
}

/// Where a game's GOG support files (installer scripts) are kept.
pub fn support_dir(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("support").join(game_id)
}

pub struct InstallRequest {
    pub game_id: String,
    pub language: Option<String>,
    pub root: PathBuf,
    pub proton: PathBuf,
    pub dlcs: DlcSelection,
    pub restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallEvent {
    Planned {
        title: String,
        version: String,
        language: String,
        languages: Vec<String>,
        download_size: u64,
        disk_size: u64,
        target: PathBuf,
        resumed: bool,
    },
    Progress(Progress),
    Finished {
        path: PathBuf,
        support_files: usize,
        skipped_links: usize,
        dependencies: Vec<String>,
    },
}

/// What slatty wrote for a game. Its presence is what marks a game as installed by slatty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub build_id: String,
    pub version: String,
    pub language: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub dlcs: Vec<String>,
    /// Build whose post-install setup has run.
    #[serde(default)]
    pub setup_build: Option<String>,
    pub files: Vec<RecordedFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedFile {
    pub path: String,
    pub size: u64,
}

pub fn recorded_files(set: &FileSet) -> Vec<RecordedFile> {
    set.files
        .iter()
        .map(|(p, f)| RecordedFile {
            path: p.to_string_lossy().into_owned(),
            size: f.size(),
        })
        .collect()
}

impl InstallRecord {
    pub fn file(dirs: &Dirs, game_id: &str) -> PathBuf {
        dirs.data.join("manifests").join(format!("{game_id}.json"))
    }

    pub fn load(dirs: &Dirs, game_id: &str) -> Result<Option<InstallRecord>> {
        let path = Self::file(dirs, game_id);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|e| Error::parse("install record", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
        }
    }

    pub fn save(&self, dirs: &Dirs, game_id: &str) -> Result<()> {
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| Error::parse("install record", e))?;
        fsutil::write_atomic(&Self::file(dirs, game_id), &json)
    }
}

/// Downloads, verifies and registers a Windows build. An interrupted install is resumed with
/// the same build, language and folder unless `restart` is set.
pub async fn install(
    db: &Db,
    dirs: &Dirs,
    http: &reqwest::Client,
    tokens: &Tokens,
    req: InstallRequest,
    emit: impl Fn(InstallEvent) + Send + Sync,
    cancel: CancellationToken,
) -> Result<Install> {
    let _busy = crate::lock::game(dirs, &req.game_id)?;
    if Install::get(db, &req.game_id)?.is_some() {
        return Err(Error::Refused(format!(
            "{} is already installed",
            req.game_id
        )));
    }
    let previous = InstallJob::load(db, &req.game_id)?;
    if req.restart
        && let Some(job) = &previous
    {
        let _ = std::fs::remove_dir_all(partial_dir(&job.root, &job.directory));
        InstallJob::delete(db, &job.game_id)?;
    }
    let job = previous.filter(|_| !req.restart);
    let (language, build_id, root, dlcs) = match &job {
        Some(j) => (
            Some(j.language.as_str()),
            Some(j.build_id.as_str()),
            j.root.clone(),
            DlcSelection::Only(j.dlcs.clone()),
        ),
        None => (
            req.language.as_deref(),
            None,
            req.root.clone(),
            req.dlcs.clone(),
        ),
    };
    let plan = plan_for(http, tokens, &req.game_id, language, build_id, &dlcs).await?;
    let directory = plan.directory_name()?;
    let target = root.join(&directory);
    let partial = partial_dir(&root, &directory);
    emit(InstallEvent::Planned {
        title: plan.title.clone(),
        version: plan.build.version_name.clone(),
        language: plan.language.clone(),
        languages: plan.languages.clone(),
        download_size: plan.download_size,
        disk_size: plan.disk_size,
        target: target.clone(),
        resumed: job.is_some(),
    });
    let mut job = InstallJob {
        game_id: req.game_id.clone(),
        build_id: plan.build.build_id.clone(),
        language: plan.language.clone(),
        root: root.clone(),
        directory: directory.clone(),
        state: "downloading".into(),
        dlcs: plan.selected_dlcs(),
    };
    job.save(db)?;

    let result = async {
        let source = galaxy::GogContent::new(http.clone(), tokens.clone());
        let set = collect_files(&source, &plan.depots).await?;
        let progress = |p| emit(InstallEvent::Progress(p));
        let dl = Download {
            source: &source,
            cancel,
            progress: &progress,
            free_space: &free_space,
        };
        dl.run(&set, &partial, &target).await?;
        dl.check_installed(&set.support_set(), &support_dir(dirs, &req.game_id), true)
            .await?;
        Ok::<FileSet, Error>(set)
    }
    .await;
    let set = match result {
        Ok(set) => set,
        Err(e) => {
            job.state = if matches!(e, Error::Cancelled) {
                "paused".into()
            } else {
                "failed".into()
            };
            job.save(db)?;
            return Err(e);
        }
    };

    InstallRecord {
        build_id: plan.build.build_id.clone(),
        version: plan.build.version_name.clone(),
        language: plan.language.clone(),
        path: Some(target.clone()),
        dlcs: plan.selected_dlcs(),
        setup_build: None,
        files: recorded_files(&set),
    }
    .save(dirs, &req.game_id)?;

    let install = Install {
        game_id: req.game_id.clone(),
        title: plan.title.clone(),
        platform: Platform::Windows,
        path: target.clone(),
        client_id: plan.meta.client_id.clone(),
        runner: Runner::Umu {
            proton: req.proton,
            prefix: dirs.data.join("prefixes").join(&req.game_id),
        },
    };
    install.save(db)?;
    InstallJob::delete(db, &req.game_id)?;
    emit(InstallEvent::Finished {
        path: target,
        support_files: set.support.len(),
        skipped_links: set.skipped_links,
        dependencies: plan.meta.dependencies.clone(),
    });
    Ok(install)
}

#[cfg(test)]
mod tests;
