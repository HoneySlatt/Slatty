use std::io::{BufRead, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Args;
use slatty_core::account::Account;
use slatty_core::installer::Progress;
use slatty_core::maintenance;
use tokio_util::sync::CancellationToken;

use crate::Ctx;

#[derive(Args)]
pub struct UninstallArgs {
    game_id: String,
    /// Also delete the Wine prefix (its `users` folder, where most saves live, is backed up first)
    #[arg(long)]
    delete_prefix: bool,
    /// Do not ask for confirmation
    #[arg(long)]
    yes: bool,
}

#[derive(Args)]
pub struct VerifyArgs {
    game_id: String,
    /// Download again the files that are missing or damaged
    #[arg(long)]
    repair: bool,
}

pub fn uninstall(ctx: &Ctx, args: UninstallArgs) -> Result<()> {
    let install = crate::games::get(ctx, &args.game_id)?;
    println!(
        "Uninstall {} from {}.",
        install.title,
        install.path.display()
    );
    println!("Only files installed by slatty are deleted; anything else in that folder is kept.");
    match (args.delete_prefix, install.runner.prefix()) {
        (true, Some(p)) => println!(
            "The Wine prefix {} is deleted after its users folder is backed up.",
            p.display()
        ),
        (false, Some(p)) => println!("The Wine prefix {} (local saves) is kept.", p.display()),
        _ => {}
    }
    if !args.yes && !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let r = maintenance::uninstall(&ctx.db, &ctx.dirs, &args.game_id, args.delete_prefix)?;
    println!("{} file(s) removed.", r.removed_files);
    if !r.kept.is_empty() {
        println!(
            "Kept in {} (not installed by slatty):",
            install.path.display()
        );
        for k in &r.kept {
            println!("  {}", k.display());
        }
    }
    if let Some(b) = &r.prefix_backup {
        println!("Prefix user folder backed up to {}", b.display());
    }
    Ok(())
}

pub async fn verify(ctx: &Ctx, args: VerifyArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let cancel = CancellationToken::new();
    let on_ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_ctrl_c.cancel();
        }
    });
    let last = Mutex::new(Instant::now() - Duration::from_secs(10));
    let progress = |p: Progress| {
        let mut last = last.lock().unwrap();
        if last.elapsed() >= Duration::from_secs(1) {
            *last = Instant::now();
            println!("  files {}/{}", p.files_done, p.files_total);
        }
    };
    let bad = maintenance::check(
        &ctx.db,
        &ctx.dirs,
        &ctx.http,
        &tokens,
        &args.game_id,
        args.repair,
        &progress,
        cancel,
    )
    .await?;
    if bad.is_empty() {
        println!("All files are intact.");
    } else {
        println!(
            "{} file(s) {}:",
            bad.len(),
            if args.repair {
                "repaired"
            } else {
                "missing or damaged"
            }
        );
        for b in &bad {
            println!("  {}", b.display());
        }
        if !args.repair {
            println!("Run with --repair to download them again.");
        }
    }
    Ok(())
}

fn confirm() -> Result<bool> {
    eprint!("Proceed? [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "o" | "oui"))
}
