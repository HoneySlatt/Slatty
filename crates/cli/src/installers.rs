//! `slatty installers`: a game's offline installers, as GOG offers them, listed or downloaded to
//! keep.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use clap::Args;
use slatty_core::account::Account;
use slatty_core::installer::{Progress, QUEUED};
use slatty_core::offline::{self, OfflineJob};
use slatty_core::settings;
use tokio_util::sync::CancellationToken;

use crate::install::size;
use crate::{Ctx, shown};

#[derive(Args)]
pub struct InstallersArgs {
    /// GOG product id (see `slatty library list`)
    game_id: String,
    /// Download them; without it, only lists what GOG offers
    #[arg(long)]
    download: bool,
    /// System the installers are for
    #[arg(long, value_enum, default_value = "windows")]
    os: System,
    /// Language codes, e.g. fr de (English when GOG has none of them)
    #[arg(long, num_args = 1.., value_name = "CODE", default_value = "en")]
    language: Vec<String>,
    /// Only the game's installer (by default those of every DLC GOG offers one for come with it)
    #[arg(long, conflicts_with = "dlc")]
    no_dlc: bool,
    /// Only these DLC (product ids, as listed)
    #[arg(long, num_args = 1.., value_name = "DLC_ID")]
    dlc: Vec<String>,
    /// Folder that receives the game's folder of installers (remembered as the default)
    #[arg(long)]
    dir: Option<PathBuf>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum System {
    Windows,
    Linux,
    Mac,
}

impl System {
    fn code(self) -> &'static str {
        match self {
            System::Windows => "windows",
            System::Linux => "linux",
            System::Mac => "mac",
        }
    }
}

pub async fn run(ctx: &Ctx, args: InstallersArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let offer = offline::offer(&ctx.http, &tokens, &args.game_id).await?;
    let root = match &args.dir {
        Some(d) if args.download => {
            let d = std::path::absolute(d)?;
            settings::set_installers_root(&ctx.db, &d)?;
            d
        }
        Some(d) => std::path::absolute(d)?,
        None => settings::installers_root(&ctx.db)?,
    };
    let folder = offline::folder_for(&root, &offer.title, &args.game_id);
    if !args.download {
        println!("{}", shown(&offer.title));
        for os in offer.systems() {
            for i in offer.installers.iter().filter(|i| i.os == os) {
                println!(
                    "  {os:<8} {:<6} {:<24} {:>10}  {} file(s)",
                    shown(&i.language),
                    shown(&i.version),
                    size(i.size()),
                    i.files.len()
                );
            }
        }
        for d in &offer.dlcs {
            let systems: Vec<&str> = offline::SYSTEMS
                .into_iter()
                .filter(|s| d.installers.iter().any(|i| i.os == *s))
                .collect();
            println!(
                "DLC {:>12}  {} ({})",
                shown(&d.id),
                shown(&d.title),
                systems.join(", ")
            );
        }
        println!("Folder: {}", folder.display());
        return Ok(());
    }

    let os = args.os.code();
    if !offer.systems().contains(&os) {
        bail!("GOG offers no {os} installer of {}", shown(&offer.title));
    }
    let dlcs: Vec<String> = if args.no_dlc {
        Vec::new()
    } else if args.dlc.is_empty() {
        offer.dlcs_for(os).map(|d| d.id.clone()).collect()
    } else {
        for id in &args.dlc {
            if !offer.dlcs_for(os).any(|d| d.id == *id) {
                bail!("DLC {} has no {os} installer", shown(id));
            }
        }
        args.dlc.clone()
    };
    let job = OfflineJob {
        game_id: args.game_id.clone(),
        os: os.into(),
        languages: args.language.clone(),
        dlcs,
        folder,
        state: QUEUED.into(),
    };
    println!(
        "Downloading the {os} installers of {} into {}. Ctrl+C pauses; run the same command to \
         resume.",
        shown(&offer.title),
        job.folder.display()
    );
    let cancel = CancellationToken::new();
    let on_ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_ctrl_c.cancel();
        }
    });
    let last = Mutex::new(None::<Instant>);
    let progress = |p: Progress| {
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
    };
    match offline::run(
        &ctx.db, &ctx.dirs, &ctx.http, &tokens, &job, &progress, &cancel,
    )
    .await
    {
        Ok(files) => {
            println!("Downloaded and checked:");
            for f in files {
                println!("  {}", f.display());
            }
            Ok(())
        }
        Err(slatty_core::Error::Cancelled) => {
            println!("Paused. Run the same command to resume.");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
