use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use clap::Subcommand;
use slatty_core::epic::{auth, library};
use slatty_core::settings;

use crate::{Ctx, shown};

#[derive(Subcommand)]
pub enum EpicCommand {
    /// Sign in to Epic Games through the system browser
    Login {
        /// Print the URL without opening a browser
        #[arg(long)]
        no_browser: bool,
    },
    /// Show the Epic account signed in
    Status,
    /// List the account's Epic games, read again from Epic
    List {
        /// List them from the cache, without asking Epic
        #[arg(long)]
        offline: bool,
    },
    /// Forget the Epic account's tokens; its library cache is kept
    Logout,
}

pub async fn run(ctx: &Ctx, cmd: EpicCommand) -> Result<()> {
    match cmd {
        EpicCommand::Login { no_browser } => login(ctx, no_browser).await,
        EpicCommand::Status => status(ctx).await,
        EpicCommand::List { offline } => list(ctx, offline).await,
        EpicCommand::Logout => {
            match auth::sign_out(&ctx.db)? {
                Some(_) => println!("Signed out of Epic Games. The library cache is kept."),
                None => println!("Not signed in to Epic Games."),
            }
            Ok(())
        }
    }
}

async fn login(ctx: &Ctx, no_browser: bool) -> Result<()> {
    let url = auth::login_url();
    eprintln!("Sign in to Epic Games in your browser:\n\n  {url}\n");
    if !no_browser && slatty_core::auth::open_in_browser(&url).is_err() {
        eprintln!("(could not open a browser, copy the URL above)");
    }
    eprintln!(
        "After signing in, the browser shows a short page of text with an authorizationCode."
    );
    eprint!("Paste that page, or the code, here and press Enter: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("reading the pasted code")?;
    let code = auth::extract_code(&line)?;
    let tokens = auth::sign_in(&ctx.http, &ctx.db, &code).await?;
    println!(
        "Signed in to Epic Games as {}.",
        shown(&tokens.display_name)
    );
    Ok(())
}

async fn status(ctx: &Ctx) -> Result<()> {
    let Some(account_id) = settings::epic_account(&ctx.db)? else {
        println!("Not signed in to Epic Games.");
        return Ok(());
    };
    let cached = library::load_cache(&ctx.dirs, &account_id)?;
    match auth::fresh(&ctx.http, &ctx.db, &ctx.dirs).await {
        Ok(tokens) => {
            println!("Account:      {}", shown(&tokens.display_name));
            println!(
                "Session:      valid until {}",
                crate::local_time(tokens.expires_at)
            );
        }
        Err(e) => println!("Session:      unavailable: {e}"),
    }
    match cached {
        Some(l) => println!(
            "Games:        {} (read {})",
            l.games.len(),
            crate::local_time(l.fetched_at)
        ),
        None => println!("Games:        not read yet (slatty epic list)"),
    }
    Ok(())
}

async fn list(ctx: &Ctx, offline: bool) -> Result<()> {
    let library = if offline {
        let account_id = settings::epic_account(&ctx.db)?.context("not signed in to Epic Games")?;
        library::load_cache(&ctx.dirs, &account_id)?
            .context("the Epic library was not read yet: run `slatty epic list`")?
    } else {
        let tokens = auth::fresh(&ctx.http, &ctx.db, &ctx.dirs).await?;
        library::refresh(&ctx.http, &ctx.dirs, &tokens).await?
    };
    for game in &library.games {
        println!(
            "{:<40} {}",
            shown(slatty_core::store::epic_id(&game.app_name)),
            shown(&game.title)
        );
    }
    println!("{} games", library.games.len());
    Ok(())
}
