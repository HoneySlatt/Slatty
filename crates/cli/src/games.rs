use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Args, ValueEnum};
use slatty_core::install::{self, Install};
use slatty_core::runner::{self, Runner};

use crate::Ctx;

#[derive(Clone, Copy, ValueEnum)]
pub enum RunnerKind {
    Native,
    Umu,
    Wine,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Directory of an installed game
    dir: Option<PathBuf>,
    /// Import a game installed by Heroic (read-only)
    #[arg(long, value_name = "GAME_ID", conflicts_with = "dir")]
    from_heroic: Option<String>,
    #[arg(long)]
    game_id: Option<String>,
    #[arg(long, value_enum)]
    runner: Option<RunnerKind>,
    /// Proton directory (contains the `proton` script)
    #[arg(long)]
    proton: Option<PathBuf>,
    /// Wine binary
    #[arg(long)]
    wine: Option<PathBuf>,
    /// Wine prefix dedicated to this game
    #[arg(long)]
    prefix: Option<PathBuf>,
}

pub fn import(ctx: &Ctx, args: ImportArgs) -> Result<()> {
    let install = match (args.from_heroic, args.dir) {
        (Some(id), _) => {
            let heroic = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
                .unwrap_or_default()
                .join("heroic");
            install::from_heroic(&heroic, &id)?
        }
        (None, Some(dir)) => {
            let runner = match (args.runner, args.proton, args.wine, args.prefix) {
                (Some(RunnerKind::Native), None, None, None) => Runner::Native,
                (Some(RunnerKind::Umu), Some(proton), None, Some(prefix)) => Runner::Umu { proton, prefix },
                (Some(RunnerKind::Wine), None, Some(wine), Some(prefix)) => Runner::Wine { wine, prefix },
                _ => bail!("use --runner native, --runner umu --proton DIR --prefix DIR, or --runner wine --wine BIN --prefix DIR"),
            };
            install::from_dir(&dir, args.game_id.as_deref(), runner)?
        }
        (None, None) => bail!("give a directory or --from-heroic GAME_ID"),
    };
    install.save(&ctx.db)?;
    println!("Imported {} ({}) from {}", install.title, install.game_id, install.path.display());
    print_runner(&install);
    Ok(())
}

pub fn list(ctx: &Ctx) -> Result<()> {
    for i in Install::list(&ctx.db)? {
        println!("{:>12}  {:<40} {:?}  {}", i.game_id, i.title, i.platform, i.path.display());
    }
    Ok(())
}

pub fn print_spec(ctx: &Ctx, game_id: &str) -> Result<()> {
    let install = get(ctx, game_id)?;
    let spec = runner::launch_spec(&install)?;
    println!("program: {}", spec.program.display());
    println!("args:    {:?}", spec.args);
    println!("cwd:     {}", spec.cwd.display());
    for (k, v) in &spec.env {
        println!("env:     {k}={v}");
    }
    Ok(())
}

pub fn get(ctx: &Ctx, game_id: &str) -> Result<Install> {
    Install::get(&ctx.db, game_id)?.ok_or_else(|| anyhow::anyhow!("{game_id} is not imported; see `slatty import`"))
}

fn print_runner(install: &Install) {
    match &install.runner {
        Runner::Native => println!("Runner: native"),
        Runner::Umu { proton, prefix } => {
            println!("Runner: umu + {}\nPrefix: {}", proton.display(), prefix.display())
        }
        Runner::Wine { wine, prefix } => println!("Runner: {}\nPrefix: {}", wine.display(), prefix.display()),
    }
}
