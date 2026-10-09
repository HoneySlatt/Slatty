use std::io::{BufRead, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Args;
use slatty_core::account::Account;
use slatty_core::installer::Progress;
use slatty_core::maintenance::{self, Change};
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
    let checked = maintenance::check(
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
    let bad = checked.bad;
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
        if checked.reused_bytes > 0 {
            println!(
                "{} of intact data copied from the damaged files instead of downloaded.",
                crate::install::size(checked.reused_bytes)
            );
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

#[derive(Args)]
pub struct UpdateArgs {
    /// Game to update; without it, every game installed by slatty is checked
    game_id: Option<String>,
    /// Only report whether an update is available
    #[arg(long)]
    check: bool,
}

pub async fn update(ctx: &Ctx, args: UpdateArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let Some(game_id) = args.game_id else {
        for install in slatty_core::install::Install::list(&ctx.db)? {
            match maintenance::check_update(
                &ctx.db,
                &ctx.dirs,
                &ctx.http,
                &tokens,
                &install.game_id,
            )
            .await
            {
                Ok(Some(u)) => println!(
                    "{:>12}  {:<40} {} -> {}",
                    install.game_id, install.title, u.installed_version, u.available_version
                ),
                Ok(None) => println!("{:>12}  {:<40} up to date", install.game_id, install.title),
                Err(slatty_core::Error::Refused(_)) => {
                    println!(
                        "{:>12}  {:<40} not installed by slatty",
                        install.game_id, install.title
                    )
                }
                Err(e) => println!("{:>12}  {:<40} error: {e}", install.game_id, install.title),
            }
        }
        return Ok(());
    };
    if args.check {
        match maintenance::check_update(&ctx.db, &ctx.dirs, &ctx.http, &tokens, &game_id).await? {
            Some(u) => println!(
                "Update available: {} -> {}",
                u.installed_version, u.available_version
            ),
            None => println!("Up to date."),
        }
        return Ok(());
    }
    apply_change(ctx, &tokens, &game_id, Change::Update).await
}

async fn apply_change(
    ctx: &Ctx,
    tokens: &slatty_core::auth::Tokens,
    game_id: &str,
    change: Change,
) -> Result<()> {
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
    match maintenance::reconfigure(
        &ctx.db, &ctx.dirs, &ctx.http, tokens, game_id, change, &progress, cancel,
    )
    .await
    {
        Ok(r) => {
            if r.resumed {
                println!(
                    "An unfinished change was completed first; run the command again for the new one."
                );
            }
            println!("Now at {} (was {}).", r.to_version, r.from_version);
            println!(
                "{} file(s) downloaded, {} removed.",
                r.downloaded.len(),
                r.removed.len()
            );
            if !r.patched.is_empty() {
                println!(
                    "{} file(s) rebuilt from GOG's binary patches ({} downloaded).",
                    r.patched.len(),
                    crate::install::size(r.patch_bytes)
                );
            }
            if r.reused_bytes > 0 {
                println!(
                    "{} of unchanged data copied from the installed files instead of downloaded.",
                    crate::install::size(r.reused_bytes)
                );
            }
            Ok(())
        }
        Err(slatty_core::Error::Cancelled) => {
            println!("Paused. The game cannot be launched until the same command completes.");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Args)]
pub struct ContentArgs {
    game_id: String,
    /// Switch the game to this language (as listed without options)
    #[arg(long, conflicts_with_all = ["add_dlc", "remove_dlc"])]
    language: Option<String>,
    /// Install these owned DLC
    #[arg(long, num_args = 1.., value_name = "DLC_ID")]
    add_dlc: Vec<String>,
    /// Remove these DLC
    #[arg(long, num_args = 1.., value_name = "DLC_ID")]
    remove_dlc: Vec<String>,
}

pub async fn content(ctx: &Ctx, args: ContentArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    if let Some(language) = args.language {
        return apply_change(ctx, &tokens, &args.game_id, Change::Language(language)).await;
    }
    if !args.add_dlc.is_empty() || !args.remove_dlc.is_empty() {
        let record = slatty_core::installer::InstallRecord::load(&ctx.dirs, &args.game_id)?
            .ok_or_else(|| anyhow::anyhow!("{} was not installed by slatty", args.game_id))?;
        let mut dlcs: Vec<String> = record
            .dlcs
            .into_iter()
            .filter(|d| !args.remove_dlc.contains(d))
            .collect();
        for d in args.add_dlc {
            if !dlcs.contains(&d) {
                dlcs.push(d);
            }
        }
        return apply_change(ctx, &tokens, &args.game_id, Change::Dlcs(dlcs)).await;
    }
    let plan =
        maintenance::content_options(&ctx.db, &ctx.dirs, &ctx.http, &tokens, &args.game_id).await?;
    println!("{} — version {}", plan.title, plan.build.version_name);
    println!(
        "Language: {} (offered: {})",
        plan.language,
        plan.languages.join(", ")
    );
    crate::install::print_dlcs(&plan.dlcs);
    Ok(())
}

#[derive(Args)]
pub struct SetupArgs {
    game_id: String,
    /// Show what would be downloaded and run, without doing it
    #[arg(long)]
    dry_run: bool,
    /// Run again even if it already ran for the installed build
    #[arg(long)]
    force: bool,
}

pub async fn setup(ctx: &Ctx, args: SetupArgs) -> Result<()> {
    let install = crate::games::get(ctx, &args.game_id)?;
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    if args.dry_run {
        let preview = slatty_core::setup::preview(&ctx.dirs, &ctx.http, &tokens, &install).await?;
        for d in &preview.dependencies {
            println!("download: {} ({})", d.readable_name, d.executable.path);
        }
        for c in &preview.commands {
            println!(
                "run: {}\n  {} {}",
                c.label,
                c.program.display(),
                c.args.join(" ")
            );
        }
        if preview.commands.is_empty() {
            println!("Nothing to run for this game.");
        }
        return Ok(());
    }
    if install
        .runner
        .prefix()
        .is_some_and(|p| !p.join("drive_c/users").is_dir())
    {
        anyhow::bail!(
            "the Wine prefix does not exist yet; launch the game once (the setup then runs automatically)"
        );
    }
    let emit = |e: slatty_core::setup::SetupEvent| match e {
        slatty_core::setup::SetupEvent::Downloading => println!("Downloading setup files…"),
        slatty_core::setup::SetupEvent::Running(label) => println!("Running {label}…"),
        slatty_core::setup::SetupEvent::NonZeroExit { label, code } => {
            println!("  {label} exited with code {code:?}")
        }
    };
    let ran = slatty_core::setup::run(
        &ctx.dirs,
        &ctx.http,
        &tokens,
        &install,
        &std::env::current_exe()?,
        args.force,
        &emit,
    )
    .await?;
    if ran.is_empty() {
        println!("Setup already done for the installed build (use --force to run it again).");
    } else {
        println!("Setup done ({} step(s)).", ran.len());
    }
    Ok(())
}
