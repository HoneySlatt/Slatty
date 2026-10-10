use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Subcommand, ValueEnum};
use slatty_core::protons::{self, Source, Stage};
use slatty_core::settings;
use tokio_util::sync::CancellationToken;

use crate::Ctx;
use crate::install::size;

#[derive(Subcommand)]
pub enum ProtonCommand {
    /// List the Proton builds on this computer, those slatty downloaded first
    List,
    /// Allow or forbid listing and downloading Proton builds from GitHub (off until turned on)
    Downloads {
        #[arg(value_enum)]
        state: State,
    },
    /// Show the newest build of each project on GitHub
    Available,
    /// Download the newest build of a project, checked against its published SHA-512 sum
    Install {
        #[arg(value_enum)]
        source: SourceArg,
    },
    /// Delete a build slatty downloaded (refused while a game or the default uses it)
    Remove { name: String },
    /// Update, at most once a day at start, the builds games follow as <project>-latest (off
    /// until turned on)
    Updates {
        #[arg(value_enum)]
        state: State,
    },
    /// Download the newest build of each project games follow as <project>-latest, now
    Update,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum State {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum SourceArg {
    GeProton,
    ProtonCachyos,
    UmuProton,
}

impl From<SourceArg> for Source {
    fn from(s: SourceArg) -> Source {
        match s {
            SourceArg::GeProton => Source::GeProton,
            SourceArg::ProtonCachyos => Source::ProtonCachyOs,
            SourceArg::UmuProton => Source::UmuProton,
        }
    }
}

pub async fn run(ctx: &Ctx, cmd: ProtonCommand) -> Result<()> {
    protons::link_newest(&ctx.dirs)?;
    match cmd {
        ProtonCommand::List => {
            for build in settings::proton_candidates(&ctx.dirs) {
                println!("{}", build.display());
            }
        }
        ProtonCommand::Downloads { state } => {
            let on = matches!(state, State::On);
            settings::set_proton_downloads(&ctx.db, on)?;
            println!(
                "Proton downloads from GitHub are {}",
                if on { "on" } else { "off" }
            );
        }
        ProtonCommand::Available => {
            protons::check_allowed(&ctx.db)?;
            for source in Source::ALL {
                let release = protons::latest(&ctx.http, protons::API, source).await?;
                let state = if release.path(&ctx.dirs).join("proton").is_file() {
                    "installed"
                } else {
                    "not installed"
                };
                println!(
                    "{:<15} {:<40} {:>9}  {state}",
                    source.name(),
                    release.name,
                    size(release.size)
                );
            }
        }
        ProtonCommand::Install { source } => {
            protons::check_allowed(&ctx.db)?;
            let release = protons::latest(&ctx.http, protons::API, source.into()).await?;
            println!(
                "Installing {} ({}). Ctrl+C stops; run the same command to resume.",
                release.name,
                size(release.size)
            );
            let progress = printer();
            let path =
                protons::install(&ctx.http, &ctx.dirs, &release, progress, &stop_on_ctrl_c())
                    .await?;
            println!("Installed in {}", path.display());
        }
        ProtonCommand::Remove { name } => {
            protons::remove(&ctx.db, &ctx.dirs, &protons::dir(&ctx.dirs).join(&name))?;
            println!("Deleted {name}");
        }
        ProtonCommand::Updates { state } => {
            let on = matches!(state, State::On);
            settings::set_proton_updates(&ctx.db, on)?;
            println!(
                "Proton builds followed as <project>-latest are {}",
                if on {
                    "updated at most once a day, at start"
                } else {
                    "updated only when asked"
                }
            );
        }
        ProtonCommand::Update => {
            let followed = protons::followed(&ctx.db, &ctx.dirs)?;
            if followed.is_empty() {
                println!(
                    "No game follows a <project>-latest build; set one in its settings, or with \
                     slatty install --proton {}",
                    protons::latest_link(&ctx.dirs, Source::GeProton).display()
                );
                return Ok(());
            }
            let print = printer();
            let updated = protons::update(
                &ctx.db,
                &ctx.http,
                &ctx.dirs,
                protons::API,
                move |_, stage| print(stage),
                &stop_on_ctrl_c(),
            )
            .await?;
            if updated.is_empty() {
                println!("Up to date");
            }
            for release in updated {
                println!("Updated to {}", release.name);
            }
        }
    }
    Ok(())
}

/// A token Ctrl+C cancels.
fn stop_on_ctrl_c() -> CancellationToken {
    let cancel = CancellationToken::new();
    let on_ctrl_c = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_ctrl_c.cancel();
        }
    });
    cancel
}

/// Prints where a download stands, at most once a second.
fn printer() -> impl Fn(Stage) + Send + Sync + 'static {
    let last = Mutex::new(None::<Instant>);
    move |stage: Stage| {
        let (what, done, total) = match stage {
            Stage::Downloading { done, total } => ("Downloading", done, total),
            Stage::Verifying => {
                println!("  Checking the SHA-512 sum");
                return;
            }
            Stage::Unpacking { done, total } => ("Unpacking", done, total),
        };
        let mut last = last.lock().unwrap();
        if last.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) || done == total {
            *last = Some(Instant::now());
            let pct = (done * 100).checked_div(total).unwrap_or(100);
            println!("  {what} {pct:>3}%");
        }
    }
}
