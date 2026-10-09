//! Installing a Windows build from GOG's content system.

mod download;
mod job;
mod plan;
mod record;

pub use download::*;
pub use job::*;
pub use plan::*;
pub use record::*;

use std::path::PathBuf;

use tokio_util::sync::CancellationToken;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::galaxy;
use crate::install::{Install, Platform};
use crate::paths::Dirs;
use crate::runner::Runner;

/// Where a game's GOG support files (installer scripts) are kept.
pub fn support_dir(dirs: &Dirs, game_id: &str) -> PathBuf {
    dirs.data.join("support").join(game_id)
}

pub struct InstallRequest {
    pub game_id: String,
    pub language: Option<String>,
    pub root: PathBuf,
    pub proton: PathBuf,
    pub dlcs: DlcSelection,
    pub restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallEvent {
    Planned {
        title: String,
        version: String,
        language: String,
        languages: Vec<String>,
        download_size: u64,
        disk_size: u64,
        target: PathBuf,
        resumed: bool,
    },
    Progress(Progress),
    Finished {
        path: PathBuf,
        support_files: usize,
        skipped_links: usize,
        dependencies: Vec<String>,
    },
}

/// Downloads, verifies and registers a Windows build. An interrupted install is resumed with
/// the same build, language and folder unless `restart` is set.
pub async fn install(
    db: &Db,
    dirs: &Dirs,
    http: &reqwest::Client,
    tokens: &Tokens,
    req: InstallRequest,
    emit: impl Fn(InstallEvent) + Send + Sync,
    cancel: CancellationToken,
) -> Result<Install> {
    let _busy = crate::lock::game(dirs, &req.game_id)?;
    if Install::get(db, &req.game_id)?.is_some() {
        return Err(Error::Refused(format!(
            "{} is already installed",
            req.game_id
        )));
    }
    let previous = InstallJob::load(db, &req.game_id)?;
    if req.restart
        && let Some(job) = &previous
    {
        let _ = std::fs::remove_dir_all(partial_dir(&job.root, &job.directory));
        InstallJob::delete(db, &job.game_id)?;
    }
    let job = previous.filter(|_| !req.restart);
    let (language, build_id, root, dlcs) = match &job {
        Some(j) => (
            Some(j.language.as_str()),
            Some(j.build_id.as_str()),
            j.root.clone(),
            DlcSelection::Only(j.dlcs.clone()),
        ),
        None => (
            req.language.as_deref(),
            None,
            req.root.clone(),
            req.dlcs.clone(),
        ),
    };
    let plan = plan_for(http, tokens, &req.game_id, language, build_id, &dlcs).await?;
    let directory = plan.directory_name()?;
    let target = root.join(&directory);
    let partial = partial_dir(&root, &directory);
    emit(InstallEvent::Planned {
        title: plan.title.clone(),
        version: plan.build.version_name.clone(),
        language: plan.language.clone(),
        languages: plan.languages.clone(),
        download_size: plan.download_size,
        disk_size: plan.disk_size,
        target: target.clone(),
        resumed: job.is_some(),
    });
    let mut job = InstallJob {
        game_id: req.game_id.clone(),
        build_id: plan.build.build_id.clone(),
        language: plan.language.clone(),
        root: root.clone(),
        directory: directory.clone(),
        state: "downloading".into(),
        dlcs: plan.selected_dlcs(),
    };
    job.save(db)?;

    let result = async {
        let source = galaxy::GogContent::new(http.clone(), tokens.clone(), dirs);
        let set = collect_files(&source, &plan.depots).await?;
        let progress = |p| emit(InstallEvent::Progress(p));
        let dl = Download {
            source: &source,
            cancel,
            progress: &progress,
            free_space: &free_space,
        };
        dl.run(&set, &partial, &target).await?;
        dl.check_installed(&set.support_set(), &support_dir(dirs, &req.game_id), true)
            .await?;
        Ok::<FileSet, Error>(set)
    }
    .await;
    let set = match result {
        Ok(set) => set,
        Err(e) => {
            job.state = if matches!(e, Error::Cancelled) {
                "paused".into()
            } else {
                "failed".into()
            };
            job.save(db)?;
            return Err(e);
        }
    };

    InstallRecord {
        build_id: plan.build.build_id.clone(),
        version: plan.build.version_name.clone(),
        language: plan.language.clone(),
        path: Some(target.clone()),
        dlcs: plan.selected_dlcs(),
        setup_build: None,
        files: recorded_files(&set),
    }
    .save(dirs, &req.game_id)?;

    let install = Install {
        game_id: req.game_id.clone(),
        title: plan.title.clone(),
        platform: Platform::Windows,
        path: target.clone(),
        client_id: plan.meta.client_id.clone(),
        runner: Runner::Umu {
            proton: req.proton,
            prefix: dirs.data.join("prefixes").join(&req.game_id),
        },
    };
    install.save(db)?;
    InstallJob::delete(db, &req.game_id)?;
    emit(InstallEvent::Finished {
        path: target,
        support_files: set.support.len(),
        skipped_links: set.skipped_links,
        dependencies: plan.meta.dependencies.clone(),
    });
    Ok(install)
}

#[cfg(test)]
mod tests;
