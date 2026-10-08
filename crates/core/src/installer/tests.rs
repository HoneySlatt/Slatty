use std::collections::HashMap;
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
        })
    }
}

impl ContentSource for MemoryContent {
    async fn depot_manifest(&self, manifest: &str) -> Result<Vec<DepotItem>> {
        self.manifests.get(manifest).cloned().ok_or(Error::Http {
            context: "test manifest",
            status: 404,
        })
    }

    async fn chunk(&self, compressed_md5: &str) -> Result<Vec<u8>> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
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
            source.file("Game.exe", b"[FICTIF] executable bytes", &["executable"]),
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
        b"[FICTIF] executable bytes"
    );
}

#[tokio::test]
async fn corrupted_chunk_stops_the_install() {
    let env = Env::new("corrupt");
    let some_chunk = env.source.chunks.keys().next().unwrap().clone();
    *env.source.corrupt.lock().unwrap() = Some(some_chunk);
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
            name: "[FICTIF] Jeu".into(),
        }],
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
fn plan_selects_base_depots_for_one_language() {
    let m = meta(vec![
        depot("1", &["*"], 100),
        depot("1", &["en-US"], 10),
        depot("1", &["fr-FR"], 20),
        depot("2", &["*"], 999),
    ]);
    let p = InstallPlan::new("1", build(), m.clone(), None).unwrap();
    assert_eq!(p.language, "en-US");
    assert_eq!(p.disk_size, 110);
    let fr = InstallPlan::new("1", build(), m.clone(), Some("FR-fr")).unwrap();
    assert_eq!(fr.disk_size, 120);
    assert!(InstallPlan::new("1", build(), m, Some("de-DE")).is_err());
}

#[test]
fn plan_refuses_unsafe_install_directory() {
    let mut m = meta(vec![depot("1", &["*"], 1)]);
    m.install_directory = "../outside".into();
    assert!(
        InstallPlan::new("1", build(), m, None)
            .unwrap()
            .directory_name()
            .is_err()
    );
}
