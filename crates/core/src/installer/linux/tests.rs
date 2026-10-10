use std::sync::Mutex;

use super::*;
use crate::installer::zip::build::{File, installer};

/// Installers in memory, sent in small pieces. `drop_once` cuts the first answer for an entry
/// starting there, as a dropped connection would.
struct Fake {
    installers: Vec<Vec<u8>>,
    drop_once: Mutex<Option<u64>>,
}

impl Fake {
    fn new(installers: Vec<Vec<u8>>) -> Self {
        Fake {
            installers,
            drop_once: Mutex::new(None),
        }
    }
}

impl Source for Fake {
    async fn size(&self, part: usize) -> Result<u64> {
        Ok(self.installers[part].len() as u64)
    }

    async fn open(
        &self,
        part: usize,
        start: u64,
        end: u64,
    ) -> Result<BoxStream<'static, Result<Vec<u8>>>> {
        let mut data = self.installers[part][start as usize..end as usize].to_vec();
        if self
            .drop_once
            .lock()
            .unwrap()
            .take_if(|at| *at == start)
            .is_some()
        {
            data.truncate(data.len() / 2);
        }
        let pieces: Vec<Result<Vec<u8>>> = data.chunks(700).map(|c| Ok(c.to_vec())).collect();
        Ok(futures::stream::iter(pieces).boxed())
    }
}

fn big() -> Vec<u8> {
    (0..300_000u32)
        .flat_map(|i| (i % 251).to_le_bytes())
        .collect()
}

fn game(big: &[u8]) -> Vec<u8> {
    installer(
        b"#!/bin/sh\n# MojoSetup\nexit 0\n",
        &[
            File {
                name: "scripts/config.lua",
                data: b"-- not the game",
                mode: 0o100644,
                deflate: false,
            },
            File {
                name: "data/noarch/",
                data: b"",
                mode: 0o40755,
                deflate: false,
            },
            File {
                name: "data/noarch/start.sh",
                data: b"#!/bin/sh\n./game/run\n",
                mode: 0o100777,
                deflate: false,
            },
            File {
                name: "data/noarch/game/data.bin",
                data: big,
                mode: 0o100664,
                deflate: true,
            },
            File {
                name: "data/noarch/game/empty.txt",
                data: b"",
                mode: 0o100644,
                deflate: false,
            },
            File {
                name: "data/noarch/game/lib/libfoo.so",
                data: b"libfoo.so.1",
                mode: 0o120777,
                deflate: false,
            },
            File {
                name: "data/noarch/game/lib/escape",
                data: b"../../../../etc/passwd",
                mode: 0o120777,
                deflate: false,
            },
            File {
                name: "data/noarch/game/lang.txt",
                data: b"en",
                mode: 0o100644,
                deflate: true,
            },
        ],
        true,
    )
}

fn dlc() -> Vec<u8> {
    installer(
        b"#!/bin/sh\n",
        &[
            File {
                name: "data/noarch/game/lang.txt",
                data: b"en+dlc",
                mode: 0o100644,
                deflate: true,
            },
            File {
                name: "data/noarch/dlc/extra.pak",
                data: &[3u8; 4000],
                mode: 0o100644,
                deflate: true,
            },
        ],
        false,
    )
}

fn installer_of(id: &str) -> Installer {
    Installer {
        product_id: id.into(),
        language: "en".into(),
        version: "1.0".into(),
        downlink: String::new(),
    }
}

async fn parts(source: &Fake) -> Vec<Part> {
    let mut out = Vec::new();
    for (i, id) in ["1", "2"].iter().enumerate() {
        out.push(Part {
            installer: installer_of(id),
            entries: read_entries(source, i).await.unwrap(),
        });
    }
    out
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slatty-linux-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn plenty(_: &Path) -> Result<u64> {
    Ok(u64::MAX / 4)
}

#[tokio::test]
async fn a_linux_download_does_not_truncate_a_staging_link_target() {
    let source = Fake::new(vec![game(&big()), dlc()]);
    let set = LinuxSet::new(&parts(&source).await).unwrap();
    let root = temp("staging-link");
    let partial = root.join("partial");
    std::fs::create_dir_all(&partial).unwrap();
    let outside = root.join("unrelated");
    std::fs::write(&outside, b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, partial.join("start.sh.slatty-dl")).unwrap();
    LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    }
    .run(&set, &partial, &root.join("game"))
    .await
    .unwrap();
    assert_eq!(std::fs::read(outside).unwrap(), b"keep");
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn installs_the_game_files_and_the_dlc_over_them() {
    let big = big();
    let source = Fake::new(vec![game(&big), dlc()]);
    let parts = parts(&source).await;
    assert!(
        parts[0]
            .entries
            .iter()
            .all(|e| e.name.starts_with(GAME_DIR))
    );
    let set = LinuxSet::new(&parts).unwrap();
    assert_eq!(set.files.len(), 5, "{:?}", set.files);
    assert_eq!(set.links.len(), 2);

    let root = temp("install");
    let (partial, target) = (root.join(".Game.partial"), root.join("Game"));
    // The first answer for the big file is cut: it is asked again.
    let big_entry = &set
        .files
        .iter()
        .find(|f| f.path.ends_with("data.bin"))
        .unwrap()
        .entry;
    *source.drop_once.lock().unwrap() = Some(big_entry.header);
    let last = Mutex::new(Progress::default());
    let progress = |p: Progress| *last.lock().unwrap() = p;
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &progress,
        free_space: &plenty,
    };
    let skipped = dl.run(&set, &partial, &target).await.unwrap();
    assert_eq!(skipped, 1, "the link out of the game is left out");
    assert!(!partial.exists());
    assert_eq!(std::fs::read(target.join("game/data.bin")).unwrap(), big);
    assert_eq!(
        std::fs::read(target.join("game/lang.txt")).unwrap(),
        b"en+dlc"
    );
    assert_eq!(
        std::fs::read(target.join("dlc/extra.pak")).unwrap(),
        [3u8; 4000]
    );
    assert_eq!(std::fs::read(target.join("game/empty.txt")).unwrap(), b"");
    assert!(!target.join("scripts").exists());
    let mode = |p: &str| {
        std::fs::metadata(target.join(p))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode("start.sh"), 0o755);
    assert_eq!(mode("game/data.bin"), 0o644);
    assert_eq!(
        std::fs::read_link(target.join("game/lib/libfoo.so")).unwrap(),
        PathBuf::from("libfoo.so.1")
    );
    assert!(std::fs::symlink_metadata(target.join("game/lib/escape")).is_err());
    let p = *last.lock().unwrap();
    assert_eq!(
        (p.files_done, p.bytes_done),
        (5, set.disk_size()),
        "counted once"
    );

    // Checked in place: a damaged file is found, then repaired.
    assert!(
        dl.check_installed(&set, &target, false)
            .await
            .unwrap()
            .bad
            .is_empty()
    );
    std::fs::write(target.join("game/data.bin"), b"damaged").unwrap();
    let checked = dl.check_installed(&set, &target, true).await.unwrap();
    assert_eq!(checked.bad, vec![PathBuf::from("game/data.bin")]);
    assert_eq!(std::fs::read(target.join("game/data.bin")).unwrap(), big);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_corrupted_download_is_never_kept() {
    let big = big();
    let mut bytes = game(&big);
    let source = Fake::new(vec![bytes.clone()]);
    let entries = read_entries(&source, 0).await.unwrap();
    let e = entries
        .iter()
        .find(|e| e.name.ends_with("data.bin"))
        .unwrap();
    // A byte of the compressed data changed on the server.
    let at = (e.header + 200) as usize;
    bytes[at] ^= 0xff;
    let source = Fake::new(vec![bytes]);
    let set = LinuxSet::new(&[Part {
        installer: installer_of("1"),
        entries,
    }])
    .unwrap();
    let root = temp("corrupt");
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    let result = dl.run(&set, &root.join(".p"), &root.join("Game")).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    assert!(!root.join("Game").exists());
    assert!(!root.join(".p/game/data.bin").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_cancelled_download_stops_and_keeps_what_is_done() {
    let source = Fake::new(vec![game(&big())]);
    let set = LinuxSet::new(&[Part {
        installer: installer_of("1"),
        entries: read_entries(&source, 0).await.unwrap(),
    }])
    .unwrap();
    let root = temp("cancel");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let dl = LinuxDownload {
        source: &source,
        cancel,
        progress: &|_| {},
        free_space: &plenty,
    };
    let result = dl.run(&set, &root.join(".p"), &root.join("Game")).await;
    assert!(matches!(result, Err(Error::Cancelled)));
    assert!(!root.join("Game").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn links_must_stay_in_the_game() {
    assert!(stays_inside(Path::new("lib/a.so"), Path::new("a.so.1")));
    assert!(stays_inside(Path::new("lib/a.so"), Path::new("../bin/a")));
    assert!(!stays_inside(Path::new("lib/a.so"), Path::new("../../a")));
    assert!(!stays_inside(Path::new("a"), Path::new("/usr/lib/a")));
}

#[test]
fn reads_the_linux_installers_of_a_game_and_its_dlc() {
    let raw = br#"{"id":1308320804,"title":"Hollow Knight","downloads":{"installers":[
        {"id":"installer_windows_en","os":"windows","language":"en","version":"1.5","files":[{"id":"a","downlink":"w"}]},
        {"id":"installer_linux_en","os":"linux","language":"en","version":"1.5.12620","files":[{"id":"en3installer0","downlink":"https://api.gog.com/x"}]}]},
        "expanded_dlcs":[
          {"id":2,"title":"Soundtrack","downloads":{"installers":[]}},
          {"id":3,"title":"Expansion","downloads":{"installers":[
            {"os":"linux","language":"fr","version":null,"files":[{"downlink":"f"}]},
            {"os":"linux","language":"en","version":"2","files":[{"downlink":"e"}]}]}}]}"#;
    let offer = parse_offer(raw).unwrap();
    assert_eq!(offer.title, "Hollow Knight");
    assert_eq!(offer.installers.len(), 1);
    assert_eq!(offer.installers[0].product_id, "1308320804");
    assert_eq!(offer.installers[0].version, "1.5.12620");
    assert_eq!(
        offer.dlcs.len(),
        1,
        "a DLC without Linux installer is not offered"
    );
    let (id, name, installers) = &offer.dlcs[0];
    assert_eq!((id.as_str(), name.as_str()), ("3", "Expansion"));
    assert_eq!(pick(installers, "fr").unwrap().downlink, "f");
    assert_eq!(
        pick(installers, "de").unwrap().downlink,
        "e",
        "English otherwise"
    );
}

#[tokio::test]
async fn links_chained_out_of_the_game_are_removed_and_never_written_through() {
    let link = |name, target: &'static [u8]| File {
        name,
        data: target,
        mode: 0o120777,
        deflate: false,
    };
    // Each stays inside by its own path; together they lead to the folder above the game.
    let bytes = installer(
        b"#!/bin/sh\n",
        &[
            File {
                name: "data/noarch/sub/",
                data: b"",
                mode: 0o40755,
                deflate: false,
            },
            link("data/noarch/sub/up", b".."),
            link("data/noarch/escape", b"sub/up/.."),
            File {
                name: "data/noarch/game.bin",
                data: b"game",
                mode: 0o100644,
                deflate: false,
            },
        ],
        false,
    );
    let source = Fake::new(vec![bytes]);
    let set = LinuxSet::new(&[Part {
        installer: installer_of("1"),
        entries: read_entries(&source, 0).await.unwrap(),
    }])
    .unwrap();
    let root = temp("chain");
    let target = root.join("Game");
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    let skipped = dl.run(&set, &root.join(".p"), &target).await.unwrap();
    assert_eq!(skipped, 1);
    assert!(
        std::fs::read_link(target.join("sub/up")).is_ok(),
        "harmless alone"
    );
    assert!(std::fs::symlink_metadata(target.join("escape")).is_err());

    // A link someone else made out of the folder is never written through.
    std::os::unix::fs::symlink(&root, target.join("outside")).unwrap();
    let through = LinuxSet {
        files: vec![LinuxFile {
            path: PathBuf::from("outside/planted.bin"),
            ..set.files[0].clone()
        }],
        ..Default::default()
    };
    let result = dl.check_installed(&through, &target, true).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    assert!(!root.join("planted.bin").exists());
    std::fs::remove_dir_all(root).unwrap();
}
