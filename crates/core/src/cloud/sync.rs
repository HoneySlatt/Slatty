use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use rusqlite::{OptionalExtension, params};

use super::plan::{self, Action, BaseEntry, ConflictKind, LocalFile, Plan, RemoteFile};
use super::scan::{self, LocalScan};
use super::transport::{CloudTransport, IGNORED_REMOTE_HASH, RemoteEntry};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::fsutil;
use crate::gameinfo::resolve_relative;
use crate::lock;
use crate::paths::Dirs;

pub struct SyncTarget<'a> {
    pub db: &'a Db,
    pub dirs: &'a Dirs,
    pub user_id: &'a str,
    pub game_id: &'a str,
    pub location: &'a str,
    pub root: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefer {
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    pub dry_run: bool,
    pub allow_deletions: bool,
    pub prefer: Option<Prefer>,
}

#[derive(Debug, Default)]
pub struct SyncReport {
    pub plan: Plan,
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub adopted: Vec<String>,
    pub deleted_remote: Vec<String>,
    pub deleted_local: Vec<String>,
    pub conflicts: Vec<(String, ConflictKind)>,
    pub pending_deletions: Vec<String>,
    pub refused: Vec<(String, String)>,
    pub errors: Vec<(String, String)>,
    pub skipped_local: Vec<String>,
    /// Cloud files left out, and the local file of the same name with them: a name a save folder
    /// cannot hold safely (`..`, absolute), or two that differ only by case (one file on Windows).
    /// Reported without making the sync unclean, so it does not block a launch.
    pub skipped_remote: Vec<String>,
    pub backup_dir: Option<PathBuf>,
}

impl SyncReport {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.refused.is_empty() && self.errors.is_empty()
    }
}

pub(crate) struct RemoteRef {
    pub name: String,
    pub rel: String,
    pub hash: String,
    pub last_modified: Option<String>,
}

enum Deferred {
    Upload,
    /// `chosen` when it is the side of a conflict the user chose: the choice is the permission.
    DeleteRemote {
        chosen: bool,
    },
}

struct Run<'a, T> {
    cloud: &'a T,
    target: &'a SyncTarget<'a>,
    opts: SyncOptions,
    local: LocalScan,
    remote: BTreeMap<String, RemoteRef>,
    report: SyncReport,
    deferred: Vec<(String, Deferred)>,
    backup_root: PathBuf,
}

pub async fn sync<T: CloudTransport>(
    cloud: &T,
    target: &SyncTarget<'_>,
    opts: SyncOptions,
) -> Result<SyncReport> {
    let lock_path = target
        .dirs
        .locks()
        .join(format!("cloud-{}-{}.lock", target.user_id, target.game_id));
    // A dry run writes nothing, and the game page starts one by itself: it must not refuse the
    // sync of a launch while it waits on the network.
    let _lock = if opts.dry_run {
        None
    } else {
        // A process another thread starts holds a copy of the lock until it runs its program, so
        // one just released can look held for an instant.
        match lock::acquire(&lock_path, std::time::Duration::from_secs(1)).await {
            Err(Error::Refused(_)) => {
                return Err(Error::Refused(
                    "a cloud sync for this game is already running".into(),
                ));
            }
            lock => Some(lock?),
        }
    };

    let local = scan::scan(target.root)?;
    let (remote, skipped_remote) = remote_map(cloud.list().await?, target.location);
    // The local file of a name left out in the cloud is left alone too: neither uploaded over
    // one of the pair, nor deleted as if the cloud had deleted it.
    let left_out: std::collections::BTreeSet<String> =
        skipped_remote.iter().map(|rel| scan::key(rel)).collect();
    let stored_root = stored_root(target)?;
    let root_changed = match stored_root {
        Some(stored) => {
            Path::new(&stored.root) != target.root
                || synced_by_another(target, stored.last_sync_at)?
                || stored
                    .identity
                    .zip(folder_identity(target.root))
                    .is_some_and(|(was, is)| was != is)
        }
        None => false,
    };
    // History recorded for another folder says nothing about this one.
    let mut base = if root_changed {
        BTreeMap::new()
    } else {
        load_base(target)?
    };
    base.retain(|k, _| !left_out.contains(k));

    let local_view: BTreeMap<String, LocalFile> = local
        .files
        .iter()
        .filter(|(k, _)| !left_out.contains(*k))
        .map(|(k, f)| {
            (
                k.clone(),
                LocalFile {
                    sha256: f.sha256.clone(),
                    size: f.size,
                },
            )
        })
        .collect();
    let remote_view: BTreeMap<String, RemoteFile> = remote
        .iter()
        .map(|(k, r)| {
            (
                k.clone(),
                RemoteFile {
                    hash: r.hash.clone(),
                },
            )
        })
        .collect();
    let plan = plan::plan(&plan::Inputs {
        local: &local_view,
        local_root_exists: local.root_exists,
        remote: &remote_view,
        base: &base,
        root_changed,
    });

    static NEXT_BACKUP: AtomicU64 = AtomicU64::new(0);
    let stamp = format!(
        "{}-{}-{}",
        Utc::now().format("%Y%m%d-%H%M%S-%f"),
        std::process::id(),
        NEXT_BACKUP.fetch_add(1, Ordering::Relaxed)
    );
    let mut run = Run {
        cloud,
        target,
        opts,
        report: SyncReport {
            skipped_local: local.skipped.clone(),
            skipped_remote,
            plan: plan.clone(),
            ..Default::default()
        },
        local,
        remote,
        deferred: Vec::new(),
        backup_root: target
            .dirs
            .backups(target.user_id, target.game_id)
            .join(stamp)
            .join(target.location),
    };
    if opts.dry_run {
        return Ok(run.report);
    }
    if root_changed {
        clear_base(target)?;
    }
    for (key, action) in &plan.files {
        if let Err(e) = run.local_phase(key, *action).await {
            run.report.errors.push((run.display(key), e.to_string()));
        }
    }
    run.remote_phase().await?;
    save_root(target)?;
    Ok(run.report)
}

impl<T: CloudTransport> Run<'_, T> {
    fn display(&self, key: &str) -> String {
        self.local
            .files
            .get(key)
            .map(|f| f.rel.clone())
            .or_else(|| self.remote.get(key).map(|r| r.rel.clone()))
            .unwrap_or_else(|| key.to_string())
    }

    async fn local_phase(&mut self, key: &str, action: Action) -> Result<()> {
        let plan = &self.report.plan;
        match action {
            Action::Keep => {}
            Action::Upload => self.deferred.push((key.into(), Deferred::Upload)),
            Action::DeleteRemote => {
                let delete = Deferred::DeleteRemote { chosen: false };
                self.deferred.push((key.into(), delete))
            }
            Action::Download => self.download_replace(key).await?,
            Action::ForgetBase => delete_base(self.target, key)?,
            Action::DeleteLocal => {
                if self.opts.allow_deletions && !plan.local_deletions_blocked() {
                    self.delete_local(key)?;
                } else {
                    self.report.pending_deletions.push(self.display(key));
                }
            }
            Action::Compare => self.compare(key, ConflictKind::Unrelated).await?,
            Action::Conflict(ConflictKind::BothModified) => {
                self.compare(key, ConflictKind::BothModified).await?
            }
            Action::Conflict(kind) => self.resolve(key, kind).await?,
        }
        Ok(())
    }

    /// Adopts a file both sides hold with the same content; else it is a conflict of `kind`.
    async fn compare(&mut self, key: &str, kind: ConflictKind) -> Result<()> {
        let remote = &self.remote[key];
        let bytes = self.cloud.download(&remote.name).await?;
        let sha = fsutil::sha256_hex(&bytes);
        if sha != self.local.files[key].sha256 {
            return self.resolve(key, kind).await;
        }
        let hash = remote.hash.clone();
        self.record(key, &sha, &hash)?;
        self.report.adopted.push(self.display(key));
        Ok(())
    }

    async fn resolve(&mut self, key: &str, kind: ConflictKind) -> Result<()> {
        let plan = &self.report.plan;
        match (self.opts.prefer, kind) {
            (None, _) => self.report.conflicts.push((self.display(key), kind)),
            (Some(Prefer::Remote), ConflictKind::LocalModifiedRemoteDeleted) => {
                if plan.local_deletions_blocked() {
                    self.report.conflicts.push((self.display(key), kind));
                } else {
                    // Choosing the cloud's side of this conflict is the permission; the local
                    // copy is backed up first.
                    self.delete_local(key)?;
                }
            }
            (Some(Prefer::Remote), _) => self.download_replace(key).await?,
            (Some(Prefer::Local), ConflictKind::LocalModifiedRemoteDeleted) => {
                self.deferred.push((key.into(), Deferred::Upload))
            }
            (Some(Prefer::Local), ConflictKind::LocalDeletedRemoteModified) => {
                if plan.remote_deletions_blocked() {
                    self.report.conflicts.push((self.display(key), kind));
                } else {
                    // Choosing the local side of this conflict is the permission; the cloud copy
                    // is kept first.
                    let delete = Deferred::DeleteRemote { chosen: true };
                    self.deferred.push((key.into(), delete));
                }
            }
            (Some(Prefer::Local), _) => self.deferred.push((key.into(), Deferred::Upload)),
        }
        Ok(())
    }

    async fn download_replace(&mut self, key: &str) -> Result<()> {
        let RemoteRef { name, hash, .. } = &self.remote[key];
        let (name, hash) = (name.clone(), hash.clone());
        let bytes = self.cloud.download(&name).await?;
        let dest = self.unchanged_local(key)?;
        if dest.exists() {
            self.backup(&dest, "local", key)?;
        }
        fsutil::write_atomic(&dest, &bytes)?;
        self.record(key, &fsutil::sha256_hex(&bytes), &hash)?;
        self.report.downloaded.push(self.display(key));
        Ok(())
    }

    async fn keep_remote_copy(&mut self, key: &str) -> Result<()> {
        let remote = &self.remote[key];
        let bytes = self.cloud.download(&remote.name).await?;
        let dest = resolve_relative(&self.backup_root.join("remote"), &remote.rel)?;
        fsutil::write_atomic(&dest, &bytes)?;
        self.report.backup_dir = Some(self.backup_root.clone());
        Ok(())
    }

    fn delete_local(&mut self, key: &str) -> Result<()> {
        let abs = self.unchanged_local(key)?;
        self.backup(&abs, "local", key)?;
        std::fs::remove_file(&abs)
            .map_err(|e| Error::io(format!("delete {}", abs.display()), e))?;
        delete_base(self.target, key)?;
        self.report.deleted_local.push(self.display(key));
        Ok(())
    }

    /// Links below the save root were skipped by the scan, not treated as absent files.
    fn local_path(&self, key: &str) -> Result<PathBuf> {
        let path = resolve_relative(self.target.root, &self.display(key))?;
        let mut current = self.target.root.to_path_buf();
        for component in path
            .strip_prefix(self.target.root)
            .expect("relative save path")
            .components()
        {
            current.push(component);
            match std::fs::symlink_metadata(&current) {
                Ok(meta) if meta.is_symlink() => {
                    return Err(Error::Refused(
                        "a save path contains a symbolic link".into(),
                    ));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::io(format!("stat {}", current.display()), e)),
            }
        }
        Ok(path)
    }

    /// Rechecks after network waits, before replacing or deleting a save.
    fn unchanged_local(&self, key: &str) -> Result<PathBuf> {
        let path = self.local_path(key)?;
        let actual = match scan::hash_file(&path) {
            Ok((sha, _)) => Some(sha),
            Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if actual.as_ref() != self.local.files.get(key).map(|f| &f.sha256) {
            return Err(Error::Refused("the file changed during the sync".into()));
        }
        Ok(path)
    }

    fn backup(&mut self, file: &Path, side: &str, key: &str) -> Result<()> {
        let dest = resolve_relative(&self.backup_root.join(side), &self.display(key))?;
        if let Some(parent) = dest.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        // On the disk before the save is replaced or deleted: a power cut must not leave neither.
        std::fs::copy(file, &dest)
            .and_then(|_| std::fs::File::open(&dest)?.sync_all())
            .map_err(|e| Error::io(format!("back up {}", file.display()), e))?;
        self.report.backup_dir = Some(self.backup_root.clone());
        Ok(())
    }

    fn record(&self, key: &str, local_sha: &str, remote_hash: &str) -> Result<()> {
        upsert_base(self.target, key, local_sha, remote_hash)
    }

    /// Remote writes run last, against a fresh listing, so a concurrent change is never overwritten.
    async fn remote_phase(&mut self) -> Result<()> {
        if self.deferred.is_empty() {
            return Ok(());
        }
        let (fresh, _) = remote_map(self.cloud.list().await?, self.target.location);
        let mut uploaded = Vec::new();
        for (key, op) in std::mem::take(&mut self.deferred) {
            let planned = self.remote.get(&key).map(|r| &r.hash);
            if fresh.get(&key).map(|r| &r.hash) != planned {
                self.report.refused.push((
                    self.display(&key),
                    "changed in the cloud during the sync".into(),
                ));
                continue;
            }
            let result = match op {
                Deferred::Upload => self
                    .upload(&key)
                    .await
                    .map(|sha| uploaded.push((key.clone(), sha))),
                Deferred::DeleteRemote { chosen } => self.delete_remote(&key, chosen).await,
            };
            if let Err(e) = result {
                self.report.errors.push((self.display(&key), e.to_string()));
            }
        }
        if uploaded.is_empty() {
            return Ok(());
        }
        // Unconfirmed uploads get no history: the next sync finds the same content on both sides.
        let listed = (self.cloud.list().await).map(|l| remote_map(l, self.target.location).0);
        let after = match listed {
            Ok(after) => after,
            Err(e) => {
                for (key, _) in uploaded {
                    let error = format!("upload not confirmed: {e}");
                    self.report.errors.push((self.display(&key), error));
                }
                return Ok(());
            }
        };
        for (key, sha) in uploaded {
            match after.get(&key) {
                Some(r) => {
                    self.record(&key, &sha, &r.hash)?;
                    self.report.uploaded.push(self.display(&key));
                }
                None => self.report.errors.push((
                    self.display(&key),
                    "upload not visible in the cloud listing".into(),
                )),
            }
        }
        Ok(())
    }

    async fn upload(&mut self, key: &str) -> Result<String> {
        let path = self.local_path(key)?;
        let file = &self.local.files[key];
        let data =
            std::fs::read(&path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        let sha = fsutil::sha256_hex(&data);
        if sha != file.sha256 {
            return Err(Error::Refused("the file changed during the sync".into()));
        }
        let modified: DateTime<Utc> = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map(DateTime::from)
            .unwrap_or_else(|_| Utc::now());
        let name = match self.remote.get(key) {
            Some(r) => r.name.clone(),
            None => format!("{}/{}", self.target.location, file.rel),
        };
        // The game may have written a damaged save (a crash): the cloud's is kept first.
        if self.remote.contains_key(key) {
            self.keep_remote_copy(key).await?;
        }
        self.cloud.upload(&name, &data, modified).await?;
        Ok(sha)
    }

    async fn delete_remote(&mut self, key: &str, chosen: bool) -> Result<()> {
        let allowed = self.opts.allow_deletions || chosen;
        if !allowed || self.report.plan.remote_deletions_blocked() {
            self.report.pending_deletions.push(self.display(key));
            return Ok(());
        }
        self.unchanged_local(key)?;
        self.keep_remote_copy(key).await?;
        self.cloud.delete(&self.remote[key].name).await?;
        delete_base(self.target, key)?;
        self.report.deleted_remote.push(self.display(key));
        Ok(())
    }
}

/// The cloud files of `location` by key, and those left out (`SyncReport::skipped_remote`): a
/// name that would leave the save folder, and both of two names that differ only by case.
pub(crate) fn remote_map(
    entries: Vec<RemoteEntry>,
    location: &str,
) -> (BTreeMap<String, RemoteRef>, Vec<String>) {
    let prefix = format!("{location}/");
    let mut map = BTreeMap::new();
    let mut skipped = Vec::new();
    let mut paired = std::collections::BTreeSet::new();
    for e in entries {
        let Some(rel) = e.name.strip_prefix(&prefix) else {
            continue;
        };
        if e.hash == IGNORED_REMOTE_HASH || rel.is_empty() {
            continue;
        }
        let rel = rel.to_string();
        // Saves are resolved with backslashes as separators too (`gameinfo::resolve_relative`).
        if !Path::new(&rel.replace('\\', "/"))
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            skipped.push(rel);
            continue;
        }
        let key = scan::key(&rel);
        if let Some(previous) = map.insert(
            key.clone(),
            RemoteRef {
                name: e.name,
                rel: rel.clone(),
                hash: e.hash,
                last_modified: e.last_modified,
            },
        ) {
            skipped.push(previous.rel);
            paired.insert(key);
        }
    }
    for key in paired {
        if let Some(last) = map.remove(&key) {
            skipped.push(last.rel);
        }
    }
    (map, skipped)
}

fn load_base(t: &SyncTarget<'_>) -> Result<BTreeMap<String, BaseEntry>> {
    let conn = t.db.conn();
    let mut stmt = conn.prepare(
        "SELECT path, local_sha256, remote_hash FROM sync_baseline
         WHERE user_id = ?1 AND game_id = ?2 AND location = ?3",
    )?;
    let rows = stmt.query_map(params![t.user_id, t.game_id, t.location], |r| {
        Ok((
            r.get::<_, String>(0)?,
            BaseEntry {
                local_sha256: r.get(1)?,
                remote_hash: r.get(2)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn upsert_base(t: &SyncTarget<'_>, key: &str, local_sha: &str, remote_hash: &str) -> Result<()> {
    t.db.conn().execute(
        "INSERT INTO sync_baseline (user_id, game_id, location, path, local_sha256, local_size, remote_hash, synced_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)
         ON CONFLICT(user_id, game_id, location, path) DO UPDATE SET
            local_sha256 = excluded.local_sha256, remote_hash = excluded.remote_hash, synced_at = excluded.synced_at",
        params![t.user_id, t.game_id, t.location, key, local_sha, remote_hash, Utc::now().timestamp()],
    )?;
    Ok(())
}

fn clear_base(t: &SyncTarget<'_>) -> Result<()> {
    t.db.conn().execute(
        "DELETE FROM sync_baseline WHERE user_id = ?1 AND game_id = ?2 AND location = ?3",
        params![t.user_id, t.game_id, t.location],
    )?;
    Ok(())
}

fn delete_base(t: &SyncTarget<'_>, key: &str) -> Result<()> {
    t.db.conn().execute(
        "DELETE FROM sync_baseline WHERE user_id = ?1 AND game_id = ?2 AND location = ?3 AND path = ?4",
        params![t.user_id, t.game_id, t.location, key],
    )?;
    Ok(())
}

/// Where this account last synced a location.
struct StoredRoot {
    root: String,
    last_sync_at: Option<i64>,
    /// Which folder it was there (`folder_identity`); none before it was recorded.
    identity: Option<String>,
}

fn stored_root(t: &SyncTarget<'_>) -> Result<Option<StoredRoot>> {
    Ok(t.db
        .conn()
        .query_row(
            "SELECT root, last_sync_at, identity FROM sync_roots
             WHERE user_id = ?1 AND game_id = ?2 AND location = ?3",
            params![t.user_id, t.game_id, t.location],
            |r| {
                Ok(StoredRoot {
                    root: r.get(0)?,
                    last_sync_at: r.get(1)?,
                    identity: r.get(2)?,
                })
            },
        )
        .optional()?)
}

/// Which folder `root` is (device and inode): a folder deleted and made again at the same path
/// (a broken prefix made again) is another one, whose files the history does not describe.
fn folder_identity(root: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(root)
        .ok()
        .map(|m| format!("{}:{}", m.dev(), m.ino()))
}

/// Whether another account synced the same folder since this one last did (accounts that own the
/// game share its prefix): this account's history no longer describes the folder.
fn synced_by_another(t: &SyncTarget<'_>, since: Option<i64>) -> Result<bool> {
    let Some(since) = since else {
        return Ok(false);
    };
    Ok(t.db.conn().query_row(
        "SELECT EXISTS (SELECT 1 FROM sync_roots
         WHERE game_id = ?1 AND location = ?2 AND root = ?3 AND user_id != ?4 AND last_sync_at >= ?5)",
        params![t.game_id, t.location, t.root.to_string_lossy(), t.user_id, since],
        |r| r.get(0),
    )?)
}

fn save_root(t: &SyncTarget<'_>) -> Result<()> {
    t.db.conn().execute(
        "INSERT INTO sync_roots (user_id, game_id, location, root, last_sync_at, identity)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(user_id, game_id, location) DO UPDATE SET root = excluded.root,
           last_sync_at = excluded.last_sync_at, identity = excluded.identity",
        params![
            t.user_id,
            t.game_id,
            t.location,
            t.root.to_string_lossy(),
            Utc::now().timestamp(),
            folder_identity(t.root)
        ],
    )?;
    Ok(())
}
