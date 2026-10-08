use anyhow::Result;
use slatty_core::account::Account;
use slatty_core::achievements;

use crate::Ctx;

pub async fn list(ctx: &Ctx, game_id: &str) -> Result<()> {
    let install = crate::games::get(ctx, game_id)?;
    let mut account = Account::load(&ctx.db, &ctx.dirs).await?;
    let tokens = account.tokens(&ctx.http).await?.clone();
    let (client_id, token) = achievements::game_token(&ctx.http, &tokens, &install).await?;
    let items = achievements::fetch(&ctx.http, &tokens.user_id, &client_id, &token).await?;
    let unlocked = items.iter().filter(|a| a.date_unlocked.is_some()).count();
    println!("{}: {unlocked}/{} unlocked", install.title, items.len());
    for a in &items {
        let mark = a.date_unlocked.as_deref().unwrap_or("locked");
        let name = if a.visible || a.date_unlocked.is_some() {
            a.name.as_str()
        } else {
            "(hidden)"
        };
        println!("  {mark:<26} {name}");
    }
    Ok(())
}
