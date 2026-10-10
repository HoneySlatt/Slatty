use anyhow::Result;
use clap::Args;
use slatty_core::play::{self, CloudSummary, PlayEvent, PlayRequest};

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
    for s in play::recover_unfinished(&ctx.db)? {
        println!(
            "Note: the session of {} started {} never recorded its end (launcher interrupted).",
            s.game_id,
            crate::local_time(s.started_at)
        );
    }
    let (stop_tx, stop_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            if stop_tx.send(()).is_err() {
                break;
            }
        }
    });
    let game_id = args.game_id.clone();
    let req = PlayRequest {
        game_id: args.game_id,
        cloud: !args.no_cloud,
        comet: !args.no_comet,
        supervisor: slatty_core::session::supervisor_exe(),
    };
    play::play(
        &ctx.db,
        &ctx.dirs,
        &ctx.http,
        req,
        |e| print_event(&game_id, e),
        stop_rx,
    )
    .await?;
    Ok(())
}

fn print_event(game_id: &str, event: PlayEvent) {
    match event {
        PlayEvent::PreparingPrefix => println!("First launch: creating the Wine prefix…"),
        PlayEvent::SetupStep(step) => println!("Setup: {step}…"),
        PlayEvent::SetupWarning(w) => println!("Setup warning: {w}"),
        PlayEvent::SetupSkipped(why) => {
            println!("Setup not run ({why}); it will be retried at the next launch.")
        }
        PlayEvent::CloudChecked(s) => print_cloud("Cloud saves checked", &s),
        PlayEvent::CloudSkipped(why) => {
            println!("Cloud saves not checked ({why}); local saves are kept.")
        }
        PlayEvent::Blocked(s) => {
            print_cloud("Cloud saves need attention", &s);
            println!(
                "Not launching. Resolve with `slatty cloud sync {game_id} --prefer local|remote` or use --no-cloud."
            );
        }
        PlayEvent::CometReady => {
            println!("Comet is running; achievements unlocked in game are sent to GOG.")
        }
        PlayEvent::CometUnavailable(why) => {
            println!("Achievements unavailable for this session: {why}")
        }
        PlayEvent::Isolated => {
            println!("Isolated from your files: the game sees its own folder and its own home.")
        }
        PlayEvent::Started { pid } => println!("Game started (pid {pid})."),
        PlayEvent::LauncherExited { code } => {
            println!("Launch process exited ({code:?}); waiting for remaining game processes…")
        }
        PlayEvent::StopRequested => println!("Stopping the game (Ctrl+C again to force)…"),
        PlayEvent::Ended {
            seconds,
            code,
            clean,
        } => println!(
            "Session ended after {seconds}s (exit code {code:?}{}).",
            if clean { "" } else { ", supervisor lost" }
        ),
        PlayEvent::CloudUploaded(s) => print_cloud("Cloud saves after playing", &s),
        PlayEvent::CloudUploadSkipped(why) => {
            println!(
                "Cloud saves not uploaded ({why}); local saves are kept. Retry with `slatty cloud sync {game_id}`."
            )
        }
        PlayEvent::PlaytimeReported(m) => println!("Play time sent to GOG: {m} min."),
        PlayEvent::PlaytimeNotReported(why) => {
            println!("Play time not sent to GOG ({why}); it will be sent after the next session.")
        }
        PlayEvent::Unlocked(names) => {
            for n in names {
                println!("Achievement unlocked on GOG: {n}");
            }
        }
        PlayEvent::NoNewAchievement => {
            println!("No new achievement recorded on GOG for this session.")
        }
        PlayEvent::AchievementsUnknown => println!("Could not read achievements back from GOG."),
    }
}

fn print_cloud(title: &str, s: &CloudSummary) {
    println!(
        "{title}: {} uploaded, {} downloaded.",
        s.uploaded, s.downloaded
    );
    for c in &s.conflicts {
        println!("  conflict: {c}");
    }
    for p in &s.problems {
        println!("  problem: {p}");
    }
    for d in &s.pending_deletions {
        println!("  deletion not applied: {d}");
    }
    if let Some(dir) = &s.backup_dir {
        println!("  previous versions saved in {}", dir.display());
    }
}
