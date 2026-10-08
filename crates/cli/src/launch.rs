use std::time::Instant;

use anyhow::Result;
use clap::Args;
use slatty_core::account::Account;
use slatty_core::achievements::{self, Achievement};
use slatty_core::auth::Tokens;
use slatty_core::cloud::sync::SyncOptions;
use slatty_core::comet::Comet;
use slatty_core::doctor::find_in_path;
use slatty_core::install::Install;
use slatty_core::runner;
use slatty_core::session::{self, SessionHandle, SupervisorEvent};

use crate::Ctx;

#[derive(Args)]
pub struct LaunchArgs {
    game_id: String,
    /// Do not synchronise cloud saves before and after playing
    #[arg(long)]
    no_cloud: bool,
    /// Do not start Comet (no achievements for this session)
    #[arg(long)]
    no_comet: bool,
}

pub async fn run(ctx: &Ctx, args: LaunchArgs) -> Result<()> {
    recover_unfinished(ctx)?;
    let install = crate::games::get(ctx, &args.game_id)?;
    let spec = runner::launch_spec(&install)?;
    if !args.no_cloud {
        println!("Checking cloud saves…");
        match crate::cloud::sync_game(ctx, &install, SyncOptions::default()).await {
            Ok(true) => {}
            Ok(false) => anyhow::bail!(
                "cloud saves need attention; resolve with `slatty cloud sync {}` or launch with --no-cloud",
                install.game_id
            ),
            Err(e) => {
                println!("Cloud saves not checked ({e}); local saves are kept and will sync later.")
            }
        }
    }
    let comet = if args.no_comet {
        None
    } else {
        start_comet(ctx, &install).await
    };
    let user = Account::active(&ctx.db)?.map(|a| a.user_id);
    let log = ctx
        .dirs
        .logs()
        .join(format!("game-{}.log", install.game_id));
    let started = Instant::now();
    let mut handle = SessionHandle::start(&std::env::current_exe()?, &spec, &log).await?;
    match handle.next_event().await? {
        Some(SupervisorEvent::Started { pid }) => println!(
            "Started {} (pid {pid}); output in {}",
            install.title,
            log.display()
        ),
        Some(SupervisorEvent::Failed { message }) => anyhow::bail!("launch failed: {message}"),
        other => anyhow::bail!("unexpected supervisor reply: {other:?}"),
    }
    let id = session::record_start(&ctx.db, &install.game_id, user.as_deref())?;
    let outcome = loop {
        tokio::select! {
            event = handle.next_event() => match event? {
                Some(SupervisorEvent::MainExited { code }) => {
                    println!("Launch process exited ({code:?}); waiting for remaining game processes…");
                }
                Some(SupervisorEvent::Ended { main_code }) => {
                    break session::SessionOutcome { main_code, clean: true };
                }
                Some(SupervisorEvent::Failed { message }) => anyhow::bail!("supervisor failed: {message}"),
                Some(SupervisorEvent::Started { .. }) => {}
                None => break session::SessionOutcome { main_code: None, clean: false },
            },
            _ = tokio::signal::ctrl_c() => {
                println!("Stopping the game (Ctrl+C again to force)…");
                handle.request_stop();
            }
        }
    };
    session::record_end(&ctx.db, id, &outcome)?;
    if let Some((comet, before)) = comet {
        comet.stop().await?;
        report_unlocks(ctx, &install, before).await;
    }
    println!(
        "Session ended after {}s (exit code {:?}{}).",
        started.elapsed().as_secs(),
        outcome.main_code,
        if outcome.clean {
            ""
        } else {
            ", supervisor lost"
        }
    );
    if !args.no_cloud {
        if outcome.clean {
            println!("Uploading cloud saves…");
            match crate::cloud::sync_game(ctx, &install, SyncOptions::default()).await {
                Ok(true) => {}
                Ok(false) => println!(
                    "Cloud saves need attention; see `slatty cloud status {}`.",
                    install.game_id
                ),
                Err(e) => println!(
                    "Cloud sync failed ({e}); local saves are kept. Retry with `slatty cloud sync {}`.",
                    install.game_id
                ),
            }
        } else {
            println!("The end of the session is uncertain, so cloud saves were not synced.");
        }
    }
    Ok(())
}

struct Snapshot(Option<Vec<Achievement>>);

async fn start_comet(ctx: &Ctx, install: &Install) -> Option<(Comet, Snapshot)> {
    let started = async {
        let bin = find_in_path("comet").ok_or_else(|| anyhow::anyhow!("`comet` is not in PATH"))?;
        let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
        let tokens = account.tokens(&ctx.http).await?.clone();
        let comet = Comet::start(&bin, &tokens, &account.info.username, &ctx.dirs).await?;
        anyhow::Ok((comet, tokens))
    };
    match started.await {
        Ok((comet, tokens)) => {
            println!("Comet is running; achievements unlocked in game are sent to GOG.");
            let before = achievements_now(ctx, &tokens, install).await;
            Some((comet, Snapshot(before)))
        }
        Err(e) => {
            println!("Achievements unavailable for this session: {e}");
            None
        }
    }
}

async fn achievements_now(
    ctx: &Ctx,
    tokens: &Tokens,
    install: &Install,
) -> Option<Vec<Achievement>> {
    let (client_id, token) = achievements::game_token(&ctx.http, tokens, install)
        .await
        .ok()?;
    achievements::fetch(&ctx.http, &tokens.user_id, &client_id, &token)
        .await
        .ok()
}

async fn report_unlocks(ctx: &Ctx, install: &Install, before: Snapshot) {
    let Some(before) = before.0 else { return };
    let after = async {
        let mut account = Account::load(&ctx.db, &ctx.dirs).await.ok()?;
        let tokens = account.tokens(&ctx.http).await.ok()?.clone();
        achievements_now(ctx, &tokens, install).await
    };
    match after.await {
        Some(after) => {
            let new = achievements::newly_unlocked(&before, &after);
            if new.is_empty() {
                println!("No new achievement recorded on GOG for this session.");
            }
            for a in new {
                println!(
                    "Achievement unlocked on GOG: {} — {}",
                    a.name, a.description
                );
            }
        }
        None => println!("Could not read achievements back from GOG."),
    }
}

fn recover_unfinished(ctx: &Ctx) -> Result<()> {
    for s in session::unfinished(&ctx.db)? {
        println!(
            "Note: the session of {} started {} never recorded its end (launcher interrupted).",
            s.game_id,
            crate::local_time(s.started_at)
        );
        session::mark_interrupted(&ctx.db, s.id)?;
    }
    Ok(())
}
