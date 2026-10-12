use std::sync::Mutex;

use super::*;
use crate::installer::zip::build::{File, installer};

/// Installers in memory, sent in small pieces. `drop_once` cuts the first answer for an entry
/// starting there, as a dropped connection would.
struct Fake {
    installers: Vec<Vec<u8>>,
    drop_once: Mutex<Option<u64>>,
    /// Stands for a disk found full as an entry is written.
    full: std::sync::atomic::AtomicBool,
    opened: AtomicUsize,
}

impl Fake {
    fn new(installers: Vec<Vec<u8>>) -> Self {
        Fake {
            installers,
            drop_once: Mutex::new(None),
            full: Default::default(),
            opened: Default::default(),
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
        self.opened.fetch_add(1, Ordering::SeqCst);
        if self.full.load(Ordering::SeqCst) {
            return Err(Error::io(
                "test write",
                std::io::ErrorKind::StorageFull.into(),
            ));
        }
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

/// A disk found full stops every file at once, rather than each being downloaded to fail the same
/// way.
#[tokio::test]
async fn a_full_disk_stops_the_other_linux_files() {
    let names: Vec<String> = (0..40)
        .map(|n| format!("data/noarch/game/[FAKE] part {n}.pak"))
        .collect();
    let files: Vec<File> = names
        .iter()
        .map(|name| File {
            name,
            data: b"[FAKE] game data",
            mode: 0o100644,
            deflate: false,
        })
        .collect();
    let source = Fake::new(vec![installer(b"#!/bin/sh\n", &files, false)]);
    let part = Part {
        installer: installer_of("1"),
        entries: read_entries(&source, 0).await.unwrap(),
    };
    let set = LinuxSet::new(&[part]).unwrap();
    let root = temp("full");
    source.full.store(true, Ordering::SeqCst);
    let opened = source.opened.load(Ordering::SeqCst);
    let err = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    }
    .run(&set, &root.join(".Game.partial"), &root.join("Game"))
    .await
    .unwrap_err();
    assert!(err.is_disk_full(), "{err}");
    let tried = source.opened.load(Ordering::SeqCst) - opened;
    assert!(tried <= FILE_CONCURRENCY, "{tried} of 40 files tried");
    std::fs::remove_dir_all(root).unwrap();
}

/// A file that cannot be written (a full disk, a quota, a network drive gone) is not taken for a
/// damaged download: a full disk stops the other files, and the error says what failed.
#[tokio::test]
async fn a_file_that_cannot_be_written_is_not_called_corrupted() {
    let source = Fake::new(vec![game(&big())]);
    let set = LinuxSet::new(&[Part {
        installer: installer_of("1"),
        entries: read_entries(&source, 0).await.unwrap(),
    }])
    .unwrap();
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    // Every write to `/dev/full` fails as on a full disk: the large file as it is written, the
    // small one as it is finished.
    for name in ["data.bin", "lang.txt"] {
        let file = set.files.iter().find(|f| f.path.ends_with(name)).unwrap();
        let full = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap();
        let err = dl
            .write_file(
                full,
                Path::new("/dev/full"),
                file,
                &|_| {},
                &AtomicU64::new(0),
            )
            .await
            .unwrap_err();
        assert!(err.is_disk_full(), "{name}: {err}");
    }
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
    // Received: the files as the installer holds them (deflated), the cut answer included.
    let compressed: u64 = set.files.iter().map(|f| f.entry.compressed).sum();
    assert!(
        p.network_bytes > compressed && p.network_bytes < set.disk_size(),
        "{} received, {compressed} compressed, {} written",
        p.network_bytes,
        set.disk_size()
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

/// An installer in several parts (not handled) is left out on its own: the game and its other DLC
/// stay installable. Only a game left with none is refused.
#[test]
fn an_installer_in_several_parts_leaves_the_rest_installable() {
    let raw = br#"{"id":1,"title":"[FAKE] Game","downloads":{"installers":[
        {"os":"linux","language":"de","version":"1","files":[{"downlink":"d1"},{"downlink":"d2"}]},
        {"os":"linux","language":"en","version":"1","files":[{"downlink":"e"}]}]},
        "expanded_dlcs":[
          {"id":2,"title":"[FAKE] Big DLC","downloads":{"installers":[
            {"os":"linux","language":"en","version":"1","files":[{"downlink":"b1"},{"downlink":"b2"}]}]}},
          {"id":3,"title":"[FAKE] Small DLC","downloads":{"installers":[
            {"os":"linux","language":"en","version":"1","files":[{"downlink":"s"}]}]}}]}"#;
    let offer = parse_offer(raw).unwrap();
    assert_eq!(offer.installers.len(), 1);
    assert_eq!(offer.installers[0].downlink, "e");
    assert_eq!(offer.dlcs.len(), 1);
    assert_eq!(offer.dlcs[0].0, "3");

    let raw = br#"{"id":1,"title":"[FAKE] Game","downloads":{"installers":[
        {"os":"linux","language":"en","version":"1","files":[{"downlink":"a"},{"downlink":"b"}]}]}}"#;
    assert!(matches!(parse_offer(raw), Err(Error::Unsupported(_))));
}

#[test]
fn reads_the_linux_installers_of_a_game_and_its_dlc() {
    let raw = br#"{"id":1000000001,"title":"[FAKE] Game","downloads":{"installers":[
        {"id":"installer_windows_en","os":"windows","language":"en","version":"1.5","files":[{"id":"a","downlink":"w"}]},
        {"id":"installer_linux_en","os":"linux","language":"en","version":"1.5.12620","files":[{"id":"en3installer0","downlink":"https://api.gog.com/x"}]}]},
        "expanded_dlcs":[
          {"id":2,"title":"Soundtrack","downloads":{"installers":[]}},
          {"id":3,"title":"Expansion","downloads":{"installers":[
            {"os":"linux","language":"fr","version":null,"files":[{"downlink":"f"}]},
            {"os":"linux","language":"en","version":"2","files":[{"downlink":"e"}]}]}}]}"#;
    let offer = parse_offer(raw).unwrap();
    assert_eq!(offer.title, "[FAKE] Game");
    assert_eq!(offer.installers.len(), 1);
    assert_eq!(offer.installers[0].product_id, "1000000001");
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

/// The address asked for a download link comes from GOG's answer: the account's token goes with
/// it only when that address is GOG's own.
#[tokio::test]
async fn a_download_link_address_outside_gog_is_not_sent_the_token() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = tokio::spawn(async move {
        let Ok(Ok((mut socket, _))) =
            tokio::time::timeout(std::time::Duration::from_secs(2), listener.accept()).await
        else {
            return String::new();
        };
        let mut buf = vec![0u8; 4096];
        let n = socket.read(&mut buf).await.unwrap_or(0);
        let _ = socket
            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            .await;
        String::from_utf8_lossy(&buf[..n]).to_lowercase()
    });
    let tokens = crate::auth::Tokens {
        user_id: "1".into(),
        access_token: crate::secret::Secret::new("FAKE-ACCESS"),
        refresh_token: crate::secret::Secret::new("FAKE-REFRESH"),
        expires_at: i64::MAX,
    };
    let installers = GogInstallers::new(
        crate::http::client().unwrap(),
        tokens,
        None,
        vec![format!("http://{addr}/downlink")],
    );
    assert!(matches!(installers.size(0).await, Err(Error::Refused(_))));
    assert!(!seen.await.unwrap().contains("fake-access"));
}

/// A file inflating to more than the installer's directory says is stopped there, before it
/// fills the disk the space check counted with the size it was given.
#[tokio::test]
async fn a_file_inflating_past_its_stated_size_is_stopped_there() {
    let zeros = vec![0u8; 8 << 20];
    let bytes = installer(
        b"#!/bin/sh\n",
        &[File {
            name: "data/noarch/zeros.bin",
            data: &zeros,
            mode: 0o100644,
            deflate: true,
        }],
        false,
    );
    let source = Fake::new(vec![bytes]);
    let mut set = LinuxSet::new(&[Part {
        installer: installer_of("1"),
        entries: read_entries(&source, 0).await.unwrap(),
    }])
    .unwrap();
    set.files[0].entry.size = 16;
    let root = temp("inflating");
    let most = Mutex::new(0);
    let progress = |p: Progress| {
        let mut most = most.lock().unwrap();
        *most = (*most).max(p.bytes_done);
    };
    let r = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &progress,
        free_space: &plenty,
    }
    .run(&set, &root.join(".p"), &root.join("Game"))
    .await;
    assert!(matches!(r, Err(Error::Refused(_))), "{r:?}");
    assert!(
        *most.lock().unwrap() <= 16,
        "{} written",
        most.lock().unwrap()
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A game writes its folder: a folder of links it replaced with a link out of it never leads a
/// repair to delete or link anything there.
#[tokio::test]
async fn a_repair_never_makes_links_through_a_folder_the_game_replaced() {
    let source = Fake::new(vec![game(&big()), dlc()]);
    let set = LinuxSet::new(&parts(&source).await).unwrap();
    let root = temp("planted-folder");
    let target = root.join("Game");
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    dl.run(&set, &root.join(".p"), &target).await.unwrap();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("libfoo.so"), b"[FAKE] user file").unwrap();
    std::fs::remove_dir_all(target.join("game/lib")).unwrap();
    std::os::unix::fs::symlink(&outside, target.join("game/lib")).unwrap();
    let result = dl.check_installed(&set, &target, true).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    assert_eq!(
        std::fs::read(outside.join("libfoo.so")).unwrap(),
        b"[FAKE] user file"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Nor are the installer's empty folders made there.
#[tokio::test]
async fn a_repair_never_makes_folders_through_a_folder_the_game_replaced() {
    let bytes = installer(
        b"#!/bin/sh\n",
        &[
            File {
                name: "data/noarch/saves/profiles/",
                data: b"",
                mode: 0o40755,
                deflate: false,
            },
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
    let root = temp("planted-dirs");
    let target = root.join("Game");
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    dl.run(&set, &root.join(".p"), &target).await.unwrap();
    assert!(target.join("saves/profiles").is_dir());
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::remove_dir_all(target.join("saves")).unwrap();
    std::os::unix::fs::symlink(&outside, target.join("saves")).unwrap();
    let result = dl.check_installed(&set, &target, true).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    assert!(!outside.join("profiles").exists());
    std::fs::remove_dir_all(root).unwrap();
}

/// An update notes itself just before the first file or link it changes, and not while it only
/// checks them: paused then, it leaves the game playable.
#[tokio::test]
async fn an_update_notes_itself_only_before_changing_a_file_or_link() {
    let source = Fake::new(vec![game(&big()), dlc()]);
    let set = LinuxSet::new(&parts(&source).await).unwrap();
    let root = temp("noted");
    let target = root.join("Game");
    let dl = LinuxDownload {
        source: &source,
        cancel: CancellationToken::new(),
        progress: &|_| {},
        free_space: &plenty,
    };
    dl.run(&set, &root.join(".p"), &target).await.unwrap();
    let seen = Mutex::new(Vec::new());
    let begin = || {
        let link = std::fs::read_link(target.join("game/lib/libfoo.so")).unwrap();
        let lang = std::fs::read(target.join("game/lang.txt")).unwrap();
        seen.lock().unwrap().push((lang, link));
        Ok(())
    };
    let first_seen = || {
        std::mem::take(&mut *seen.lock().unwrap())
            .into_iter()
            .next()
    };
    dl.replace_bad(&set, &target, &begin).await.unwrap();
    assert_eq!(first_seen(), None, "nothing to change");

    std::fs::remove_file(target.join("game/lib/libfoo.so")).unwrap();
    std::os::unix::fs::symlink("elsewhere", target.join("game/lib/libfoo.so")).unwrap();
    dl.replace_bad(&set, &target, &begin).await.unwrap();
    assert_eq!(
        first_seen(),
        Some((b"en+dlc".to_vec(), PathBuf::from("elsewhere"))),
        "noted before the link changes"
    );

    std::fs::write(target.join("game/lang.txt"), b"[FAKE] damaged").unwrap();
    dl.replace_bad(&set, &target, &begin).await.unwrap();
    assert_eq!(
        first_seen(),
        Some((b"[FAKE] damaged".to_vec(), PathBuf::from("libfoo.so.1"))),
        "noted before the file changes"
    );
    assert_eq!(
        std::fs::read(target.join("game/lang.txt")).unwrap(),
        b"en+dlc"
    );
    std::fs::remove_dir_all(root).unwrap();
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
