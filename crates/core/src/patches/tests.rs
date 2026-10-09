use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use flate2::Compression;
use flate2::write::ZlibEncoder;
use oxidelta::compress::encoder::{self, CompressOptions};

use super::*;
use crate::galaxy::{Depot, DepotFile, DepotItem};
use crate::installer::{Download, recorded_files};

const CHUNK: usize = 4096;

/// In-memory CDN; content is fictitious test data.
#[derive(Default)]
struct Store {
    chunks: HashMap<String, Vec<u8>>,
    fetched: AtomicUsize,
    products: Mutex<HashSet<String>>,
}

impl Store {
    fn chunks(&mut self, data: &[u8]) -> Vec<Chunk> {
        data.chunks(CHUNK)
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
            .collect()
    }
}

impl ContentSource for Store {
    async fn depot_manifest(&self, _: &Depot) -> Result<Vec<DepotItem>> {
        Err(Error::Unsupported("no manifests in this test".into()))
    }

    async fn chunk(&self, product_id: &str, compressed_md5: &str) -> Result<Vec<u8>> {
        self.fetched.fetch_add(1, Ordering::SeqCst);
        self.products.lock().unwrap().insert(product_id.to_string());
        self.chunks.get(compressed_md5).cloned().ok_or(Error::Http {
            context: "test chunk",
            status: 404,
        })
    }
}

/// Deterministic pseudo-random bytes, so deltas are not trivially compressible.
fn bytes(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (x >> 16) as u8
        })
        .collect()
}

fn md5(data: &[u8]) -> String {
    fsutil::hex(&Md5::digest(data))
}

struct Env {
    root: PathBuf,
    store: Store,
    old: Vec<u8>,
    new: Vec<u8>,
    set: FileSet,
    record: InstallRecord,
    patch: FilePatch,
}

impl Env {
    fn new(name: &str) -> Env {
        let root = std::env::temp_dir().join(format!("slatty-patch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Data")).unwrap();
        let old = bytes(200_000, 1);
        let mut new = old.clone();
        new[50_000..50_100].copy_from_slice(&bytes(100, 2));
        new.extend_from_slice(b"[FAKE] appended in the new build");
        std::fs::write(root.join("Data/level1"), &old).unwrap();

        let mut store = Store::default();
        let delta =
            encoder::encode_all(Vec::new(), &old, &new, CompressOptions::default()).unwrap();
        assert!(delta.len() < new.len() / 10, "the delta is small");
        let patch = FilePatch {
            source: "Data\\level1".into(),
            target: "Data\\level1".into(),
            md5_source: md5(&old),
            md5_target: md5(&new),
            chunks: store.chunks(&delta),
            product_id: "1".into(),
        };
        let file = DepotFile {
            path: "Data\\level1".into(),
            chunks: store.chunks(&new),
            flags: vec![],
            md5: None,
            sfc_ref: None,
            product_id: "1".into(),
        };
        let set = FileSet {
            files: vec![(PathBuf::from("Data/level1"), file)],
            ..Default::default()
        };
        let old_set = FileSet {
            files: vec![(
                PathBuf::from("Data/level1"),
                DepotFile {
                    chunks: store.chunks(&old),
                    ..set.files[0].1.clone()
                },
            )],
            ..Default::default()
        };
        let record = InstallRecord {
            build_id: "b1".into(),
            version: "1.0".into(),
            language: "en-US".into(),
            path: Some(root.clone()),
            dlcs: vec![],
            setup_build: None,
            files: recorded_files(&old_set),
        };
        Env {
            root,
            store,
            old,
            new,
            set,
            record,
            patch,
        }
    }

    fn file(&self) -> Vec<u8> {
        std::fs::read(self.root.join("Data/level1")).unwrap()
    }

    fn leftovers(&self) -> Vec<String> {
        std::fs::read_dir(self.root.join("Data"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n != "level1")
            .collect()
    }

    async fn apply(&self) -> Patched {
        apply_all(
            &self.store,
            std::slice::from_ref(&self.patch),
            &self.set,
            &self.record,
            &self.root,
            &CancellationToken::new(),
        )
        .await
        .unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn a_patch_rebuilds_the_new_file_from_the_old_one() {
    let env = Env::new("apply");
    let patched = env.apply().await;
    assert_eq!(patched.files, [PathBuf::from("Data/level1")]);
    assert_eq!(patched.delta_bytes, env.patch.delta_size());
    assert_eq!(env.file(), env.new);
    assert!(env.leftovers().is_empty(), "{:?}", env.leftovers());
    assert_eq!(
        *env.store.products.lock().unwrap(),
        HashSet::from([galaxy::patch_store("1")])
    );
}

#[tokio::test]
async fn an_update_downloads_only_the_delta_of_a_patched_file() {
    let env = Env::new("update");
    let free_space = |_: &Path| Ok(1 << 40);
    let dl = Download {
        source: &env.store,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &free_space,
    };
    let report = crate::maintenance::apply_update(
        &dl,
        &env.set,
        &env.record,
        &env.root,
        std::slice::from_ref(&env.patch),
    )
    .await
    .unwrap();
    assert_eq!(report.patched, [PathBuf::from("Data/level1")]);
    assert!(report.downloaded.is_empty());
    assert_eq!(
        env.store.fetched.load(Ordering::SeqCst),
        env.patch.chunks.len(),
        "no chunk of the new file was downloaded"
    );
    assert_eq!(env.file(), env.new);
}

#[tokio::test]
async fn a_locally_modified_source_is_not_patched() {
    let env = Env::new("modified");
    let mut changed = env.old.clone();
    changed[0] ^= 0xff;
    std::fs::write(env.root.join("Data/level1"), &changed).unwrap();
    let patched = env.apply().await;
    assert!(patched.files.is_empty());
    assert_eq!(env.store.fetched.load(Ordering::SeqCst), 0);
    assert_eq!(env.file(), changed);
}

#[tokio::test]
async fn a_wrong_result_leaves_the_file_untouched() {
    let mut env = Env::new("wrong");
    env.patch.md5_target = md5(b"[FAKE] something else");
    let patched = env.apply().await;
    assert!(patched.files.is_empty());
    assert_eq!(env.file(), env.old);
    assert!(env.leftovers().is_empty(), "{:?}", env.leftovers());
}

#[tokio::test]
async fn a_corrupted_delta_chunk_leaves_the_file_untouched() {
    let mut env = Env::new("corrupt");
    let key = env.patch.chunks[0].compressed_md5.clone();
    env.store.chunks.get_mut(&key).unwrap()[0] ^= 0xff;
    let patched = env.apply().await;
    assert!(patched.files.is_empty());
    assert_eq!(env.file(), env.old);
    assert!(env.leftovers().is_empty(), "{:?}", env.leftovers());
}

#[test]
fn only_xdelta3_depots_of_installed_products_and_language_are_used() {
    let data = |algorithm: &str| PatchData {
        algorithm: algorithm.into(),
        base_product_id: "1".into(),
        depots: vec![
            PatchDepot {
                product_id: "1".into(),
                languages: vec!["*".into()],
                manifest: "m-base".into(),
            },
            PatchDepot {
                product_id: "1".into(),
                languages: vec!["fr-FR".into()],
                manifest: "m-fr".into(),
            },
            PatchDepot {
                product_id: "2".into(),
                languages: vec!["en-US".into()],
                manifest: "m-dlc".into(),
            },
        ],
    };
    let manifests = |d: Option<Vec<PatchDepot>>| -> Vec<String> {
        d.unwrap().into_iter().map(|d| d.manifest).collect()
    };
    assert_eq!(
        manifests(usable_depots(data("xdelta3"), "en-US", &["1".into()])),
        ["m-base"]
    );
    assert_eq!(
        manifests(usable_depots(
            data("xdelta3"),
            "en-US",
            &["1".into(), "2".into()]
        )),
        ["m-base", "m-dlc"]
    );
    assert!(usable_depots(data("bsdiff"), "en-US", &["1".into()]).is_none());
}

#[test]
fn patch_manifests_are_parsed_and_unknown_items_refused() {
    let items: Vec<serde_json::Value> = serde_json::from_str(
        r#"[{"type":"DepotDiff","path_source":"a\\b","path_target":"a\\b","md5_source":"s",
             "md5_target":"t","md5":"d",
             "chunks":[{"md5":"x","compressedMd5":"y","size":3,"compressedSize":2}]}]"#,
    )
    .unwrap();
    let patches = parse_diffs(items, "7").unwrap();
    assert_eq!(patches[0].product_id, "7");
    assert_eq!(patches[0].delta_size(), 2);
    let odd: Vec<serde_json::Value> = serde_json::from_str(r#"[{"type":"DepotLink"}]"#).unwrap();
    assert!(parse_diffs(odd, "7").is_err());
}
