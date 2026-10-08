use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use slatty_core::account::Account;
use slatty_core::cloud::{
    self,
    plan::Action,
    sync::{Prefer, SyncOptions, SyncReport},
};
use slatty_core::install::Install;

use crate::Ctx;

#[derive(Subcommand)]
pub enum CloudCommand {
    /// Show save locations and what a sync would do, without changing anything
    Status { game_id: String },
    /// Synchronise cloud saves
    Sync(SyncArgs),
    /// Compare local and cloud copies file by file (read-only; cloud copies go to a separate folder)
    Diff {
        game_id: String,
        /// Only paths containing this text
        filter: Option<String>,
    },
}

#[derive(Args)]
pub struct SyncArgs {
    game_id: String,
    /// Resolve every conflict of this run in favour of one side (the other is backed up)
    #[arg(long, value_enum)]
    prefer: Option<Side>,
    /// Propagate file deletions (never applied when a folder looks empty or moved)
    #[arg(long)]
    allow_deletions: bool,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Side {
    Local,
    Remote,
}

pub async fn run(ctx: &Ctx, cmd: CloudCommand) -> Result<()> {
    match cmd {
        CloudCommand::Status { game_id } => {
            let opts = SyncOptions {
                dry_run: true,
                ..Default::default()
            };
            sync_game(ctx, &crate::games::get(ctx, &game_id)?, opts)
                .await
                .map(drop)
        }
        CloudCommand::Diff { game_id, filter } => diff(ctx, &game_id, filter.as_deref()).await,
        CloudCommand::Sync(args) => {
            let opts = SyncOptions {
                dry_run: false,
                allow_deletions: args.allow_deletions,
                prefer: args.prefer.map(|s| match s {
                    Side::Local => Prefer::Local,
                    Side::Remote => Prefer::Remote,
                }),
            };
            let clean = sync_game(ctx, &crate::games::get(ctx, &args.game_id)?, opts).await?;
            if !clean {
                bail!("the sync needs attention (see above)");
            }
            Ok(())
        }
    }
}

/// Returns whether every location ended without conflict, refusal or error.
pub async fn sync_game(ctx: &Ctx, install: &Install, opts: SyncOptions) -> Result<bool> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let Some(outcomes) =
        cloud::sync_game(&ctx.db, &ctx.dirs, &ctx.http, &tokens, install, opts).await?
    else {
        println!("GOG has no cloud saves for {}.", install.title);
        return Ok(true);
    };
    for o in &outcomes {
        match (&o.root, &o.result) {
            (Some(root), Ok(report)) => {
                println!("[{}] {}", o.name, root.display());
                print_report(report, opts.dry_run);
            }
            (_, Err(e)) => println!("[{}] {} — {e}", o.name, o.template),
            (None, Ok(_)) => unreachable!("a report needs a resolved root"),
        }
    }
    Ok(outcomes.iter().all(|o| o.is_clean()))
}

fn print_report(r: &SyncReport, dry_run: bool) {
    for w in &r.plan.warnings {
        println!("  warning: {w:?}");
    }
    if dry_run {
        let p = &r.plan;
        println!(
            "  plan: {} unchanged, {} to upload, {} to download, {} to compare, {} remote / {} local deletions, {} conflicts",
            p.count(Action::Keep),
            p.count(Action::Upload),
            p.count(Action::Download),
            p.count(Action::Compare),
            p.count(Action::DeleteRemote),
            p.count(Action::DeleteLocal),
            p.conflicts().count()
        );
        for (path, kind) in p.conflicts() {
            println!("  conflict: {path} ({kind:?})");
        }
        return;
    }
    let list = |label: &str, items: &[String]| {
        if !items.is_empty() {
            println!("  {label}: {}", items.join(", "));
        }
    };
    list("uploaded", &r.uploaded);
    list("downloaded", &r.downloaded);
    list("already identical", &r.adopted);
    list("deleted in cloud", &r.deleted_remote);
    list("deleted locally", &r.deleted_local);
    list(
        "deletions waiting for --allow-deletions",
        &r.pending_deletions,
    );
    list("skipped local entries", &r.skipped_local);
    for (path, kind) in &r.conflicts {
        println!("  CONFLICT {path} ({kind:?}): rerun with --prefer local or --prefer remote");
    }
    for (path, why) in &r.refused {
        println!("  refused {path}: {why}");
    }
    for (path, err) in &r.errors {
        println!("  ERROR {path}: {err}");
    }
    if let Some(dir) = &r.backup_dir {
        println!("  previous versions saved in {}", dir.display());
    }
    if r.is_clean() && r.plan.is_noop() {
        println!("  up to date");
    }
}

async fn diff(ctx: &Ctx, game_id: &str, filter: Option<&str>) -> Result<()> {
    let install = crate::games::get(ctx, game_id)?;
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let Some(game_cloud) = cloud::open(&ctx.http, &tokens, &install).await? else {
        println!("GOG has no cloud saves for {}.", install.title);
        return Ok(());
    };
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    for (location, root) in &game_cloud.locations {
        let Ok(root) = root else { continue };
        let copies = ctx
            .dirs
            .data
            .join("diagnostics")
            .join(game_id)
            .join(&stamp)
            .join(&location.name);
        println!("[{}] {}", location.name, root.display());
        let files =
            cloud::inspect::compare(&game_cloud.transport, &location.name, root, &copies, filter)
                .await?;
        for f in &files {
            println!(
                "  {}{}",
                f.path,
                if f.identical() { "  (identical)" } else { "" }
            );
            match &f.local {
                Some(l) => println!(
                    "    local : {} bytes, modified {}, sha256 {}",
                    l.size,
                    l.modified
                        .map(|m| crate::local_time(m.timestamp()))
                        .unwrap_or_default(),
                    &l.sha256[..16]
                ),
                None => println!("    local : absent"),
            }
            match &f.remote {
                Some(r) => {
                    println!(
                        "    cloud : {} bytes, modified {}, sha256 {}, listed hash {}",
                        r.size,
                        r.last_modified.as_deref().unwrap_or("?"),
                        &r.sha256[..16],
                        r.listed_hash
                    );
                    if r.gzip_magic {
                        println!(
                            "    WARNING: the cloud copy still looks gzip-compressed (transport decoding issue)"
                        );
                    }
                    println!("    cloud copy saved to {}", r.copy.display());
                }
                None => println!("    cloud : absent"),
            }
        }
    }
    Ok(())
}
