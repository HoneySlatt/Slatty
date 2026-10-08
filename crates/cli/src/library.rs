use anyhow::Result;
use clap::Subcommand;
use slatty_core::account::Account;
use slatty_core::library::{self, MetadataSource};

use crate::Ctx;

#[derive(Subcommand)]
pub enum LibraryCommand {
    /// Download the library and refresh the local cache
    Sync,
    /// List games from the local cache (works offline)
    List {
        /// Case-insensitive title filter
        search: Option<String>,
    },
}

pub async fn run(ctx: &Ctx, cmd: LibraryCommand) -> Result<()> {
    match cmd {
        LibraryCommand::Sync => {
            let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
            let tokens = account.tokens(&ctx.http).await?.clone();
            let games = library::fetch(&ctx.http, &tokens).await?;
            let missing = games.iter().filter(|g| g.metadata != MetadataSource::Gamesdb).count();
            let cache = library::save_cache(&ctx.dirs, &account.info.user_id, games)?;
            println!("{} games cached ({missing} with partial metadata).", cache.games.len());
        }
        LibraryCommand::List { search } => {
            let info = Account::active(&ctx.db)?.ok_or(slatty_core::Error::NotLoggedIn)?;
            let Some(cache) = library::load_cache(&ctx.dirs, &info.user_id)? else {
                println!("No cached library yet. Run `slatty library sync`.");
                return Ok(());
            };
            let needle = search.map(|s| s.to_lowercase());
            for g in cache.games.iter().filter(|g| needle.as_ref().is_none_or(|n| g.title.to_lowercase().contains(n))) {
                println!("{:>12}  {:<50} {}", g.id, g.title, g.os.join(","));
            }
            println!("(cache from {})", crate::local_time(cache.fetched_at));
        }
    }
    Ok(())
}
