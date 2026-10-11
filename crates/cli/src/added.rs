use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use slatty_core::added::{self, NewGame};

use crate::{Ctx, shown};

#[derive(Args)]
pub struct AddArgs {
    /// The program that starts the game: a Windows .exe, or a Linux program, script or AppImage
    #[arg(required_unless_present = "installer")]
    program: Option<PathBuf>,
    /// The title shown in the library
    #[arg(long, required_unless_present = "installer")]
    title: Option<String>,
    /// Run this Windows installer first, in a Wine prefix made for the game; its program, once
    /// installed there, is added with `--installed-by`
    #[arg(long, conflicts_with_all = ["program", "installed_by"])]
    installer: Option<PathBuf>,
    /// The game an installer was run for (`local-<number>`), whose prefix holds the program
    #[arg(long)]
    installed_by: Option<String>,
    /// The game's folder, which holds the program; an isolated game can write in it [default: the
    /// program's folder]
    #[arg(long)]
    folder: Option<PathBuf>,
    /// Where the program starts [default: the program's folder]
    #[arg(long)]
    working_dir: Option<PathBuf>,
    /// Proton build a Windows program runs with (contains the `proton` script) [default: the
    /// default one]
    #[arg(long)]
    proton: Option<PathBuf>,
    /// Run the game without access to your files [default: as a game installed now]
    #[arg(long, conflicts_with = "not_isolated")]
    isolated: bool,
    /// Give the game access to your files
    #[arg(long)]
    not_isolated: bool,
    /// Arguments given to the program, after `--`
    #[arg(last = true)]
    args: Vec<String>,
}

pub async fn add(ctx: &Ctx, args: AddArgs) -> Result<()> {
    let isolated = (args.isolated || args.not_isolated).then_some(args.isolated);
    if let Some(installer) = args.installer {
        let isolated = match isolated {
            Some(isolated) => isolated,
            None => slatty_core::settings::isolate_new_games(&ctx.db)?,
        };
        println!("Running the installer; finish it in its own window.");
        let id = added::run_installer(
            &ctx.db,
            &ctx.dirs,
            &slatty_core::session::supervisor_exe(),
            &installer,
            args.proton,
            isolated,
        )
        .await?;
        let drive = ctx.dirs.data.join("prefixes").join(&id).join("drive_c");
        println!(
            "The installer ended. Add the game it installed with:\n  slatty add '{}/<folder>/<game>.exe' --title <title> --installed-by {id}\nor throw it away with `slatty remove {id}`.",
            shown(drive.display())
        );
        return Ok(());
    }
    let (Some(program), Some(title)) = (args.program, args.title) else {
        anyhow::bail!("give the game's program and --title");
    };
    let folder = args
        .folder
        .clone()
        .or_else(|| program.parent().map(Into::into));
    if let Some(id) = folder.as_deref().and_then(added::gog_game_in) {
        eprintln!(
            "This folder holds a GOG game ({id}). Installed from GOG (`slatty install {id}`), it \
             would have its cloud saves and achievements."
        );
    }
    let install = added::add(
        &ctx.db,
        &ctx.dirs,
        NewGame {
            title,
            program,
            args: args.args,
            folder: args.folder,
            working_dir: args.working_dir,
            proton: args.proton,
            isolated,
            installed_by: args.installed_by,
        },
    )?;
    println!(
        "Added {} as {} ({:?}){}.",
        shown(&install.title),
        install.game_id,
        install.platform,
        if install.isolated { ", isolated" } else { "" }
    );
    Ok(())
}

#[derive(Args)]
pub struct RemoveArgs {
    game_id: String,
    /// Also delete the Wine prefix SlattyLauncher made for it (its saves are copied to the
    /// backups first)
    #[arg(long)]
    delete_prefix: bool,
}

pub fn remove(ctx: &Ctx, args: RemoveArgs) -> Result<()> {
    let Some(title) = slatty_core::install::Install::get(&ctx.db, &args.game_id)?.map(|i| i.title)
    else {
        // An installer was run for it, and the game never added.
        added::discard_installer(&ctx.db, &ctx.dirs, &args.game_id)?;
        println!(
            "What the installer put in {}'s prefix was thrown away; its saves were copied to the backups.",
            args.game_id
        );
        return Ok(());
    };
    let report = added::remove(&ctx.db, &ctx.dirs, &args.game_id, args.delete_prefix)?;
    println!(
        "{} is no longer in the library; its files are untouched.",
        shown(&title)
    );
    if let Some(backup) = report.prefix_backup {
        println!("Saves of its prefix copied to {}", shown(backup.display()));
    }
    if report.prefix_removed {
        println!("Its prefix was deleted.");
    }
    Ok(())
}
