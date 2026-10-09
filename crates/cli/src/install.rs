use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use clap::Args;
use slatty_core::account::Account;
use slatty_core::installer::{
    self, DlcChoice, DlcSelection, InstallEvent, InstallJob, InstallRequest,
};
use slatty_core::settings;
use tokio_util::sync::CancellationToken;

use crate::Ctx;

#[derive(Args)]
pub struct InstallArgs {
    /// GOG product id (see `slatty library list`)
    game_id: String,
    /// Language code offered by the build, e.g. fr-FR (defaults to English)
    #[arg(long)]
    language: Option<String>,
    /// Folder that receives the game folder (remembered as the default)
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Proton build used to run the game (remembered as the default)
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
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();

    if args.info {
        let plan = installer::plan_for(
            &ctx.http,
            &tokens,
            &args.game_id,
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
    let root = match args.dir {
        Some(d) => {
            let d = std::path::absolute(&d)?;
            settings::set_library_root(&ctx.db, &d)?;
            d
        }
        None => settings::library_root(&ctx.db)?,
    };
    let proton = match args.proton {
        Some(p) => {
            let p = std::path::absolute(&p)?;
            settings::set_default_proton(&ctx.db, &p)?;
            p
        }
        None => match settings::default_proton(&ctx.db)? {
            Some(p) => p,
            None => bail!(
                "choose a Proton build once with --proton <dir containing `proton`>, e.g. one of:\n{}",
                settings::proton_candidates()
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

    let cancel = CancellationToken::new();
    let on_ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_ctrl_c.cancel();
        }
    });
    let last = Mutex::new(None::<Instant>);
    let emit = |e: InstallEvent| match e {
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
            println!(
                "{} {title} {version} ({language}) into {}\nDownload {}, on disk {}. Ctrl+C pauses; run the same command to resume.",
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
    };
    let req = InstallRequest {
        game_id: args.game_id.clone(),
        dlcs,
        language: args.language,
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
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KiB", b >> 10),
    }
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
            d.id,
            d.name,
            state,
            size(d.disk_size)
        );
    }
}
