use std::path::PathBuf;

use super::*;
use crate::cloud::sync::{Prefer, SyncReport};
use crate::install::Platform;
use crate::runner::Runner;

/// Epic's storage in memory: files by key, under the prefix Epic's keys begin with, each with the
/// order it was stored in as its `lastModified`.
#[derive(Default)]
struct FakeStore {
    files: Mutex<BTreeMap<String, (Vec<u8>, String)>>,
    writes: Mutex<usize>,
    /// Epic answers listings and reads, and refuses writes.
    refuse_writes: Mutex<bool>,
}

const PREFIX: &str = "fakeaccount/FakeApp";

impl FakeStore {
    fn snapshots(&self) -> Vec<String> {
        self.files
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.contains("/manifests/"))
            .cloned()
            .collect()
    }
}

impl SaveStore for FakeStore {
    async fn list(&self) -> Result<Vec<Stored>> {
        Ok(self
            .files
            .lock()
            .unwrap()
            .iter()
            .map(|(k, (_, at))| Stored {
                key: k.clone(),
                last_modified: Some(at.clone()),
            })
            .collect())
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(d, _)| d.clone())
            .ok_or_else(|| Error::NotFound(key.into()))
    }

    async fn write(&self, files: Vec<(String, Vec<u8>)>) -> Result<()> {
        if *self.refuse_writes.lock().unwrap() {
            return Err(Error::Http {
                context: "[FAKE] Epic",
                status: 503,
            });
        }
        for (name, data) in files {
            let mut n = self.writes.lock().unwrap();
            *n += 1;
            let at = format!("2026-10-27T{:08}Z", *n);
            self.files
                .lock()
                .unwrap()
                .insert(format!("{PREFIX}/{name}"), (data, at));
        }
        Ok(())
    }
}

fn cloud() -> EpicCloud<FakeStore> {
    EpicCloud::new(
        FakeStore::default(),
        "FakeApp",
        "fakeaccount",
        "{AppData}/Fake/Saved",
    )
}

/// The files of the newest snapshot, by name.
async fn remote(cloud: &EpicCloud<FakeStore>) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    for e in cloud.list().await.unwrap() {
        let data = cloud.download(&e.name).await.unwrap();
        out.insert(e.name, data);
    }
    out
}

async fn put(cloud: &EpicCloud<FakeStore>, files: &[(&str, &[u8])]) {
    cloud.list().await.unwrap();
    for (name, data) in files {
        cloud
            .upload(&format!("saves/{name}"), data, Utc::now())
            .await
            .unwrap();
    }
    cloud.commit().await.unwrap();
}

fn content(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// A snapshot is chunks of 1 MiB, padded, that files share and cross, and a manifest naming the
/// folder template; read back, every file is whole, and listed with its SHA-1.
#[tokio::test]
async fn a_snapshot_is_read_back_as_it_was_written() {
    let cloud = cloud();
    let big = content(1, (1 << 20) + 300);
    put(
        &cloud,
        &[
            ("Profile/Slot1.sav", &big),
            ("settings.ini", b"[FAKE] a=1"),
            ("Empty.sav", b""),
        ],
    )
    .await;
    let keys: Vec<String> = cloud.store.files.lock().unwrap().keys().cloned().collect();
    let chunks: Vec<&String> = keys.iter().filter(|k| k.contains("/ChunksV4/")).collect();
    assert_eq!(chunks.len(), 2, "{keys:?}");
    assert_eq!(cloud.store.snapshots().len(), 1);

    let listed = cloud.list().await.unwrap();
    let slot = listed
        .iter()
        .find(|e| e.name == "saves/Profile/Slot1.sav")
        .unwrap();
    assert_eq!(slot.hash, fsutil::hex(&crate::epic::sha1(&big)));
    let files = remote(&cloud).await;
    assert_eq!(files["saves/Profile/Slot1.sav"], big);
    assert_eq!(files["saves/settings.ini"], b"[FAKE] a=1");
    assert_eq!(files["saves/Empty.sav"], b"");

    let key = &cloud.store.snapshots()[0];
    let raw = cloud.store.read(key).await.unwrap();
    let m = manifest::parse(&raw, &HashMap::new()).unwrap();
    assert_eq!(m.app_name, "FakeAppfakeaccount");
    assert_eq!(m.feature_level, FEATURE_LEVEL);
    let names: Vec<&str> = m.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Empty.sav", "Profile/Slot1.sav", "settings.ini"]);
    // The custom fields, at the end of the body: the folder's template, as legendary keeps it.
    let mut body = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&raw[41..]), &mut body).unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("CloudSaveFolder\0") && text.contains("{AppData}/Fake/Saved\0"));
}

/// What a sync sends is one new snapshot, made from the newest: the files not asked for are read
/// back from it, the uploads replace theirs whatever their case, the deletions are left out. The
/// older snapshot stays; the new one is named after it.
#[tokio::test]
async fn a_new_snapshot_is_the_newest_with_the_changes() {
    let cloud = cloud();
    put(
        &cloud,
        &[
            ("A.sav", b"[FAKE] a"),
            ("B.sav", b"[FAKE] b"),
            ("C.sav", b"[FAKE] c"),
        ],
    )
    .await;
    cloud.list().await.unwrap();
    cloud
        .upload("saves/b.SAV", b"[FAKE] b2", Utc::now())
        .await
        .unwrap();
    cloud.delete("saves/C.sav").await.unwrap();
    cloud
        .upload("saves/D.sav", b"[FAKE] d", Utc::now())
        .await
        .unwrap();
    cloud.commit().await.unwrap();

    let snapshots = cloud.store.snapshots();
    assert_eq!(snapshots.len(), 2, "the older is kept");
    assert!(snapshots[0] < snapshots[1]);
    let files = remote(&cloud).await;
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        ["saves/A.sav", "saves/D.sav", "saves/b.SAV"]
    );
    assert_eq!(files["saves/b.SAV"], b"[FAKE] b2");
    assert_eq!(files["saves/A.sav"], b"[FAKE] a");

    // Nothing asked: nothing sent.
    let writes = *cloud.store.writes.lock().unwrap();
    cloud.commit().await.unwrap();
    assert_eq!(*cloud.store.writes.lock().unwrap(), writes);
}

/// A new snapshot keeps what the newest holds and the sync does not change: two files whose
/// names differ only by case (the sync leaves both alone), and links.
#[tokio::test]
async fn a_new_snapshot_keeps_case_pairs_and_links() {
    let cloud = cloud();
    let link = FileEntry {
        name: "Latest.sav".into(),
        symlink_target: "Save.dat".into(),
        sha1: [0; 20],
        flags: 0,
        parts: Vec::new(),
    };
    let first = cloud.pack(
        vec![
            ("Save.dat".into(), b"[FAKE] upper".to_vec()),
            ("save.dat".into(), b"[FAKE] lower".to_vec()),
        ],
        vec![link],
        "2026.10.27-10.00.00",
    );
    cloud.store.write(first).await.unwrap();
    put(&cloud, &[("Other.sav", b"[FAKE] other")]).await;
    let files = remote(&cloud).await;
    assert_eq!(files["saves/Save.dat"], b"[FAKE] upper");
    assert_eq!(files["saves/save.dat"], b"[FAKE] lower");
    assert_eq!(files["saves/Other.sav"], b"[FAKE] other");
    let newest = cloud.state.lock().unwrap().newest.clone().unwrap();
    assert!(
        newest
            .manifest
            .files
            .iter()
            .any(|f| f.name == "Latest.sav" && f.symlink_target == "Save.dat")
    );
}

/// Another computer's snapshot sent between the sync's listing and its commit, changing a file
/// the sync sends: nothing is sent, and theirs stays the newest.
#[tokio::test]
async fn a_file_changed_elsewhere_meanwhile_is_not_overwritten() {
    let cloud = cloud();
    put(&cloud, &[("A.sav", b"[FAKE] a")]).await;
    cloud.list().await.unwrap();
    cloud
        .upload("saves/A.sav", b"[FAKE] mine", Utc::now())
        .await
        .unwrap();
    let theirs = cloud.pack(
        vec![("A.sav".into(), b"[FAKE] theirs".to_vec())],
        Vec::new(),
        "2999.01.01-00.00.00",
    );
    cloud.store.write(theirs).await.unwrap();
    assert!(matches!(cloud.commit().await, Err(Error::Refused(_))));
    assert_eq!(cloud.store.snapshots().len(), 2);
    assert_eq!(remote(&cloud).await["saves/A.sav"], b"[FAKE] theirs");
}

/// The newest snapshot is the one stored last by date, read as a date: `…00.500Z` is after
/// `…00Z`, though not as text.
#[tokio::test]
async fn the_newest_snapshot_is_dated_as_a_date() {
    let cloud = cloud();
    put(&cloud, &[("A.sav", b"[FAKE] older")]).await;
    put(&cloud, &[("A.sav", b"[FAKE] newer")]).await;
    {
        let mut files = cloud.store.files.lock().unwrap();
        let snapshots: Vec<String> = files
            .keys()
            .filter(|k| k.contains("/manifests/"))
            .cloned()
            .collect();
        files.get_mut(&snapshots[0]).unwrap().1 = "2026-10-27T10:00:00Z".into();
        files.get_mut(&snapshots[1]).unwrap().1 = "2026-10-27T10:00:00.500Z".into();
    }
    assert_eq!(remote(&cloud).await["saves/A.sav"], b"[FAKE] newer");
}

/// Epic keeps no empty snapshot: deleting the last files is refused, and the cloud keeps them.
#[tokio::test]
async fn the_last_files_are_not_deleted_from_epics_cloud() {
    let cloud = cloud();
    put(&cloud, &[("A.sav", b"[FAKE] a")]).await;
    cloud.list().await.unwrap();
    cloud.delete("saves/A.sav").await.unwrap();
    assert!(matches!(cloud.commit().await, Err(Error::Refused(_))));
    assert_eq!(remote(&cloud).await.len(), 1);
}

/// The newest snapshot is the one stored last, whatever its name; a new one is named after every
/// one stored, even one dated in the future.
#[test]
fn snapshots_are_named_after_every_one_stored() {
    assert_eq!(
        snapshot_key("acct/app/manifests/2026.10.27-10.00.00.manifest"),
        Some(("2026.10.27-10.00.00", "acct/app"))
    );
    assert_eq!(
        snapshot_key("manifests/2026.10.27-10.00.00.manifest"),
        Some(("2026.10.27-10.00.00", ""))
    );
    assert_eq!(snapshot_key("acct/app/ChunksV4/01/x.chunk"), None);
    let future = "2999.01.01-00.00.00";
    assert_eq!(
        EpicCloud::<FakeStore>::next_stamp(Some(future)),
        "2999.01.01-00.00.01"
    );
    let now = EpicCloud::<FakeStore>::next_stamp(None);
    assert!(now.as_str() > "2026.01.01-00.00.00" && now.len() == future.len());
}

fn install(root: &std::path::Path, isolated: bool) -> Install {
    Install {
        umu_id: None,
        game_id: "epic-FakeApp".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: root.join("games/Fake"),
        client_id: None,
        runner: Runner::Umu {
            proton: "/proton".into(),
            prefix: root.join("prefix"),
        },
        isolated,
    }
}

/// Epic's templates resolve inside the prefix or the game folder, the account id in place of
/// `{EpicID}`; a variable Slatty does not know is refused, never left as a folder name.
#[test]
fn epic_save_folders_resolve_in_the_prefix() {
    let root = std::env::temp_dir().join(format!("slatty-epic-saves-{}", std::process::id()));
    let install = install(&root, false);
    let user = root.join("prefix/drive_c/users/steamuser");
    assert_eq!(
        save_root("{AppData}/Fake/Saved/{EpicID}", "fakeaccount", &install).unwrap(),
        user.join("AppData/Local/Fake/Saved/fakeaccount")
    );
    assert_eq!(
        save_root("{UserDir}\\My Games\\Fake", "a", &install).unwrap(),
        user.join("Documents/My Games/Fake")
    );
    assert_eq!(
        save_root("{usersavedgames}/Fake", "a", &install).unwrap(),
        user.join("Saved Games/Fake")
    );
    assert_eq!(
        save_root("{InstallDir}/Saves", "a", &install).unwrap(),
        root.join("games/Fake/Saves")
    );
    // Unity's games climb from Local AppData to LocalLow, as Epic's templates write it.
    assert_eq!(
        save_root("{AppData}/../LocalLow/Fake Studio/Fake/", "a", &install).unwrap(),
        user.join("AppData/LocalLow/Fake Studio/Fake")
    );
    assert_eq!(
        save_root("{appdata}/./../Roaming/Fake", "a", &install).unwrap(),
        user.join("AppData/Roaming/Fake")
    );
    for refused in [
        "{UserProfile}/Fake",
        "Fake/Saved",
        "{AppData}/{Unknown}/Saved",
        "{AppData}/../../Fake",
        "{AppData}/../../../../../etc",
        "{InstallDir}/../Fake",
        "{UserDir}/../Desktop/Fake",
        // A whole folder, not a save folder.
        "{InstallDir}",
        "{AppData}/",
        "{UserDir}",
        "{AppData}/../Local",
    ] {
        assert!(save_root(refused, "a", &install).is_err(), "{refused}");
    }
}

/// A pattern of many `*` against a long path is matched without trying every way to split it.
#[test]
fn many_stars_against_a_long_path_take_no_time() {
    let chars = |s: &str| s.chars().collect::<Vec<_>>();
    let long = chars(&"a".repeat(400));
    assert!(!glob(&long, &chars("*a*a*a*a*a*a*a*a*b")));
    assert!(glob(&long, &chars("*a*a*a*a*a*a*a*a*")));
    assert!(glob(&chars("save/slot1.sav"), &chars("*/slot?.sav")));
    assert!(
        glob(&chars("x[y"), &chars("x[y")),
        "a [ not closed is plain"
    );
    assert!(!glob(&chars("slot3.sav"), &chars("slot[12].sav")));
}

/// legendary's matching: a folder, an end of the path, a shell pattern; regardless of case.
#[test]
fn include_and_exclude_lists_match_as_legendarys() {
    let filter = SaveFilter {
        include: SaveFilter::list(Some("SaveGames/, *.sav ,Settings.ini")),
        // A shell pattern is of the whole path (`fnmatch`).
        exclude: SaveFilter::list(Some("*.bak,SaveGames/Cache/,*slot[0-1].tmp,[0-9].dat")),
    };
    assert!(filter.keeps("SaveGames/Slot1.dat"));
    assert!(filter.keeps("Profile/slot2.SAV"));
    assert!(filter.keeps("Config/settings.ini"));
    assert!(!filter.keeps("Logs/game.log"));
    assert!(!filter.keeps("SaveGames/Slot1.bak"));
    assert!(!filter.keeps("SaveGames/Cache/x.dat"));
    assert!(!filter.keeps("SaveGames/slot1.tmp"));
    assert!(filter.keeps("SaveGames/slot2.tmp"));
    assert!(filter.keeps("SaveGames/1.dat"), "not the whole path");
    assert!(SaveFilter::default().keeps("anything/at/all"));
    assert!(glob(&['a', 'b'], &['[', '!', 'x', ']', 'b']));
    assert!(!glob(&['x', 'b'], &['[', '!', 'x', ']', 'b']));
}

struct Env {
    tmp: PathBuf,
    dirs: Dirs,
    db: Db,
    cloud: EpicCloud<FakeStore>,
}

impl Env {
    fn new(name: &str) -> Env {
        let tmp =
            std::env::temp_dir().join(format!("slatty-epic-sync-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        Env {
            dirs: Dirs::under(&tmp.join("app")),
            tmp,
            db: Db::in_memory().unwrap(),
            cloud: cloud(),
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

    async fn sync(&self, opts: SyncOptions) -> SyncReport {
        let filter = SaveFilter {
            include: Vec::new(),
            exclude: SaveFilter::list(Some("*.log")),
        };
        let keep = |rel: &str| filter.keeps(rel);
        let root = self.root();
        let target = SyncTarget {
            db: &self.db,
            dirs: &self.dirs,
            user_id: "fakeaccount",
            game_id: "epic-FakeApp",
            location: LOCATION,
            root: &root,
            keep: Some(&keep),
        };
        sync::sync(&self.cloud, &target, opts).await.unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

/// Slatty's rules over Epic's snapshots: the first sync sends the saves as one snapshot, leaving
/// out what the game excludes; a save changed here is sent as another, one changed on another
/// computer comes down, unchanged ones stay; both changed is a conflict, which sends nothing
/// until a side is chosen.
#[tokio::test]
async fn epic_saves_sync_by_slattys_rules() {
    let env = Env::new("rules");
    env.write("Slot1.sav", "[FAKE] one");
    env.write("Slot2.sav", "[FAKE] two");
    env.write("game.log", "[FAKE] log");
    let r = env.sync(SyncOptions::default()).await;
    assert!(r.is_clean(), "{r:?}");
    assert_eq!(r.uploaded.len(), 2);
    assert_eq!(env.cloud.store.snapshots().len(), 1, "one snapshot");
    assert!(!remote(&env.cloud).await.contains_key("saves/game.log"));

    env.write("Slot1.sav", "[FAKE] one, later");
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.uploaded, ["Slot1.sav"]);
    let files = remote(&env.cloud).await;
    assert_eq!(files["saves/Slot1.sav"], b"[FAKE] one, later");
    assert_eq!(
        files["saves/Slot2.sav"], b"[FAKE] two",
        "kept in the new snapshot"
    );

    // Another computer sends a snapshot.
    put(&env.cloud, &[("Slot2.sav", b"[FAKE] two, elsewhere")]).await;
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.downloaded, ["Slot2.sav"]);
    assert!(r.uploaded.is_empty());
    assert_eq!(env.read("Slot2.sav").unwrap(), "[FAKE] two, elsewhere");

    // Both sides change the same save.
    let snapshots = env.cloud.store.snapshots().len();
    env.write("Slot1.sav", "[FAKE] here");
    put(&env.cloud, &[("Slot1.sav", b"[FAKE] there")]).await;
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.conflicts.len(), 1);
    assert_eq!(
        env.cloud.store.snapshots().len(),
        snapshots + 1,
        "only the other's"
    );
    assert_eq!(env.read("Slot1.sav").unwrap(), "[FAKE] here");
    let r = env
        .sync(SyncOptions {
            prefer: Some(Prefer::Local),
            ..Default::default()
        })
        .await;
    assert!(r.is_clean(), "{r:?}");
    assert_eq!(remote(&env.cloud).await["saves/Slot1.sav"], b"[FAKE] here");
}

/// Epic out of reach when the snapshot is sent: the uploads are errors, no history advances, and
/// the next sync sends them.
#[tokio::test]
async fn a_snapshot_not_sent_is_sent_at_the_next_sync() {
    let env = Env::new("offline");
    env.write("Slot1.sav", "[FAKE] one");
    env.sync(SyncOptions::default()).await;
    env.write("Slot1.sav", "[FAKE] one, later");
    *env.cloud.store.refuse_writes.lock().unwrap() = true;
    let r = env.sync(SyncOptions::default()).await;
    assert!(r.uploaded.is_empty());
    assert!(r.errors[0].1.starts_with("upload not sent"), "{r:?}");
    assert_eq!(remote(&env.cloud).await["saves/Slot1.sav"], b"[FAKE] one");

    *env.cloud.store.refuse_writes.lock().unwrap() = false;
    let r = env.sync(SyncOptions::default()).await;
    assert_eq!(r.uploaded, ["Slot1.sav"]);
    assert_eq!(
        remote(&env.cloud).await["saves/Slot1.sav"],
        b"[FAKE] one, later"
    );
}
