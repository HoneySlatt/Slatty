use anyhow::{Result, bail};
use clap::{Args, Subcommand, ValueEnum};
use slatty_core::account::Account;
use slatty_core::cloud::{
    self,
    plan::Action,
    sync::{Prefer, SyncOptions, SyncReport},
};
use slatty_core::install::Install;
use std::io::Write;

use crate::{Ctx, shown};

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
    let (store, outcomes) = if slatty_core::store::epic_app(&install.game_id).is_some() {
        let synced =
            slatty_core::epic::cloud::sync_game(&ctx.db, &ctx.dirs, &ctx.http, install, opts);
        ("Epic", synced.await?)
    } else {
        let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
        let tokens = account.tokens(&ctx.http).await?.clone();
        let synced = cloud::sync_game(&ctx.db, &ctx.dirs, &ctx.http, &tokens, install, opts);
        ("GOG", synced.await?)
    };
    let Some(outcomes) = outcomes else {
        println!("{store} has no cloud saves for {}.", shown(&install.title));
        return Ok(true);
    };
    for o in &outcomes {
        match (&o.root, &o.result) {
            (Some(root), Ok(report)) => {
                println!("[{}] {}", shown(&o.name), shown(root.display()));
                print_report(report, opts.dry_run);
            }
            (_, Err(e)) => println!("[{}] {} — {}", shown(&o.name), shown(&o.template), shown(e)),
            (None, Ok(_)) => unreachable!("a report needs a resolved root"),
        }
    }
    Ok(outcomes.iter().all(|o| o.is_clean()))
}

fn print_report(r: &SyncReport, dry_run: bool) {
    let _ = write_report(&mut std::io::stdout().lock(), r, dry_run);
}

/// The report of a sync. Save names come from the game's folder and the cloud: they are shown
/// escaped.
fn write_report(out: &mut impl Write, r: &SyncReport, dry_run: bool) -> std::io::Result<()> {
    for w in &r.plan.warnings {
        writeln!(out, "  warning: {}", shown(format!("{w:?}")))?;
    }
    if dry_run {
        let p = &r.plan;
        writeln!(
            out,
            "  plan: {} unchanged, {} to upload, {} to download, {} to compare, {} remote / {} local deletions, {} conflicts",
            p.count(Action::Keep),
            p.count(Action::Upload),
            p.count(Action::Download),
            p.count(Action::Compare),
            p.count(Action::DeleteRemote),
            p.count(Action::DeleteLocal),
            p.conflicts().count()
        )?;
        for (path, kind) in p.conflicts() {
            writeln!(out, "  conflict: {} ({kind:?})", shown(path))?;
        }
        return Ok(());
    }
    let mut list = |label: &str, items: &[String]| {
        if items.is_empty() {
            return Ok(());
        }
        let items: Vec<String> = items.iter().map(shown).collect();
        writeln!(out, "  {label}: {}", items.join(", "))
    };
    list("uploaded", &r.uploaded)?;
    list("downloaded", &r.downloaded)?;
    list("already identical", &r.adopted)?;
    list("deleted in cloud", &r.deleted_remote)?;
    list("deleted locally", &r.deleted_local)?;
    list(
        "deletions waiting for --allow-deletions",
        &r.pending_deletions,
    )?;
    list("skipped local entries", &r.skipped_local)?;
    list(
        "cloud files left out (unsafe name, or two names differing only by case)",
        &r.skipped_remote,
    )?;
    for (path, kind) in &r.conflicts {
        writeln!(
            out,
            "  CONFLICT {} ({kind:?}): rerun with --prefer local or --prefer remote",
            shown(path)
        )?;
    }
    for (path, why) in &r.refused {
        writeln!(out, "  refused {}: {}", shown(path), shown(why))?;
    }
    for (path, err) in &r.errors {
        writeln!(out, "  ERROR {}: {}", shown(path), shown(err))?;
    }
    if let Some(dir) = &r.backup_dir {
        writeln!(out, "  previous versions saved in {}", shown(dir.display()))?;
    }
    if r.is_clean() && r.plan.is_noop() {
        writeln!(out, "  up to date")?;
    }
    Ok(())
}

async fn diff(ctx: &Ctx, game_id: &str, filter: Option<&str>) -> Result<()> {
    let install = crate::games::get(ctx, game_id)?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let copies = |location: &str| {
        ctx.dirs
            .data
            .join("diagnostics")
            .join(game_id)
            .join(&stamp)
            .join(location)
    };
    if slatty_core::store::epic_app(game_id).is_some() {
        use slatty_core::epic::cloud::{LOCATION, open};
        let Some(saves) = open(&ctx.db, &ctx.dirs, &ctx.http, &install).await? else {
            println!("Epic has no cloud saves for {}.", shown(&install.title));
            return Ok(());
        };
        let root = saves.root?;
        println!("[{LOCATION}] {}", shown(root.display()));
        let copies = copies(LOCATION);
        let files = cloud::inspect::compare(&saves.cloud, LOCATION, &root, &copies, filter).await?;
        print_comparisons(&files);
        return Ok(());
    }
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let Some(game_cloud) = cloud::open(&ctx.http, &tokens, &install).await? else {
        println!("GOG has no cloud saves for {}.", shown(&install.title));
        return Ok(());
    };
    for (location, root) in &game_cloud.locations {
        let Ok(root) = root else { continue };
        let copies = copies(&location.name);
        println!("[{}] {}", shown(&location.name), shown(root.display()));
        let files =
            cloud::inspect::compare(&game_cloud.transport, &location.name, root, &copies, filter)
                .await?;
        print_comparisons(&files);
    }
    Ok(())
}

fn print_comparisons(files: &[cloud::inspect::FileComparison]) {
    for f in files {
        println!(
            "  {}{}",
            shown(&f.path),
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
                    shown(r.last_modified.as_deref().unwrap_or("?")),
                    &r.sha256[..16],
                    shown(&r.listed_hash)
                );
                if r.gzip_magic {
                    println!(
                        "    WARNING: the cloud copy still looks gzip-compressed (transport decoding issue)"
                    );
                }
                println!("    cloud copy saved to {}", shown(r.copy.display()));
            }
            None => println!("    cloud : absent"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game names the files of its save folder: escape sequences there, printed as they are,
    /// would hide the conflicts below them, rewrite earlier lines, set the terminal's title or
    /// write the clipboard.
    #[test]
    fn save_names_cannot_drive_the_terminal() {
        let report = SyncReport {
            uploaded: vec![
                "[FAKE] a\u{1b}[8m.sav".into(),
                "b\u{1b}]52;c;Zm9v\u{7}".into(),
            ],
            refused: vec![("c\u{202e}vas.exe".into(), "\u{1b}]0;title\u{7}".into())],
            ..Default::default()
        };
        let mut out = Vec::new();
        write_report(&mut out, &report, false).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains(['\u{1b}', '\u{7}', '\u{202e}']), "{text:?}");
        assert!(text.contains("[FAKE] a\\u{1b}[8m.sav"), "{text}");
    }
}
