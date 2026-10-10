mod achievements;
mod auth;
mod cloud;
mod games;
mod install;
mod launch;
mod library;
mod maintenance;

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
    /// Download, verify and register a Windows build (Galaxy depots)
    Install(install::InstallArgs),
    /// Register a game that is already installed
    Import(games::ImportArgs),
    /// List installed games and interrupted installs
    Installs,
    /// Show the command that would launch a game
    LaunchSpec { game_id: String },
    /// Remove a game from slatty's records; files, prefix and saves are left untouched
    Forget { game_id: String },
    /// Delete a game installed by slatty (saves are kept unless asked otherwise)
    Uninstall(maintenance::UninstallArgs),
    /// Check an installed game's files, optionally repairing them
    Verify(maintenance::VerifyArgs),
    /// Check for and apply game updates
    Update(maintenance::UpdateArgs),
    /// Show or change the language and DLC of an installed game
    Content(maintenance::ContentArgs),
    /// Run GOG's post-install setup (installer scripts, redistributables)
    Setup(maintenance::SetupArgs),
    /// Launch a game and follow its session until every process has exited
    Launch(launch::LaunchArgs),
    /// Cloud saves
    #[command(subcommand)]
    Cloud(cloud::CloudCommand),
    /// Achievements as recorded on GOG; can also unlock or clear them manually
    Achievements(achievements::AchievementsArgs),
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
    dirs.keep_private()?;
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
        Command::Install(args) => install::run(&ctx, args).await,
        Command::Installs => games::list(&ctx).and_then(|()| install::list_jobs(&ctx)),
        Command::LaunchSpec { game_id } => games::print_spec(&ctx, &game_id),
        Command::Forget { game_id } => games::forget(&ctx, &game_id),
        Command::Uninstall(args) => maintenance::uninstall(&ctx, args),
        Command::Verify(args) => maintenance::verify(&ctx, args).await,
        Command::Update(args) => maintenance::update(&ctx, args).await,
        Command::Content(args) => maintenance::content(&ctx, args).await,
        Command::Setup(args) => maintenance::setup(&ctx, args).await,
        Command::Launch(args) => launch::run(&ctx, args).await,
        Command::Cloud(cmd) => cloud::run(&ctx, cmd).await,
        Command::Achievements(args) => achievements::run(&ctx, args).await,
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
