//! Installing an Epic game: the newest build of its Windows release, downloaded from Epic's CDN
//! into a partial folder, verified, then renamed into place and registered like a GOG game's, to
//! run through umu and Proton. The manifest and what launching needs are kept beside its install
//! record.

use std::collections::{HashMap, HashSet};
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
use crate::maintenance::{UpdateCheck, UpdateReport};
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
    /// The deployment of Epic's online services the game is given at launch, from the build's
    /// sidecar, when it has one.
    pub deployment_id: Option<String>,
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
        // A JSON object written as a string (`LegendaryCore.get_game_and_dlc_list`).
        deployment_id: element["sidecar"]["config"]
            .as_str()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(c).ok())
            .and_then(|c| text(&c["deploymentId"]))
            .filter(|d| !d.is_empty()),
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
    /// What the catalog says of its launch: whether it asks for an ownership token, and the
    /// arguments it adds (`OwnershipToken`, `AdditionalCommandline`).
    pub ownership_token: bool,
    pub additional_args: String,
    /// The folder its cloud saves are kept from (`CloudSaveFolder`), when Epic keeps any.
    pub cloud_folder: Option<String>,
    pub base_url: String,
    pub download_size: u64,
    pub disk_size: u64,
}

impl EpicPlan {
    pub fn build_id(&self) -> String {
        format!("epic:{}", self.build.build_version)
    }

    /// The install record of this build, installed in `target`.
    fn record(&self, target: &Path) -> InstallRecord {
        InstallRecord {
            build_id: self.build_id(),
            version: self.build.build_version.clone(),
            language: String::new(),
            path: Some(target.to_path_buf()),
            dlcs: Vec::new(),
            setup_build: None,
            setup_failures: 0,
            files: self
                .files
                .files
                .iter()
                .map(|(p, f)| RecordedFile {
                    path: p.to_string_lossy().into_owned(),
                    size: f.size(),
                })
                .collect(),
        }
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
    let build = newest_build(
        &source,
        &game.namespace,
        &game.catalog_item_id,
        &game.app_name,
    )
    .await?;
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
    let attribute = |name: &str| {
        item.as_ref()
            .and_then(|i| i.custom_attributes.get(name))
            .map(|a| a.value.trim().to_string())
            .unwrap_or_default()
    };
    let ownership_token = attribute("OwnershipToken").eq_ignore_ascii_case("true");
    let additional_args = attribute("AdditionalCommandline");
    let cloud_folder = Some(attribute("CloudSaveFolder")).filter(|f| !f.is_empty());
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
        ownership_token,
        additional_args,
        cloud_folder,
    })
}

/// What Epic's launcher service says of a game's newest Windows build.
async fn newest_build(
    source: &Epic<'_>,
    namespace: &str,
    catalog_item_id: &str,
    app_name: &str,
) -> Result<BuildInfo> {
    let url = format!(
        "{ASSETS_HOST}/launcher/api/public/assets/v2/platform/Windows/namespace/{}/catalogItem/{}/app/{}/label/Live",
        path_part(namespace)?,
        path_part(catalog_item_id)?,
        app_name
    );
    let answer: serde_json::Value = super::library::session_checked(
        http::json(source.get(&url), "asking Epic for the game's build").await,
    )?;
    build_info(&answer)
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
    /// The arguments Epic's catalog adds, whether the game asks for an ownership token, and its
    /// deployment of Epic's online services (see `epic::launch`).
    #[serde(default)]
    pub additional_args: String,
    #[serde(default)]
    pub ownership_token: bool,
    #[serde(default)]
    pub deployment_id: Option<String>,
    /// The build's keys, to read its manifest again when it is encrypted.
    #[serde(default)]
    pub secrets: HashMap<String, String>,
}

impl EpicInstall {
    /// What is kept of `game` installed from `plan`.
    fn of(game: &EpicGame, plan: &EpicPlan) -> EpicInstall {
        EpicInstall {
            app_name: game.app_name.clone(),
            namespace: game.namespace.clone(),
            catalog_item_id: game.catalog_item_id.clone(),
            build_version: plan.build.build_version.clone(),
            launch_exe: plan.manifest.launch_exe.clone(),
            launch_command: plan.manifest.launch_command.clone(),
            additional_args: plan.additional_args.clone(),
            ownership_token: plan.ownership_token,
            deployment_id: plan.build.deployment_id.clone(),
            secrets: plan.build.secrets.clone(),
        }
    }

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

    pub(crate) fn save(&self, dirs: &Dirs, raw_manifest: &[u8]) -> Result<()> {
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
        EpicInstall::of(&req.game, &plan).save(dirs, &plan.raw_manifest)?;
        plan.record(&target).save(dirs, &game_id)?;
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

/// Verifies an installed Epic game against the manifest of the build it was installed from; with
/// `repair`, bad or missing files are downloaded again from the CDN Epic names now, as legendary's
/// `--repair` does. Verifying alone asks Epic nothing. Returns the files that were not right.
pub async fn check(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    game_id: &str,
    repair: bool,
    progress: &(dyn Fn(installer::Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<installer::Checked> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let (install, _, kept) = installed(db, dirs, game_id)?;
    // The manifest kept is still the old build's: repairing would bring its files back.
    if crate::maintenance::update_pending(db, game_id)?.is_some() {
        return Err(Error::Refused(
            "an update is unfinished; run the update again first".into(),
        ));
    }
    // Never asked when only verifying, which downloads nothing.
    let mut base = String::new();
    if repair {
        let tokens = super::auth::fresh(http, db, dirs).await?;
        let source = Epic {
            http,
            token: &tokens.access_token,
        };
        let build = newest_build(
            &source,
            &kept.namespace,
            &kept.catalog_item_id,
            &kept.app_name,
        )
        .await?;
        base = https(&base_url(&build.manifest_urls[0]))?;
    }
    let source = EpicCdn {
        http: http.clone(),
        base,
    };
    check_files(dirs, &install, &kept, &source, repair, progress, cancel).await
}

/// An Epic game installed by Slatty, its folder there, its install record and what was kept of it.
fn installed(db: &Db, dirs: &Dirs, game_id: &str) -> Result<(Install, InstallRecord, EpicInstall)> {
    let (install, record) = crate::maintenance::installed_by_slatty(db, dirs, game_id)?;
    crate::maintenance::folder_present(&install)?;
    let kept = EpicInstall::load(dirs, game_id)?.ok_or_else(|| {
        Error::NotFound(format!(
            "the manifest of {} was not kept: install it again",
            install.title
        ))
    })?;
    Ok((install, record, kept))
}

/// The newest build Epic offers for an installed game, when it is not the one installed.
pub async fn check_update(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    game_id: &str,
) -> Result<Option<UpdateCheck>> {
    let (_, record) = crate::maintenance::installed_by_slatty(db, dirs, game_id)?;
    let kept = EpicInstall::load(dirs, game_id)?
        .ok_or_else(|| Error::NotFound(format!("the manifest of {game_id} was not kept")))?;
    let tokens = super::auth::fresh(http, db, dirs).await?;
    let source = Epic {
        http,
        token: &tokens.access_token,
    };
    let build = newest_build(
        &source,
        &kept.namespace,
        &kept.catalog_item_id,
        &kept.app_name,
    )
    .await?;
    Ok(
        (build.build_version != kept.build_version).then(|| UpdateCheck {
            installed_version: record.version,
            available_build: format!("epic:{}", build.build_version),
            available_version: build.build_version,
        }),
    )
}

/// Brings an installed Epic game to the newest build Epic offers: the files missing or different
/// are downloaded, those Slatty installed that the build no longer has are deleted, and nothing
/// else in the folder is touched. Noted as an update just before the first file changes: one cut
/// off from then on is finished, on Epic's newest build, before the game can start.
pub async fn update(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    game_id: &str,
    progress: &(dyn Fn(installer::Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<UpdateReport> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let (install, record, kept) = installed(db, dirs, game_id)?;
    let tokens = super::auth::fresh(http, db, dirs).await?;
    let game = EpicGame {
        app_name: kept.app_name.clone(),
        namespace: kept.namespace.clone(),
        catalog_item_id: kept.catalog_item_id.clone(),
        title: install.title.clone(),
        cover: None,
        background: None,
        logo: None,
        os: Vec::new(),
    };
    let plan = plan(http, &tokens, &game).await?;
    let source = EpicCdn {
        http: http.clone(),
        base: https(&plan.base_url)?,
    };
    update_planned(
        db, dirs, &source, &install, &record, &game, plan, progress, cancel,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_planned<S: ChunkSource>(
    db: &Db,
    dirs: &Dirs,
    source: &S,
    install: &Install,
    record: &InstallRecord,
    game: &EpicGame,
    plan: EpicPlan,
    progress: &(dyn Fn(installer::Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<UpdateReport> {
    let game_id = &install.game_id;
    let pending = InstallJob::load(db, game_id)?.filter(InstallJob::is_update);
    if pending.is_none() && plan.build_id() == record.build_id {
        return Err(Error::Refused(format!(
            "{} is already up to date",
            install.title
        )));
    }
    let job = InstallJob {
        game_id: game_id.clone(),
        build_id: plan.build_id(),
        language: String::new(),
        root: install
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        directory: install
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        state: crate::maintenance::UPDATING.into(),
        dlcs: Vec::new(),
        proton: None,
    };
    // Windows sees differently cased paths as one file.
    let kept: HashSet<String> = plan
        .files
        .files
        .iter()
        .map(|(p, _)| p.to_string_lossy().to_lowercase())
        .collect();
    let mut removed = Vec::new();
    let applied = Download {
        source,
        manifest: &plan.manifest,
        secrets: &plan.build.secrets,
        cancel,
        progress,
        free_space: &installer::free_space,
    }
    .update_installed(&plan.files, &install.path, || {
        job.save(db)?;
        removed = crate::maintenance::remove_obsolete(record, &install.path, |rel| {
            kept.contains(&rel.to_string_lossy().to_lowercase())
        })?;
        Ok(())
    })
    .await;
    let downloaded = match applied {
        Err(Error::Cancelled) => {
            if let Some(mut job) = InstallJob::load(db, game_id)? {
                job.state = crate::maintenance::UPDATE_PAUSED.into();
                job.save(db)?;
            }
            return Err(Error::Cancelled);
        }
        other => other?,
    };
    EpicInstall::of(game, &plan).save(dirs, &plan.raw_manifest)?;
    plan.record(&install.path).save(dirs, game_id)?;
    InstallJob::delete(db, game_id)?;
    Ok(UpdateReport {
        from_version: record.version.clone(),
        to_version: plan.build.build_version.clone(),
        downloaded,
        removed,
        resumed: pending.is_some(),
        ..Default::default()
    })
}

async fn check_files<S: ChunkSource>(
    dirs: &Dirs,
    install: &Install,
    kept: &EpicInstall,
    source: &S,
    repair: bool,
    progress: &(dyn Fn(installer::Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<installer::Checked> {
    let manifest = kept.manifest(dirs)?;
    let files = BuildFiles::of(&manifest)?;
    let bad = Download {
        source,
        manifest: &manifest,
        secrets: &kept.secrets,
        cancel,
        progress,
        free_space: &installer::free_space,
    }
    .check_installed(&files, &install.path, repair)
    .await?;
    Ok(installer::Checked {
        bad,
        reused_bytes: 0,
    })
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
        let (plan, cdn) = plan_of(&files, chunk, "1.0-[FAKE]");
        (plan, cdn, files)
    }

    /// A build of version `version` made of `files`, and the CDN serving its chunks.
    fn plan_of(files: &[(&str, Vec<u8>)], chunk: usize, version: &str) -> (EpicPlan, FakeCdn) {
        let (mut manifest, cdn) = build(files, chunk);
        manifest.build_version = version.into();
        let set = BuildFiles::of(&manifest).unwrap();
        let plan = EpicPlan {
            game_id: "epic-Fake".into(),
            title: "[FAKE] Game".into(),
            build: BuildInfo {
                build_version: manifest.build_version.clone(),
                hash: String::new(),
                manifest_urls: Vec::new(),
                secrets: HashMap::new(),
                deployment_id: None,
            },
            raw_manifest: manifest::write::manifest(&manifest, &HashMap::new()),
            download_size: set.download_size(&manifest),
            disk_size: set.disk_size(),
            files: set,
            manifest,
            folder: "Fake Game".into(),
            ownership_token: false,
            additional_args: String::new(),
            cloud_folder: None,
            base_url: String::new(),
        };
        (plan, cdn)
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

    /// Verify finds the files damaged or missing from the manifest kept at install, downloading
    /// nothing, its progress counting the files found right; Repair downloads the chunks of those
    /// files alone.
    #[tokio::test]
    async fn an_epic_game_is_verified_and_repaired_from_its_kept_manifest() {
        let root = root("check");
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        // Chunks of 500 bytes: no file shares one.
        let (plan, cdn, files) = fake_plan(500);
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
        let kept = EpicInstall::load(&dirs, "epic-Fake").unwrap().unwrap();
        std::fs::write(install.path.join("Data/a.pak"), b"[FAKE] damaged").unwrap();
        std::fs::remove_file(install.path.join("Game.exe")).unwrap();
        cdn.asked.lock().unwrap().clear();
        let seen = std::sync::Mutex::new(Vec::new());
        let progress = |p: installer::Progress| seen.lock().unwrap().push(p);
        let check = |repair| {
            check_files(
                &dirs,
                &install,
                &kept,
                &cdn,
                repair,
                &progress,
                CancellationToken::new(),
            )
        };

        let checked = check(false).await.unwrap();
        assert_eq!(
            checked.bad,
            [PathBuf::from("Data/a.pak"), PathBuf::from("Game.exe")]
        );
        assert!(cdn.asked.lock().unwrap().is_empty(), "nothing downloaded");
        let last = *seen.lock().unwrap().last().unwrap();
        assert_eq!((last.files_done, last.bytes_done), (1, 1000), "b.pak");
        assert_eq!((last.files_total, last.bytes_total), (3, 6500));

        check(true).await.unwrap();
        for (name, data) in &files {
            assert_eq!(&std::fs::read(install.path.join(name)).unwrap(), data);
        }
        let manifest = kept.manifest(&dirs).unwrap();
        let intact = manifest
            .files
            .iter()
            .find(|f| f.name == "Data/b.pak")
            .unwrap();
        let asked = cdn.asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 11, "the chunks of Game.exe and a.pak");
        assert!(
            intact
                .parts
                .iter()
                .all(|p| !asked.contains(&manifest.chunks[&p.guid].path(18)))
        );
        assert!(check(false).await.unwrap().bad.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Installs the first build of `fake_plan` in a folder of its own.
    async fn installed_v1(root: &Path, db: &Db, dirs: &Dirs) -> Install {
        let (plan, cdn, _) = fake_plan(500);
        let req = request(root);
        install_planned(
            db,
            dirs,
            &cdn,
            plan,
            None,
            req.root.clone(),
            req,
            |_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap()
    }

    /// The second build: Game.exe the same, a.pak changed, b.pak gone, c.pak new.
    fn v2() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("Game.exe", content(1, 3000)),
            ("Data/a.pak", content(9, 2500)),
            ("Data/c.pak", content(4, 500)),
        ]
    }

    async fn update_to(
        db: &Db,
        dirs: &Dirs,
        cdn: &FakeCdn,
        plan: EpicPlan,
        cancel: CancellationToken,
    ) -> Result<UpdateReport> {
        let install = Install::get(db, "epic-Fake").unwrap().unwrap();
        let record = InstallRecord::load(dirs, "epic-Fake").unwrap().unwrap();
        let game = request(Path::new("/unused")).game;
        update_planned(
            db,
            dirs,
            cdn,
            &install,
            &record,
            &game,
            plan,
            &|_| {},
            cancel,
        )
        .await
    }

    /// An update writes the files that changed and the new ones, downloading only their chunks;
    /// deletes those the build no longer has but leaves the player's own; and keeps the new
    /// build's manifest. Up to date, it is refused.
    #[tokio::test]
    async fn an_epic_update_changes_only_what_changed() {
        let root = root("update");
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        let install = installed_v1(&root, &db, &dirs).await;
        std::fs::write(install.path.join("Data/mine.sav"), b"[FAKE] save").unwrap();
        let files = v2();
        let (plan, cdn) = plan_of(&files, 500, "2.0-[FAKE]");

        let report = update_to(&db, &dirs, &cdn, plan, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            report.downloaded,
            [PathBuf::from("Data/a.pak"), PathBuf::from("Data/c.pak")]
        );
        assert_eq!(report.removed, [PathBuf::from("Data/b.pak")]);
        assert_eq!(
            (report.from_version.as_str(), report.to_version.as_str()),
            ("1.0-[FAKE]", "2.0-[FAKE]")
        );
        assert!(!report.resumed);
        assert_eq!(cdn.asked.lock().unwrap().len(), 6, "a.pak's 5, c.pak's 1");
        for (name, data) in &files {
            assert_eq!(&std::fs::read(install.path.join(name)).unwrap(), data);
        }
        assert!(!install.path.join("Data/b.pak").exists());
        assert!(install.path.join("Data/mine.sav").exists(), "not Slatty's");
        let record = InstallRecord::load(&dirs, "epic-Fake").unwrap().unwrap();
        assert_eq!(record.build_id, "epic:2.0-[FAKE]");
        assert_eq!(record.files.len(), 3);
        let kept = EpicInstall::load(&dirs, "epic-Fake").unwrap().unwrap();
        assert_eq!(kept.build_version, "2.0-[FAKE]");
        assert_eq!(kept.manifest(&dirs).unwrap().build_version, "2.0-[FAKE]");
        assert!(InstallJob::load(&db, "epic-Fake").unwrap().is_none());

        let (plan, cdn) = plan_of(&files, 500, "2.0-[FAKE]");
        let e = update_to(&db, &dirs, &cdn, plan, CancellationToken::new())
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Paused before any file changed, the game stays as it was. Cut off once files changed, the
    /// update must be finished before the game starts, Verify refuses meanwhile (the manifest kept
    /// is the old build's), and asked again it finishes.
    #[tokio::test]
    async fn an_interrupted_epic_update_is_finished_first() {
        let root = root("update-cut");
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        installed_v1(&root, &db, &dirs).await;
        let files = v2();

        let (plan, cdn) = plan_of(&files, 500, "2.0-[FAKE]");
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let e = update_to(&db, &dirs, &cdn, plan, cancelled)
            .await
            .unwrap_err();
        assert!(matches!(e, Error::Cancelled));
        assert_eq!(
            crate::maintenance::update_pending(&db, "epic-Fake").unwrap(),
            None,
            "still playable"
        );

        let (plan, cdn) = plan_of(&files, 500, "2.0-[FAKE]");
        // c.pak, written after a.pak, cannot be downloaded.
        let c = plan
            .manifest
            .files
            .iter()
            .find(|f| f.name == "Data/c.pak")
            .unwrap();
        let broken = plan.manifest.chunks[&c.parts[0].guid].path(18);
        cdn.broken.lock().unwrap().insert(broken);
        assert!(
            update_to(&db, &dirs, &cdn, plan, CancellationToken::new())
                .await
                .is_err()
        );
        assert_eq!(
            crate::maintenance::update_pending(&db, "epic-Fake")
                .unwrap()
                .as_deref(),
            Some("epic:2.0-[FAKE]")
        );
        let e = check(
            &db,
            &dirs,
            &Client::new(),
            "epic-Fake",
            true,
            &|_| {},
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert!(matches!(e, Error::Refused(_)), "{e}");

        cdn.broken.lock().unwrap().clear();
        let (plan, _) = plan_of(&files, 500, "2.0-[FAKE]");
        let report = update_to(&db, &dirs, &cdn, plan, CancellationToken::new())
            .await
            .unwrap();
        assert!(report.resumed);
        assert_eq!(report.downloaded, [PathBuf::from("Data/c.pak")]);
        assert_eq!(
            crate::maintenance::update_pending(&db, "epic-Fake").unwrap(),
            None
        );
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
                "secrets": {"0a0b0c0d": "00112233"},
                "sidecar": {"rvn": 1, "config": "{\"deploymentId\": \"[FAKE]deployment\"}"}
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
        assert_eq!(build.deployment_id.as_deref(), Some("[FAKE]deployment"));
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
