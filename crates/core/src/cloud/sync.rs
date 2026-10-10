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
    DeleteRemote,
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
    let _lock = lock::try_acquire(&lock_path)?
        .ok_or_else(|| Error::Refused("a cloud sync for this game is already running".into()))?;

    let local = scan::scan(target.root)?;
    let remote = remote_map(cloud.list().await?, target.location)?;
    let stored_root = stored_root(target)?;
    let root_changed = stored_root.is_some_and(|r| Path::new(&r) != target.root);
    // History recorded for another folder says nothing about this one.
    let base = if root_changed {
        BTreeMap::new()
    } else {
        load_base(target)?
    };

    let local_view: BTreeMap<String, LocalFile> = local
        .files
        .iter()
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
            Action::DeleteRemote => self.deferred.push((key.into(), Deferred::DeleteRemote)),
            Action::Download => self.download_replace(key).await?,
            Action::ForgetBase => delete_base(self.target, key)?,
            Action::DeleteLocal => {
                if self.opts.allow_deletions && !plan.local_deletions_blocked() {
                    self.delete_local(key)?;
                } else {
                    self.report.pending_deletions.push(self.display(key));
                }
            }
            Action::Compare => {
                let remote = &self.remote[key];
                let bytes = self.cloud.download(&remote.name).await?;
                if fsutil::sha256_hex(&bytes) == self.local.files[key].sha256 {
                    let hash = remote.hash.clone();
                    self.record(key, &fsutil::sha256_hex(&bytes), &hash)?;
                    self.report.adopted.push(self.display(key));
                } else {
                    self.resolve(key, ConflictKind::Unrelated).await?;
                }
            }
            Action::Conflict(kind) => self.resolve(key, kind).await?,
        }
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
                    self.keep_remote_copy(key).await?;
                    self.deferred.push((key.into(), Deferred::DeleteRemote));
                }
            }
            (Some(Prefer::Local), _) => {
                self.keep_remote_copy(key).await?;
                self.deferred.push((key.into(), Deferred::Upload));
            }
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
        std::fs::copy(file, &dest)
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
        let fresh = remote_map(self.cloud.list().await?, self.target.location)?;
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
                Deferred::DeleteRemote => self.delete_remote(&key).await,
            };
            if let Err(e) = result {
                self.report.errors.push((self.display(&key), e.to_string()));
            }
        }
        if uploaded.is_empty() {
            return Ok(());
        }
        let after = remote_map(self.cloud.list().await?, self.target.location)?;
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
        self.cloud.upload(&name, &data, modified).await?;
        Ok(sha)
    }

    async fn delete_remote(&mut self, key: &str) -> Result<()> {
        if !self.opts.allow_deletions || self.report.plan.remote_deletions_blocked() {
            self.report.pending_deletions.push(self.display(key));
            return Ok(());
        }
        self.unchanged_local(key)?;
        self.cloud.delete(&self.remote[key].name).await?;
        delete_base(self.target, key)?;
        self.report.deleted_remote.push(self.display(key));
        Ok(())
    }
}

pub(crate) fn remote_map(
    entries: Vec<RemoteEntry>,
    location: &str,
) -> Result<BTreeMap<String, RemoteRef>> {
    let prefix = format!("{location}/");
    let mut map = BTreeMap::new();
    for e in entries {
        let Some(rel) = e.name.strip_prefix(&prefix) else {
            continue;
        };
        if e.hash == IGNORED_REMOTE_HASH || rel.is_empty() {
            continue;
        }
        let rel = rel.to_string();
        if let Some(previous) = map.insert(
            scan::key(&rel),
            RemoteRef {
                name: e.name,
                rel: rel.clone(),
                hash: e.hash,
                last_modified: e.last_modified,
            },
        ) {
            return Err(Error::Refused(format!(
                "cloud holds `{}` and `{rel}`, which differ only by case",
                previous.rel
            )));
        }
    }
    Ok(map)
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

fn stored_root(t: &SyncTarget<'_>) -> Result<Option<String>> {
    Ok(t.db
        .conn()
        .query_row(
            "SELECT root FROM sync_roots WHERE user_id = ?1 AND game_id = ?2 AND location = ?3",
            params![t.user_id, t.game_id, t.location],
            |r| r.get(0),
        )
        .optional()?)
}

fn save_root(t: &SyncTarget<'_>) -> Result<()> {
    t.db.conn().execute(
        "INSERT INTO sync_roots (user_id, game_id, location, root, last_sync_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(user_id, game_id, location) DO UPDATE SET root = excluded.root, last_sync_at = excluded.last_sync_at",
        params![t.user_id, t.game_id, t.location, t.root.to_string_lossy(), Utc::now().timestamp()],
    )?;
    Ok(())
}
