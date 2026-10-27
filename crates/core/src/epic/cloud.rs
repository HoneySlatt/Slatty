//! Epic's cloud saves, as legendary reads and writes them (`legendary/core.py`: `get_save_path`,
//! `upload_save`, `download_saves`; `legendary/utils/savegame_helper.py`). Epic keeps whole
//! snapshots of a game's save folder: a manifest named after the time it was made, and the chunks
//! it lists, as for a build. Slatty's sync rules work on files (`cloud::sync`): the newest snapshot
//! is the cloud's list of files, with their SHA-1, and what a sync uploads or deletes is sent as
//! one new snapshot, made from the newest one with those changes.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::{NaiveDateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use url::Url;

use super::chunk;
use super::install::EpicInstall;
use super::library::{Epic, Source};
use super::manifest::{self, ChunkPart, FileEntry, Guid, Manifest};
use crate::cloud::sync::{self, SyncOptions, SyncTarget};
use crate::cloud::transport::{CloudTransport, RemoteEntry};
use crate::cloud::{LocationOutcome, locations};
use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::Install;
use crate::paths::Dirs;
use crate::secret::Secret;
use crate::{fsutil, http};

const DATASTORAGE: &str = "https://datastorage-public-service-liveegs.live.use1a.on.epicgames.com/api/v1/access/egstore/savesync/";
/// The one save location of an Epic game: it names its backups and its files in the sync.
pub const LOCATION: &str = "saves";
/// The version of the manifests legendary writes for saves.
const FEATURE_LEVEL: u32 = 18;
/// How a snapshot is named, in UTC (`package_savegame`).
const STAMP: &str = "%Y.%m.%d-%H.%M.%S";

/// A file of the game's cloud storage, by its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub key: String,
    pub last_modified: Option<String>,
}

/// Where a game's cloud saves are kept: Epic's data storage, or a test double.
pub trait SaveStore: Send + Sync {
    /// Every file stored for the game.
    fn list(&self) -> impl Future<Output = Result<Vec<Stored>>> + Send;
    /// A file the last listing named.
    fn read(&self, key: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
    /// Writes files by their name under the game's storage (`manifests/…`, `ChunksV4/…`), in
    /// order.
    fn write(&self, files: Vec<(String, Vec<u8>)>) -> impl Future<Output = Result<()>> + Send;
}

/// Epic's data storage for one game of the account. Listing and asking where to write take the
/// account's token; the addresses Epic gives to read and write are used without it.
pub struct EpicStore {
    http: Client,
    token: Secret,
    url: Url,
    read_links: Mutex<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    files: HashMap<String, Link>,
}

#[derive(Deserialize)]
struct Link {
    #[serde(rename = "readLink")]
    read_link: Option<String>,
    #[serde(rename = "writeLink")]
    write_link: Option<String>,
    #[serde(rename = "lastModified")]
    last_modified: Option<String>,
}

impl EpicStore {
    pub fn new(http: Client, token: Secret, account_id: &str, app_name: &str) -> Result<Self> {
        let mut url = Url::parse(DATASTORAGE).map_err(|e| Error::parse("Epic's storage", e))?;
        url.path_segments_mut()
            .map_err(|_| Error::parse("Epic's storage", "not a base address"))?
            .pop_if_empty()
            .push(account_id)
            .push(app_name)
            .push("");
        Ok(Self {
            http,
            token,
            url,
            read_links: Mutex::new(HashMap::new()),
        })
    }
}

/// An address Epic named for a file, used only over https.
fn link(url: &str) -> Result<&str> {
    match Url::parse(url) {
        Ok(u) if u.scheme() == "https" && u.host_str().is_some() => Ok(url),
        _ => Err(Error::Refused(
            "Epic named a cloud save address Slatty does not use".into(),
        )),
    }
}

impl SaveStore for EpicStore {
    async fn list(&self) -> Result<Vec<Stored>> {
        let req = self
            .http
            .get(self.url.clone())
            .bearer_auth(self.token.expose())
            .header(reqwest::header::USER_AGENT, super::USER_AGENT);
        let listing: Listing =
            super::library::session_checked(http::json(req, "listing the Epic cloud saves").await)?;
        let mut links = HashMap::new();
        let mut stored = Vec::new();
        for (key, l) in listing.files {
            if let Some(read) = l.read_link {
                links.insert(key.clone(), read);
            }
            stored.push(Stored {
                key,
                last_modified: l.last_modified,
            });
        }
        *self.read_links.lock().unwrap() = links;
        Ok(stored)
    }

    async fn read(&self, key: &str) -> Result<Vec<u8>> {
        let url = self.read_links.lock().unwrap().get(key).cloned();
        let url = url.ok_or_else(|| Error::NotFound(format!("{key} is not in Epic's cloud")))?;
        let req = self
            .http
            .get(link(&url)?)
            .header(reqwest::header::USER_AGENT, super::USER_AGENT);
        http::bytes(req, "downloading an Epic cloud save").await
    }

    async fn write(&self, files: Vec<(String, Vec<u8>)>) -> Result<()> {
        let context = "asking Epic where to upload a cloud save";
        let names: Vec<&String> = files.iter().map(|(n, _)| n).collect();
        let req = self
            .http
            .post(self.url.clone())
            .bearer_auth(self.token.expose())
            .header(reqwest::header::USER_AGENT, super::USER_AGENT)
            .json(&serde_json::json!({ "files": names }));
        let resp = super::library::session_checked(http::send(req, context).await)?;
        let raw = http::read_up_to(resp, context, http::MAX_BODY).await?;
        let listing: Listing =
            serde_json::from_slice(&raw).map_err(|e| Error::parse(context, e))?;
        // Named as asked (`upload_save`); a full key is taken too.
        let write_link = |name: &str| {
            listing
                .files
                .iter()
                .find(|(k, _)| *k == name || k.ends_with(&format!("/{name}")))
                .and_then(|(_, l)| l.write_link.clone())
        };
        for (name, data) in files {
            let url = write_link(&name)
                .ok_or_else(|| Error::parse(context, "no address for a file of the save"))?;
            let req = self
                .http
                .put(link(&url)?)
                .header(reqwest::header::USER_AGENT, super::USER_AGENT)
                .body(data);
            http::send(req, "uploading an Epic cloud save").await?;
        }
        Ok(())
    }
}

/// The newest snapshot, as last listed.
#[derive(Clone)]
struct Snapshot {
    /// The keys of the game's files begin with it (`<…>/manifests/<stamp>.manifest`).
    prefix: String,
    manifest: Arc<Manifest>,
}

impl Snapshot {
    fn key(&self, name: &str) -> String {
        match self.prefix.as_str() {
            "" => name.to_string(),
            prefix => format!("{prefix}/{name}"),
        }
    }
}

#[derive(Default)]
struct State {
    newest: Option<Snapshot>,
    /// The newest snapshot's name of all, which a new one must come after.
    latest_stamp: Option<String>,
    /// Unpacked chunks of the newest snapshot, read once.
    chunks: HashMap<Guid, Arc<Vec<u8>>>,
    /// What the next commit sends, by path in the save folder.
    put: BTreeMap<String, Vec<u8>>,
    delete: BTreeSet<String>,
}

/// A game's Epic cloud saves as a `CloudTransport`.
pub struct EpicCloud<S> {
    store: S,
    /// Named in each snapshot, as legendary names them: the app name followed by the account id.
    meta_name: String,
    /// The save folder's template, kept in each snapshot (`CloudSaveFolder`).
    template: String,
    state: Mutex<State>,
}

/// The manifest's name without `.manifest`, and the prefix of the game's keys, of a key naming a
/// snapshot.
fn snapshot_key(key: &str) -> Option<(&str, &str)> {
    let (dir, file) = key.rsplit_once('/')?;
    let stamp = file.strip_suffix(".manifest")?;
    let prefix = match dir {
        "manifests" => "",
        dir => dir.strip_suffix("/manifests")?,
    };
    Some((stamp, prefix))
}

fn rel_of(name: &str) -> Result<&str> {
    name.strip_prefix(LOCATION)
        .and_then(|n| n.strip_prefix('/'))
        .ok_or_else(|| Error::NotFound(format!("{name} is not an Epic cloud save")))
}

impl<S: SaveStore> EpicCloud<S> {
    pub fn new(store: S, app_name: &str, account_id: &str, template: &str) -> Self {
        Self {
            store,
            meta_name: format!("{app_name}{account_id}"),
            template: template.to_string(),
            state: Mutex::new(State::default()),
        }
    }

    /// Lists the storage and reads its newest snapshot: the one stored last, as legendary takes
    /// it (`get_save_games`, by `lastModified`), its name breaking ties.
    async fn refresh(&self) -> Result<Option<Snapshot>> {
        let stored = self.store.list().await?;
        let snapshots: Vec<(&Stored, &str, &str)> = stored
            .iter()
            .filter_map(|s| snapshot_key(&s.key).map(|(stamp, prefix)| (s, stamp, prefix)))
            .collect();
        let latest_stamp = snapshots.iter().map(|(_, stamp, _)| *stamp).max();
        let newest = snapshots
            .iter()
            .max_by(|a, b| (&a.0.last_modified, a.1).cmp(&(&b.0.last_modified, b.1)));
        let snapshot = match newest {
            None => None,
            Some((s, _, prefix)) => {
                let raw = self.store.read(&s.key).await?;
                Some(Snapshot {
                    prefix: prefix.to_string(),
                    manifest: Arc::new(manifest::parse(&raw, &HashMap::new())?),
                })
            }
        };
        let mut state = self.state.lock().unwrap();
        state.newest = snapshot.clone();
        state.latest_stamp = latest_stamp.map(str::to_string);
        state.chunks.clear();
        Ok(snapshot)
    }

    /// A file of `snapshot`, put together from its chunks and checked against its SHA-1.
    async fn assemble(&self, snapshot: &Snapshot, file: &FileEntry) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(file.size() as usize);
        for part in &file.parts {
            let cached = self.state.lock().unwrap().chunks.get(&part.guid).cloned();
            let chunk = match cached {
                Some(c) => c,
                None => {
                    let info = snapshot.manifest.chunks.get(&part.guid).ok_or_else(|| {
                        Error::parse("an Epic cloud save", "a part names no chunk")
                    })?;
                    let raw = self
                        .store
                        .read(&snapshot.key(&info.path(snapshot.manifest.feature_level)))
                        .await?;
                    let unpacked = Arc::new(chunk::unpack(&raw, info, &HashMap::new())?);
                    self.state
                        .lock()
                        .unwrap()
                        .chunks
                        .insert(part.guid, unpacked.clone());
                    unpacked
                }
            };
            let (at, size) = (part.offset as usize, part.size as usize);
            let piece = chunk.get(at..at + size).ok_or_else(|| {
                Error::parse("an Epic cloud save", "a part lies outside its chunk")
            })?;
            data.extend_from_slice(piece);
        }
        if super::sha1(&data) != file.sha1 {
            return Err(Error::Refused(format!(
                "{} does not match its checksum in Epic's cloud",
                file.name
            )));
        }
        Ok(data)
    }

    /// The name of a new snapshot: now, after every one stored.
    fn next_stamp(latest: Option<&str>) -> String {
        let now = Utc::now().naive_utc();
        let after = latest
            .and_then(|l| NaiveDateTime::parse_from_str(l, STAMP).ok())
            .map(|l| l + chrono::Duration::seconds(1));
        match after {
            Some(after) if after > now => after,
            _ => now,
        }
        .format(STAMP)
        .to_string()
    }

    /// A snapshot of `files` (path, content), in the order legendary writes them (by path,
    /// regardless of case): its chunks by name, then its manifest.
    fn pack(&self, files: Vec<(String, Vec<u8>)>, stamp: &str) -> Vec<(String, Vec<u8>)> {
        let mut m = Manifest {
            feature_level: FEATURE_LEVEL,
            app_name: self.meta_name.clone(),
            build_version: stamp.to_string(),
            launch_exe: String::new(),
            launch_command: String::new(),
            prerequisites: None,
            chunks: HashMap::new(),
            files: Vec::new(),
        };
        let mut out = Vec::new();
        let mut shas = HashMap::new();
        let mut buf = Vec::with_capacity(chunk::SAVE_CHUNK);
        let mut guid = new_guid();
        let mut finish = |buf: &mut Vec<u8>, guid: Guid, m: &mut Manifest| {
            let (info, sha, bytes) = chunk::pack_save(guid, buf);
            out.push((info.path(FEATURE_LEVEL), bytes));
            shas.insert(guid, sha);
            m.chunks.insert(guid, info);
            buf.clear();
        };
        for (name, data) in files {
            let mut parts = Vec::new();
            let mut at = 0;
            while at < data.len() {
                let n = (data.len() - at).min(chunk::SAVE_CHUNK - buf.len());
                parts.push(ChunkPart {
                    guid,
                    offset: buf.len() as u32,
                    size: n as u32,
                });
                buf.extend_from_slice(&data[at..at + n]);
                at += n;
                if buf.len() == chunk::SAVE_CHUNK {
                    finish(&mut buf, guid, &mut m);
                    guid = new_guid();
                }
            }
            m.files.push(FileEntry {
                name,
                symlink_target: String::new(),
                sha1: super::sha1(&data),
                flags: 0,
                parts,
            });
        }
        if !buf.is_empty() {
            finish(&mut buf, guid, &mut m);
        }
        let raw = manifest::write::manifest_with(&m, &shas, &[("CloudSaveFolder", &self.template)]);
        out.push((format!("manifests/{stamp}.manifest"), raw));
        out
    }
}

/// A chunk's guid: random, as legendary's (`uuid4`).
fn new_guid() -> Guid {
    let mut bytes = [0u8; 16];
    aws_lc_rs::rand::fill(&mut bytes).expect("the system gives random bytes");
    std::array::from_fn(|i| u32::from_be_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}

impl<S: SaveStore> CloudTransport for EpicCloud<S> {
    async fn list(&self) -> Result<Vec<RemoteEntry>> {
        let Some(snapshot) = self.refresh().await? else {
            return Ok(Vec::new());
        };
        Ok(snapshot
            .manifest
            .files
            .iter()
            .filter(|f| f.symlink_target.is_empty())
            .map(|f| RemoteEntry {
                name: format!("{LOCATION}/{}", f.name),
                hash: fsutil::hex(&f.sha1),
                last_modified: None,
            })
            .collect())
    }

    async fn download(&self, name: &str) -> Result<Vec<u8>> {
        let rel = rel_of(name)?;
        let snapshot = self.state.lock().unwrap().newest.clone();
        let snapshot =
            snapshot.ok_or_else(|| Error::NotFound(format!("{rel} is not in Epic's cloud")))?;
        let file = snapshot
            .manifest
            .files
            .iter()
            .find(|f| f.name == rel)
            .ok_or_else(|| Error::NotFound(format!("{rel} is not in Epic's cloud")))?;
        self.assemble(&snapshot, file).await
    }

    async fn upload(&self, name: &str, data: &[u8], _: chrono::DateTime<Utc>) -> Result<()> {
        let rel = rel_of(name)?.to_string();
        let mut state = self.state.lock().unwrap();
        state.delete.remove(&rel);
        state.put.insert(rel, data.to_vec());
        Ok(())
    }

    async fn delete(&self, name: &str) -> Result<()> {
        let rel = rel_of(name)?.to_string();
        let mut state = self.state.lock().unwrap();
        state.put.remove(&rel);
        state.delete.insert(rel);
        Ok(())
    }

    /// One snapshot: the newest one listed now, its files not asked for read back from it, with
    /// the uploads and without the deletions. Its chunks go first, its manifest last, so that no
    /// snapshot ever names a chunk not stored yet.
    async fn commit(&self) -> Result<()> {
        let (put, delete) = {
            let mut state = self.state.lock().unwrap();
            (
                std::mem::take(&mut state.put),
                std::mem::take(&mut state.delete),
            )
        };
        if put.is_empty() && delete.is_empty() {
            return Ok(());
        }
        // Windows saves are case-insensitive: one file whatever the case it is named with.
        let changed: BTreeSet<String> = put
            .keys()
            .chain(&delete)
            .map(|r| r.to_lowercase())
            .collect();
        let mut files: BTreeMap<String, (String, Vec<u8>)> = BTreeMap::new();
        if let Some(snapshot) = self.refresh().await? {
            for f in &snapshot.manifest.files {
                if !f.symlink_target.is_empty() || changed.contains(&f.name.to_lowercase()) {
                    continue;
                }
                let data = self.assemble(&snapshot, f).await?;
                files.insert(f.name.to_lowercase(), (f.name.clone(), data));
            }
        }
        for (rel, data) in put {
            files.insert(rel.to_lowercase(), (rel, data));
        }
        if files.is_empty() {
            return Err(Error::Refused(
                "Epic keeps no empty cloud save: its last files are left there".into(),
            ));
        }
        let latest = self.state.lock().unwrap().latest_stamp.clone();
        let stamp = Self::next_stamp(latest.as_deref());
        let snapshot = self.pack(files.into_values().collect(), &stamp);
        self.store.write(snapshot).await
    }
}

/// The lists of files a game includes in its cloud saves and excludes from them
/// (`CloudIncludeList`, `CloudExcludeList`), matched as legendary matches them
/// (`_filename_matches`), regardless of case as on Windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveFilter {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl SaveFilter {
    fn list(value: Option<&str>) -> Vec<String> {
        value
            .unwrap_or_default()
            .split(',')
            .map(|p| p.trim().replace('\\', "/").to_lowercase())
            .filter(|p| !p.is_empty())
            .collect()
    }

    /// Whether the file at `rel` (`/` between folders) is a save.
    pub fn keeps(&self, rel: &str) -> bool {
        let rel = rel.to_lowercase();
        let any = |patterns: &[String]| patterns.iter().any(|p| matches(&rel, p));
        (self.include.is_empty() || any(&self.include)) && !any(&self.exclude)
    }
}

/// A folder (`dir/`) the path is in, an end of the path, or a shell pattern of it (`*` across
/// folders too, `?`, `[…]`, `[!…]`), as Python's `fnmatch`.
fn matches(rel: &str, pattern: &str) -> bool {
    (pattern.ends_with('/') && rel.starts_with(pattern))
        || rel.ends_with(pattern)
        || glob(
            &rel.chars().collect::<Vec<_>>(),
            &pattern.chars().collect::<Vec<_>>(),
        )
}

fn glob(s: &[char], p: &[char]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some('*') => (0..=s.len()).any(|i| glob(&s[i..], &p[1..])),
        Some('?') => !s.is_empty() && glob(&s[1..], &p[1..]),
        Some('[') => {
            let Some(close) = p.iter().skip(2).position(|c| *c == ']').map(|i| i + 2) else {
                return s.first() == Some(&'[') && glob(&s[1..], &p[1..]);
            };
            let Some(c) = s.first() else {
                return false;
            };
            let (negated, set) = match p[1] {
                '!' => (true, &p[2..close]),
                _ => (false, &p[1..close]),
            };
            let mut found = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    found |= (set[i]..=set[i + 2]).contains(c);
                    i += 3;
                } else {
                    found |= set[i] == *c;
                    i += 1;
                }
            }
            found != negated && glob(&s[1..], &p[close + 1..])
        }
        Some(c) => s.first() == Some(c) && glob(&s[1..], &p[1..]),
    }
}

/// The save folder a template names inside the game's prefix or folder (`get_save_path`): a
/// whole part `{AppData}` (Local AppData), `{UserDir}` (Documents), `{UserSavedGames}` or
/// `{InstallDir}` first, `{EpicID}` (the account id) anywhere after. Resolved as GOG's are, and
/// so refused through a link out of an isolated game's prefix or folder.
pub fn save_root(template: &str, account_id: &str, install: &Install) -> Result<PathBuf> {
    let unsupported = || {
        Error::Unsupported(format!(
            "Epic's save folder {template} is not one Slatty can find"
        ))
    };
    let template = template.trim().replace('\\', "/");
    let mut parts = template.split('/').filter(|p| !p.is_empty());
    let root = match parts.next().map(str::to_lowercase).as_deref() {
        Some("{appdata}") => "<?APPLICATION_DATA_LOCAL?>",
        Some("{userdir}") => "<?DOCUMENTS?>",
        Some("{usersavedgames}") => "<?SAVED_GAMES?>",
        Some("{installdir}") => "<?INSTALL?>",
        _ => return Err(unsupported()),
    };
    let mut gog = root.to_string();
    for part in parts {
        let part = if part.eq_ignore_ascii_case("{epicid}") {
            account_id
        } else if part.contains(['{', '}']) {
            return Err(unsupported());
        } else {
            part
        };
        gog.push('/');
        gog.push_str(part);
    }
    locations::resolve(&gog, install)
}

/// What syncing a game's Epic cloud saves needs.
pub struct GameSaves {
    pub cloud: EpicCloud<EpicStore>,
    pub account_id: String,
    pub template: String,
    pub root: Result<PathBuf>,
    pub filter: SaveFilter,
}

/// `None` when Epic keeps no cloud saves for the game (its catalog names no `CloudSaveFolder`).
pub async fn open(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
) -> Result<Option<GameSaves>> {
    let kept = EpicInstall::load(dirs, &install.game_id)?.ok_or_else(|| {
        Error::NotFound(format!(
            "what Slatty keeps of {} is missing: install it again",
            install.title
        ))
    })?;
    let tokens = super::auth::fresh(http, db, dirs).await?;
    let item = Epic {
        http,
        token: &tokens.access_token,
    }
    .catalog_item(kept.namespace.clone(), kept.catalog_item_id.clone())
    .await?;
    let attribute = |name: &str| {
        item.as_ref()
            .and_then(|i| i.custom_attributes.get(name))
            .map(|a| a.value.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let Some(template) = attribute("CloudSaveFolder") else {
        return Ok(None);
    };
    let store = EpicStore::new(
        http.clone(),
        tokens.access_token.clone(),
        &tokens.account_id,
        &kept.app_name,
    )?;
    Ok(Some(GameSaves {
        cloud: EpicCloud::new(store, &kept.app_name, &tokens.account_id, &template),
        root: save_root(&template, &tokens.account_id, install),
        filter: SaveFilter {
            include: SaveFilter::list(attribute("CloudIncludeList").as_deref()),
            exclude: SaveFilter::list(attribute("CloudExcludeList").as_deref()),
        },
        account_id: tokens.account_id,
        template,
    }))
}

/// Syncs an Epic game's cloud saves by Slatty's rules (docs/cloud-saves.md). `None` when Epic
/// keeps none for it. A sync that writes is refused while the game is played, changed or synced
/// by another operation.
pub async fn sync_game(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
    opts: SyncOptions,
) -> Result<Option<Vec<LocationOutcome>>> {
    let _busy = if opts.dry_run || !crate::runner::prefix_ready(install) {
        None
    } else {
        Some(crate::lock::game(dirs, &install.game_id)?)
    };
    sync_held(db, dirs, http, install, opts).await
}

/// `sync_game` for the play flow, which already holds the game's lock.
pub(crate) async fn sync_held(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    install: &Install,
    opts: SyncOptions,
) -> Result<Option<Vec<LocationOutcome>>> {
    crate::cloud::launched_once(install, opts)?;
    let Some(saves) = open(db, dirs, http, install).await? else {
        return Ok(None);
    };
    let result = match &saves.root {
        Err(e) => Err(Error::Unsupported(format!(
            "cannot resolve {}: {e}",
            saves.template
        ))),
        Ok(root) => {
            let keep = |rel: &str| saves.filter.keeps(rel);
            let target = SyncTarget {
                db,
                dirs,
                user_id: &saves.account_id,
                game_id: &install.game_id,
                location: LOCATION,
                root,
                keep: Some(&keep),
            };
            sync::sync(&saves.cloud, &target, opts).await
        }
    };
    Ok(Some(vec![LocationOutcome {
        name: LOCATION.into(),
        template: saves.template,
        root: saves.root.ok(),
        result,
    }]))
}

#[cfg(test)]
mod tests;
