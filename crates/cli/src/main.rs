mod achievements;
mod auth;
mod cloud;
mod games;
mod launch;
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
    /// Register a game that is already installed
    Import(games::ImportArgs),
    /// List imported games
    Installs,
    /// Show the command that would launch a game
    LaunchSpec { game_id: String },
    /// Launch a game and follow its session until every process has exited
    Launch(launch::LaunchArgs),
    /// Cloud saves
    #[command(subcommand)]
    Cloud(cloud::CloudCommand),
    /// Achievements of an imported game, as recorded on GOG
    Achievements { game_id: String },
}

pub struct Ctx {
    pub dirs: Dirs,
    pub db: Db,
    pub http: reqwest::Client,
}

fn main() -> Result<()> {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == slatty_core::session::SUPERVISE_ARG)
    {
        std::process::exit(slatty_core::session::supervise_main());
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("SLATTY_LOG"))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let dirs = Dirs::from_system()?;
    if let Command::Doctor = cli.command {
        for c in slatty_core::doctor::run(&dirs) {
            println!(
                "[{}] {:<16} {}",
                if c.ok { " ok " } else { "FAIL" },
                c.name,
                c.detail
            );
        }
        return Ok(());
    }
    let ctx = Ctx {
        db: Db::open(&dirs.db_file())?,
        http: slatty_core::http::client()?,
        dirs,
    };
    match cli.command {
        Command::Doctor => unreachable!(),
        Command::Auth(cmd) => auth::run(&ctx, cmd).await,
        Command::Library(cmd) => library::run(&ctx, cmd).await,
        Command::Import(args) => games::import(&ctx, args),
        Command::Installs => games::list(&ctx),
        Command::LaunchSpec { game_id } => games::print_spec(&ctx, &game_id),
        Command::Launch(args) => launch::run(&ctx, args).await,
        Command::Cloud(cmd) => cloud::run(&ctx, cmd).await,
        Command::Achievements { game_id } => achievements::list(&ctx, &game_id).await,
    }
}

pub fn local_time(ts: i64) -> String {
    use chrono::TimeZone;
    chrono::Utc
        .timestamp_opt(ts, 0)
        .single()
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
}
