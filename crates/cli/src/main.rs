mod auth;
mod library;

use anyhow::Result;
use clap::{Parser, Subcommand};
use slatty_core::db::Db;
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
    /// GOG account
    #[command(subcommand)]
    Auth(auth::AuthCommand),
    /// Game library
    #[command(subcommand)]
    Library(library::LibraryCommand),
}

pub struct Ctx {
    pub dirs: Dirs,
    pub db: Db,
    pub http: reqwest::Client,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("SLATTY_LOG"))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let dirs = Dirs::from_system()?;
    if let Command::Doctor = cli.command {
        for c in slatty_core::doctor::run(&dirs) {
            println!("[{}] {:<16} {}", if c.ok { " ok " } else { "FAIL" }, c.name, c.detail);
        }
        return Ok(());
    }
    let ctx = Ctx { db: Db::open(&dirs.db_file())?, http: slatty_core::http::client()?, dirs };
    match cli.command {
        Command::Doctor => unreachable!(),
        Command::Auth(cmd) => auth::run(&ctx, cmd).await,
        Command::Library(cmd) => library::run(&ctx, cmd).await,
    }
}

pub fn local_time(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(ts, 0)
        .single()
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string())
}
