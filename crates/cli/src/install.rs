use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Args;
use slatty_core::account::Account;
use slatty_core::installer::{self, InstallEvent, InstallJob, InstallRequest};
use tokio_util::sync::CancellationToken;

use crate::Ctx;

const LIBRARY_ROOT: &str = "library_root";
const DEFAULT_PROTON: &str = "default_proton";

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
}

pub async fn run(ctx: &Ctx, args: InstallArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();

    if args.info {
        let plan = installer::plan_for(
            &ctx.http,
            &tokens,
            &args.game_id,
            args.language.as_deref(),
            None,
        )
        .await?;
        println!(
            "{} — version {} (build {})",
            plan.title, plan.build.version_name, plan.build.build_id
        );
        println!(
            "Language: {} (offered: {})",
            plan.language,
            plan.languages.join(", ")
        );
        println!(
            "Download: {}, on disk: {}",
            size(plan.download_size),
            size(plan.disk_size)
        );
        println!("Folder name: {}", plan.directory_name()?);
        if !plan.meta.dependencies.is_empty() {
            println!(
                "Redistributables not installed by slatty: {}",
                plan.meta.dependencies.join(", ")
            );
        }
        return Ok(());
    }

    let root = match args.dir {
        Some(d) => remember(ctx, LIBRARY_ROOT, d)?,
        None => match ctx.db.setting(LIBRARY_ROOT)? {
            Some(d) => PathBuf::from(d),
            None => {
                let home = std::env::var_os("HOME").context("HOME is not set")?;
                PathBuf::from(home).join("Games/GOG")
            }
        },
    };
    let proton = match args.proton {
        Some(p) => remember(ctx, DEFAULT_PROTON, p)?,
        None => match ctx.db.setting(DEFAULT_PROTON)? {
            Some(p) => PathBuf::from(p),
            None => bail!(
                "choose a Proton build once with --proton <dir containing `proton`>, e.g. one of:\n{}",
                proton_candidates().join("\n")
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
    let last = Mutex::new(Instant::now() - Duration::from_secs(10));
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
            if last.elapsed() >= Duration::from_secs(1) || p.files_done == p.files_total {
                *last = Instant::now();
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
            skipped_support,
            skipped_links,
            dependencies,
        } => {
            println!("Installed and verified in {}", path.display());
            if skipped_support > 0 {
                println!(
                    "  {skipped_support} GOG support file(s) skipped (installer scripts, icons)"
                );
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
        language: args.language,
        root,
        proton,
        restart: args.restart,
    };
    match installer::install(&ctx.db, &ctx.dirs, &ctx.http, &tokens, req, emit, &cancel).await {
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

fn remember(ctx: &Ctx, key: &str, path: PathBuf) -> Result<PathBuf> {
    let path = std::path::absolute(&path)?;
    ctx.db.set_setting(key, Some(&path.to_string_lossy()))?;
    Ok(path)
}

fn proton_candidates() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let base = PathBuf::from(home).join(".local/share/Steam/compatibilitytools.d");
    std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join("proton").is_file())
        .map(|p| format!("  {}", p.display()))
        .collect()
}

fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b => format!("{} KiB", b >> 10),
    }
}
