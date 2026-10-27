//! Installing an Epic game: the newest build of its Windows release, downloaded from Epic's CDN
//! into a partial folder, verified, then renamed into place and registered like a GOG game's, to
//! run through umu and Proton. The manifest and what launching needs are kept beside its install
//! record.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use super::auth::Tokens;
use super::download::{BuildFiles, ChunkSource, Download, EpicCdn};
use super::library::{Epic, EpicGame, Source};
use super::manifest::{self, Manifest};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::{Install, Platform};
use crate::installer::{
    self, DOWNLOADING, FAILED, InstallEvent, InstallJob, InstallRecord, RecordedFile, STAGED_MARK,
    partial_dir,
};
use crate::paths::Dirs;
use crate::runner::Runner;
use crate::{fsutil, http, store};

const ASSETS_HOST: &str = "https://launcher-public-service-prod06.ol.epicgames.com";

/// What Epic's launcher service says of a game's newest build: its version, its manifest's
/// addresses and checksum, and the keys of an encrypted build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub build_version: String,
    /// The manifest's SHA-1, in hexadecimal.
    pub hash: String,
    pub manifest_urls: Vec<String>,
    pub secrets: HashMap<String, String>,
}

/// The launcher service's answer (`EPCAPI.get_game_manifest`).
fn build_info(answer: &serde_json::Value) -> Result<BuildInfo> {
    let bad = |detail: &str| Error::parse("Epic's build service", detail);
    let element = answer["elements"]
        .get(0)
        .ok_or_else(|| bad("it names no build"))?;
    let text = |v: &serde_json::Value| v.as_str().map(str::to_owned);
    let mut manifest_urls = Vec::new();
    for m in element["manifests"].as_array().into_iter().flatten() {
        let Some(uri) = text(&m["uri"]) else {
            continue;
        };
        let params: Vec<String> = m["queryParams"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| Some(format!("{}={}", p["name"].as_str()?, p["value"].as_str()?)))
            .collect();
        manifest_urls.push(if params.is_empty() {
            uri
        } else {
            format!("{uri}?{}", params.join("&"))
        });
    }
    if manifest_urls.is_empty() {
        return Err(bad("it gives no address for the manifest"));
    }
    Ok(BuildInfo {
        build_version: text(&element["buildVersion"]).unwrap_or_default(),
        hash: text(&element["hash"])
            .ok_or_else(|| bad("it gives no checksum"))?
            .to_lowercase(),
        manifest_urls,
        secrets: element["secrets"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| Some((k.to_uppercase(), v.as_str()?.to_owned())))
            .collect(),
    })
}

/// An address on Epic's CDN, over HTTPS whatever the service says (the launcher itself uses
/// plain HTTP, for caches on the way).
fn https(url: &str) -> Result<String> {
    let mut parsed =
        url::Url::parse(url).map_err(|_| Error::parse("Epic's build service", "a bad address"))?;
    if parsed.scheme() == "http" {
        let _ = parsed.set_scheme("https");
    }
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(Error::Refused(format!(
            "Epic named a download address Slatty does not use: {}",
            parsed.scheme()
        )));
    }
    Ok(parsed.to_string())
}

/// A manifest address's folder, under which the build's chunks are.
fn base_url(manifest_url: &str) -> String {
    let path = manifest_url.split('?').next().unwrap_or(manifest_url);
    path.rsplit_once('/')
        .map_or(path, |(base, _)| base)
        .to_string()
}

/// What installing a game takes.
pub struct EpicPlan {
    pub game_id: String,
    pub title: String,
    pub build: BuildInfo,
    /// The manifest as downloaded, kept with the install.
    pub raw_manifest: Vec<u8>,
    pub manifest: Manifest,
    pub files: BuildFiles,
    /// The folder the game installs into, under the library's.
    pub folder: String,
    pub base_url: String,
    pub download_size: u64,
    pub disk_size: u64,
}

impl EpicPlan {
    pub fn build_id(&self) -> String {
        format!("epic:{}", self.build.build_version)
    }
}

/// Asks Epic for the newest Windows build of `game` and reads its manifest.
pub async fn plan(http: &Client, tokens: &Tokens, game: &EpicGame) -> Result<EpicPlan> {
    let game_id = store::epic_id(&game.app_name);
    store::epic_app(&game_id).ok_or_else(|| Error::Refused(format!("unexpected id {game_id}")))?;
    let source = Epic {
        http,
        token: &tokens.access_token,
    };
    let url = format!(
        "{ASSETS_HOST}/launcher/api/public/assets/v2/platform/Windows/namespace/{}/catalogItem/{}/app/{}/label/Live",
        path_part(&game.namespace)?,
        path_part(&game.catalog_item_id)?,
        game.app_name
    );
    let answer: serde_json::Value = super::library::session_checked(
        http::json(source.get(&url), "asking Epic for the game's build").await,
    )?;
    let build = build_info(&answer)?;
    let item = source
        .catalog_item(game.namespace.clone(), game.catalog_item_id.clone())
        .await?;
    let folder = item
        .as_ref()
        .and_then(|i| i.custom_attributes.get("FolderName"))
        .map(|a| a.value.trim().to_string())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| game.app_name.clone());
    let folder = check_folder(&folder)?;
    let (raw_manifest, manifest_url) = download_manifest(http, &build).await?;
    let manifest = manifest::parse(&raw_manifest, &build.secrets)?;
    let files = BuildFiles::of(&manifest)?;
    Ok(EpicPlan {
        game_id,
        title: game.title.clone(),
        download_size: files.download_size(&manifest),
        disk_size: files.disk_size(),
        base_url: base_url(&manifest_url),
        build,
        raw_manifest,
        manifest,
        files,
        folder,
    })
}

/// A namespace or catalog id going into an address: Epic's are letters and digits.
fn path_part(s: &str) -> Result<&str> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Error::Refused(format!("unexpected Epic id `{s}`")));
    }
    Ok(s)
}

/// A folder name of a single part, never `..` nor hidden.
fn check_folder(name: &str) -> Result<String> {
    let ok = matches!(
        Path::new(name).components().collect::<Vec<_>>().as_slice(),
        [std::path::Component::Normal(_)]
    ) && !name.starts_with('.');
    if !ok {
        return Err(Error::Refused(format!("unsafe install folder `{name}`")));
    }
    Ok(name.to_string())
}

/// The manifest from the first of its addresses that answers, checked with the checksum the
/// launcher service gave; and the address it came from. The account's token never goes with it.
async fn download_manifest(http: &Client, build: &BuildInfo) -> Result<(Vec<u8>, String)> {
    let mut last = None;
    for url in &build.manifest_urls {
        let url = https(url)?;
        let req = http
            .get(&url)
            .header(reqwest::header::USER_AGENT, super::USER_AGENT);
        match http::bytes(req, "downloading the Epic manifest").await {
            Ok(raw) => {
                if crate::fsutil::hex(&super::sha1(&raw)) != build.hash {
                    return Err(Error::Refused(
                        "the Epic manifest does not match its checksum".into(),
                    ));
                }
                return Ok((raw, url));
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.expect("at least one address"))
}

/// What Slatty keeps of an installed Epic game beside its install record: what its launch needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicInstall {
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
    pub build_version: String,
    pub launch_exe: String,
    pub launch_command: String,
    /// The build's keys, to read its manifest again when it is encrypted.
    #[serde(default)]
    pub secrets: HashMap<String, String>,
}

impl EpicInstall {
    fn folder(dirs: &Dirs) -> PathBuf {
        dirs.data.join("epic")
    }

    pub fn load(dirs: &Dirs, game_id: &str) -> Result<Option<EpicInstall>> {
        store::epic_app(game_id)
            .ok_or_else(|| Error::Refused(format!("{game_id} is not Epic's")))?;
        let path = Self::folder(dirs).join(format!("{game_id}.json"));
        match std::fs::read(&path) {
            Ok(raw) => serde_json::from_slice(&raw)
                .map(Some)
                .map_err(|e| Error::parse("an Epic install", e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::io(format!("read {}", path.display()), e)),
        }
    }

    /// The manifest of the build installed.
    pub fn manifest(&self, dirs: &Dirs) -> Result<Manifest> {
        let path = Self::folder(dirs).join(format!("{}.manifest", store::epic_id(&self.app_name)));
        let raw =
            std::fs::read(&path).map_err(|e| Error::io(format!("read {}", path.display()), e))?;
        manifest::parse(&raw, &self.secrets)
    }

    fn save(&self, dirs: &Dirs, raw_manifest: &[u8]) -> Result<()> {
        let game_id = store::epic_id(&self.app_name);
        let folder = Self::folder(dirs);
        fsutil::write_atomic(&folder.join(format!("{game_id}.manifest")), raw_manifest)?;
        let json =
            serde_json::to_vec_pretty(self).map_err(|e| Error::parse("an Epic install", e))?;
        fsutil::write_atomic(&folder.join(format!("{game_id}.json")), &json)
    }

    /// Deletes what Slatty kept of an Epic game, once uninstalled or forgotten.
    pub fn remove(dirs: &Dirs, game_id: &str) {
        if store::epic_app(game_id).is_none() {
            return;
        }
        for ext in ["json", "manifest"] {
            let _ = std::fs::remove_file(Self::folder(dirs).join(format!("{game_id}.{ext}")));
        }
    }
}

pub struct EpicInstallRequest {
    pub game: EpicGame,
    /// The folder the game's folder goes into.
    pub root: PathBuf,
    pub proton: PathBuf,
    /// Starts an interrupted install again rather than resume it.
    pub restart: bool,
}

/// Downloads, verifies and registers an Epic game. An interrupted install is resumed into the same
/// folder unless `restart` is set; one whose build Epic replaced since starts again.
pub async fn install(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    req: EpicInstallRequest,
    emit: impl Fn(InstallEvent) + Send + Sync,
    cancel: CancellationToken,
) -> Result<Install> {
    let game_id = store::epic_id(&req.game.app_name);
    let _busy = crate::lock::game(dirs, &game_id)?;
    if Install::get(db, &game_id)?.is_some() {
        return Err(Error::Refused(format!("{game_id} is already installed")));
    }
    let previous = InstallJob::load(db, &game_id)?;
    if req.restart
        && let Some(job) = &previous
    {
        installer::restart(db, job)?;
    }
    let job = previous.filter(|_| !req.restart);
    let root = job.as_ref().map_or(req.root.clone(), |j| j.root.clone());
    // The folder a started download was going into is gone (a drive not mounted): starting again
    // would fill the empty mount point instead.
    if let Some(j) = job.as_ref().filter(|j| !j.is_queued())
        && !root.is_dir()
    {
        InstallJob {
            state: FAILED.into(),
            ..j.clone()
        }
        .save(db)?;
        return Err(Error::NotFound(format!(
            "{} is missing: is its drive mounted?",
            root.display()
        )));
    }
    let plan = plan(http, tokens, &req.game).await?;
    let source = EpicCdn {
        http: http.clone(),
        base: https(&plan.base_url)?,
    };
    install_planned(db, dirs, &source, plan, job, root, req, emit, cancel).await
}

/// Installs what `plan` says from `source`, the game locked and its interrupted job, if any,
/// given with the folder it goes into.
#[allow(clippy::too_many_arguments)]
async fn install_planned<S: ChunkSource>(
    db: &Db,
    dirs: &Dirs,
    source: &S,
    plan: EpicPlan,
    job: Option<InstallJob>,
    root: PathBuf,
    req: EpicInstallRequest,
    emit: impl Fn(InstallEvent) + Send + Sync,
    cancel: CancellationToken,
) -> Result<Install> {
    let game_id = plan.game_id.clone();
    let directory = match &job {
        Some(j) => j.directory.clone(),
        None => plan.folder.clone(),
    };
    let target = root.join(&directory);
    let partial = partial_dir(&root, &directory);
    let resumed = job.as_ref().is_some_and(|j| !j.is_queued());
    emit(InstallEvent::Planned {
        title: plan.title.clone(),
        version: plan.build.build_version.clone(),
        language: String::new(),
        languages: Vec::new(),
        download_size: plan.download_size,
        disk_size: plan.disk_size,
        target: target.clone(),
        resumed,
    });
    let build_id = plan.build_id();
    installer::drop_replaced(
        &partial,
        &game_id,
        job.as_ref()
            .filter(|_| resumed)
            .map(|j| j.build_id.as_str()),
        &build_id,
    )?;
    let mut job = InstallJob {
        game_id: game_id.clone(),
        build_id: build_id.clone(),
        language: String::new(),
        root: root.clone(),
        directory: directory.clone(),
        state: DOWNLOADING.into(),
        dlcs: Vec::new(),
        proton: Some(req.proton.clone()),
    };
    job.save(db)?;

    let progress = |p| emit(InstallEvent::Progress(p));
    let published = installer::published_unregistered(resumed, &partial, &target, &game_id);
    let result = async {
        if !published && !target.exists() {
            installer::mark_staged(&partial, &game_id, &build_id)?;
        }
        let dl = Download {
            source,
            manifest: &plan.manifest,
            secrets: &plan.build.secrets,
            cancel,
            progress: &progress,
            free_space: &installer::free_space,
        };
        if published {
            // Interrupted between publishing the folder and registering the game: its files are
            // checked where they now are.
            dl.check_installed(&plan.files, &target, true).await?;
        } else {
            dl.run(&plan.files, &partial, &target).await?;
        }
        Ok::<_, Error>(())
    }
    .await;
    let registered = result.and_then(|()| {
        EpicInstall {
            app_name: req.game.app_name.clone(),
            namespace: req.game.namespace.clone(),
            catalog_item_id: req.game.catalog_item_id.clone(),
            build_version: plan.build.build_version.clone(),
            launch_exe: plan.manifest.launch_exe.clone(),
            launch_command: plan.manifest.launch_command.clone(),
            secrets: plan.build.secrets.clone(),
        }
        .save(dirs, &plan.raw_manifest)?;
        InstallRecord {
            build_id: build_id.clone(),
            version: plan.build.build_version.clone(),
            language: String::new(),
            path: Some(target.clone()),
            dlcs: Vec::new(),
            setup_build: None,
            setup_failures: 0,
            files: plan
                .files
                .files
                .iter()
                .map(|(p, f)| RecordedFile {
                    path: p.to_string_lossy().into_owned(),
                    size: f.size(),
                })
                .collect(),
        }
        .save(dirs, &game_id)?;
        let runner = Runner::Umu {
            proton: req.proton.clone(),
            prefix: dirs.data.join("prefixes").join(&game_id),
        };
        let install = Install {
            umu_id: None,
            game_id: game_id.clone(),
            title: plan.title.clone(),
            platform: Platform::Windows,
            path: target.clone(),
            client_id: None,
            isolated: crate::install::isolated_when_installed(db, Platform::Windows, &runner)?,
            runner,
        };
        install.save(db)?;
        InstallJob::delete(db, &game_id)?;
        Ok(install)
    });
    let install = match registered {
        Ok(install) => install,
        Err(e) => {
            job.state = installer::state_after(&e).into();
            job.save(db)?;
            return Err(e);
        }
    };
    let _ = std::fs::remove_file(target.join(STAGED_MARK));
    emit(InstallEvent::Finished {
        path: target,
        support_files: 0,
        skipped_links: plan.files.skipped_links,
        dependencies: plan
            .manifest
            .prerequisites
            .iter()
            .map(|p| p.name.clone())
            .collect(),
    });
    Ok(install)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epic::download::tests::{FakeCdn, build, content};

    fn root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("slatty-epic-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// A build of three files, cut into chunks of `chunk` bytes.
    fn fake_plan(chunk: usize) -> (EpicPlan, FakeCdn, Vec<(&'static str, Vec<u8>)>) {
        let files = vec![
            ("Game.exe", content(1, 3000)),
            ("Data/a.pak", content(2, 2500)),
            ("Data/b.pak", content(3, 1000)),
        ];
        let (manifest, cdn) = build(&files, chunk);
        let set = BuildFiles::of(&manifest).unwrap();
        let plan = EpicPlan {
            game_id: "epic-Fake".into(),
            title: "[FAKE] Game".into(),
            build: BuildInfo {
                build_version: manifest.build_version.clone(),
                hash: String::new(),
                manifest_urls: Vec::new(),
                secrets: HashMap::new(),
            },
            raw_manifest: manifest::write::manifest(&manifest, &HashMap::new()),
            download_size: set.download_size(&manifest),
            disk_size: set.disk_size(),
            files: set,
            manifest,
            folder: "Fake Game".into(),
            base_url: String::new(),
        };
        (plan, cdn, files)
    }

    fn request(root: &Path) -> EpicInstallRequest {
        EpicInstallRequest {
            game: EpicGame {
                app_name: "Fake".into(),
                namespace: "fakens".into(),
                catalog_item_id: "fakeitem".into(),
                title: "[FAKE] Game".into(),
                cover: None,
                background: None,
                logo: None,
                os: vec!["windows".into()],
            },
            root: root.join("games"),
            proton: "/proton/[FAKE]".into(),
            restart: false,
        }
    }

    /// An installed Epic game is registered like a GOG one, to run through Proton in a prefix of
    /// its own, with what its launch needs kept beside it; uninstalling deletes its files and that.
    #[tokio::test]
    async fn an_epic_game_installs_and_uninstalls() {
        let root = root("whole");
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        let (plan, cdn, files) = fake_plan(1024);
        let req = request(&root);
        let install = install_planned(
            &db,
            &dirs,
            &cdn,
            plan,
            None,
            req.root.clone(),
            req,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap();
        let target = root.join("games/Fake Game");
        assert_eq!(install.path, target);
        assert_eq!(
            install.runner.prefix(),
            Some(dirs.data.join("prefixes/epic-Fake").as_path())
        );
        assert_eq!(Install::get(&db, "epic-Fake").unwrap(), Some(install));
        for (name, data) in &files {
            assert_eq!(&std::fs::read(target.join(name)).unwrap(), data);
        }
        assert!(!target.join(STAGED_MARK).exists());
        assert!(InstallJob::load(&db, "epic-Fake").unwrap().is_none());
        let record = InstallRecord::load(&dirs, "epic-Fake").unwrap().unwrap();
        assert_eq!(record.files.len(), 3);
        let kept = EpicInstall::load(&dirs, "epic-Fake").unwrap().unwrap();
        assert_eq!(kept.launch_exe, "Game.exe");
        assert_eq!(kept.manifest(&dirs).unwrap().files.len(), 3);

        crate::maintenance::uninstall(&db, &dirs, "epic-Fake", false).unwrap();
        assert!(!target.join("Game.exe").exists());
        assert!(EpicInstall::load(&dirs, "epic-Fake").unwrap().is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Epic out of reach midway: the job stays to resume by itself, into the same folder, with
    /// the files already verified.
    #[tokio::test]
    async fn an_interrupted_install_resumes_where_it_stopped() {
        let root = root("resumed");
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        // Chunks of 500 bytes: no file shares one. Game.exe is written last.
        let (plan, cdn, _) = fake_plan(500);
        let last: Vec<String> = plan.files.files[2]
            .1
            .parts
            .iter()
            .map(|p| plan.manifest.chunks[&p.guid].path(18))
            .collect();
        cdn.broken.lock().unwrap().extend(last.iter().cloned());
        let req = request(&root);
        let run = |plan, job| {
            let req = request(&root);
            let root = req.root.clone();
            install_planned(
                &db,
                &dirs,
                &cdn,
                plan,
                job,
                root,
                req,
                |_| {},
                CancellationToken::new(),
            )
        };
        assert!(run(plan, None).await.is_err());
        let job = InstallJob::load(&db, "epic-Fake").unwrap().unwrap();
        assert_eq!(job.state, DOWNLOADING, "resumed by itself");
        assert_eq!(job.build_id, "epic:1.0-[FAKE]");
        assert_eq!(job.root, req.root);

        cdn.broken.lock().unwrap().clear();
        cdn.asked.lock().unwrap().clear();
        let (plan, _, _) = fake_plan(500);
        run(plan, Some(job)).await.unwrap();
        assert_eq!(
            *cdn.asked.lock().unwrap(),
            last,
            "the files verified are kept"
        );
        assert!(Install::get(&db, "epic-Fake").unwrap().is_some());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// The build service's answer, as legendary reads it.
    #[test]
    fn the_build_service_names_the_manifest_and_its_keys() {
        let answer = serde_json::json!({
            "elements": [{
                "appName": "Fake",
                "labelName": "Live",
                "buildVersion": "1.2.3-[FAKE]",
                "hash": "ABCDEF0123",
                "manifests": [
                    {"uri": "https://cdn.example/Builds/Org/x/default/abc.manifest",
                     "queryParams": [{"name": "f_token", "value": "[FAKE]"}]},
                    {"uri": "http://cdn2.example/Builds/Org/x/default/abc.manifest"}
                ],
                "secrets": {"0a0b0c0d": "00112233"}
            }]
        });
        let build = build_info(&answer).unwrap();
        assert_eq!(build.build_version, "1.2.3-[FAKE]");
        assert_eq!(build.hash, "abcdef0123");
        assert_eq!(
            build.manifest_urls,
            [
                "https://cdn.example/Builds/Org/x/default/abc.manifest?f_token=[FAKE]",
                "http://cdn2.example/Builds/Org/x/default/abc.manifest"
            ]
        );
        assert_eq!(build.secrets["0A0B0C0D"], "00112233");
        assert_eq!(
            base_url(&build.manifest_urls[0]),
            "https://cdn.example/Builds/Org/x/default"
        );
        assert_eq!(
            https(&build.manifest_urls[1]).unwrap(),
            "https://cdn2.example/Builds/Org/x/default/abc.manifest"
        );
        assert!(https("file:///etc/passwd").is_err());
        assert!(build_info(&serde_json::json!({"elements": []})).is_err());
    }

    #[test]
    fn install_folders_and_ids_are_checked() {
        assert_eq!(check_folder("Fake Game").unwrap(), "Fake Game");
        for bad in ["", "..", ".hidden", "a/b", "/abs"] {
            assert!(check_folder(bad).is_err(), "{bad:?}");
        }
        assert!(path_part("abc123").is_ok());
        assert!(path_part("../x").is_err() && path_part("").is_err());
    }
}
