use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;
use crate::install::Platform;

fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("slatty-protons-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[derive(Clone, Copy)]
enum Kind {
    Dir,
    File(u32),
    Link,
    Hard,
}

/// A tar entry written as given, `..` and `/` included, as a hostile archive would be.
fn entry(tar: &mut tar::Builder<Vec<u8>>, path: &str, kind: Kind, data: &[u8], link: &str) {
    let mut h = tar::Header::new_old();
    h.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
    h.as_old_mut().linkname[..link.len()].copy_from_slice(link.as_bytes());
    let (t, mode) = match kind {
        Kind::Dir => (tar::EntryType::Directory, 0o755),
        Kind::File(mode) => (tar::EntryType::Regular, mode),
        Kind::Link => (tar::EntryType::Symlink, 0o777),
        Kind::Hard => (tar::EntryType::Link, 0o644),
    };
    h.set_entry_type(t);
    h.set_mode(mode);
    h.set_size(data.len() as u64);
    h.set_cksum();
    tar.append(&h, data).unwrap();
}

/// A build as the projects publish it: one folder holding `proton` and the rest.
fn build_tar(extra: impl Fn(&mut tar::Builder<Vec<u8>>)) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    entry(&mut tar, "GE-Proton11-7/", Kind::Dir, b"", "");
    entry(
        &mut tar,
        "GE-Proton11-7/proton",
        Kind::File(0o755),
        b"#!/bin/sh\n",
        "",
    );
    entry(&mut tar, "GE-Proton11-7/files/lib/", Kind::Dir, b"", "");
    entry(
        &mut tar,
        "GE-Proton11-7/files/lib/wine.so",
        Kind::File(0o644),
        b"[FAKE] wine",
        "",
    );
    entry(
        &mut tar,
        "GE-Proton11-7/files/lib64",
        Kind::Link,
        b"",
        "lib",
    );
    extra(&mut tar);
    tar.into_inner().unwrap()
}

fn gz(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn xz(data: &[u8]) -> Vec<u8> {
    let mut e = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::default()).unwrap();
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

fn sha512_hex(data: &[u8]) -> String {
    crate::fsutil::hex(&Sha512::digest(data))
}

#[test]
fn the_archive_for_this_computer_is_picked() {
    let ge = [
        "GE-Proton11-7-aarch64.sha512sum",
        "GE-Proton11-7-aarch64.tar.gz",
        "GE-Proton11-7-x86_64.sha512sum",
        "GE-Proton11-7-x86_64.tar.gz",
    ];
    assert_eq!(
        Source::GeProton.pick(&ge, true),
        Some("GE-Proton11-7-x86_64.tar.gz")
    );
    // Older GE releases had a single archive.
    let old = ["GE-Proton9-7.sha512sum", "GE-Proton9-7.tar.gz"];
    assert_eq!(
        Source::GeProton.pick(&old, false),
        Some("GE-Proton9-7.tar.gz")
    );
    let umu = ["UMU-Proton-10.0-4.sha512sum", "UMU-Proton-10.0-4.tar.gz"];
    assert_eq!(
        Source::UmuProton.pick(&umu, false),
        Some("UMU-Proton-10.0-4.tar.gz")
    );
    let cachy = [
        "proton-cachyos-11.0-20261005-slr-arm64.tar.xz",
        "proton-cachyos-11.0-20261005-slr-x86_64.tar.xz",
        "proton-cachyos-11.0-20261005-slr-x86_64_v3.tar.xz",
    ];
    assert_eq!(
        Source::ProtonCachyOs.pick(&cachy, true),
        Some("proton-cachyos-11.0-20261005-slr-x86_64_v3.tar.xz")
    );
    assert_eq!(
        Source::ProtonCachyOs.pick(&cachy, false),
        Some("proton-cachyos-11.0-20261005-slr-x86_64.tar.xz")
    );
    assert_eq!(Source::ProtonCachyOs.pick(&["notes.txt"], true), None);
}

#[test]
fn the_sum_is_the_one_published_for_the_archive() {
    let hex = "a".repeat(128);
    let raw = format!("{hex}  GE-Proton11-7-x86_64.tar.gz\n");
    assert_eq!(
        parse_sum(raw.as_bytes(), "GE-Proton11-7-x86_64.tar.gz").unwrap(),
        hex
    );
    assert!(parse_sum(raw.as_bytes(), "other.tar.gz").is_err());
    assert!(
        parse_sum(
            b"abc  GE-Proton11-7-x86_64.tar.gz",
            "GE-Proton11-7-x86_64.tar.gz"
        )
        .is_err()
    );
}

#[test]
fn an_archive_unpacks_into_its_folder_and_nothing_leaves_it() {
    let root = root("unpack");
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let tar = build_tar(|t| {
        // Climbing out, by name or from `/`.
        entry(
            t,
            "GE-Proton11-7/../../outside/a",
            Kind::File(0o644),
            b"x",
            "",
        );
        entry(t, "/outside/b", Kind::File(0o644), b"x", "");
        // Links out: absolute, by `..`, and through another link (`self` is the folder itself,
        // so `self/up` lands in it and `..` from there is out).
        entry(t, "GE-Proton11-7/abs", Kind::Link, b"", "/etc");
        entry(t, "GE-Proton11-7/dots", Kind::Link, b"", "../../outside");
        entry(t, "GE-Proton11-7/self", Kind::Link, b"", ".");
        entry(t, "GE-Proton11-7/self/up", Kind::Link, b"", "..");
        entry(
            t,
            "GE-Proton11-7/self/up/escaped/c",
            Kind::File(0o644),
            b"x",
            "",
        );
        // A file written where a link was is not written through it.
        entry(
            t,
            "GE-Proton11-7/files/lib64",
            Kind::File(0o644),
            b"replaced",
            "",
        );
        entry(
            t,
            "GE-Proton11-7/files/wine.so",
            Kind::Hard,
            b"",
            "GE-Proton11-7/files/lib/wine.so",
        );
        entry(
            t,
            "GE-Proton11-7/files/passwd",
            Kind::Hard,
            b"",
            "/etc/passwd",
        );
    });
    for (name, data) in [("b.tar.gz", gz(&tar)), ("b.tar.xz", xz(&tar))] {
        let archive = root.join(name);
        std::fs::write(&archive, data).unwrap();
        let to = root.join("to");
        unpack(&archive, &to, &CancellationToken::new(), &|_, _| {}).unwrap();

        let proton = to.join("proton");
        assert_eq!(std::fs::read(&proton).unwrap(), b"#!/bin/sh\n");
        assert_eq!(
            std::fs::metadata(&proton).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert_eq!(
            std::fs::read(to.join("files/lib/wine.so")).unwrap(),
            b"[FAKE] wine"
        );
        assert_eq!(std::fs::read(to.join("files/lib64")).unwrap(), b"replaced");
        assert_eq!(
            std::fs::read(to.join("files/wine.so")).unwrap(),
            b"[FAKE] wine"
        );
        assert!(std::fs::read_link(to.join("self")).is_ok());
        for gone in ["abs", "dots", "up", "files/passwd"] {
            assert!(
                std::fs::symlink_metadata(to.join(gone)).is_err(),
                "{name}: {gone}"
            );
        }
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "{name}");
        assert!(!root.join("escaped").exists(), "{name}");
        std::fs::remove_dir_all(&to).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}

type Routes = HashMap<String, (u16, Vec<u8>)>;

/// Serves the routes `routes` makes from the server's address, with byte ranges, and keeps each
/// request's path and range.
async fn serve(routes: impl FnOnce(&str) -> Routes) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = routes(&base);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = req.split_whitespace().nth(1).unwrap_or("").to_string();
            let range = req.lines().find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("range: bytes=")
                    .and_then(|r| r.trim_end_matches('-').parse::<usize>().ok())
            });
            log.lock().unwrap().push(format!("{path} {range:?}"));
            let (status, body) = routes.get(&path).cloned().unwrap_or((404, Vec::new()));
            let (status, body) = match range {
                Some(start) if status == 200 => (206, body[start.min(body.len())..].to_vec()),
                _ => (status, body),
            };
            let head = format!(
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body).await;
        }
    });
    (base, seen)
}

const LATEST: &str = "/repos/GloriousEggroll/proton-ge-custom/releases/latest";
const ARCHIVE: &str = "GE-Proton11-7-x86_64.tar.gz";

/// GitHub with a GE-Proton release serving `archive`, published with the sum `sum`.
async fn github(archive: Vec<u8>, sum: String) -> (String, Arc<Mutex<Vec<String>>>) {
    serve(move |base| {
        let release = format!(
            r#"{{"tag_name":"GE-Proton11-7","assets":[
              {{"name":"GE-Proton11-7-aarch64.tar.gz","size":1,"browser_download_url":"{base}/arm"}},
              {{"name":"{ARCHIVE}","size":{},"browser_download_url":"{base}/dl/{ARCHIVE}"}},
              {{"name":"GE-Proton11-7-x86_64.sha512sum","size":1,"browser_download_url":"{base}/dl/sum"}}]}}"#,
            archive.len()
        );
        HashMap::from([
            (LATEST.to_string(), (200, release.into_bytes())),
            (format!("/dl/{ARCHIVE}"), (200, archive)),
            ("/dl/sum".to_string(), (200, format!("{sum}  {ARCHIVE}\n").into_bytes())),
        ])
    })
    .await
}

#[tokio::test]
async fn a_build_is_downloaded_checked_and_unpacked_and_a_cut_download_goes_on() {
    let root = root("install");
    let dirs = Dirs::under(&root);
    let archive = gz(&build_tar(|_| {}));
    let (api, seen) = github(archive.clone(), sha512_hex(&archive)).await;
    let http = crate::http::client().unwrap();
    let release = latest(&http, &api, Source::GeProton).await.unwrap();
    assert_eq!(release.version, "GE-Proton11-7");
    assert_eq!(release.name, "GE-Proton11-7-x86_64");
    assert_eq!(release.size, archive.len() as u64);

    // A download cut half way: only the rest is asked for.
    let staging = dir(&dirs).join(".staging");
    std::fs::create_dir_all(&staging).unwrap();
    let half = archive.len() / 2;
    std::fs::write(staging.join(ARCHIVE), &archive[..half]).unwrap();
    let stages = Arc::new(Mutex::new(Vec::new()));
    let log = stages.clone();
    let path = install(
        &http,
        &dirs,
        &release,
        move |s| log.lock().unwrap().push(s),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(path, dir(&dirs).join("GE-Proton11-7-x86_64"));
    assert!(path.join("proton").is_file());
    assert_eq!(installed(&dirs), std::slice::from_ref(&path));
    assert!(
        seen.lock()
            .unwrap()
            .contains(&format!("/dl/{ARCHIVE} Some({half})"))
    );
    {
        let stages = stages.lock().unwrap();
        assert!(stages.contains(&Stage::Verifying));
        assert!(stages.iter().any(|s| matches!(s, Stage::Unpacking { .. })));
    }
    assert!(!staging.join(ARCHIVE).exists(), "the archive is not kept");

    // Already there: nothing is downloaded again.
    let before = seen.lock().unwrap().len();
    install(&http, &dirs, &release, |_| {}, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), before);
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_build_that_does_not_match_its_sum_is_deleted_unopened() {
    let root = root("sum");
    let dirs = Dirs::under(&root);
    let archive = gz(&build_tar(|_| {}));
    let (api, _) = github(archive, "0".repeat(128)).await;
    let http = crate::http::client().unwrap();
    let release = latest(&http, &api, Source::GeProton).await.unwrap();
    let result = install(&http, &dirs, &release, |_| {}, &CancellationToken::new()).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    assert!(installed(&dirs).is_empty());
    let staging = dir(&dirs).join(".staging");
    assert!(!staging.join(ARCHIVE).exists());
    assert!(
        !staging.join("GE-Proton11-7-x86_64").exists(),
        "never unpacked"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn a_release_without_a_sum_or_a_limited_github_is_refused() {
    let http = crate::http::client().unwrap();
    let (api, _) = serve(|base| {
        let release = format!(
            r#"{{"tag_name":"GE-Proton11-7","assets":[
              {{"name":"{ARCHIVE}","size":1,"browser_download_url":"{base}/dl"}}]}}"#
        );
        HashMap::from([(LATEST.to_string(), (200, release.into_bytes()))])
    })
    .await;
    let result = latest(&http, &api, Source::GeProton).await;
    assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
    let (api, _) = serve(|_| HashMap::from([(LATEST.to_string(), (403, Vec::new()))])).await;
    let result = latest(&http, &api, Source::GeProton).await;
    assert!(
        matches!(&result, Err(Error::Refused(m)) if m.contains("try again later")),
        "{result:?}"
    );
}

#[test]
fn a_build_a_game_or_the_default_uses_is_not_removed() {
    let root = root("remove");
    let dirs = Dirs::under(&root);
    let db = Db::in_memory().unwrap();
    let build = dir(&dirs).join("GE-Proton11-7-x86_64");
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("proton"), b"").unwrap();
    let game = Install {
        game_id: "1".into(),
        title: "[FAKE] Game".into(),
        platform: Platform::Windows,
        path: root.join("game"),
        client_id: None,
        runner: Runner::Umu {
            proton: build.clone(),
            prefix: root.join("pfx"),
        },
        umu_id: None,
        isolated: true,
    };
    game.save(&db).unwrap();
    let result = remove(&db, &dirs, &build);
    assert!(
        matches!(&result, Err(Error::Refused(m)) if m.contains("[FAKE] Game")),
        "{result:?}"
    );
    std::fs::create_dir_all(root.join("other")).unwrap();
    std::fs::write(root.join("other/proton"), b"").unwrap();
    crate::install::set_proton(&db, "1", &root.join("other")).unwrap();
    crate::settings::set_default_proton(&db, &build).unwrap();
    assert!(matches!(remove(&db, &dirs, &build), Err(Error::Refused(_))));
    crate::settings::set_default_proton(&db, &root.join("other")).unwrap();
    // Only what SlattyLauncher downloaded.
    for other in [root.join("game"), dir(&dirs).join(".staging"), dir(&dirs)] {
        assert!(
            matches!(remove(&db, &dirs, &other), Err(Error::Refused(_))),
            "{other:?}"
        );
    }
    remove(&db, &dirs, &build).unwrap();
    assert!(!build.exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn github_is_asked_only_once_downloads_are_turned_on() {
    let db = Db::in_memory().unwrap();
    assert!(matches!(check_allowed(&db), Err(Error::Refused(_))));
    crate::settings::set_proton_downloads(&db, true).unwrap();
    assert!(check_allowed(&db).is_ok());
    crate::settings::set_proton_downloads(&db, false).unwrap();
    assert!(matches!(check_allowed(&db), Err(Error::Refused(_))));
}

/// A downloaded build, as `install` leaves it.
fn fake_build(dirs: &Dirs, name: &str) -> PathBuf {
    let build = dir(dirs).join(name);
    std::fs::create_dir_all(&build).unwrap();
    std::fs::write(build.join("proton"), b"").unwrap();
    build
}

fn game_on(db: &Db, root: &Path, id: &str, proton: &Path) {
    Install {
        game_id: id.into(),
        title: format!("[FAKE] Game {id}"),
        platform: Platform::Windows,
        path: root.join(format!("game{id}")),
        client_id: None,
        runner: Runner::Umu {
            proton: proton.to_path_buf(),
            prefix: root.join(format!("pfx{id}")),
        },
        umu_id: None,
        isolated: true,
    }
    .save(db)
    .unwrap();
}

#[tokio::test]
async fn a_followed_build_is_updated_and_the_one_before_kept_to_go_back_to() {
    let root = root("update");
    let dirs = Dirs::under(&root);
    let db = Db::in_memory().unwrap();
    crate::settings::set_proton_downloads(&db, true).unwrap();
    let pinned = fake_build(&dirs, "GE-Proton11-4-x86_64");
    let old = fake_build(&dirs, "GE-Proton11-5-x86_64");
    let current = fake_build(&dirs, "GE-Proton11-6-x86_64");
    let cachy = fake_build(&dirs, "proton-cachyos-11.0-20260602-slr-x86_64_v3");
    std::os::unix::fs::symlink("GE-Proton11-6-x86_64", latest_link(&dirs, Source::GeProton))
        .unwrap();
    // One game follows GE-Proton, another stays on an older build.
    game_on(&db, &root, "1", &latest_link(&dirs, Source::GeProton));
    game_on(&db, &root, "2", &pinned);
    assert_eq!(followed(&db, &dirs).unwrap(), [Source::GeProton]);

    let archive = gz(&build_tar(|_| {}));
    let (api, seen) = github(archive.clone(), sha512_hex(&archive)).await;
    let http = crate::http::client().unwrap();
    let names = Arc::new(Mutex::new(Vec::new()));
    let log = names.clone();
    let updated = update(
        &db,
        &http,
        &dirs,
        &api,
        move |name, _| log.lock().unwrap().push(name.to_string()),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(updated.len(), 1);
    let newest = dir(&dirs).join("GE-Proton11-7-x86_64");
    assert!(
        names
            .lock()
            .unwrap()
            .iter()
            .all(|n| n == "GE-Proton11-7-x86_64")
    );
    assert_eq!(latest_target(&dirs, Source::GeProton), Some(newest.clone()));
    assert!(
        latest_link(&dirs, Source::GeProton)
            .join("proton")
            .is_file()
    );
    // The one before stays to go back to, as does a build a game is set to; an older one goes,
    // and another project's builds are left alone.
    assert!(current.is_dir() && pinned.is_dir() && cachy.is_dir());
    assert!(!old.exists());
    assert!(crate::settings::proton_checked_at(&db).unwrap().is_some());

    // Up to date: GitHub is asked, nothing is downloaded.
    let asked = seen.lock().unwrap().len();
    let updated = update(
        &db,
        &http,
        &dirs,
        &api,
        |_, _| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(updated.is_empty());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), asked + 1);
    assert!(seen.last().unwrap().starts_with(LATEST));
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn nothing_followed_asks_github_nothing() {
    let root = root("unfollowed");
    let dirs = Dirs::under(&root);
    let db = Db::in_memory().unwrap();
    crate::settings::set_proton_downloads(&db, true).unwrap();
    let build = fake_build(&dirs, "GE-Proton11-6-x86_64");
    game_on(&db, &root, "1", &build);
    let (api, seen) = github(Vec::new(), String::new()).await;
    let http = crate::http::client().unwrap();
    let updated = update(
        &db,
        &http,
        &dirs,
        &api,
        |_, _| {},
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(updated.is_empty());
    assert!(seen.lock().unwrap().is_empty());
    // Downloads off: refused before anything.
    crate::settings::set_proton_downloads(&db, false).unwrap();
    let result = update(
        &db,
        &http,
        &dirs,
        &api,
        |_, _| {},
        &CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, Err(Error::Refused(_))));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn builds_are_checked_at_most_once_a_day_and_only_once_both_switches_are_on() {
    let db = Db::in_memory().unwrap();
    let now = 1_791_600_000;
    assert!(!update_due(&db, now).unwrap());
    crate::settings::set_proton_updates(&db, true).unwrap();
    assert!(!update_due(&db, now).unwrap(), "downloads still off");
    crate::settings::set_proton_downloads(&db, true).unwrap();
    assert!(update_due(&db, now).unwrap(), "never checked");
    crate::settings::set_proton_checked_at(&db, now - 3600).unwrap();
    assert!(!update_due(&db, now).unwrap());
    crate::settings::set_proton_checked_at(&db, now - 24 * 3600).unwrap();
    assert!(update_due(&db, now).unwrap());
}

#[test]
fn the_newest_build_stays_while_games_follow_it_and_goes_with_its_link_otherwise() {
    let root = root("remove-latest");
    let dirs = Dirs::under(&root);
    let db = Db::in_memory().unwrap();
    let build = fake_build(&dirs, "GE-Proton11-7-x86_64");
    let link = latest_link(&dirs, Source::GeProton);
    std::os::unix::fs::symlink("GE-Proton11-7-x86_64", &link).unwrap();
    assert_eq!(
        installed(&dirs),
        std::slice::from_ref(&build),
        "the link is not a build"
    );
    // The link itself is not a build to delete.
    assert!(matches!(remove(&db, &dirs, &link), Err(Error::Refused(_))));

    game_on(&db, &root, "1", &link);
    let result = remove(&db, &dirs, &build);
    assert!(
        matches!(&result, Err(Error::Refused(m)) if m.contains("GE-Proton-latest")),
        "{result:?}"
    );
    assert!(build.is_dir());

    db.conn().execute("DELETE FROM installs", []).unwrap();
    remove(&db, &dirs, &build).unwrap();
    assert!(!build.exists());
    assert!(
        std::fs::symlink_metadata(&link).is_err(),
        "the link went with it"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn builds_downloaded_before_the_links_get_one_to_the_last_unpacked() {
    let root = root("link-newest");
    let dirs = Dirs::under(&root);
    let at = |build: &Path, secs: u64| {
        std::fs::File::open(build)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
            .unwrap();
    };
    // By when it was unpacked, not by name: 11-10 sorts before 11-9.
    at(&fake_build(&dirs, "GE-Proton11-10-x86_64"), 2_000);
    at(&fake_build(&dirs, "GE-Proton11-9-x86_64"), 1_000);
    let umu = fake_build(&dirs, "UMU-Proton-10.0-4");
    std::os::unix::fs::symlink("elsewhere", latest_link(&dirs, Source::UmuProton)).unwrap();
    link_newest(&dirs).unwrap();
    assert_eq!(
        latest_target(&dirs, Source::GeProton),
        Some(dir(&dirs).join("GE-Proton11-10-x86_64"))
    );
    // A link already there is left as it is; a project without builds gets none.
    assert_eq!(
        latest_target(&dirs, Source::UmuProton),
        Some(dir(&dirs).join("elsewhere"))
    );
    assert!(umu.is_dir());
    assert!(std::fs::symlink_metadata(latest_link(&dirs, Source::ProtonCachyOs)).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
