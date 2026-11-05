//! `slatty-measure`: measures Slatty's interface at 10,000 games, on a fake profile it makes
//! itself, and prints the figures with their spread. It knows nothing of the interface's framework:
//! it builds `slatty-gui` with its `probe` feature and reads the log the probe writes
//! (`docs/testing.md`), so that it measures any interface that writes the same.
//!
//! It runs on Linux only: it reads `/proc` and `/sys`, and starts sway. It builds elsewhere, so
//! that the workspace does, and says so.

#[cfg(target_os = "linux")]
mod log;
#[cfg(target_os = "linux")]
mod measure;
#[cfg(target_os = "linux")]
mod profile;
#[cfg(target_os = "linux")]
mod session;
#[cfg(target_os = "linux")]
mod stats;
#[cfg(target_os = "linux")]
mod system;

#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use anyhow::Result;
#[cfg(target_os = "linux")]
use clap::{Parser, Subcommand};

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("slatty-measure runs on Linux only");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
#[derive(Parser)]
#[command(about = "Measures Slatty's interface at 10,000 games, on a fake profile")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[cfg(target_os = "linux")]
#[derive(Subcommand)]
enum Command {
    /// Makes the fake profile in FOLDER, which must be missing or empty.
    Profile { folder: PathBuf },
    /// Start, rest, memory and video memory, in a headless compositor: nothing shows. Makes the
    /// fake profile in FOLDER first when it is missing or empty. About 45 minutes.
    Headless {
        folder: PathBuf,
        /// How many times each measurement runs.
        #[arg(long, default_value_t = 10)]
        runs: usize,
    },
    /// Start, scrolling and the graphics card's power on this session's display: a window opens
    /// and closes by itself, again and again, for about five minutes; leave the computer alone
    /// meanwhile, its screen kept on. Makes the fake profile in FOLDER first when it is missing or
    /// empty.
    Desktop {
        folder: PathBuf,
        /// How many times each measurement runs.
        #[arg(long, default_value_t = 10)]
        runs: usize,
    },
}

#[cfg(target_os = "linux")]
fn main() -> Result<()> {
    let report = match Cli::parse().command {
        Command::Profile { folder } => {
            profile::Profile::open_or_make(&folder)?;
            return Ok(());
        }
        Command::Headless { folder, runs } => {
            let setup = measure::Setup::new(&folder, runs)?;
            (measure::headless(&setup)?, setup)
        }
        Command::Desktop { folder, runs } => {
            let setup = measure::Setup::new(&folder, runs)?;
            (measure::desktop(&setup)?, setup)
        }
    };
    let (report, setup) = report;
    std::fs::write(setup.folder.join("report.md"), &report)?;
    print!("{report}");
    Ok(())
}
