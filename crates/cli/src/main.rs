use anyhow::Result;
use clap::{Parser, Subcommand};
use slatty_core::paths::Dirs;

#[derive(Parser)]
#[command(name = "slatty", version, about = "SlattyLauncher command line")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check the local environment
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("SLATTY_LOG"))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let dirs = Dirs::from_system()?;
    match cli.command {
        Command::Doctor => {
            let checks = slatty_core::doctor::run(&dirs);
            for c in &checks {
                println!("[{}] {:<16} {}", if c.ok { " ok " } else { "FAIL" }, c.name, c.detail);
            }
        }
    }
    Ok(())
}
