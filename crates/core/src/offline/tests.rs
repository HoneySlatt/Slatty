use std::sync::Mutex;

use futures::stream::BoxStream;

use super::*;

/// Installer files in memory, sent in small pieces. `drop_once` cuts the first answer for a file
/// starting there, as a dropped connection would; `corrupt_once` changes a byte of the first
/// answer that holds it.
struct Fake {
    files: Vec<(String, Vec<u8>, Option<String>)>,
    drop_once: Mutex<Option<(usize, u64)>>,
    corrupt_once: Mutex<Option<(usize, u64)>>,
    opened: Mutex<Vec<(usize, u64, u64)>>,
}

impl Fake {
    fn new(files: Vec<(&str, Vec<u8>, Option<String>)>) -> Self {
        Fake {
            files: files
                .into_iter()
                .map(|(n, d, x)| (n.to_owned(), d, x))
                .collect(),
            drop_once: Mutex::new(None),
            corrupt_once: Mutex::new(None),
            opened: Mutex::new(Vec::new()),
        }
    }
}

impl Bytes for Fake {
    async fn size(&self, part: usize) -> Result<u64> {
        Ok(self.files[part].1.len() as u64)
    }

    async fn open(
        &self,
        part: usize,
        start: u64,
        end: u64,
    ) -> Result<BoxStream<'static, Result<Vec<u8>>>> {
        self.opened.lock().unwrap().push((part, start, end));
        let mut data = self.files[part].1[start as usize..end as usize].to_vec();
        if let Some((_, at)) = self
            .corrupt_once
            .lock()
            .unwrap()
            .take_if(|(p, at)| *p == part && (start..end).contains(at))
        {
            data[(at - start) as usize] ^= 0xff;
        }
        if self
            .drop_once
            .lock()
            .unwrap()
            .take_if(|(p, at)| *p == part && *at == start)
            .is_some()
        {
            data.truncate(data.len() / 2);
        }
        let pieces: Vec<Result<Vec<u8>>> = data.chunks(700).map(|c| Ok(c.to_vec())).collect();
        Ok(futures::stream::iter(pieces).boxed())
    }
}

impl Source for Fake {
    fn parts(&self) -> usize {
        self.files.len()
    }

    async fn name(&self, part: usize) -> Result<String> {
        Ok(self.files[part].0.clone())
    }

    async fn checksums(&self, part: usize) -> Result<Option<Vec<u8>>> {
        Ok(self.files[part].2.clone().map(String::into_bytes))
    }
}

fn data(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn md5_hex(data: &[u8]) -> String {
    fsutil::hex(&Md5::digest(data))
}

/// GOG's checksum XML for `data`, in chunks of `chunk` bytes.
fn xml(name: &str, data: &[u8], chunk: usize) -> String {
    let chunks: String = data
        .chunks(chunk)
        .enumerate()
        .map(|(i, c)| {
            let from = i * chunk;
            format!(
                "\t<chunk id=\"{i}\" from=\"{from}\" to=\"{}\" method=\"md5\">{}</chunk>\n",
                from + c.len() - 1,
                md5_hex(c)
            )
        })
        .collect();
    format!(
        "<file name=\"{name}\" available=\"1\" notavailablemsg=\"\" md5=\"{}\" chunks=\"{}\" \
         timestamp=\"2020-01-01 00:00:00\" total_size=\"{}\">\n{chunks}</file>\n",
        md5_hex(data),
        data.len().div_ceil(chunk),
        data.len()
    )
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("slatty-offline-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn run_download(fake: &Fake, folder: &Path) -> Result<Vec<PathBuf>> {
    download(fake, folder, &|_| {}, &CancellationToken::new()).await
}

#[test]
fn every_system_and_the_dlc_with_installers_are_offered() {
    let raw = br#"{
        "id": 1, "title": "[FAKE] Game",
        "downloads": {"installers": [
            {"os": "windows", "language": "en", "version": "1.0", "files": [
                {"id": "en1installer0", "size": 1000, "downlink": "https://api.gog.com/d/1"},
                {"id": "en1installer1", "size": 2000, "downlink": "https://api.gog.com/d/2"}]},
            {"os": "windows", "language": "fr", "version": "1.0", "files": [
                {"id": "fr1installer0", "size": 1000, "downlink": "https://api.gog.com/d/3"}]},
            {"os": "mac", "language": "en", "version": null, "files": [
                {"id": "en2installer0", "size": 5, "downlink": "https://api.gog.com/d/4"}]},
            {"os": "linux", "language": "en", "version": "1.0", "files": []}]},
        "expanded_dlcs": [
            {"id": 2, "title": "[FAKE] Owned", "downloads": {"installers": [
                {"os": "windows", "language": "en", "version": "1", "files": [
                    {"size": 7, "downlink": "https://api.gog.com/d/5"}]}]}},
            {"id": 3, "title": "[FAKE] Not owned", "downloads": {"installers": []}}]
    }"#;
    let offer = parse_offer(raw).unwrap();
    assert_eq!(offer.title, "[FAKE] Game");
    // An installer without files cannot be downloaded.
    assert_eq!(offer.systems(), ["windows", "mac"]);
    assert_eq!(offer.languages("windows"), ["en", "fr"]);
    assert_eq!(offer.installers[0].size(), 3000);
    assert_eq!(offer.dlcs.len(), 1);
    assert_eq!(offer.dlcs[0].id, "2");
    assert_eq!(offer.dlcs_for("windows").count(), 1);
    assert_eq!(offer.dlcs_for("mac").count(), 0);
    // The DLC has no French installer: its English one comes with the French game.
    let chosen = offer.chosen("windows", "fr", &["2".into()]);
    assert_eq!(
        chosen.iter().map(|i| &i.language).collect::<Vec<_>>(),
        ["fr", "en"]
    );
    assert_eq!(chosen[1].files[0].downlink, "https://api.gog.com/d/5");
    assert!(offer.chosen("linux", "en", &[]).is_empty());
    assert_eq!(offer.chosen("mac", "en", &["2".into()]).len(), 1);
}

#[test]
fn checksums_are_read_from_gogs_xml() {
    let data = data(2500, 1);
    let sums = parse_checksums(xml("setup.exe", &data, 1000).as_bytes()).unwrap();
    assert_eq!(sums.md5, md5_hex(&data));
    assert_eq!(
        sums.chunks
            .iter()
            .map(|c| (c.from, c.to))
            .collect::<Vec<_>>(),
        [(0, 999), (1000, 1999), (2000, 2499)]
    );
    // Chunks out of order, overlapping or without a sum are not trusted.
    let md5 = md5_hex(b"x");
    for chunks in [
        format!(
            "<chunk from=\"10\" to=\"19\" method=\"md5\">{md5}</chunk>\
             <chunk from=\"0\" to=\"9\" method=\"md5\">{md5}</chunk>"
        ),
        format!(
            "<chunk from=\"0\" to=\"9\" method=\"md5\">{md5}</chunk>\
             <chunk from=\"5\" to=\"19\" method=\"md5\">{md5}</chunk>"
        ),
        "<chunk from=\"0\" to=\"9\" method=\"md5\">nope</chunk>".to_owned(),
    ] {
        let xml = format!("<file name=\"a\" md5=\"{md5}\">{chunks}</file>");
        assert!(parse_checksums(xml.as_bytes()).is_err(), "{chunks}");
    }
    assert!(parse_checksums(b"<file name=\"a\"></file>").is_err());
}

#[test]
fn only_plain_file_names_are_taken() {
    assert_eq!(
        file_name("setup_fake_game_1.0_%2812345%29.exe").unwrap(),
        "setup_fake_game_1.0_(12345).exe"
    );
    for bad in [
        "",
        "..",
        ".hidden",
        "a%2Fb.exe",
        "a%5Cb.exe",
        "a%00.exe",
        "a%2",
        "a%zz.exe",
        "setup.exe.slatty-dl",
    ] {
        assert!(file_name(bad).is_err(), "{bad}");
    }
}

#[tokio::test]
async fn installers_are_downloaded_checked_and_named() {
    let dir = temp("download");
    let (a, b) = (data(5000, 1), data(3000, 2));
    let fake = Fake::new(vec![
        (
            "setup_fake.exe",
            a.clone(),
            Some(xml("setup_fake.exe", &a, 1024)),
        ),
        (
            "setup_fake-1.bin",
            b.clone(),
            Some(xml("setup_fake-1.bin", &b, 1024)),
        ),
    ]);
    let last = Mutex::new(Progress::default());
    let files = download(
        &fake,
        &dir,
        &|p| *last.lock().unwrap() = p,
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        files,
        [dir.join("setup_fake.exe"), dir.join("setup_fake-1.bin")]
    );
    assert_eq!(std::fs::read(&files[0]).unwrap(), a);
    assert_eq!(std::fs::read(&files[1]).unwrap(), b);
    let last = *last.lock().unwrap();
    assert_eq!((last.files_done, last.files_total), (2, 2));
    assert_eq!((last.bytes_done, last.bytes_total), (8000, 8000));
    // Nothing staged is left behind.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_partial_file_is_taken_up_where_it_stopped() {
    let dir = temp("resume");
    let a = data(5000, 3);
    std::fs::write(dir.join("setup_fake.exe.slatty-dl"), &a[..2100]).unwrap();
    let fake = Fake::new(vec![(
        "setup_fake.exe",
        a.clone(),
        Some(xml("setup_fake.exe", &a, 1024)),
    )]);
    run_download(&fake, &dir).await.unwrap();
    assert_eq!(std::fs::read(dir.join("setup_fake.exe")).unwrap(), a);
    assert_eq!(*fake.opened.lock().unwrap(), [(0, 2100, 5000)]);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_dropped_connection_is_taken_up() {
    let dir = temp("dropped");
    let a = data(5000, 4);
    let fake = Fake::new(vec![("setup_fake.exe", a.clone(), None)]);
    *fake.drop_once.lock().unwrap() = Some((0, 0));
    run_download(&fake, &dir).await.unwrap();
    assert_eq!(std::fs::read(dir.join("setup_fake.exe")).unwrap(), a);
    assert_eq!(
        *fake.opened.lock().unwrap(),
        [(0, 0, 5000), (0, 2500, 5000)]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_damaged_part_is_downloaded_again() {
    let dir = temp("damaged");
    let a = data(5000, 5);
    let fake = Fake::new(vec![(
        "setup_fake.exe",
        a.clone(),
        Some(xml("setup_fake.exe", &a, 1024)),
    )]);
    *fake.corrupt_once.lock().unwrap() = Some((0, 3000));
    run_download(&fake, &dir).await.unwrap();
    assert_eq!(std::fs::read(dir.join("setup_fake.exe")).unwrap(), a);
    // Only the chunk holding the damaged byte.
    assert_eq!(
        *fake.opened.lock().unwrap(),
        [(0, 0, 5000), (0, 2048, 3072)]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_file_that_does_not_match_gogs_sums_is_not_kept() {
    let dir = temp("mismatch");
    let a = data(5000, 6);
    // Sums of other bytes: the file stays wrong once its parts are downloaded again.
    let fake = Fake::new(vec![(
        "setup_fake.exe",
        a,
        Some(xml("setup_fake.exe", &data(5000, 7), 1024)),
    )]);
    assert!(run_download(&fake, &dir).await.is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_file_already_there_is_kept_and_another_is_not_replaced() {
    let dir = temp("there");
    let a = data(5000, 8);
    std::fs::write(dir.join("setup_fake.exe"), &a).unwrap();
    let fake = Fake::new(vec![("setup_fake.exe", a.clone(), None)]);
    run_download(&fake, &dir).await.unwrap();
    assert!(fake.opened.lock().unwrap().is_empty());

    std::fs::write(dir.join("setup_fake.exe"), b"[FAKE] mine").unwrap();
    assert!(matches!(
        run_download(&fake, &dir).await,
        Err(Error::Refused(_))
    ));
    assert_eq!(
        std::fs::read(dir.join("setup_fake.exe")).unwrap(),
        b"[FAKE] mine"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_link_in_place_of_the_staged_file_is_not_followed() {
    let dir = temp("link");
    let outside = dir.join("outside");
    std::fs::write(&outside, b"[FAKE] keep").unwrap();
    let folder = dir.join("game");
    std::fs::create_dir(&folder).unwrap();
    std::os::unix::fs::symlink(&outside, folder.join("setup_fake.exe.slatty-dl")).unwrap();
    let fake = Fake::new(vec![("setup_fake.exe", data(5000, 9), None)]);
    assert!(run_download(&fake, &folder).await.is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"[FAKE] keep");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_name_that_is_not_a_file_name_writes_nothing() {
    let dir = temp("name");
    let folder = dir.join("game");
    let fake = Fake::new(vec![
        ("setup_fake.exe", data(10, 1), None),
        ("..%2Fescape.exe", data(10, 2), None),
    ]);
    assert!(run_download(&fake, &folder).await.is_err());
    assert!(!folder.exists());
    assert!(fake.opened.lock().unwrap().is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn a_paused_download_keeps_what_it_has() {
    let dir = temp("paused");
    let fake = Fake::new(vec![("setup_fake.exe", data(5000, 10), None)]);
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        download(&fake, &dir, &|_| {}, &cancel).await,
        Err(Error::Cancelled)
    ));
    assert!(dir.join("setup_fake.exe.slatty-dl").exists());
    assert!(!dir.join("setup_fake.exe").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn jobs_are_kept_until_forgotten() {
    let db = Db::in_memory().unwrap();
    let job = OfflineJob {
        game_id: "1".into(),
        os: "windows".into(),
        language: "fr".into(),
        dlcs: vec!["2".into(), "3".into()],
        folder: PathBuf::from("/tmp/[FAKE] Game"),
        state: crate::installer::QUEUED.into(),
    };
    job.save(&db).unwrap();
    assert_eq!(OfflineJob::load(&db, "1").unwrap(), Some(job.clone()));
    assert_eq!(OfflineJob::list(&db).unwrap(), std::slice::from_ref(&job));
    OfflineJob::delete(&db, "1").unwrap();
    assert!(OfflineJob::list(&db).unwrap().is_empty());
    // A Steam game or one added by hand has no GOG installers.
    let mut other = OfflineJob {
        game_id: "steam-10".into(),
        ..job
    };
    assert!(other.save(&db).is_err());
    other.game_id = "local-1".into();
    assert!(other.save(&db).is_err());
}

#[test]
fn installers_go_into_a_folder_named_after_the_game() {
    assert_eq!(
        folder_for(Path::new("/x"), "[FAKE] Game: Part 2"),
        Path::new("/x/[FAKE] Game Part 2")
    );
}
