use std::io::{BufRead, Write};

use anyhow::{Result, bail};
use clap::Args;
use slatty_core::account::Account;
use slatty_core::achievements::{self, Achievement};
use slatty_core::install::Install;
use slatty_core::library;

use crate::Ctx;

#[derive(Args)]
pub struct AchievementsArgs {
    /// GOG product id of an owned game (installed or not)
    game_id: String,
    /// Unlock these achievements (key, id or exact name) without playing
    #[arg(long, num_args = 1.., value_name = "ACHIEVEMENT")]
    unlock: Vec<String>,
    /// Unlock every locked achievement
    #[arg(long, conflicts_with_all = ["unlock", "clear"])]
    unlock_all: bool,
    /// Clear these achievements (key, id or exact name)
    #[arg(long, num_args = 1.., value_name = "ACHIEVEMENT")]
    clear: Vec<String>,
    /// Do not ask for confirmation
    #[arg(long)]
    yes: bool,
}

pub async fn run(ctx: &Ctx, args: AchievementsArgs) -> Result<()> {
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let install = Install::get(&ctx.db, &args.game_id)?;
    let (client_id, token) = match &install {
        Some(i) => achievements::game_token(&ctx.http, &tokens, i).await?,
        None => achievements::product_token(&ctx.http, &tokens, &args.game_id).await?,
    };
    let title = install
        .map(|i| i.title)
        .or_else(|| {
            library::load_cache(&ctx.dirs, &tokens.user_id)
                .ok()
                .flatten()
                .and_then(|c| {
                    c.games
                        .into_iter()
                        .find(|g| g.id == args.game_id)
                        .map(|g| g.title)
                })
        })
        .unwrap_or_else(|| args.game_id.clone());
    let list = achievements::fetch(&ctx.http, &tokens.user_id, &client_id, &token).await?;

    let mut changes: Vec<(&Achievement, bool)> = Vec::new();
    if args.unlock_all {
        changes.extend(
            list.iter()
                .filter(|a| a.date_unlocked.is_none())
                .map(|a| (a, true)),
        );
    }
    for (queries, unlock) in [(&args.unlock, true), (&args.clear, false)] {
        for q in queries {
            let Some(a) = achievements::select(&list, q) else {
                bail!("no achievement matches `{q}` in {title}; run without options to list them");
            };
            if a.date_unlocked.is_some() != unlock {
                changes.push((a, unlock));
            }
        }
    }

    if changes.is_empty() {
        if !args.unlock.is_empty() || !args.clear.is_empty() || args.unlock_all {
            println!("Nothing to change.");
        }
        print_list(&title, &list);
        return Ok(());
    }

    println!(
        "{title}: {} change(s) on your GOG profile, made outside the game:",
        changes.len()
    );
    for (a, unlock) in &changes {
        println!("  {} {}", if *unlock { "unlock" } else { "clear " }, a.name);
    }
    if !args.yes && !confirm()? {
        println!("Cancelled.");
        return Ok(());
    }
    let mut failed = 0;
    for (a, unlock) in &changes {
        match achievements::set_unlocked(
            &ctx.http,
            &tokens.user_id,
            &client_id,
            &token,
            &a.achievement_id,
            *unlock,
        )
        .await
        {
            Ok(()) => println!("  done: {}", a.name),
            Err(e) => {
                failed += 1;
                println!("  FAILED {}: {e}", a.name);
            }
        }
    }
    let after = achievements::fetch(&ctx.http, &tokens.user_id, &client_id, &token).await?;
    print_list(&title, &after);
    if failed > 0 {
        bail!("{failed} change(s) failed");
    }
    Ok(())
}

fn confirm() -> Result<bool> {
    eprint!("Apply? [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "o" | "oui"))
}

fn print_list(title: &str, list: &[Achievement]) {
    let unlocked = list.iter().filter(|a| a.date_unlocked.is_some()).count();
    println!("{title}: {unlocked}/{} unlocked", list.len());
    for a in list {
        let mark = a.date_unlocked.as_deref().unwrap_or("locked");
        println!(
            "  {mark:<26} {:>5.1}%  {:<40} {}",
            a.rarity, a.name, a.achievement_key
        );
    }
}
