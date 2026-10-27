//! Downloading an Epic build into a folder: its chunks fetched in the order its files use them,
//! several at once, each file written from the parts it is made of beside its place, checked with
//! its SHA-1, then given its name. legendary does the same with a cache of chunks in shared memory
//! (`legendary/downloader/mp/manager.py`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures::StreamExt;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use super::manifest::{FileEntry, Guid, Manifest};
use crate::error::{Error, Result};
use crate::fsutil;
use crate::http;
use crate::installer::{Progress, refuse_outside, safe_relative};

/// Chunks downloaded at once.
const AT_ONCE: usize = 8;
/// Chunks kept for the files that use them again, at most: past this, the one used again the
/// latest is dropped, and downloaded again when its turn comes.
const CACHE_LIMIT: usize = 512 << 20;
const DOWNLOAD_SUFFIX: &str = ".slatty-dl";

/// Where chunks come from: Epic's CDN, or a test double.
pub trait ChunkSource: Send + Sync {
    /// A chunk as stored, at its address under the build's base address.
    fn chunk(&self, path: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

/// Epic's CDN, at the base address of a build. Chunks are asked for without the account's token.
pub struct EpicCdn {
    pub http: Client,
    pub base: String,
}

impl ChunkSource for EpicCdn {
    async fn chunk(&self, path: &str) -> Result<Vec<u8>> {
        let req = self
            .http
            .get(format!("{}/{path}", self.base))
            .header(reqwest::header::USER_AGENT, super::USER_AGENT);
        let limit = super::manifest::MAX_CHUNK as u64 + 4096;
        http::bytes_up_to(req, "downloading an Epic chunk", limit).await
    }
}

/// The files of a build, by their place in the game folder, in the order they are written.
#[derive(Debug, Default)]
pub struct BuildFiles {
    pub files: Vec<(PathBuf, FileEntry)>,
    /// Symbolic links the manifest lists, never made.
    pub skipped_links: usize,
}

impl BuildFiles {
    /// The files of `manifest`, each path checked to stay in the game folder. Windows treats
    /// differently cased paths as one file: the last listed of those is kept. They are written in
    /// the order legendary writes them, by path regardless of case, which keeps the chunks a few
    /// files share close together.
    pub fn of(manifest: &Manifest) -> Result<BuildFiles> {
        let mut set = BuildFiles::default();
        let mut index: HashMap<String, usize> = HashMap::new();
        for f in &manifest.files {
            if !f.symlink_target.is_empty() {
                set.skipped_links += 1;
                continue;
            }
            let rel = safe_relative(&f.name)?;
            // Names Slatty gives its own files there: a file written beside its place, the mark
            // of a folder being installed.
            let name = rel.file_name().map(|n| n.to_string_lossy());
            if name
                .as_deref()
                .is_some_and(|n| n.ends_with(DOWNLOAD_SUFFIX))
                || rel == Path::new(crate::installer::STAGED_MARK)
            {
                return Err(Error::Refused(format!(
                    "Epic's build names a file `{}` as Slatty names its own",
                    rel.display()
                )));
            }
            let key = rel.to_string_lossy().to_lowercase();
            match index.get(&key) {
                Some(&i) => set.files[i] = (rel, f.clone()),
                None => {
                    index.insert(key, set.files.len());
                    set.files.push((rel, f.clone()));
                }
            }
        }
        set.files
            .sort_by_cached_key(|(rel, _)| rel.to_string_lossy().to_lowercase());
        Ok(set)
    }

    pub fn disk_size(&self) -> u64 {
        self.files.iter().map(|(_, f)| f.size()).sum()
    }

    /// What downloading them all takes, as their chunks are sent (each once).
    pub fn download_size(&self, manifest: &Manifest) -> u64 {
        let chunks: HashSet<Guid> = self
            .files
            .iter()
            .flat_map(|(_, f)| f.parts.iter().map(|p| p.guid))
            .collect();
        chunks
            .iter()
            .filter_map(|g| manifest.chunks.get(g))
            .map(|c| c.file_size)
            .sum()
    }
}

pub struct Download<'a, S> {
    pub source: &'a S,
    pub manifest: &'a Manifest,
    /// The build's keys, for its encrypted chunks.
    pub secrets: &'a HashMap<String, String>,
    pub cancel: CancellationToken,
    pub progress: &'a (dyn Fn(Progress) + Send + Sync),
    pub free_space: &'a (dyn Fn(&Path) -> Result<u64> + Send + Sync),
}

impl<S: ChunkSource> Download<'_, S> {
    /// Fills `partial`, keeping the files an earlier attempt verified, then renames it to
    /// `target`. Nothing exists at `target` unless every file was verified.
    pub async fn run(&self, set: &BuildFiles, partial: &Path, target: &Path) -> Result<()> {
        if target.exists() {
            return Err(Error::Refused(format!(
                "{} already exists",
                target.display()
            )));
        }
        crate::paths::ensure_dir(partial)?;
        self.check_space(set, partial)?;
        self.fill(set, partial, true).await?;
        std::fs::rename(partial, target).map_err(|e| {
            Error::io(
                format!("publish {} as {}", partial.display(), target.display()),
                e,
            )
        })?;
        fsutil::sync_filesystem(target)
    }

    /// Checks the files of a game in place; with `repair`, downloads again those missing or wrong.
    /// The files that were.
    pub async fn check_installed(
        &self,
        set: &BuildFiles,
        dir: &Path,
        repair: bool,
    ) -> Result<Vec<PathBuf>> {
        let bad = self.bad_files(set, dir, false).await?;
        if repair && !bad.is_empty() {
            self.check_rewrite_space(set, &bad, dir)?;
            self.write(set, &bad, dir).await?;
            fsutil::sync_filesystem(dir)?;
        }
        Ok(bad.into_iter().map(|i| set.files[i].0.clone()).collect())
    }

    /// Brings the files of a game in place to `set`: checks them all, then calls `begin` and writes
    /// those missing or different. Nothing changes before `begin`; refused when the disk cannot
    /// hold them. The files written.
    pub async fn update_installed(
        &self,
        set: &BuildFiles,
        dir: &Path,
        begin: impl FnOnce() -> Result<()>,
    ) -> Result<Vec<PathBuf>> {
        let bad = self.bad_files(set, dir, false).await?;
        self.check_rewrite_space(set, &bad, dir)?;
        begin()?;
        self.write(set, &bad, dir).await?;
        fsutil::sync_filesystem(dir)?;
        Ok(bad.into_iter().map(|i| set.files[i].0.clone()).collect())
    }

    /// Refuses writing again the files of `set` at `which` in `dir` when the disk cannot hold it:
    /// each is written beside the one it replaces, which goes once it is renamed, so the most
    /// taken is what the files grow by, and the largest one replaced at once.
    fn check_rewrite_space(&self, set: &BuildFiles, which: &[usize], dir: &Path) -> Result<()> {
        let (mut grow, mut largest) = (0u64, 0u64);
        for &i in which {
            let (rel, f) = &set.files[i];
            let old = std::fs::symlink_metadata(dir.join(rel)).map_or(0, |m| m.len());
            grow += f.size().saturating_sub(old);
            largest = largest.max(old.min(f.size()));
        }
        self.require(grow + largest, dir)
    }

    fn require(&self, needed: u64, dir: &Path) -> Result<()> {
        let free = (self.free_space)(dir)?;
        if needed + needed / 50 > free {
            return Err(Error::Refused(format!(
                "not enough disk space: {} needed, {} free",
                crate::units::size(needed),
                crate::units::size(free)
            )));
        }
        Ok(())
    }

    fn check_space(&self, set: &BuildFiles, dir: &Path) -> Result<()> {
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
        self.require(set.disk_size().saturating_sub(present), dir)
    }

    async fn fill(&self, set: &BuildFiles, dir: &Path, resumed: bool) -> Result<()> {
        let bad = self.bad_files(set, dir, resumed).await?;
        self.write(set, &bad, dir).await
    }

    /// The files of `set` not already in `dir` as they should be, by their place in `set`. With
    /// `resumed`, `dir` is a partial folder: a file synced there is taken on its size. Progress
    /// counts the files found right, where writing the others then goes on from.
    async fn bad_files(&self, set: &BuildFiles, dir: &Path, resumed: bool) -> Result<Vec<usize>> {
        let files: Vec<(PathBuf, [u8; 20], u64)> = set
            .files
            .iter()
            .map(|(rel, f)| (dir.join(rel), f.sha1, f.size()))
            .collect();
        let cancel = self.cancel.clone();
        let (good_tx, mut good_rx) = tokio::sync::mpsc::unbounded_channel();
        let checking = tokio::spawn(fsutil::in_background(move || {
            let mut bad = Vec::new();
            for (i, (path, sha1, size)) in files.iter().enumerate() {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let good = (resumed && crate::installer::synced(path, *size))
                    || verify_file(path, sha1, *size, &cancel)?;
                if good {
                    let _ = good_tx.send(*size);
                } else {
                    bad.push(i);
                }
            }
            Ok(bad)
        }));
        let mut done = Progress {
            files_total: set.files.len(),
            bytes_total: set.disk_size(),
            ..Default::default()
        };
        (self.progress)(done);
        // Ends once the check has, which drops its sender.
        while let Some(size) = good_rx.recv().await {
            done.files_done += 1;
            done.bytes_done += size;
            (self.progress)(done);
        }
        checking.await.expect("file check panicked")
    }

    /// Writes the files of `set` at the places `which` gives, in order.
    async fn write(&self, set: &BuildFiles, which: &[usize], dir: &Path) -> Result<()> {
        let total = Progress {
            files_total: set.files.len(),
            bytes_total: set.disk_size(),
            files_done: set.files.len() - which.len(),
            bytes_done: set.disk_size() - which.iter().map(|&i| set.files[i].1.size()).sum::<u64>(),
            network_bytes: 0,
        };
        let mut done = total;
        (self.progress)(done);

        // Every part to write, in order, and the chunks they need, in the order first needed.
        let uses: Vec<Guid> = which
            .iter()
            .flat_map(|&i| set.files[i].1.parts.iter().map(|p| p.guid))
            .collect();
        let mut pending: HashMap<Guid, VecDeque<usize>> = HashMap::new();
        let mut order = Vec::new();
        for (n, g) in uses.iter().enumerate() {
            let at = pending.entry(*g).or_default();
            if at.is_empty() {
                order.push(*g);
            }
            at.push_back(n);
        }
        let network = AtomicU64::new(0);
        let network = &network;
        let mut stream = futures::stream::iter(order.clone())
            .map(|g| self.fetch(g, network))
            .buffered(AT_ONCE);
        let mut next_new = 0;
        let mut cache = Cache::default();
        let mut n = 0;

        for &i in which {
            let (rel, file) = &set.files[i];
            let dest = dir.join(rel);
            refuse_outside(dir, &dest)?;
            if let Some(parent) = dest.parent() {
                crate::paths::ensure_dir(parent)?;
            }
            let tmp = dest.with_file_name(format!(
                "{}{DOWNLOAD_SUFFIX}",
                dest.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
            let out = fsutil::restart_staging(&tmp)?;
            let written = async {
                let mut hash =
                    aws_lc_rs::digest::Context::new(&aws_lc_rs::digest::SHA1_FOR_LEGACY_USE_ONLY);
                let mut at = 0u64;
                for part in &file.parts {
                    if self.cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    let g = part.guid;
                    let data = match cache.take(&g) {
                        Some(data) => data,
                        None if order.get(next_new) == Some(&g) => {
                            next_new += 1;
                            self.unless_cancelled(async {
                                stream.next().await.expect("one chunk for each")
                            })
                            .await?
                        }
                        // Dropped from the cache: downloaded again.
                        None => self.unless_cancelled(self.fetch(g, network)).await?,
                    };
                    let slice = &data[part.offset as usize..(part.offset + part.size) as usize];
                    out.write_all_at(slice, at)
                        .map_err(|e| Error::io(format!("write {}", tmp.display()), e))?;
                    hash.update(slice);
                    at += part.size as u64;
                    let uses = pending.get_mut(&g).expect("counted");
                    uses.pop_front();
                    if let Some(&next) = uses.front() {
                        cache.keep(g, data, next);
                    }
                    n += 1;
                    done.bytes_done += part.size as u64;
                    done.network_bytes = network.load(Ordering::Relaxed);
                    (self.progress)(done);
                }
                if hash.finish().as_ref() != file.sha1 {
                    return Err(Error::Refused(format!(
                        "{} does not match its checksum: Epic sent damaged data",
                        rel.display()
                    )));
                }
                if file.size() >= crate::installer::SYNCED_FROM {
                    out.sync_all()
                        .map_err(|e| Error::io(format!("write {}", tmp.display()), e))?;
                }
                Ok(())
            }
            .await;
            drop(out);
            written.inspect_err(|_| drop(std::fs::remove_file(&tmp)))?;
            if file.is_executable() {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
            }
            std::fs::rename(&tmp, &dest)
                .map_err(|e| Error::io(format!("rename {}", tmp.display()), e))?;
            done.files_done += 1;
            (self.progress)(done);
        }
        debug_assert_eq!(n, uses.len());
        Ok(())
    }

    /// `work`, stopped as soon as the download is: a chunk waiting on the network (its retries
    /// take a minute and a half) does not hold a pause.
    async fn unless_cancelled<T>(&self, work: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(Error::Cancelled),
            done = work => done,
        }
    }

    /// A chunk, downloaded and unpacked.
    async fn fetch(&self, g: Guid, network: &AtomicU64) -> Result<Arc<Vec<u8>>> {
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let info = self
            .manifest
            .chunks
            .get(&g)
            .expect("checked as read")
            .clone();
        let raw = self
            .source
            .chunk(&info.path(self.manifest.feature_level))
            .await?;
        network.fetch_add(raw.len() as u64, Ordering::Relaxed);
        let secrets = self.secrets.clone();
        tokio::task::spawn_blocking(move || super::chunk::unpack(&raw, &info, &secrets))
            .await
            .expect("chunk task panicked")
            .map(Arc::new)
    }
}

/// The chunks used again later, with the place of their next use; bounded by `CACHE_LIMIT`.
#[derive(Default)]
struct Cache {
    chunks: HashMap<Guid, (Arc<Vec<u8>>, usize)>,
    bytes: usize,
}

impl Cache {
    fn take(&mut self, g: &Guid) -> Option<Arc<Vec<u8>>> {
        let (data, _) = self.chunks.remove(g)?;
        self.bytes -= data.len();
        Some(data)
    }

    fn keep(&mut self, g: Guid, data: Arc<Vec<u8>>, next: usize) {
        self.bytes += data.len();
        self.chunks.insert(g, (data, next));
        while self.bytes > CACHE_LIMIT {
            let latest = *self
                .chunks
                .iter()
                .max_by_key(|(_, (_, next))| *next)
                .expect("not empty while over the limit")
                .0;
            self.take(&latest);
        }
    }
}

/// Whether `path` holds exactly the file of that SHA-1 and size. One that cannot be read (a bad
/// sector, no permission) is not: repairing writes it again. Stops when `cancel` is, between
/// reads.
pub fn verify_file(
    path: &Path,
    sha1: &[u8; 20],
    size: u64,
    cancel: &CancellationToken,
) -> Result<bool> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(false);
    };
    if !meta.is_file() || meta.len() != size {
        return Ok(false);
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return Ok(false);
    };
    let mut hash = aws_lc_rs::digest::Context::new(&aws_lc_rs::digest::SHA1_FOR_LEGACY_USE_ONLY);
    let mut buf = vec![0; 1 << 20];
    loop {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let Ok(n) = f.read(&mut buf) else {
            return Ok(false);
        };
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finish().as_ref() == sha1)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::epic::chunk;
    use crate::epic::manifest::{ChunkInfo, ChunkPart};

    /// Epic's CDN in memory: chunks by address, the addresses asked for, and those that fail.
    #[derive(Default)]
    pub(crate) struct FakeCdn {
        pub chunks: HashMap<String, Vec<u8>>,
        pub asked: Mutex<Vec<String>>,
        pub broken: Mutex<HashSet<String>>,
        /// Chunks it never answers for, as a stalled connection.
        pub stalled: Mutex<HashSet<String>>,
    }

    impl ChunkSource for FakeCdn {
        async fn chunk(&self, path: &str) -> Result<Vec<u8>> {
            self.asked.lock().unwrap().push(path.to_string());
            if self.stalled.lock().unwrap().contains(path) {
                std::future::pending::<()>().await;
            }
            if self.broken.lock().unwrap().contains(path) {
                return Err(Error::Http {
                    context: "downloading an Epic chunk",
                    status: 503,
                });
            }
            self.chunks.get(path).cloned().ok_or(Error::Http {
                context: "downloading an Epic chunk",
                status: 404,
            })
        }
    }

    /// A build of these files (path, content), cut into chunks of `chunk` bytes as Epic packs
    /// them: the files one after the other, so that a chunk holds the end of one and the start
    /// of the next. Its manifest, and the CDN serving its chunks.
    pub(crate) fn build(files: &[(&str, Vec<u8>)], chunk: usize) -> (Manifest, FakeCdn) {
        let stream: Vec<u8> = files.iter().flat_map(|(_, c)| c.clone()).collect();
        let mut cdn = FakeCdn::default();
        let mut chunks = HashMap::new();
        for (n, data) in stream.chunks(chunk).enumerate() {
            let info = ChunkInfo {
                guid: [n as u32 + 1, 0xFA4E, 0, 0],
                hash: n as u64,
                group: (n % 100) as u8,
                window_size: data.len() as u32,
                file_size: 0,
                secret: None,
            };
            let packed = chunk::pack(&info, data, None);
            let info = ChunkInfo {
                file_size: packed.len() as u64,
                ..info
            };
            cdn.chunks.insert(info.path(18), packed);
            chunks.insert(info.guid, info);
        }
        let mut at = 0usize;
        let entries = files
            .iter()
            .map(|(name, content)| {
                let mut parts = Vec::new();
                let mut left = content.len();
                while left > 0 {
                    let n = at / chunk;
                    let offset = at % chunk;
                    let size = left.min(chunk - offset);
                    parts.push(ChunkPart {
                        guid: [n as u32 + 1, 0xFA4E, 0, 0],
                        offset: offset as u32,
                        size: size as u32,
                    });
                    at += size;
                    left -= size;
                }
                FileEntry {
                    name: name.to_string(),
                    symlink_target: String::new(),
                    sha1: crate::epic::sha1(content),
                    flags: 0,
                    parts,
                }
            })
            .collect();
        let manifest = Manifest {
            feature_level: 18,
            app_name: "Fake".into(),
            build_version: "1.0-[FAKE]".into(),
            launch_exe: "Game.exe".into(),
            launch_command: String::new(),
            prerequisites: None,
            chunks,
            files: entries,
        };
        (manifest, cdn)
    }

    pub(crate) fn content(seed: u8, len: usize) -> Vec<u8> {
        (0..len)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    fn root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("slatty-epic-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn plenty(_: &Path) -> Result<u64> {
        Ok(u64::MAX / 4)
    }

    fn sample() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("Game.exe", content(1, 2500)),
            ("Data/small.txt", content(2, 10)),
            ("Data/Pak/big.pak", content(3, 5000)),
            ("Empty.ini", Vec::new()),
        ]
    }

    async fn install(
        manifest: &Manifest,
        cdn: &FakeCdn,
        partial: &Path,
        target: &Path,
    ) -> Result<()> {
        let set = BuildFiles::of(manifest)?;
        let secrets = HashMap::new();
        Download {
            source: cdn,
            manifest,
            secrets: &secrets,
            cancel: CancellationToken::new(),
            progress: &|_| {},
            free_space: &plenty,
        }
        .run(&set, partial, target)
        .await
    }

    /// Every file is written whole, from parts of chunks shared with its neighbours, each chunk
    /// downloaded once.
    #[tokio::test]
    async fn a_build_is_written_file_by_file_from_shared_chunks() {
        let root = root("whole");
        let files = sample();
        let (manifest, cdn) = build(&files, 1024);
        let (partial, target) = (root.join(".Game.part"), root.join("Game"));
        install(&manifest, &cdn, &partial, &target).await.unwrap();
        for (name, content) in &files {
            assert_eq!(
                &std::fs::read(target.join(name)).unwrap(),
                content,
                "{name}"
            );
        }
        assert!(!partial.exists());
        let asked = cdn.asked.lock().unwrap();
        let unique: HashSet<&String> = asked.iter().collect();
        assert_eq!(asked.len(), unique.len(), "each chunk once");
        assert_eq!(asked.len(), cdn.chunks.len());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A chunk the server damaged leaves no file in its place and no game folder.
    #[tokio::test]
    async fn damaged_data_never_reaches_the_game_folder() {
        let root = root("damaged");
        let (mut manifest, cdn) = build(&sample(), 1024);
        manifest.files[2].sha1 = [0; 20];
        let (partial, target) = (root.join(".Game.part"), root.join("Game"));
        let e = install(&manifest, &cdn, &partial, &target)
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");
        assert!(!target.exists());
        assert!(!partial.join("Data/Pak/big.pak").exists());
        assert!(!partial.join("Data/Pak/big.pak.slatty-dl").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An interrupted download keeps the files it verified: resumed, it downloads only the chunks
    /// of the others.
    #[tokio::test]
    async fn a_resumed_download_keeps_what_it_verified() {
        let root = root("resumed");
        let files = vec![
            ("A.bin", content(1, 3000)),
            ("B.bin", content(2, 3000)),
            ("C.bin", content(3, 3000)),
        ];
        let (manifest, cdn) = build(&files, 1000);
        let (partial, target) = (root.join(".Game.part"), root.join("Game"));
        // The chunks of the last file fail: the first two are written and verified.
        let last: Vec<String> = manifest.files[2]
            .parts
            .iter()
            .map(|p| manifest.chunks[&p.guid].path(18))
            .collect();
        cdn.broken.lock().unwrap().extend(last.iter().cloned());
        assert!(install(&manifest, &cdn, &partial, &target).await.is_err());
        assert!(partial.join("A.bin").is_file() && partial.join("B.bin").is_file());

        cdn.broken.lock().unwrap().clear();
        cdn.asked.lock().unwrap().clear();
        install(&manifest, &cdn, &partial, &target).await.unwrap();
        let asked = cdn.asked.lock().unwrap().clone();
        assert_eq!(asked, last, "the last file's chunks alone");
        assert_eq!(std::fs::read(target.join("C.bin")).unwrap(), files[2].1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A manifest's path leading out of the game folder, or a link on the way, is refused.
    #[tokio::test]
    async fn paths_out_of_the_game_folder_are_refused() {
        let (mut manifest, _) = build(&sample(), 1024);
        manifest.files[0].name = "../escape.exe".into();
        assert!(BuildFiles::of(&manifest).is_err());

        let root = root("link");
        let outside = root.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let (manifest, cdn) = build(&sample(), 1024);
        let partial = root.join(".Game.part");
        std::fs::create_dir_all(&partial).unwrap();
        std::os::unix::fs::symlink(&outside, partial.join("Data")).unwrap();
        let e = install(&manifest, &cdn, &partial, &root.join("Game"))
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Past the cache's limit, a chunk used again much later is downloaded again rather than
    /// kept; the files are the same.
    #[test]
    fn the_cache_drops_the_chunk_used_again_the_latest() {
        let mut cache = Cache::default();
        let big = Arc::new(vec![0u8; CACHE_LIMIT / 2 + 1]);
        cache.keep([1; 4], big.clone(), 10);
        cache.keep([2; 4], big.clone(), 5);
        assert!(cache.take(&[1; 4]).is_none(), "used again the latest");
        assert!(cache.take(&[2; 4]).is_some());
        assert_eq!(cache.bytes, 0);
    }

    fn download<'a>(
        manifest: &'a Manifest,
        cdn: &'a FakeCdn,
        secrets: &'a HashMap<String, String>,
        free: &'a (dyn Fn(&Path) -> Result<u64> + Send + Sync),
        cancel: CancellationToken,
    ) -> Download<'a, FakeCdn> {
        Download {
            source: cdn,
            manifest,
            secrets,
            cancel,
            progress: &|_| {},
            free_space: free,
        }
    }

    /// A file that cannot be read is one to repair, not a reason to stop: Verify lists it, Repair
    /// writes it again.
    #[tokio::test]
    async fn an_unreadable_file_is_repaired() {
        use std::os::unix::fs::PermissionsExt;
        let root = root("unreadable");
        let (manifest, cdn) = build(&sample(), 1024);
        let game = root.join("Game");
        install(&manifest, &cdn, &root.join(".Game.part"), &game)
            .await
            .unwrap();
        let locked = game.join("Data/small.txt");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::File::open(&locked).is_ok() {
            // Run as root, which reads anything: nothing to check.
            std::fs::remove_dir_all(&root).unwrap();
            return;
        }
        let set = BuildFiles::of(&manifest).unwrap();
        let secrets = HashMap::new();
        let dl = download(&manifest, &cdn, &secrets, &plenty, CancellationToken::new());
        let bad = dl.check_installed(&set, &game, false).await.unwrap();
        assert_eq!(bad, [PathBuf::from("Data/small.txt")]);
        dl.check_installed(&set, &game, true).await.unwrap();
        assert_eq!(std::fs::read(&locked).unwrap(), sample()[1].1);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Pausing while a chunk waits on the network stops at once, not after its retries.
    #[tokio::test]
    async fn a_pause_does_not_wait_for_a_stalled_chunk() {
        let root = root("stalled");
        let (manifest, cdn) = build(&sample(), 1024);
        let first = manifest.chunks.values().next().unwrap().path(18);
        cdn.stalled.lock().unwrap().insert(first);
        let set = BuildFiles::of(&manifest).unwrap();
        let secrets = HashMap::new();
        let cancel = CancellationToken::new();
        let dl = download(&manifest, &cdn, &secrets, &plenty, cancel.clone());
        let stop = async {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            cancel.cancel();
        };
        let (partial, target) = (root.join(".Game.part"), root.join("Game"));
        let (ran, ()) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(dl.run(&set, &partial, &target), stop)
        })
        .await
        .expect("stopped at once");
        assert!(matches!(ran, Err(Error::Cancelled)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A changed file of the same size is written beside the one it replaces: an update needs
    /// room for it, though its size does not change.
    #[tokio::test]
    async fn an_update_needs_room_for_a_file_beside_the_one_it_replaces() {
        let root = root("room");
        let (manifest, cdn) = build(&sample(), 1024);
        let game = root.join("Game");
        install(&manifest, &cdn, &root.join(".Game.part"), &game)
            .await
            .unwrap();
        let mut files = sample();
        files[2].1 = content(9, 5000);
        let (newer, cdn) = build(&files, 1024);
        let set = BuildFiles::of(&newer).unwrap();
        let secrets = HashMap::new();
        let full = |_: &Path| Ok(100);
        let dl = download(&newer, &cdn, &secrets, &full, CancellationToken::new());
        let e = dl
            .update_installed(&set, &game, || Ok(()))
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");
        assert_eq!(
            std::fs::read(game.join("Data/Pak/big.pak")).unwrap(),
            sample()[2].1
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A build naming a file as Slatty names its own (written beside its place, the mark of a
    /// folder being installed) is refused: one would be taken for the other.
    #[test]
    fn names_slatty_gives_its_own_files_are_refused() {
        for name in ["Data/x.slatty-dl", ".slatty-staged"] {
            let (manifest, _) = build(&[("Game.exe", content(1, 5)), (name, content(2, 5))], 64);
            assert!(
                matches!(BuildFiles::of(&manifest), Err(Error::Refused(_))),
                "{name}"
            );
        }
    }

    /// Two paths differing by case are one file on Windows: the last listed is written.
    #[test]
    fn paths_differing_by_case_are_one_file() {
        let (manifest, _) = build(&[("a.txt", content(1, 5)), ("A.TXT", content(2, 5))], 64);
        let set = BuildFiles::of(&manifest).unwrap();
        assert_eq!(set.files.len(), 1);
        assert_eq!(set.files[0].1.sha1, crate::epic::sha1(&content(2, 5)));
    }
}
