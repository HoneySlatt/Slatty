use std::time::Instant;

use anyhow::Result;
use clap::Args;
use slatty_core::account::Account;
use slatty_core::runner;
use slatty_core::session::{self, SessionHandle, SupervisorEvent};

use crate::Ctx;

#[derive(Args)]
pub struct LaunchArgs {
    game_id: String,
}

pub async fn run(ctx: &Ctx, args: LaunchArgs) -> Result<()> {
    recover_unfinished(ctx)?;
    let install = crate::games::get(ctx, &args.game_id)?;
    let spec = runner::launch_spec(&install)?;
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
    Ok(())
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
