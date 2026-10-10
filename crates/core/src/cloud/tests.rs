use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use super::plan::{ConflictKind, Warning};
use super::sync::{Prefer, SyncOptions, SyncReport, SyncTarget, sync};
use super::transport::IGNORED_REMOTE_HASH;
use super::transport::memory::MemoryCloud;
use crate::db::Db;
use crate::paths::Dirs;

struct Env {
    tmp: PathBuf,
    dirs: Dirs,
    db: Db,
    cloud: MemoryCloud,
}

impl Env {
    fn new(name: &str) -> Env {
        let tmp = std::env::temp_dir().join(format!("slatty-cloud-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        Env {
            dirs: Dirs::under(&tmp.join("app")),
            tmp,
            db: Db::in_memory().unwrap(),
            cloud: MemoryCloud::default(),
        }
    }

    fn root(&self) -> PathBuf {
        self.tmp.join("saves")
    }

    fn write(&self, rel: &str, data: &str) {
        let p = self.root().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.root().join(rel)).ok()
    }

    async fn sync_as(&self, user: &str, root: &Path, opts: SyncOptions) -> SyncReport {
        let target = SyncTarget {
            db: &self.db,
            dirs: &self.dirs,
            user_id: user,
            game_id: "g",
            location: "saves",
            root,
        };
        sync(&self.cloud, &target, opts).await.unwrap()
    }

    async fn sync(&self, opts: SyncOptions) -> SyncReport {
        self.sync_as("u", &self.root(), opts).await
    }

    fn remote(&self, rel: &str) -> Option<String> {
        self.cloud
            .get(&format!("saves/{rel}"))
            .map(|b| String::from_utf8(b).unwrap())
    }

    fn put_remote(&self, rel: &str, data: &str) {
        self.cloud.put(&format!("saves/{rel}"), data.as_bytes());
    }

    fn backups(&self) -> Vec<String> {
        let root = self.dirs.backups("u", "g");
        let mut out = Vec::new();
        collect(&root, &root, &mut out);
        out.sort();
        out
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            collect(root, &e.path(), out);
        } else {
            let rel = e
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let content = std::fs::read_to_string(e.path()).unwrap();
            out.push(format!("{}={content}", rel.split_once('/').unwrap().1));
        }
    }
}

const ALLOW_DELETIONS: SyncOptions = SyncOptions {
    dry_run: false,
    allow_deletions: true,
    prefer: None,
};

#[tokio::test]
async fn first_upload_then_nothing_to_do() {
    let env = Env::new("first");
    env.write("slot1.sav", "A");
    env.write("Profile/opts.cfg", "O");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.uploaded.len(), 2);
    assert_eq!(env.remote("Profile/opts.cfg").as_deref(), Some("O"));
    let again = env.sync(SyncOptions::default()).await;
    assert!(again.plan.is_noop() && again.uploaded.is_empty() && again.is_clean());
}

#[tokio::test]
async fn local_change_is_uploaded_alone() {
    let env = Env::new("localchange");
    env.write("a.sav", "1");
    env.write("b.sav", "1");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "2");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.uploaded, vec!["a.sav"]);
    assert_eq!(env.remote("a.sav").as_deref(), Some("2"));
}

#[tokio::test]
async fn remote_change_is_downloaded_with_a_backup() {
    let env = Env::new("remotechange");
    env.write("a.sav", "mine");
    env.sync(SyncOptions::default()).await;
    env.put_remote("a.sav", "other machine");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.downloaded, vec!["a.sav"]);
    assert_eq!(env.read("a.sav").as_deref(), Some("other machine"));
    assert_eq!(env.backups(), vec!["saves/local/a.sav=mine"]);
}

#[tokio::test]
async fn concurrent_edits_are_a_conflict_and_nothing_is_written() {
    let env = Env::new("conflict");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.put_remote("a.sav", "remote edit");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(
        r.conflicts,
        vec![("a.sav".to_string(), ConflictKind::BothModified)]
    );
    assert_eq!(env.read("a.sav").as_deref(), Some("local edit"));
    assert_eq!(env.remote("a.sav").as_deref(), Some("remote edit"));
    assert_eq!(env.cloud.uploads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn keeping_remote_version_backs_up_local_one() {
    let env = Env::new("preferremote");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.put_remote("a.sav", "remote edit");
    let r = env
        .sync(SyncOptions {
            prefer: Some(Prefer::Remote),
            ..Default::default()
        })
        .await;
    assert!(r.is_clean());
    assert_eq!(env.read("a.sav").as_deref(), Some("remote edit"));
    assert_eq!(env.backups(), vec!["saves/local/a.sav=local edit"]);
}

#[tokio::test]
async fn keeping_local_version_preserves_remote_copy() {
    let env = Env::new("preferlocal");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.put_remote("a.sav", "remote edit");
    let r = env
        .sync(SyncOptions {
            prefer: Some(Prefer::Local),
            ..Default::default()
        })
        .await;
    assert!(r.is_clean());
    assert_eq!(env.remote("a.sav").as_deref(), Some("local edit"));
    assert_eq!(env.backups(), vec!["saves/remote/a.sav=remote edit"]);
}

#[tokio::test]
async fn missing_or_empty_local_folder_gets_the_cloud_back() {
    // As after uninstalling a game with its prefix and installing it again: the history of the
    // saves remains, the folder is gone.
    let env = Env::new("emptylocal");
    env.write("a.sav", "1");
    env.write("b.sav", "2");
    env.sync(SyncOptions::default()).await;

    std::fs::remove_dir_all(env.root()).unwrap();
    let r = env.sync(ALLOW_DELETIONS).await;
    assert!(r.plan.warnings.contains(&Warning::LocalRootMissing));
    assert!(r.deleted_remote.is_empty());
    assert_eq!(r.downloaded.len(), 2);
    assert_eq!(
        std::fs::read_to_string(env.root().join("a.sav")).unwrap(),
        "1"
    );

    for f in ["a.sav", "b.sav"] {
        std::fs::remove_file(env.root().join(f)).unwrap();
    }
    let r = env.sync(ALLOW_DELETIONS).await;
    assert!(r.plan.warnings.contains(&Warning::LocalEmptyWithHistory));
    assert!(r.deleted_remote.is_empty());
    assert_eq!(r.downloaded.len(), 2);
    assert!(env.remote("a.sav").is_some() && env.remote("b.sav").is_some());
}

#[tokio::test]
async fn single_deletion_needs_explicit_permission() {
    let env = Env::new("deletion");
    env.write("a.sav", "1");
    env.write("b.sav", "2");
    env.sync(SyncOptions::default()).await;
    std::fs::remove_file(env.root().join("b.sav")).unwrap();

    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.pending_deletions, vec!["b.sav"]);
    assert!(env.remote("b.sav").is_some());

    let r = env.sync(ALLOW_DELETIONS).await;
    assert_eq!(r.deleted_remote, vec!["b.sav"]);
    assert!(env.remote("b.sav").is_none());
}

#[tokio::test]
async fn empty_cloud_with_history_never_deletes_local_files() {
    let env = Env::new("emptyremote");
    env.write("a.sav", "1");
    env.sync(SyncOptions::default()).await;
    env.cloud.files.lock().unwrap().clear();
    let r = env.sync(ALLOW_DELETIONS).await;
    assert!(r.plan.warnings.contains(&Warning::RemoteEmptyWithHistory));
    assert!(r.deleted_local.is_empty());
    assert_eq!(env.read("a.sav").as_deref(), Some("1"));
}

#[tokio::test]
async fn failed_download_leaves_local_file_and_history_untouched() {
    let env = Env::new("netfail");
    env.write("a.sav", "mine");
    env.sync(SyncOptions::default()).await;
    env.put_remote("a.sav", "newer");
    env.cloud.fail_downloads.store(true, Ordering::SeqCst);
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.errors.len(), 1);
    assert_eq!(env.read("a.sav").as_deref(), Some("mine"));
    env.cloud.fail_downloads.store(false, Ordering::SeqCst);
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.downloaded, vec!["a.sav"]);
}

#[tokio::test]
async fn offline_sync_fails_without_touching_anything() {
    let env = Env::new("offline");
    env.write("a.sav", "1");
    env.cloud.offline.store(true, Ordering::SeqCst);
    let target = SyncTarget {
        db: &env.db,
        dirs: &env.dirs,
        user_id: "u",
        game_id: "g",
        location: "saves",
        root: &env.root(),
    };
    assert!(
        sync(&env.cloud, &target, SyncOptions::default())
            .await
            .is_err()
    );
    assert_eq!(env.read("a.sav").as_deref(), Some("1"));
}

#[tokio::test]
async fn upload_is_refused_when_cloud_changes_mid_sync() {
    let env = Env::new("race");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.cloud.after_list(env.cloud.list_count() + 1, |files| {
        files.insert("saves/a.sav".into(), b"sneaky remote edit".to_vec());
    });
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.refused.len(), 1);
    assert_eq!(env.remote("a.sav").as_deref(), Some("sneaky remote edit"));
    let next = env.sync(SyncOptions::default()).await;
    assert_eq!(next.conflicts.len(), 1);
}

/// Known limit: without conditional uploads, a change landing between the fresh listing
/// and the upload is overwritten. This test pins the behaviour so it is not forgotten.
#[tokio::test]
async fn residual_window_after_fresh_listing_is_not_detected() {
    let env = Env::new("residual");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.cloud.after_list(env.cloud.list_count() + 2, |files| {
        files.insert("saves/a.sav".into(), b"too late".to_vec());
    });
    let r = env.sync(SyncOptions::default()).await;
    assert!(r.refused.is_empty());
    assert_eq!(env.remote("a.sav").as_deref(), Some("local edit"));
}

#[tokio::test]
async fn identical_files_without_history_are_adopted() {
    let env = Env::new("adopt");
    env.write("a.sav", "same");
    env.write("b.sav", "local");
    env.put_remote("a.sav", "same");
    env.put_remote("b.sav", "remote");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.adopted, vec!["a.sav"]);
    assert_eq!(
        r.conflicts,
        vec![("b.sav".to_string(), ConflictKind::Unrelated)]
    );
    assert_eq!(env.cloud.uploads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn another_account_does_not_inherit_history() {
    let env = Env::new("accounts");
    env.write("a.sav", "1");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "2");
    env.put_remote("a.sav", "other account data");
    let r = env
        .sync_as("someone-else", &env.root(), SyncOptions::default())
        .await;
    assert_eq!(
        r.conflicts,
        vec![("a.sav".to_string(), ConflictKind::Unrelated)]
    );
    assert_eq!(env.read("a.sav").as_deref(), Some("2"));
}

#[tokio::test]
async fn changed_save_folder_does_not_reuse_history() {
    let env = Env::new("rootchange");
    env.write("a.sav", "old prefix");
    env.sync(SyncOptions::default()).await;
    let other = env.tmp.join("other-prefix");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("a.sav"), "new prefix").unwrap();
    let r = env.sync_as("u", &other, ALLOW_DELETIONS).await;
    assert!(r.plan.warnings.contains(&Warning::RootChanged));
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(env.remote("a.sav").as_deref(), Some("old prefix"));
}

#[tokio::test]
async fn case_differences_match_the_same_file() {
    let env = Env::new("case");
    env.write("Slot1.SAV", "1");
    env.put_remote("slot1.sav", "1");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.adopted.len(), 1);
    assert!(r.is_clean());
}

#[tokio::test]
async fn ignored_marker_entries_are_not_downloaded() {
    let env = Env::new("marker");
    std::fs::create_dir_all(env.root()).unwrap();
    env.cloud
        .files
        .lock()
        .unwrap()
        .insert("saves/ghost.sav".into(), b"x".to_vec());
    let listing = env.cloud.files.lock().unwrap().len();
    assert_eq!(listing, 1);
    let r = super::sync::sync(
        &MarkerCloud(&env.cloud),
        &SyncTarget {
            db: &env.db,
            dirs: &env.dirs,
            user_id: "u",
            game_id: "g",
            location: "saves",
            root: &env.root(),
        },
        SyncOptions::default(),
    )
    .await
    .unwrap();
    assert!(r.plan.files.is_empty());
}

#[tokio::test]
async fn remote_paths_cannot_escape_the_save_folder() {
    let env = Env::new("escape");
    std::fs::create_dir_all(env.root()).unwrap();
    env.put_remote("../../evil.sh", "x");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.errors.len(), 1);
    assert!(
        !env.tmp.join("evil.sh").exists() && !env.tmp.parent().unwrap().join("evil.sh").exists()
    );
}

#[tokio::test]
async fn a_download_cannot_follow_a_directory_symlink_outside_saves() {
    let env = Env::new("symlink-download");
    let outside = env.tmp.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(env.root()).unwrap();
    std::fs::write(outside.join("a.sav"), "keep me").unwrap();
    std::os::unix::fs::symlink(&outside, env.root().join("linked")).unwrap();
    env.put_remote("linked/a.sav", "remote");
    let r = env.sync(SyncOptions::default()).await;
    assert!(!r.is_clean());
    assert!(r.downloaded.is_empty());
    assert_eq!(
        std::fs::read_to_string(outside.join("a.sav")).unwrap(),
        "keep me"
    );
}

#[tokio::test]
async fn a_download_refuses_a_local_change_after_the_scan() {
    let env = Env::new("local-download-race");
    env.write("a.sav", "base");
    env.sync(SyncOptions::default()).await;
    env.put_remote("a.sav", "remote");
    let path = env.root().join("a.sav");
    env.cloud.after_list(env.cloud.list_count() + 1, move |_| {
        std::fs::write(path, "new local save").unwrap();
    });
    let r = env.sync(SyncOptions::default()).await;
    assert!(!r.is_clean());
    assert!(r.downloaded.is_empty());
    assert_eq!(env.read("a.sav").as_deref(), Some("new local save"));
    assert_eq!(env.sync(SyncOptions::default()).await.conflicts.len(), 1);
}

#[tokio::test]
async fn cloud_deletion_refuses_a_local_file_recreated_after_the_scan() {
    let env = Env::new("recreated-save");
    env.write("a.sav", "base");
    env.write("b.sav", "keep root nonempty");
    env.sync(SyncOptions::default()).await;
    std::fs::remove_file(env.root().join("a.sav")).unwrap();
    let path = env.root().join("a.sav");
    env.cloud.after_list(env.cloud.list_count() + 1, move |_| {
        std::fs::write(path, "recreated").unwrap();
    });
    let r = env.sync(ALLOW_DELETIONS).await;
    assert!(!r.is_clean() && r.deleted_remote.is_empty());
    assert_eq!(env.remote("a.sav").as_deref(), Some("base"));
}

#[tokio::test]
async fn consecutive_syncs_preserve_each_previous_version() {
    let env = Env::new("backup-versions");
    env.write("a.sav", "first");
    env.sync(SyncOptions::default()).await;
    env.put_remote("a.sav", "second");
    let first = env.sync(SyncOptions::default()).await;
    env.put_remote("a.sav", "third");
    let second = env.sync(SyncOptions::default()).await;
    assert_ne!(first.backup_dir, second.backup_dir);
    assert_eq!(
        env.backups(),
        vec!["saves/local/a.sav=first", "saves/local/a.sav=second"]
    );
}

#[tokio::test]
async fn keeping_the_cloud_side_of_a_cloud_deletion_backs_up_then_deletes() {
    // The interface's "Keep the cloud version" never sets allow_deletions: the choice itself
    // must resolve the conflict, or the game stays blocked.
    let env = Env::new("prefer-remote-deletion");
    env.write("a.sav", "base");
    env.write("b.sav", "keep cloud nonempty");
    env.sync(SyncOptions::default()).await;
    env.write("a.sav", "local edit");
    env.cloud.files.lock().unwrap().remove("saves/a.sav");
    assert_eq!(env.sync(SyncOptions::default()).await.conflicts.len(), 1);
    let r = env
        .sync(SyncOptions {
            prefer: Some(Prefer::Remote),
            ..Default::default()
        })
        .await;
    assert_eq!(r.deleted_local, vec!["a.sav"]);
    assert!(env.read("a.sav").is_none());
    assert_eq!(env.backups(), vec!["saves/local/a.sav=local edit"]);
    assert!(env.sync(SyncOptions::default()).await.is_clean());
}

#[tokio::test]
async fn concurrent_sync_of_the_same_game_is_refused() {
    let env = Env::new("lock");
    let _held = crate::lock::try_acquire(&env.dirs.locks().join("cloud-u-g.lock"))
        .unwrap()
        .unwrap();
    let target = SyncTarget {
        db: &env.db,
        dirs: &env.dirs,
        user_id: "u",
        game_id: "g",
        location: "saves",
        root: &env.root(),
    };
    assert!(matches!(
        sync(&env.cloud, &target, SyncOptions::default()).await,
        Err(crate::Error::Refused(_))
    ));
}

struct MarkerCloud<'a>(&'a MemoryCloud);

impl super::transport::CloudTransport for MarkerCloud<'_> {
    async fn list(&self) -> crate::Result<Vec<super::transport::RemoteEntry>> {
        let mut entries = self.0.list().await?;
        for e in &mut entries {
            e.hash = IGNORED_REMOTE_HASH.into();
        }
        Ok(entries)
    }
    async fn download(&self, name: &str) -> crate::Result<Vec<u8>> {
        self.0.download(name).await
    }
    async fn upload(
        &self,
        name: &str,
        data: &[u8],
        m: chrono::DateTime<chrono::Utc>,
    ) -> crate::Result<()> {
        self.0.upload(name, data, m).await
    }
    async fn delete(&self, name: &str) -> crate::Result<()> {
        self.0.delete(name).await
    }
}

#[tokio::test]
async fn inspection_is_read_only_and_flags_compressed_downloads() {
    let env = Env::new("inspect");
    env.write("same.sav", "x");
    env.write("diff.sav", "local");
    env.put_remote("same.sav", "x");
    env.cloud.put("saves/diff.sav", b"\x1f\x8bstill gzip");
    let copies = env.tmp.join("copies");
    let files = super::inspect::compare(&env.cloud, "saves", &env.root(), &copies, None)
        .await
        .unwrap();
    let same = files.iter().find(|f| f.path == "same.sav").unwrap();
    let diff = files.iter().find(|f| f.path == "diff.sav").unwrap();
    assert!(same.identical() && !diff.identical());
    assert!(diff.remote.as_ref().unwrap().gzip_magic);
    assert_eq!(env.read("diff.sav").as_deref(), Some("local"));
    assert!(copies.join("diff.sav").exists());
    assert!(env.cloud.uploads.load(Ordering::SeqCst) == 0);
}

#[tokio::test]
async fn nothing_is_written_into_a_prefix_proton_has_not_created_yet() {
    use crate::auth::Tokens;
    use crate::install::{Install, Platform};
    use crate::runner::Runner;
    use crate::secret::Secret;

    let root = std::env::temp_dir().join(format!("slatty-cloud-noprefix-{}", std::process::id()));
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::in_memory().unwrap();
    let install = Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: Some("c".into()),
        runner: Runner::Umu {
            proton: "/p".into(),
            prefix: root.join("pfx"),
        },
        umu_id: None,
        isolated: false,
    };
    let tokens = Tokens {
        user_id: "u".into(),
        access_token: Secret::new("[FAKE]"),
        refresh_token: Secret::new("[FAKE]"),
        expires_at: i64::MAX,
    };
    let http = crate::http::client().unwrap();
    let result =
        super::sync_game(&db, &dirs, &http, &tokens, &install, SyncOptions::default()).await;
    assert!(matches!(result, Err(crate::Error::Refused(_))));
    assert!(
        !root.exists(),
        "the prefix stays untouched for Proton to create"
    );
}

#[tokio::test]
async fn saves_are_not_synced_while_the_game_is_in_use() {
    use crate::auth::Tokens;
    use crate::install::{Install, Platform};
    use crate::runner::Runner;
    use crate::secret::Secret;

    let root = std::env::temp_dir().join(format!("slatty-cloud-busy-{}", std::process::id()));
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::in_memory().unwrap();
    std::fs::create_dir_all(root.join("pfx/drive_c/users")).unwrap();
    let install = Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: Some("c".into()),
        runner: Runner::Umu {
            proton: "/p".into(),
            prefix: root.join("pfx"),
        },
        umu_id: None,
        isolated: false,
    };
    let tokens = Tokens {
        user_id: "u".into(),
        access_token: Secret::new("[FAKE]"),
        refresh_token: Secret::new("[FAKE]"),
        expires_at: i64::MAX,
    };
    let http = crate::http::client().unwrap();
    let playing = crate::lock::game(&dirs, "1").unwrap();
    let result =
        super::sync_game(&db, &dirs, &http, &tokens, &install, SyncOptions::default()).await;
    assert!(matches!(result, Err(crate::Error::Refused(_))));
    drop(playing);
    std::fs::remove_dir_all(&root).unwrap();
}
