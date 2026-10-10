//! Start-up: everything read before the first page shows, off the interface thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use slatty_core::account::{Account, AccountInfo};
use slatty_core::db::Db;
use slatty_core::install::{Install, Platform};
use slatty_core::installer::{InstallJob, InstallRecord};
use slatty_core::library::LibraryCache;
use slatty_core::overview::{self, GameOverview};
use slatty_core::paths::Dirs;
use slatty_core::session::{self, Playtime};
use tokio::sync::Semaphore;

use crate::{Core, InstallSummary, Interrupted, Sort, err};

#[derive(Debug, Clone)]
pub struct Boot {
    pub core: Core,
    pub account: Option<AccountInfo>,
    pub library: Option<LibraryCache>,
    pub installs: Vec<Install>,
    pub records: HashMap<String, InstallSummary>,
    pub launch_options: HashMap<String, Vec<String>>,
    pub launch_choices: HashMap<String, String>,
    pub interrupted: Vec<String>,
    pub library_root: PathBuf,
    pub default_platform: Platform,
    pub umu_lookup: bool,
    pub report_playtime: bool,
    pub game_achievements: bool,
    pub proton: Option<PathBuf>,
    pub proton_choices: Vec<PathBuf>,
    pub favorites: Vec<String>,
    pub playtime: HashMap<String, Playtime>,
    pub overview: HashMap<String, GameOverview>,
    pub jobs: Vec<(String, Interrupted)>,
    pub queue: Vec<String>,
    pub customs: HashMap<String, slatty_core::custom::Custom>,
    pub cover_width: Option<f32>,
    pub sort: Option<Sort>,
}

pub fn summary(r: &InstallRecord) -> InstallSummary {
    InstallSummary {
        version: r.version.clone(),
        size: r.files.iter().map(|f| f.size).sum(),
    }
}

pub async fn boot() -> Result<Boot, String> {
    let dirs = Dirs::from_system().map_err(err)?;
    dirs.keep_private().map_err(err)?;
    let db = Db::open(&dirs.db_file()).map_err(err)?;
    let http = slatty_core::http::client().map_err(err)?;
    let interrupted = slatty_core::play::recover_unfinished(&db)
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
    let launch_options = installs
        .iter()
        .map(|i| (i.game_id.clone(), slatty_core::runner::launch_options(i)))
        .collect();
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
    let umu_lookup = slatty_core::settings::umu_lookup(&db).map_err(err)?;
    let report_playtime = slatty_core::settings::report_playtime(&db).map_err(err)?;
    let game_achievements = slatty_core::settings::game_achievements(&db).map_err(err)?;
    // Steam libraries can sit on slow or network drives: listed here, off the interface thread.
    let proton_choices = slatty_core::settings::proton_candidates();
    let favorites = slatty_core::settings::favorites(&db).map_err(err)?;
    let customs = slatty_core::custom::all(&db).map_err(err)?;
    let cover_width = slatty_core::settings::cover_width(&db).map_err(err)?;
    let sort = slatty_core::settings::library_sort(&db)
        .map_err(err)?
        .and_then(|s| Sort::from_key(&s));
    let playtime = session::playtime(&db).map_err(err)?;
    InstallJob::forget_finished(&db).map_err(err)?;
    let (queued, jobs): (Vec<InstallJob>, Vec<InstallJob>) = InstallJob::list(&db)
        .map_err(err)?
        .into_iter()
        .partition(InstallJob::is_queued);
    let jobs = jobs
        .iter()
        .map(|j| (j.game_id.clone(), Interrupted::of(j)))
        .collect();
    // The queue in its saved order; a queued job missing from it goes last.
    let mut queue: Vec<String> = slatty_core::settings::download_queue(&db)
        .map_err(err)?
        .into_iter()
        .filter(|id| queued.iter().any(|j| &j.game_id == id))
        .collect();
    for j in queued {
        if !queue.contains(&j.game_id) {
            queue.push(j.game_id);
        }
    }
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
        launch_options,
        launch_choices,
        records,
        interrupted,
        library_root,
        proton,
        proton_choices,
        default_platform,
        umu_lookup,
        report_playtime,
        game_achievements,
        favorites,
        playtime,
        overview,
        jobs,
        queue,
        customs,
        cover_width,
        sort,
    })
}
