//! Start-up: everything read before the first page shows, off the interface thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use slatty_core::account::{Account, AccountInfo};
use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::installer::{DOWNLOADING, FAILED, InstallJob, InstallRecord, PAUSED, QUEUED};
use slatty_core::library::LibraryCache;
use slatty_core::lock::FileLock;
use slatty_core::offline::OfflineJob;
use slatty_core::overview::{self, GameOverview};
use slatty_core::paths::Dirs;
use slatty_core::session::{self, Playtime};
use tokio::sync::Semaphore;

use crate::{Core, Filters, InstallSummary, Interrupted, Sort, err, offline};

#[derive(Debug, Clone)]
pub struct Boot {
    pub core: Core,
    pub account: Option<AccountInfo>,
    pub library: Option<LibraryCache>,
    pub installs: Vec<Install>,
    pub records: HashMap<String, InstallSummary>,
    pub launch_choices: HashMap<String, String>,
    pub interrupted: Vec<String>,
    pub library_root: PathBuf,
    pub installers_root: PathBuf,
    pub default_platform: Platform,
    pub twin_playtime: slatty_core::settings::TwinPlaytime,
    pub umu_lookup: bool,
    pub report_playtime: bool,
    pub game_achievements: bool,
    pub isolate_new_games: bool,
    pub manual_achievements: bool,
    /// Whether SteamGridDB is turned on; whether its key is in the keyring is asked once the
    /// library shows.
    pub steamgriddb: bool,
    /// The name of the account signed in to Steam here, when Steam is installed, whether its
    /// games show, and whether images its client has not saved come from Steam's servers.
    pub steam_found: Option<String>,
    pub steam_games: bool,
    pub steam_images: bool,
    /// Whether Steam's Web API is asked; whether its key is in the keyring is asked once the
    /// library shows.
    pub steam_web_api: bool,
    pub proton: Option<PathBuf>,
    /// Whether Proton builds may be downloaded from GitHub, and those already downloaded.
    pub proton_downloads: bool,
    pub downloaded_protons: Vec<PathBuf>,
    /// Whether followed Proton builds are kept up to date, and whether a check is due now.
    pub proton_updates: bool,
    pub proton_update_due: bool,
    /// Whether the downloaded Proton builds are linked into Steam's `compatibilitytools.d`.
    pub proton_shared: bool,
    /// Whether games on their newest build are updated without asking.
    pub auto_update: bool,
    pub favorites: Vec<String>,
    pub playtime: HashMap<String, Playtime>,
    pub overview: HashMap<String, GameOverview>,
    pub jobs: Vec<(String, Interrupted)>,
    pub queue: Vec<String>,
    /// Offline installers paused, or stopped by an error.
    pub offline_stopped: Vec<(String, Option<String>)>,
    pub customs: HashMap<String, slatty_core::custom::Custom>,
    /// What each game sorts by, made here rather than in the first view (10 ms at 10,000 games).
    pub sort_keys: Vec<(String, crate::library::NaturalKey)>,
    pub cover_width: Option<f32>,
    pub sort: Option<Sort>,
    pub remember_filters: bool,
    pub filters: Option<Filters>,
}

pub fn summary(r: &InstallRecord) -> InstallSummary {
    InstallSummary {
        version: r.version.clone(),
        size: r.files.iter().map(|f| f.size).sum(),
    }
}

/// The queue, from its saved order and the queued jobs (one missing from it goes last), and the
/// offline installers paused or failed. Installers cut off go first: they resume by themselves.
pub(crate) fn downloads_from(
    saved: Vec<String>,
    queued: Vec<InstallJob>,
    offline: &[OfflineJob],
) -> (Vec<String>, Vec<(String, Option<String>)>) {
    let offline_in = |state: &str| -> Vec<String> {
        offline
            .iter()
            .filter(|j| j.state == state)
            .map(|j| offline::entry(&j.game_id))
            .collect()
    };
    let mut queue: Vec<String> = saved
        .into_iter()
        .filter(|id| match offline::of_entry(id) {
            Some(_) => offline_in(QUEUED).contains(id),
            None => queued.iter().any(|j| &j.game_id == id),
        })
        .collect();
    for id in queued
        .into_iter()
        .map(|j| j.game_id)
        .chain(offline_in(QUEUED))
    {
        if !queue.contains(&id) {
            queue.push(id);
        }
    }
    queue.splice(0..0, offline_in(DOWNLOADING));
    let stopped = offline
        .iter()
        .filter(|j| j.state == PAUSED || j.state == FAILED)
        .map(|j| {
            let failed = (j.state == FAILED).then(|| "It stopped on an error.".to_string());
            (j.game_id.clone(), failed)
        })
        .collect();
    (queue, stopped)
}

/// The interface's own lock, held until it closes.
static INSTANCE: std::sync::OnceLock<FileLock> = std::sync::OnceLock::new();

/// One interface at a time, taken before anything is read or changed. The command line may still
/// run beside it: each of its changes to a game takes that game's lock.
fn hold_interface(dirs: &Dirs) -> Result<FileLock, String> {
    slatty_core::lock::try_acquire(&dirs.locks().join("interface.lock"))
        .map_err(err)?
        .ok_or_else(|| "SlattyLauncher is already open.".to_string())
}

/// The start-up read, begun by `start` before the window opens.
static STARTED: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<Result<Boot, String>>>> =
    std::sync::Mutex::new(None);

/// Begins reading what the first page needs on a thread of its own, at once: the window and the
/// graphics driver take about a hundred milliseconds to set up, which this read no longer waits
/// for.
pub fn start(backends: Option<wgpu::Backends>) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    *STARTED.lock().unwrap_or_else(|e| e.into_inner()) = Some(rx);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(err);
        let result = match &runtime {
            Ok(runtime) => runtime.block_on(boot()),
            Err(e) => Err(e.clone()),
        };
        let _ = tx.send(result);
        // Which backends the next start can do with, once this one is over.
        if let (Ok(runtime), Some(backends)) = (runtime, backends) {
            std::thread::sleep(crate::graphics::CHECK_AFTER);
            if let Ok(dirs) = Dirs::from_system()
                && let Ok(db) = Db::open(&dirs.db_file())
            {
                runtime.block_on(crate::graphics::check(&db, backends));
            }
        }
    });
}

/// What `start` read, once it has; read here if it was not started.
pub async fn started() -> Result<Boot, String> {
    let rx = STARTED.lock().unwrap_or_else(|e| e.into_inner()).take();
    match rx {
        Some(rx) => rx.await.unwrap_or_else(|_| Err("start-up stopped".into())),
        None => boot().await,
    }
}

async fn boot() -> Result<Boot, String> {
    let dirs = Dirs::from_system().map_err(err)?;
    dirs.keep_private().map_err(err)?;
    let _ = INSTANCE.set(hold_interface(&dirs)?);
    let db = Db::open(&dirs.db_file()).map_err(err)?;
    let http = slatty_core::http::client().map_err(err)?;
    let interrupted = slatty_core::play::recover_unfinished(&db, &dirs)
        .map_err(err)?
        .into_iter()
        .map(|s| s.game_id)
        .collect();
    let account = Account::active(&db).map_err(err)?;
    let (library, overview) = match &account {
        Some(a) => (
            slatty_core::library::load_cache(&dirs, &a.user_id).map_err(err)?,
            overview::load(&dirs, &a.user_id).unwrap_or_default(),
        ),
        None => (None, HashMap::new()),
    };
    let installs = Install::list(&db).map_err(err)?;
    let launch_choices = installs
        .iter()
        .filter_map(|i| {
            slatty_core::settings::launch_choice(&db, &i.game_id)
                .ok()
                .flatten()
                .map(|c| (i.game_id.clone(), c))
        })
        .collect();
    let records = installs
        .iter()
        .filter_map(|i| {
            InstallRecord::load(&dirs, &i.game_id)
                .ok()
                .flatten()
                .map(|r| (i.game_id.clone(), summary(&r)))
        })
        .collect();
    let library_root = slatty_core::settings::library_root(&db).map_err(err)?;
    let proton = slatty_core::settings::default_proton(&db).map_err(err)?;
    let default_platform = slatty_core::settings::default_platform(&db).map_err(err)?;
    let twin_playtime = slatty_core::settings::twin_playtime(&db).map_err(err)?;
    let umu_lookup = slatty_core::settings::umu_lookup(&db).map_err(err)?;
    let report_playtime = slatty_core::settings::report_playtime(&db).map_err(err)?;
    let game_achievements = slatty_core::settings::game_achievements(&db).map_err(err)?;
    let isolate_new_games = slatty_core::settings::isolate_new_games(&db).map_err(err)?;
    let manual_achievements = slatty_core::settings::manual_achievements(&db).map_err(err)?;
    let steam_games = slatty_core::settings::steam_games(&db).map_err(err)?;
    let steam_images = slatty_core::settings::steam_images(&db).map_err(err)?;
    let steam_web_api = slatty_core::settings::steam_web_api(&db).map_err(err)?;
    // Steam's own small file in the home folder; its library folders are read later.
    let steam_found = std::env::var_os("HOME")
        .and_then(|home| slatty_core::steam::local::find(std::path::Path::new(&home)))
        .and_then(|steam| slatty_core::steam::local::signed_in_user(&steam))
        .map(|user| user.name);
    let steamgriddb = slatty_core::settings::steamgriddb(&db).map_err(err)?;
    // A link it could not make only leaves that `-latest` entry out of the menus. Game folders and
    // Steam libraries, which can sit on a network drive still waking up, are read once the library
    // shows (`App::read_launch_options`, `runners::list`).
    let _ = slatty_core::protons::link_newest(&dirs);
    let proton_downloads = slatty_core::settings::proton_downloads(&db).map_err(err)?;
    let downloaded_protons = slatty_core::protons::installed(&dirs);
    let proton_updates = slatty_core::settings::proton_updates(&db).map_err(err)?;
    let proton_shared = slatty_core::settings::share_protons(&db).map_err(err)?;
    let auto_update = slatty_core::settings::auto_update(&db).map_err(err)?;
    let proton_update_due =
        slatty_core::protons::update_due(&db, chrono::Utc::now().timestamp()).map_err(err)?;
    let favorites = slatty_core::settings::favorites(&db).map_err(err)?;
    let customs = slatty_core::custom::all(&db).map_err(err)?;
    let sort_keys = library
        .as_ref()
        .map(|l| crate::library::sort_names(&l.games, &customs))
        .unwrap_or_default();
    let cover_width = slatty_core::settings::cover_width(&db).map_err(err)?;
    let sort = slatty_core::settings::library_sort(&db)
        .map_err(err)?
        .and_then(|s| Sort::from_key(&s));
    let (remember_filters, filters) = crate::library::kept_filters(&db).map_err(err)?;
    let playtime =
        session::playtime(&db, account.as_ref().map(|a| a.user_id.as_str())).map_err(err)?;
    InstallJob::forget_finished(&db).map_err(err)?;
    let (queued, jobs): (Vec<InstallJob>, Vec<InstallJob>) = InstallJob::list(&db)
        .map_err(err)?
        .into_iter()
        .partition(InstallJob::is_queued);
    let jobs = jobs
        .iter()
        .map(|j| (j.game_id.clone(), Interrupted::of(j)))
        .collect();
    let (queue, offline_stopped) = downloads_from(
        slatty_core::settings::download_queue(&db).map_err(err)?,
        queued,
        &OfflineJob::list(&db).map_err(err)?,
    );
    let installers_root = slatty_core::settings::installers_root(&db).map_err(err)?;
    let core = Core {
        dirs,
        db: Arc::new(db),
        http,
        downloads: Arc::new(Semaphore::new(6)),
    };
    Ok(Boot {
        core,
        account,
        library,
        installs,
        launch_choices,
        records,
        interrupted,
        library_root,
        installers_root,
        proton,
        proton_downloads,
        downloaded_protons,
        proton_updates,
        proton_update_due,
        proton_shared,
        auto_update,
        default_platform,
        twin_playtime,
        umu_lookup,
        report_playtime,
        game_achievements,
        isolate_new_games,
        manual_achievements,
        steamgriddb,
        steam_found,
        steam_games,
        steam_images,
        steam_web_api,
        favorites,
        playtime,
        overview,
        jobs,
        queue,
        offline_stopped,
        customs,
        sort_keys,
        cover_width,
        sort,
        remember_filters,
        filters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second interface is refused while one is open: two would each keep a download queue
    /// and overwrite the other's, and each take the other's running game for a crashed session.
    #[test]
    fn one_interface_at_a_time() {
        let dirs = Dirs::under(
            &std::env::temp_dir().join(format!("slatty-gui-instance-{}", std::process::id())),
        );
        let first = hold_interface(&dirs).unwrap();
        assert_eq!(
            hold_interface(&dirs).unwrap_err(),
            "SlattyLauncher is already open."
        );
        drop(first);
        assert!(
            hold_interface(&dirs).is_ok(),
            "free once the first one closed"
        );
        let _ = std::fs::remove_dir_all(dirs.state.parent().unwrap());
    }
}
