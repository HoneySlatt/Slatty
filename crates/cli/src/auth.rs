use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use clap::Subcommand;
use slatty_core::account::Account;
use slatty_core::auth;

use crate::Ctx;

#[derive(Subcommand)]
pub enum AuthCommand {
    /// Log in through the system browser
    Login {
        /// Print the URL without opening a browser
        #[arg(long)]
        no_browser: bool,
    },
    /// Show the active account
    Status,
    /// Force a token refresh
    Refresh,
    /// Experiment: check whether refreshing rotates the refresh token
    ProbeRotation,
    /// Forget the stored tokens of the active account
    Logout,
}

pub async fn run(ctx: &Ctx, cmd: AuthCommand) -> Result<()> {
    match cmd {
        AuthCommand::Login { no_browser } => login(ctx, no_browser).await,
        AuthCommand::Status => status(ctx).await,
        AuthCommand::Refresh => {
            let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
            account.refresh(&ctx.http, true).await?;
            println!(
                "Session renewed, valid until {}",
                crate::local_time(account.expires_at())
            );
            Ok(())
        }
        AuthCommand::ProbeRotation => {
            let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
            let report = account.probe_rotation(&ctx.http).await?;
            println!("refresh token rotated on refresh: {}", report.rotated);
            println!(
                "previous refresh token still accepted: {}",
                report.old_still_valid
            );
            Ok(())
        }
        AuthCommand::Logout => {
            let account = Account::load(&ctx.db, &ctx.dirs).await?;
            let name = account.info.username.clone();
            account.logout(&ctx.db).await?;
            println!("Logged out {name}. Local cache and saves are kept.");
            Ok(())
        }
    }
}

async fn login(ctx: &Ctx, no_browser: bool) -> Result<()> {
    let url = auth::login_url();
    eprintln!("Log in to GOG in your browser:\n\n  {url}\n");
    if !no_browser
        && std::process::Command::new("xdg-open")
            .arg(url.as_str())
            .spawn()
            .is_err()
    {
        eprintln!("(could not open a browser, copy the URL above)");
    }
    eprintln!("After logging in, the browser lands on a mostly blank embed.gog.com page.");
    eprint!("Paste that page's full URL here and press Enter: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .context("reading the pasted URL")?;
    let code = auth::extract_code(&line)?;
    let account = Account::login(&ctx.http, &ctx.db, &ctx.dirs, &code).await?;
    println!(
        "Logged in as {} (user id {}).",
        account.info.username, account.info.user_id
    );
    Ok(())
}

async fn status(ctx: &Ctx) -> Result<()> {
    let Some(info) = Account::active(&ctx.db)? else {
        println!("Not logged in.");
        return Ok(());
    };
    println!("Account: {} (user id {})", info.username, info.user_id);
    match Account::load(&ctx.db, &ctx.dirs).await {
        Ok(account) => println!(
            "Access token valid until {}",
            crate::local_time(account.expires_at())
        ),
        Err(e) => println!("Tokens unavailable: {e}"),
    }
    Ok(())
}
