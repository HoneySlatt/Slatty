//! Installing a game: a Windows build from GOG's content system, or a Linux build from GOG's
//! offline installer.

mod download;
mod job;
pub mod linux;
mod plan;
mod record;
pub mod zip;

pub use download::*;
pub use job::*;
pub use plan::*;
pub use record::*;

use std::path::{Path, PathBuf};

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
    /// A build GOG offers for the game; the newest public one when `None`.
    pub build: Option<String>,
    /// An interrupted install resumes on its own platform whatever this says.
    pub platform: Platform,
    pub root: PathBuf,
    /// Unused for a Linux build.
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
    let (platform, language, build_id, root, dlcs) = match &job {
        Some(j) => (
            if linux::is_linux_build(&j.build_id) {
                Platform::Linux
            } else {
                Platform::Windows
            },
            Some(j.language.as_str()),
            Some(j.build_id.as_str()),
            j.root.clone(),
            DlcSelection::Only(j.dlcs.clone()),
        ),
        None => (
            req.platform,
            req.language.as_deref(),
            req.build.as_deref(),
            req.root.clone(),
            req.dlcs.clone(),
        ),
    };
    let plan = plan_for(
        http,
        tokens,
        &req.game_id,
        platform,
        language,
        build_id,
        &dlcs,
    )
    .await?;
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
        resumed: job.as_ref().is_some_and(|j| !j.is_queued()),
    });
    // A queued install has not started: a folder already where it goes is not its own.
    let job_resumed = job.as_ref().is_some_and(|j| !j.is_queued());
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

    let progress = |p| emit(InstallEvent::Progress(p));
    let resumed_published = published_unregistered(job_resumed, &partial, &target);
    // What was written: the game's files, GOG's support files, links left out.
    let result = async {
        if plan.platform == Platform::Linux {
            let source = linux::GogInstallers::new(
                http.clone(),
                tokens.clone(),
                Some(dirs),
                plan.linux
                    .iter()
                    .map(|p| p.installer.downlink.clone())
                    .collect(),
            );
            let set = linux::LinuxSet::new(&plan.linux)?;
            let dl = linux::LinuxDownload {
                source: &source,
                cancel,
                progress: &progress,
                free_space: &free_space,
            };
            let skipped = if resumed_published {
                dl.check_installed(&set, &target, true).await?;
                0
            } else {
                dl.run(&set, &partial, &target).await?
            };
            return Ok::<_, Error>((set.recorded_files(), 0, skipped));
        }
        let source = galaxy::GogContent::new(http.clone(), tokens.clone(), dirs);
        let set = collect_files(&source, &plan.depots).await?;
        let dl = Download {
            source: &source,
            cancel,
            progress: &progress,
            free_space: &free_space,
        };
        if resumed_published {
            // Interrupted between publishing the folder and registering the game: its files are
            // checked where they now are.
            dl.check_installed(&set, &target, true).await?;
        } else {
            dl.run(&set, &partial, &target).await?;
        }
        dl.check_installed(&set.support_set(), &support_dir(dirs, &req.game_id), true)
            .await?;
        Ok((recorded_files(&set), set.support.len(), set.skipped_links))
    }
    .await;
    let (files, support_files, skipped_links) = match result {
        Ok(written) => written,
        Err(e) => {
            job.state = if matches!(e, Error::Cancelled) {
                PAUSED.into()
            } else {
                FAILED.into()
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
        files,
    }
    .save(dirs, &req.game_id)?;

    let runner = match plan.platform {
        Platform::Linux => Runner::Native,
        Platform::Windows => Runner::Umu {
            proton: req.proton,
            prefix: dirs.data.join("prefixes").join(&req.game_id),
        },
    };
    let install = Install {
        umu_id: None,
        game_id: req.game_id.clone(),
        title: plan.title.clone(),
        platform: plan.platform,
        path: target.clone(),
        client_id: plan.meta.client_id.clone(),
        isolated: crate::install::isolated_when_installed(db, plan.platform, &runner)?,
        runner,
    };
    install.save(db)?;
    InstallJob::delete(db, &req.game_id)?;
    emit(InstallEvent::Finished {
        path: target,
        support_files,
        skipped_links,
        dependencies: plan.meta.dependencies.clone(),
    });
    Ok(install)
}

/// The partial folder of a resumed install is gone and the game folder exists: the download
/// finished and was renamed into place, but the game was not registered yet.
fn published_unregistered(resumed: bool, partial: &Path, target: &Path) -> bool {
    resumed && !partial.exists() && target.is_dir()
}

#[cfg(test)]
mod tests;
