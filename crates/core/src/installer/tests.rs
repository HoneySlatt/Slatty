use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use md5::{Digest, Md5};
use tokio_util::sync::CancellationToken;

use super::*;
use crate::fsutil;
use crate::galaxy::{Build, Chunk, ContentSource, Depot, DepotFile, DepotItem, Meta, Product};

const CHUNK: usize = 4;

/// In-memory CDN with fault injection; content is fictitious test data.
#[derive(Default)]
struct MemoryContent {
    manifests: HashMap<String, Vec<DepotItem>>,
    chunks: HashMap<String, Vec<u8>>,
    fetched: AtomicUsize,
    fail_on: Mutex<Option<String>>,
    corrupt: Mutex<Option<String>>,
    /// This chunk arrives late, after the ones that follow it.
    slow: Mutex<Option<String>>,
    products: Mutex<std::collections::HashSet<String>>,
}

impl MemoryContent {
    fn file(&mut self, path: &str, content: &[u8], flags: &[&str]) -> DepotItem {
        let chunks = content
            .chunks(CHUNK)
            .map(|part| {
                let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
                z.write_all(part).unwrap();
                let compressed = z.finish().unwrap();
                let c = Chunk {
                    md5: fsutil::hex(&Md5::digest(part)),
                    compressed_md5: fsutil::hex(&Md5::digest(&compressed)),
                    size: part.len() as u64,
                    compressed_size: compressed.len() as u64,
                };
                self.chunks.insert(c.compressed_md5.clone(), compressed);
                c
            })
            .collect();
        DepotItem::DepotFile(DepotFile {
            path: path.into(),
            chunks,
            flags: flags.iter().map(|f| f.to_string()).collect(),
            md5: None,
            sfc_ref: None,
            product_id: String::new(),
        })
    }
}

impl ContentSource for MemoryContent {
    async fn depot_manifest(&self, depot: &Depot) -> Result<Vec<DepotItem>> {
        self.manifests
            .get(&depot.manifest)
            .cloned()
            .ok_or(Error::Http {
                context: "test manifest",
                status: 404,
            })
    }

    async fn chunk(&self, product_id: &str, compressed_md5: &str) -> Result<Vec<u8>> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
        self.products.lock().unwrap().insert(product_id.to_string());
        if self.fail_on.lock().unwrap().as_deref() == Some(compressed_md5) {
            return Err(Error::Http {
                context: "test network",
                status: 503,
            });
        }
        let mut data = self
            .chunks
            .get(compressed_md5)
            .cloned()
            .ok_or(Error::Http {
                context: "test chunk",
                status: 404,
            })?;
        if self.corrupt.lock().unwrap().as_deref() == Some(compressed_md5) {
            data[0] ^= 0xff;
        }
        let slow = self.slow.lock().unwrap().as_deref() == Some(compressed_md5);
        if slow {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        Ok(data)
    }
}

struct Env {
    root: PathBuf,
    source: MemoryContent,
    depots: Vec<Depot>,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("slatty-inst-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut source = MemoryContent::default();
        let items = vec![
            source.file("Game.exe", b"[FAKE] executable bytes", &["executable"]),
            source.file("data\\level1.pak", b"0123456789abcdefghij", &[]),
            source.file("goggame-1.info", b"{}", &[]),
            source.file("empty.txt", b"", &[]),
            source.file("support.ico", b"icon", &["support"]),
            DepotItem::DepotDirectory {
                path: "saves".into(),
            },
        ];
        source.manifests.insert("m1".into(), items);
        let depots = vec![Depot {
            product_id: "1".into(),
            languages: vec!["*".into()],
            manifest: "m1".into(),
            size: 0,
            compressed_size: 0,
        }];
        Env {
            root,
            source,
            depots,
        }
    }

    fn target(&self) -> PathBuf {
        self.root.join("Game")
    }

    fn partial(&self) -> PathBuf {
        partial_dir(&self.root, "Game")
    }

    async fn run(&self, cancel: &CancellationToken, free: u64) -> Result<()> {
        let set = collect_files(&self.source, &self.depots).await?;
        let free_space = move |_: &Path| Ok(free);
        Download {
            source: &self.source,
            cancel: cancel.clone(),
            progress: &|_| {},
            free_space: &free_space,
        }
        .run(&set, &self.partial(), &self.target())
        .await
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const PLENTY: u64 = 1 << 40;

#[tokio::test]
async fn downloading_cannot_truncate_a_temporary_symlink_target() {
    let env = Env::new("temporary-link");
    std::fs::create_dir_all(env.partial()).unwrap();
    let outside = env.root.join("unrelated");
    std::fs::write(&outside, b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, env.partial().join("Game.exe.slatty-dl")).unwrap();
    let _ = env.run(&CancellationToken::new(), PLENTY).await;
    assert_eq!(std::fs::read(outside).unwrap(), b"keep");
}

#[tokio::test]
async fn installs_verified_files_and_publishes_atomically() {
    let env = Env::new("ok");
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let t = env.target();
    assert_eq!(
        std::fs::read(t.join("data/level1.pak")).unwrap(),
        b"0123456789abcdefghij"
    );
    assert_eq!(std::fs::read(t.join("empty.txt")).unwrap(), b"");
    assert!(t.join("saves").is_dir());
    assert!(!t.join("support.ico").exists());
    assert_eq!(
        std::fs::metadata(t.join("Game.exe"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0o111
    );
    assert!(!env.partial().exists());
}

#[tokio::test]
async fn chunks_arriving_out_of_order_land_at_their_place() {
    let env = Env::new("order");
    let level = env.source.manifests["m1"]
        .iter()
        .find_map(|i| match i {
            DepotItem::DepotFile(f) if f.path.ends_with("level1.pak") => Some(f.clone()),
            _ => None,
        })
        .unwrap();
    *env.source.slow.lock().unwrap() = Some(level.chunks[0].compressed_md5.clone());
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    assert_eq!(
        std::fs::read(env.target().join("data/level1.pak")).unwrap(),
        b"0123456789abcdefghij"
    );
}

#[tokio::test]
async fn interrupted_install_resumes_without_refetching_finished_files() {
    let env = Env::new("resume");
    let total_chunks = env.source.chunks.len() - 1;
    let DepotItem::DepotFile(exe) = &env.source.manifests["m1"][0] else {
        unreachable!()
    };
    *env.source.fail_on.lock().unwrap() = Some(exe.chunks.last().unwrap().compressed_md5.clone());
    assert!(env.run(&CancellationToken::new(), PLENTY).await.is_err());
    assert!(
        !env.target().exists(),
        "nothing may be published after a failure"
    );
    assert!(env.partial().exists());

    *env.source.fail_on.lock().unwrap() = None;
    let before = env.source.fetched.load(Ordering::SeqCst);
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let refetched = env.source.fetched.load(Ordering::SeqCst) - before;
    assert!(
        refetched < total_chunks,
        "{refetched} of {total_chunks} chunks fetched again"
    );
    assert_eq!(
        std::fs::read(env.target().join("Game.exe")).unwrap(),
        b"[FAKE] executable bytes"
    );
}

#[tokio::test]
async fn corrupted_chunk_stops_the_install() {
    let env = Env::new("corrupt");
    let DepotItem::DepotFile(pak) = &env.source.manifests["m1"][1] else {
        unreachable!()
    };
    *env.source.corrupt.lock().unwrap() = Some(pak.chunks[2].compressed_md5.clone());
    let err = env
        .run(&CancellationToken::new(), PLENTY)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("corrupted"));
    assert!(!env.target().exists());
}

#[tokio::test]
async fn tampered_file_left_from_an_earlier_attempt_is_redownloaded() {
    let env = Env::new("tamper");
    std::fs::create_dir_all(env.partial().join("data")).unwrap();
    std::fs::write(
        env.partial().join("data/level1.pak"),
        b"XXXXXXXXXXXXXXXXXXXX",
    )
    .unwrap();
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    assert_eq!(
        std::fs::read(env.target().join("data/level1.pak")).unwrap(),
        b"0123456789abcdefghij"
    );
}

#[tokio::test]
async fn paths_escaping_the_install_folder_are_refused() {
    for bad in [
        "..\\..\\evil.dll",
        "/etc/passwd",
        "C:\\Windows\\x.dll",
        "a/../../b",
    ] {
        let mut env = Env::new("escape");
        let item = env.source.file(bad, b"x", &[]);
        env.source.manifests.get_mut("m1").unwrap().push(item);
        assert!(
            matches!(
                env.run(&CancellationToken::new(), PLENTY).await,
                Err(Error::Refused(_))
            ),
            "{bad}"
        );
        assert_eq!(
            env.source.fetched.load(Ordering::SeqCst),
            0,
            "nothing downloaded for {bad}"
        );
    }
}

#[tokio::test]
async fn cancellation_keeps_partial_data_and_publishes_nothing() {
    let env = Env::new("cancel");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        env.run(&cancel, PLENTY).await,
        Err(Error::Cancelled)
    ));
    assert!(!env.target().exists());
}

#[tokio::test]
async fn insufficient_space_is_refused_before_downloading() {
    let env = Env::new("space");
    assert!(matches!(
        env.run(&CancellationToken::new(), 10).await,
        Err(Error::Refused(_))
    ));
    assert_eq!(env.source.fetched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn existing_destination_is_never_overwritten() {
    let env = Env::new("exists");
    std::fs::create_dir_all(env.target()).unwrap();
    std::fs::write(env.target().join("mine.txt"), b"keep").unwrap();
    assert!(env.run(&CancellationToken::new(), PLENTY).await.is_err());
    assert_eq!(
        std::fs::read(env.target().join("mine.txt")).unwrap(),
        b"keep"
    );
}

#[tokio::test]
async fn paths_differing_by_case_are_one_file() {
    let mut env = Env::new("case");
    let item = env.source.file("DATA\\LEVEL1.PAK", b"override!", &[]);
    env.source.manifests.get_mut("m1").unwrap().push(item);
    let set = collect_files(&env.source, &env.depots).await.unwrap();
    let paks: Vec<_> = set
        .files
        .iter()
        .filter(|(p, _)| p.to_string_lossy().to_lowercase().contains("level1"))
        .collect();
    assert_eq!(paks.len(), 1);
    assert_eq!(paks[0].0, PathBuf::from("data/level1.pak"));
    assert_eq!(paks[0].1.size(), 9);
}

fn build() -> Build {
    Build {
        build_id: "b1".into(),
        version_name: "1.0".into(),
        link: String::new(),
        branch: None,
        generation: 2,
        date_published: None,
    }
}

fn meta(depots: Vec<Depot>) -> Meta {
    Meta {
        version: Some(2),
        base_product_id: "1".into(),
        client_id: Some("c".into()),
        install_directory: "Game".into(),
        depots,
        dependencies: vec!["MSVC2017".into()],
        products: vec![Product {
            product_id: "1".into(),
            name: "[FAKE] Game".into(),
            temp_executable: None,
            temp_arguments: None,
        }],
        script_interpreter: false,
    }
}

fn depot(product: &str, langs: &[&str], size: u64) -> Depot {
    Depot {
        product_id: product.into(),
        languages: langs.iter().map(|l| l.to_string()).collect(),
        manifest: format!("{product}-{}", langs.join("+")),
        size,
        compressed_size: size / 2,
    }
}

#[test]
fn plan_selects_depots_for_one_language_and_owned_dlc() {
    let mut m = meta(vec![
        depot("1", &["*"], 100),
        depot("1", &["en-US"], 10),
        depot("1", &["fr-FR"], 20),
        depot("2", &["*"], 999),
        depot("3", &["*"], 7),
    ]);
    m.products.push(Product {
        product_id: "2".into(),
        name: "[FAKE] Owned DLC".into(),
        temp_executable: None,
        temp_arguments: None,
    });
    m.products.push(Product {
        product_id: "3".into(),
        name: "[FAKE] Other DLC".into(),
        temp_executable: None,
        temp_arguments: None,
    });
    let owned: HashSet<String> = ["1", "2"].map(String::from).into();
    let plan = |lang: Option<&str>, sel: DlcSelection| {
        InstallPlan::new("1", build(), m.clone(), lang, &owned, &sel)
    };

    let all = plan(None, DlcSelection::AllOwned).unwrap();
    assert_eq!(all.language, "en-US");
    assert_eq!(all.disk_size, 110 + 999);
    assert_eq!(all.selected_dlcs(), vec!["2"]);
    assert!(
        all.dlcs
            .iter()
            .any(|d| d.id == "3" && !d.owned && !d.selected)
    );

    let base_only = plan(Some("FR-fr"), DlcSelection::Only(vec![])).unwrap();
    assert_eq!(base_only.disk_size, 120);
    assert!(
        plan(None, DlcSelection::Only(vec!["3".into()])).is_err(),
        "unowned DLC is refused"
    );
    assert!(plan(Some("de-DE"), DlcSelection::AllOwned).is_err());
}

#[test]
fn plan_refuses_unsafe_install_directory() {
    let mut m = meta(vec![depot("1", &["*"], 1)]);
    m.install_directory = "../outside".into();
    let plan = InstallPlan::new(
        "1",
        build(),
        m,
        None,
        &HashSet::new(),
        &DlcSelection::AllOwned,
    )
    .unwrap();
    assert!(plan.directory_name().is_err());
}

#[tokio::test]
async fn check_finds_damaged_files_and_repair_fetches_only_those() {
    let env = Env::new("repair");
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let game = env.target();
    std::fs::write(game.join("data/level1.pak"), b"0123456789abcdefghiX").unwrap();
    std::fs::remove_file(game.join("goggame-1.info")).unwrap();
    std::fs::write(game.join("saves/mine.sav"), b"keep me").unwrap();

    let set = collect_files(&env.source, &env.depots).await.unwrap();
    let free_space = |_: &Path| Ok(PLENTY);
    let dl = Download {
        source: &env.source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };

    let before = env.source.fetched.load(Ordering::SeqCst);
    let bad = dl.check_installed(&set, &game, false).await.unwrap().bad;
    assert_eq!(
        bad,
        vec![
            PathBuf::from("data/level1.pak"),
            PathBuf::from("goggame-1.info")
        ]
    );
    assert_eq!(
        env.source.fetched.load(Ordering::SeqCst),
        before,
        "checking downloads nothing"
    );

    let repaired = dl.check_installed(&set, &game, true).await.unwrap();
    assert_eq!(repaired.bad, bad);
    let fetched = env.source.fetched.load(Ordering::SeqCst) - before;
    assert_eq!(
        fetched,
        1 + 1,
        "only the damaged chunk of level1.pak, and the missing file"
    );
    assert_eq!(repaired.reused_bytes, 16, "intact chunks copied locally");
    assert_eq!(
        std::fs::read(game.join("data/level1.pak")).unwrap(),
        b"0123456789abcdefghij"
    );
    assert_eq!(
        std::fs::read(game.join("saves/mine.sav")).unwrap(),
        b"keep me"
    );
    assert!(
        dl.check_installed(&set, &game, false)
            .await
            .unwrap()
            .bad
            .is_empty()
    );
}

#[tokio::test]
async fn update_replaces_changed_files_and_removes_dropped_ones_only() {
    let mut env = Env::new("update");
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let game = env.target();
    std::fs::write(game.join("saves/mine.sav"), b"keep me").unwrap();
    let old_set = collect_files(&env.source, &env.depots).await.unwrap();
    let old_record = InstallRecord {
        build_id: "b1".into(),
        version: "1.0".into(),
        language: "en-US".into(),
        path: Some(game.clone()),
        dlcs: vec![],
        setup_build: None,
        files: recorded_files(&old_set),
    };

    let items = vec![
        env.source
            .file("Game.exe", b"[FAKE] executable bytes", &["executable"]),
        env.source
            .file("data\\level1.pak", b"0123456789 version two", &[]),
        env.source.file("data\\level2.pak", b"brand new level", &[]),
        env.source.file("empty.txt", b"", &[]),
        DepotItem::DepotDirectory {
            path: "saves".into(),
        },
    ];
    env.source.manifests.insert("m2".into(), items);
    let depots = vec![Depot {
        manifest: "m2".into(),
        ..env.depots[0].clone()
    }];
    let new_set = collect_files(&env.source, &depots).await.unwrap();

    let free_space = |_: &Path| Ok(PLENTY);
    let dl = Download {
        source: &env.source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };
    let before = env.source.fetched.load(Ordering::SeqCst);
    let report = crate::maintenance::apply_update(&dl, &new_set, &old_record, &game, &[])
        .await
        .unwrap();

    assert_eq!(
        report.downloaded,
        vec![
            PathBuf::from("data/level1.pak"),
            PathBuf::from("data/level2.pak")
        ]
    );
    assert_eq!(report.removed, vec![PathBuf::from("goggame-1.info")]);
    let fetched = env.source.fetched.load(Ordering::SeqCst) - before;
    // level1.pak keeps its first two chunks ("0123", "4567"); level2.pak is new.
    assert_eq!(fetched, 4 + 4, "only new chunks of changed and new files");
    assert_eq!(report.reused_bytes, 8);
    assert_eq!(
        std::fs::read(game.join("data/level1.pak")).unwrap(),
        b"0123456789 version two"
    );
    assert_eq!(
        std::fs::read(game.join("saves/mine.sav")).unwrap(),
        b"keep me"
    );
    assert!(!game.join("goggame-1.info").exists());
}

#[tokio::test]
async fn chunks_moved_inside_a_changed_file_are_reused() {
    let mut env = Env::new("moved");
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let game = env.target();
    // A whole chunk inserted at the start shifts the old chunks by one chunk.
    let items = vec![
        env.source
            .file("data\\level1.pak", b"NEW!0123456789abcdefghij", &[]),
    ];
    env.source.manifests.insert("m2".into(), items);
    let depots = vec![Depot {
        manifest: "m2".into(),
        ..env.depots[0].clone()
    }];
    let set = collect_files(&env.source, &depots).await.unwrap();
    let free_space = |_: &Path| Ok(PLENTY);
    let dl = Download {
        source: &env.source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };
    let before = env.source.fetched.load(Ordering::SeqCst);
    let checked = dl.check_installed(&set, &game, true).await.unwrap();
    assert_eq!(env.source.fetched.load(Ordering::SeqCst) - before, 1);
    assert_eq!(checked.reused_bytes, 20);
    assert_eq!(
        std::fs::read(game.join("data/level1.pak")).unwrap(),
        b"NEW!0123456789abcdefghij"
    );
}

#[tokio::test]
async fn dlc_files_come_from_their_product_and_removing_the_dlc_deletes_only_them() {
    let mut env = Env::new("dlc");
    let dlc_items = vec![
        env.source
            .file("dlc\\expansion.pak", b"[FAKE] expansion data", &[]),
        env.source.file("goggame-2.info", b"{}", &[]),
    ];
    env.source.manifests.insert("dlc".into(), dlc_items);
    let base = env.depots[0].clone();
    let dlc = Depot {
        product_id: "2".into(),
        manifest: "dlc".into(),
        ..base.clone()
    };
    env.depots = vec![base.clone(), dlc];
    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    let game = env.target();
    assert!(game.join("dlc/expansion.pak").exists());
    let products = env.source.products.lock().unwrap().clone();
    assert_eq!(products, ["1", "2"].map(String::from).into());

    let with_dlc = collect_files(&env.source, &env.depots).await.unwrap();
    let record = InstallRecord {
        build_id: "b1".into(),
        version: "1.0".into(),
        language: "en-US".into(),
        path: Some(game.clone()),
        dlcs: vec!["2".into()],
        setup_build: None,
        files: recorded_files(&with_dlc),
    };
    let base_only = collect_files(&env.source, &[base]).await.unwrap();
    let free_space = |_: &Path| Ok(PLENTY);
    let dl = Download {
        source: &env.source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };
    let report = crate::maintenance::apply_update(&dl, &base_only, &record, &game, &[])
        .await
        .unwrap();
    assert!(report.downloaded.is_empty());
    assert_eq!(
        report.removed,
        vec![
            PathBuf::from("dlc/expansion.pak"),
            PathBuf::from("goggame-2.info")
        ]
    );
    assert!(!game.join("dlc").exists(), "emptied DLC folder removed");
    assert!(game.join("Game.exe").exists());
}

#[tokio::test]
async fn support_files_stay_out_of_the_game_folder_and_dependencies_come_from_the_store() {
    let mut env = Env::new("support");
    let dep_items = vec![
        env.source
            .file("DirectX\\dxsetup.exe", b"[FAKE] redist", &[]),
    ];
    env.source.manifests.insert("dep".into(), dep_items);
    env.depots.push(Depot {
        product_id: crate::galaxy::REDIST.into(),
        manifest: "dep".into(),
        ..env.depots[0].clone()
    });
    let set = collect_files(&env.source, &env.depots).await.unwrap();
    assert_eq!(set.support.len(), 1);
    assert_eq!(set.support[0].0, PathBuf::from("1/support.ico"));

    env.run(&CancellationToken::new(), PLENTY).await.unwrap();
    assert!(!env.target().join("support.ico").exists());
    assert!(env.target().join("DirectX/dxsetup.exe").exists());
    assert!(
        env.source
            .products
            .lock()
            .unwrap()
            .contains(crate::galaxy::REDIST)
    );

    let support = env.root.join("support");
    let free_space = |_: &Path| Ok(PLENTY);
    let dl = Download {
        source: &env.source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };
    dl.check_installed(&set.support_set(), &support, true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(support.join("1/support.ico")).unwrap(),
        b"icon"
    );
}

#[test]
fn discarding_an_unfinished_install_removes_only_its_partial_folder() {
    let root = std::env::temp_dir().join(format!("slatty-discard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dirs = Dirs::under(&root.join("app"));
    let db = Db::in_memory().unwrap();
    std::fs::create_dir_all(partial_dir(&root, "Game").join("data")).unwrap();
    std::fs::create_dir_all(root.join("Other")).unwrap();
    InstallJob {
        game_id: "1".into(),
        build_id: "b".into(),
        language: "en-US".into(),
        root: root.clone(),
        directory: "Game".into(),
        state: "downloading".into(),
        dlcs: vec![],
    }
    .save(&db)
    .unwrap();
    assert_eq!(
        discard(&db, &dirs, "1").unwrap(),
        Some(partial_dir(&root, "Game"))
    );
    assert!(!partial_dir(&root, "Game").exists());
    assert!(root.join("Other").exists());
    assert!(InstallJob::load(&db, "1").unwrap().is_none());
    assert_eq!(discard(&db, &dirs, "1").unwrap(), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_build_without_language_packs_can_be_planned_again() {
    let m = meta(vec![depot("1", &["*"], 100)]);
    let plan = |lang: Option<&str>| {
        InstallPlan::new(
            "1",
            build(),
            m.clone(),
            lang,
            &HashSet::new(),
            &DlcSelection::AllOwned,
        )
    };
    let first = plan(None).unwrap();
    assert_eq!(first.language, "*");
    assert!(first.languages.is_empty());
    let again = plan(Some(&first.language)).unwrap();
    assert_eq!(again.disk_size, 100);
    assert!(plan(Some("fr-FR")).is_err());
}

#[test]
fn an_install_cut_off_after_publishing_is_checked_where_it_landed() {
    let root = std::env::temp_dir().join(format!("slatty-published-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (partial, target) = (partial_dir(&root, "Game"), root.join("Game"));
    std::fs::create_dir_all(&partial).unwrap();
    assert!(
        !published_unregistered(true, &partial, &target),
        "still downloading"
    );
    std::fs::rename(&partial, &target).unwrap();
    assert!(published_unregistered(true, &partial, &target));
    assert!(
        !published_unregistered(false, &partial, &target),
        "a new install never adopts an existing folder"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn jobs_left_behind_by_a_registered_install_are_forgotten() {
    use crate::install::{Install, Platform};
    use crate::runner::Runner;
    let db = Db::in_memory().unwrap();
    let job = |id: &str, state: &str| InstallJob {
        game_id: id.into(),
        build_id: "b".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game".into(),
        state: state.into(),
        dlcs: vec![],
    };
    job("1", "downloading").save(&db).unwrap();
    job("2", crate::maintenance::UPDATING).save(&db).unwrap();
    job("3", PAUSED).save(&db).unwrap();
    job("4", crate::maintenance::UPDATE_PAUSED)
        .save(&db)
        .unwrap();
    for id in ["1", "2", "4"] {
        Install {
            game_id: id.into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
        }
        .save(&db)
        .unwrap();
    }
    InstallJob::forget_finished(&db).unwrap();
    let left: Vec<String> = InstallJob::list(&db)
        .unwrap()
        .into_iter()
        .map(|j| j.game_id)
        .collect();
    assert_eq!(
        left,
        ["2", "3", "4"],
        "updates of installed games, paused or not, and other installs stay"
    );
    assert!(InstallJob::load(&db, "3").unwrap().unwrap().is_paused());
    let paused_update = InstallJob::load(&db, "4").unwrap().unwrap();
    assert!(paused_update.is_update() && paused_update.is_paused());
    assert!(
        crate::maintenance::update_pending(&db, "4")
            .unwrap()
            .is_some(),
        "a paused update still holds the game back"
    );
}

#[test]
fn steam_games_are_never_recorded_as_installed_here() {
    use crate::install::{Install, Platform};
    use crate::runner::Runner;
    let root = std::env::temp_dir().join(format!("slatty-steam-ids-{}", std::process::id()));
    let dirs = Dirs::under(&root);
    let db = Db::in_memory().unwrap();
    let refused = |r: Result<()>| assert!(matches!(r, Err(Error::Refused(_))), "{r:?}");
    refused(
        Install {
            game_id: "steam-440".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Native,
            umu_id: None,
        }
        .save(&db),
    );
    refused(
        InstallJob {
            game_id: "steam-440".into(),
            build_id: "b".into(),
            language: "en-US".into(),
            root: "/games".into(),
            directory: "Game".into(),
            state: QUEUED.into(),
            dlcs: vec![],
        }
        .save(&db),
    );
    let record = InstallRecord {
        build_id: "b".into(),
        version: "1".into(),
        language: "en-US".into(),
        path: None,
        dlcs: vec![],
        setup_build: None,
        files: vec![],
    };
    for id in ["steam-440", "../1"] {
        refused(record.save(&dirs, id));
    }
    assert!(Install::list(&db).unwrap().is_empty());
    assert!(InstallJob::list(&db).unwrap().is_empty());
    assert!(!root.exists(), "nothing written");
}

#[test]
fn operations_on_a_game_id_unsafe_in_a_file_name_are_refused() {
    let root = std::env::temp_dir().join(format!("slatty-unsafe-id-{}", std::process::id()));
    let dirs = Dirs::under(&root);
    assert!(matches!(
        crate::lock::game(&dirs, "../1"),
        Err(Error::Refused(_))
    ));
    assert!(crate::lock::game(&dirs, "steam-440").is_ok());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_queued_install_waits_and_keeps_its_place() {
    let db = Db::in_memory().unwrap();
    let job = InstallJob {
        game_id: "1".into(),
        build_id: "b".into(),
        language: "en-US".into(),
        root: "/games".into(),
        directory: "Game".into(),
        state: QUEUED.into(),
        dlcs: vec![],
    };
    job.save(&db).unwrap();
    let job = InstallJob::load(&db, "1").unwrap().unwrap();
    assert!(job.is_queued() && !job.is_paused() && !job.is_update());
    let order = vec!["2".to_string(), "1".to_string()];
    crate::settings::set_download_queue(&db, &order).unwrap();
    assert_eq!(crate::settings::download_queue(&db).unwrap(), order);
    crate::settings::set_download_queue(&db, &[]).unwrap();
    assert!(crate::settings::download_queue(&db).unwrap().is_empty());
}
