use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;
use slatty_core::steam::library::{self, SteamLibrary};
use slatty_core::steam::local::{self, Steam};

use crate::{Ctx, shown};

#[derive(Subcommand)]
pub enum SteamCommand {
    /// Whether Steam is found, who is signed in to it, how many of its games there are and how
    /// many are installed; read from the Steam client's files, nothing changed
    Status,
}

pub fn run(ctx: &Ctx, cmd: SteamCommand) -> Result<()> {
    match cmd {
        SteamCommand::Status => status(ctx),
    }
}

/// The Steam installation in the home folder, if any.
fn find() -> Option<Steam> {
    local::find(&PathBuf::from(std::env::var_os("HOME")?))
}

/// The Steam games the library shows, once turned on (Settings → Steam).
pub fn games(ctx: &Ctx) -> Result<Option<SteamLibrary>> {
    if !slatty_core::settings::steam_games(&ctx.db)? {
        return Ok(None);
    }
    Ok(find().and_then(|steam| library::read(&steam, false)))
}

fn status(ctx: &Ctx) -> Result<()> {
    let Some(steam) = find() else {
        println!(
            "Steam was not found (natively or as a Flatpak), or no account ever signed in to it."
        );
        return Ok(());
    };
    let kind = if steam.flatpak { "Flatpak" } else { "native" };
    println!("Steam:        {} ({kind})", shown(steam.root.display()));
    let Some(library) = library::read(&steam, false) else {
        println!("Account:      none signed in");
        return Ok(());
    };
    println!("Account:      {}", shown(&library.user.name));
    let ids: HashSet<&str> = library.games.iter().map(|g| g.id.as_str()).collect();
    let installed: Vec<_> = local::installed(&steam)
        .into_iter()
        .filter(|i| ids.contains(slatty_core::store::steam_id(i.app_id).as_str()))
        .collect();
    let updating = installed
        .iter()
        .filter(|i| i.update.is_some_and(|u| u.started))
        .count();
    match &library.unreadable {
        Some(e) => println!(
            "Games:        none can be named: appcache/appinfo.vdf could not be read ({})",
            shown(e)
        ),
        None => println!(
            "Games:        {} ({} installed, {updating} of them downloading or updating)",
            library.games.len(),
            installed.len()
        ),
    }
    let on = |on: bool| if on { "on" } else { "off" };
    println!(
        "In the library: {} (Settings → Steam in slatty-gui)",
        on(slatty_core::settings::steam_games(&ctx.db)?)
    );
    println!(
        "Images from Steam's servers: {}",
        on(slatty_core::settings::steam_images(&ctx.db)?)
    );
    println!(
        "Steam Web API: {}",
        on(slatty_core::settings::steam_web_api(&ctx.db)?)
    );
    Ok(())
}
