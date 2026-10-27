use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use slatty_core::account::Account;
use slatty_core::install::Platform;
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest,
};
use slatty_core::settings;
use tokio_util::sync::CancellationToken;

use crate::{Ctx, shown};

#[derive(Args)]
pub struct InstallArgs {
    /// GOG product id, or `epic-<app name>` for an Epic game (see `slatty library list`, `slatty
    /// epic list`)
    game_id: String,
    /// Language code offered by the build, e.g. fr-FR (defaults to English)
    #[arg(long)]
    language: Option<String>,
    /// Build to install (defaults to the setting: Windows until changed in the app)
    #[arg(long, value_enum)]
    platform: Option<PlatformArg>,
    /// Folder that receives the game folder (remembered as the default)
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Proton build used to run a Windows build (remembered as the default)
    #[arg(long)]
    proton: Option<PathBuf>,
    /// Show what would be installed without downloading
    #[arg(long)]
    info: bool,
    /// Discard an interrupted install of this game and start over
    #[arg(long)]
    restart: bool,
    /// Abandon an unfinished install: delete its partial download and forget it
    #[arg(long, conflicts_with_all = ["info", "restart"])]
    cancel: bool,
    /// Install only the base game (by default every owned DLC is installed)
    #[arg(long, conflicts_with = "dlc")]
    no_dlc: bool,
    /// Install only these owned DLC (product ids, see --info)
    #[arg(long, num_args = 1.., value_name = "DLC_ID")]
    dlc: Vec<String>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PlatformArg {
    Windows,
    Linux,
}

impl InstallArgs {
    fn dlc_selection(&self) -> DlcSelection {
        if self.no_dlc {
            DlcSelection::Only(Vec::new())
        } else if self.dlc.is_empty() {
            DlcSelection::AllOwned
        } else {
            DlcSelection::Only(self.dlc.clone())
        }
    }
}

pub async fn run(ctx: &Ctx, args: InstallArgs) -> Result<()> {
    if args.cancel {
        match installer::discard(&ctx.db, &ctx.dirs, &args.game_id)? {
            Some(partial) => println!(
                "Install of {} cancelled; deleted {}",
                args.game_id,
                partial.display()
            ),
            None => println!("No unfinished install of {}.", args.game_id),
        }
        return Ok(());
    }
    if slatty_core::store::epic_app(&args.game_id).is_some() {
        return install_epic(ctx, args).await;
    }
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let platform = match args.platform {
        Some(PlatformArg::Windows) => Platform::Windows,
        Some(PlatformArg::Linux) => Platform::Linux,
        None => settings::default_platform(&ctx.db)?,
    };

    if args.info {
        let plan = installer::plan_for(
            &ctx.http,
            &tokens,
            &args.game_id,
            platform,
            args.language.as_deref(),
            None,
            &args.dlc_selection(),
        )
        .await?;
        println!(
            "{} — version {} (build {})",
            plan.title, plan.build.version_name, plan.build.build_id
        );
        print_language(&plan);
        println!(
            "Download: {}, on disk: {}",
            size(plan.download_size),
            size(plan.disk_size)
        );
        println!("Folder name: {}", plan.directory_name()?);
        print_dlcs(&plan.dlcs);
        for d in &plan.dependencies {
            let place = if d.is_shared() {
                "shared, installed at first launch"
            } else {
                "game folder"
            };
            println!(
                "Dependency: {} ({place})",
                if d.readable_name.is_empty() {
                    &d.dependency_id
                } else {
                    &d.readable_name
                }
            );
        }
        return Ok(());
    }

    let dlcs = args.dlc_selection();
    let (root, proton) = folders(ctx, args.dir, args.proton, platform)?;

    let cancel = paused_by_ctrl_c();
    let emit = print_events();
    let req = InstallRequest {
        game_id: args.game_id.clone(),
        dlcs,
        language: args.language,
        build: None,
        platform,
        root,
        proton,
        restart: args.restart,
    };
    match installer::install(
        &ctx.db,
        &ctx.dirs,
        &ctx.http,
        &tokens,
        req,
        emit,
        cancel.clone(),
    )
    .await
    {
        Ok(install) => {
            println!("Ready: `slatty launch {}`", install.game_id);
            Ok(())
        }
        Err(slatty_core::Error::Cancelled) => {
            println!(
                "Paused. Verified files are kept; run `slatty install {}` to resume.",
                args.game_id
            );
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Installs the Windows build of an Epic game of the account's library, as last read (`slatty
/// epic list`).
async fn install_epic(ctx: &Ctx, args: InstallArgs) -> Result<()> {
    use slatty_core::epic;
    if args.language.is_some()
        || args.no_dlc
        || !args.dlc.is_empty()
        || matches!(args.platform, Some(PlatformArg::Linux))
    {
        bail!(
            "an Epic game installs its Windows build alone: --language, --platform linux, --dlc \
             and --no-dlc do not apply"
        );
    }
    let app = slatty_core::store::epic_app(&args.game_id).expect("checked by the caller");
    let account = settings::epic_account(&ctx.db)?
        .context("not signed in to Epic Games: run `slatty epic login`")?;
    let game = epic::library::load_cache(&ctx.dirs, &account)?
        .and_then(|l| l.games.into_iter().find(|g| g.app_name == app))
        .with_context(|| {
            format!(
                "{} is not in your Epic library as last read: run `slatty epic list`",
                args.game_id
            )
        })?;
    let tokens = epic::auth::fresh(&ctx.http, &ctx.db, &ctx.dirs).await?;
    if args.info {
        let plan = epic::install::plan(&ctx.http, &tokens, &game).await?;
        println!(
            "{} — build {}",
            shown(&plan.title),
            shown(&plan.build.build_version)
        );
        println!(
            "Download: {}, on disk: {}",
            size(plan.download_size),
            size(plan.disk_size)
        );
        println!("Folder name: {}", shown(&plan.folder));
        println!("Files: {}", plan.files.files.len());
        println!(
            "Starts: {} {}",
            shown(&plan.manifest.launch_exe),
            shown(&plan.manifest.launch_command)
        );
        if let Some(p) = &plan.manifest.prerequisites {
            println!(
                "Prerequisite, not installed: {} ({})",
                shown(&p.name),
                shown(&p.path)
            );
        }
        return Ok(());
    }
    let (root, proton) = folders(ctx, args.dir, args.proton, Platform::Windows)?;
    let req = epic::install::EpicInstallRequest {
        game,
        root,
        proton,
        restart: args.restart,
    };
    let installed = epic::install::install(
        &ctx.db,
        &ctx.dirs,
        &ctx.http,
        &tokens,
        req,
        print_events(),
        paused_by_ctrl_c(),
    )
    .await;
    match installed {
        Ok(_) => {
            println!("Installed. Playing Epic games from Slatty comes in a later version.");
            Ok(())
        }
        Err(slatty_core::Error::Cancelled) => {
            println!(
                "Paused. Verified files are kept; run `slatty install {}` to resume.",
                args.game_id
            );
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// A token Ctrl+C cancels: the install pauses, its verified files kept.
fn paused_by_ctrl_c() -> CancellationToken {
    let cancel = CancellationToken::new();
    let on_ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_ctrl_c.cancel();
        }
    });
    cancel
}

/// Prints what an install does, its progress at most once a second.
fn print_events() -> impl Fn(InstallEvent) + Send + Sync {
    let last = Mutex::new(None::<Instant>);
    move |e: InstallEvent| match e {
        InstallEvent::Planned {
            title,
            version,
            language,
            download_size,
            disk_size,
            target,
            resumed,
            ..
        } => {
            // An Epic build has no language of its own.
            let language = if language.is_empty() {
                String::new()
            } else {
                format!(" ({language})")
            };
            println!(
                "{} {title} {version}{language} into {}\nDownload {}, on disk {}. Ctrl+C pauses; run the same command to resume.",
                if resumed { "Resuming" } else { "Installing" },
                target.display(),
                size(download_size),
                size(disk_size)
            );
        }
        InstallEvent::Progress(p) => {
            let mut last = last.lock().unwrap();
            if last.is_none_or(|t| t.elapsed() >= Duration::from_secs(1))
                || p.files_done == p.files_total
            {
                *last = Some(Instant::now());
                let pct = (p.bytes_done * 100)
                    .checked_div(p.bytes_total)
                    .unwrap_or(100);
                println!(
                    "  {pct:>3}%  {} / {}  files {}/{}",
                    size(p.bytes_done),
                    size(p.bytes_total),
                    p.files_done,
                    p.files_total
                );
            }
        }
        InstallEvent::Finished {
            path,
            support_files,
            skipped_links,
            dependencies,
        } => {
            println!("Installed and verified in {}", path.display());
            if support_files > 0 {
                println!("  {support_files} GOG support file(s) stored for the first-launch setup");
            }
            if skipped_links > 0 {
                println!("  {skipped_links} symbolic link(s) skipped");
            }
            if !dependencies.is_empty() {
                println!(
                    "  Redistributables not installed: {}",
                    dependencies.join(", ")
                );
            }
        }
    }
}

/// The folder the game's folder goes into and the Proton build it runs with, remembered as the
/// defaults when given.
fn folders(
    ctx: &Ctx,
    dir: Option<PathBuf>,
    proton: Option<PathBuf>,
    platform: Platform,
) -> Result<(PathBuf, PathBuf)> {
    let root = match dir {
        Some(d) => {
            let d = std::path::absolute(&d)?;
            settings::set_library_root(&ctx.db, &d)?;
            d
        }
        None => settings::library_root(&ctx.db)?,
    };
    let proton = if platform == Platform::Linux {
        PathBuf::new()
    } else {
        let proton = match proton {
            Some(p) => {
                let p = std::path::absolute(&p)?;
                settings::set_default_proton(&ctx.db, &p)?;
                p
            }
            None => match settings::default_proton(&ctx.db)? {
                Some(p) => p,
                None => bail!(
                    "choose a Proton build once with --proton <dir containing `proton`>, e.g. one of:\n{}",
                    settings::proton_candidates(&ctx.dirs)
                        .iter()
                        .map(|p| format!("  {}", p.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            },
        };
        if !proton.join("proton").is_file() {
            bail!("{} does not contain a `proton` script", proton.display());
        }
        proton
    };

    Ok((root, proton))
}

pub fn list_jobs(ctx: &Ctx) -> Result<()> {
    for j in InstallJob::list(&ctx.db)? {
        println!(
            "{:>12}  {:<12} {} ({}, build {})",
            j.game_id,
            j.state,
            j.root.join(&j.directory).display(),
            j.language,
            j.build_id
        );
    }
    Ok(())
}

pub fn size(bytes: u64) -> String {
    slatty_core::units::size(bytes)
}

pub fn print_language(plan: &slatty_core::installer::InstallPlan) {
    if plan.language == "*" {
        println!("Language: every language in one download, chosen in the game");
    } else {
        println!(
            "Language: {} (offered: {})",
            plan.language,
            plan.languages.join(", ")
        );
    }
}

pub fn print_dlcs(dlcs: &[DlcChoice]) {
    if dlcs.is_empty() {
        return;
    }
    println!("DLC:");
    for d in dlcs {
        let state = match (d.owned, d.selected) {
            (true, true) => "selected",
            (true, false) => "owned",
            (false, _) => "not owned",
        };
        println!(
            "  {:>12}  {:<40} {:<10} {}",
            shown(&d.id),
            shown(&d.name),
            state,
            size(d.disk_size)
        );
    }
}
