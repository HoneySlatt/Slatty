//! Verified downloads: the files of a build, written chunk by chunk, checked twice, reusing what
//! is already on disk.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use flate2::read::ZlibDecoder;
use futures::StreamExt;
use md5::{Digest, Md5};
use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::fsutil;
use crate::galaxy::{ContentSource, Depot, DepotFile, DepotItem};

const FILE_CONCURRENCY: usize = 4;
const CHUNK_CONCURRENCY: usize = 2;
const DOWNLOAD_SUFFIX: &str = ".slatty-dl";

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
