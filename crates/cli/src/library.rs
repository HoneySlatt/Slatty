use anyhow::Result;
use clap::Subcommand;
use slatty_core::account::Account;
use slatty_core::library::{self, LibraryGame, MetadataSource};

use crate::{Ctx, shown};

#[derive(Subcommand)]
pub enum LibraryCommand {
    /// Download the library and refresh the local cache
    Sync,
    /// List games from the local cache (works offline), Steam's once turned on, and those added
    /// by hand
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
            let missing = games
                .iter()
                .filter(|g| g.metadata != MetadataSource::Gamesdb)
                .count();
            let cache = library::save_cache(&ctx.dirs, &account.info.user_id, games)?;
            println!(
                "{} games cached ({missing} with partial metadata).",
                cache.games.len()
            );
        }
        LibraryCommand::List { search } => {
            let needle = search.map(|s| s.to_lowercase());
            let print = |games: &[LibraryGame]| {
                for g in games.iter().filter(|g| {
                    needle
                        .as_ref()
                        .is_none_or(|n| g.title.to_lowercase().contains(n))
                }) {
                    println!(
                        "{:>12}  {:<50} {}",
                        shown(&g.id),
                        shown(&g.title),
                        shown(g.os.join(","))
                    );
                }
            };
            let steam = crate::steam::games(ctx)?;
            let added = slatty_core::added::library(&ctx.db)?;
            match Account::active(&ctx.db)? {
                Some(info) => match library::load_cache(&ctx.dirs, &info.user_id)? {
                    Some(cache) => {
                        print(&cache.games);
                        println!("(cache from {})", crate::local_time(cache.fetched_at));
                    }
                    None => println!("No cached library yet. Run `slatty library sync`."),
                },
                // Steam's games and those added by hand need no GOG account.
                None if steam.is_some() || !added.is_empty() => {}
                None => return Err(slatty_core::Error::NotLoggedIn.into()),
            }
            if let Some(steam) = steam {
                print(&steam.games);
                println!("(Steam games, read from the Steam client's files)");
            }
            if !added.is_empty() {
                print(&added);
                println!("(games added by hand)");
            }
        }
    }
    Ok(())
}
