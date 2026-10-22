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
    /// Install the game first, into a folder named after --title in --into: a Windows installer
    /// runs in a Wine prefix made for it, where that folder is drive D:; a Linux installer
    /// (MojoSetup .sh) or archive (.zip, .tar.gz, .tar.xz) is unpacked there, never run. Its
    /// program is then added with `--installed-by`
    #[arg(long, conflicts_with_all = ["program", "installed_by"])]
    installer: Option<PathBuf>,
    /// Where the game is installed or unpacked, in a folder named after its title [default: the
    /// default installation path]
    #[arg(long, requires = "installer")]
    into: Option<PathBuf>,
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
    if let Some(installer) = &args.installer {
        let Some(title) = args.title else {
            anyhow::bail!("give the game a --title: its folder is named after it");
        };
        let into = match args.into {
            Some(into) => into,
            None => slatty_core::settings::library_root(&ctx.db)?,
        };
        std::fs::create_dir_all(&into)?;
        if !added::is_archive(installer) {
            let isolated = match isolated {
                Some(isolated) => isolated,
                None => slatty_core::settings::isolate_new_games(&ctx.db)?,
            };
            println!(
                "Running the installer; finish it in its own window, installing the game on \
                 drive D:."
            );
            let (id, folder) = added::run_installer(
                &ctx.db,
                &ctx.dirs,
                &slatty_core::session::supervisor_exe(),
                installer,
                &into,
                &title,
                args.proton,
                isolated,
            )
            .await?;
            println!(
                "The installer ended. Add the game it installed in {} with:\n  slatty add '{}/<program>.exe' --title {} --installed-by {id}\nor throw it away with `slatty remove {id}`.",
                shown(folder.display()),
                shown(folder.display()),
                shown(&title)
            );
            return Ok(());
        }
        let (id, folder) = added::unpack(
            &ctx.db,
            &ctx.dirs,
            installer,
            &into,
            &title,
            Default::default(),
            &|_, _| {},
        )
        .await?;
        println!(
            "Unpacked into {}. Add the game with:\n  slatty add '{}/<program>' --title {} --installed-by {id}\nor throw it away with `slatty remove {id}`.",
            shown(folder.display()),
            shown(folder.display()),
            shown(&title)
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
    /// Also delete the Wine prefix Slatty made for it (its saves, and a game its installer
    /// put in it, are copied to the backups first)
    #[arg(long)]
    delete_prefix: bool,
}

pub fn remove(ctx: &Ctx, args: RemoveArgs) -> Result<()> {
    let Some(title) = slatty_core::install::Install::get(&ctx.db, &args.game_id)?.map(|i| i.title)
    else {
        // An installer was run for it, and the game never added.
        added::discard_installer(&ctx.db, &ctx.dirs, &args.game_id)?;
        println!(
            "What was installed for {}, never added, was thrown away (a prefix's saves copied to the backups first).",
            args.game_id
        );
        return Ok(());
    };
    if slatty_core::store::is_emulated(&args.game_id) {
        slatty_core::emulated::remove(&ctx.db, &ctx.dirs, &args.game_id)?;
        println!(
            "{} is no longer in the library. Its files are untouched.",
            shown(&title)
        );
        return Ok(());
    }
    let report = added::remove(&ctx.db, &ctx.dirs, &args.game_id, args.delete_prefix)?;
    println!("{} is no longer in the library.", shown(&title));
    if report.removed_files > 0 {
        println!(
            "{} file(s) Slatty unpacked for it were deleted.",
            report.removed_files
        );
    } else if report.game_backup.is_none() {
        println!("Its files are untouched.");
    }
    if !report.kept.is_empty() {
        println!("{} other file(s) kept in its folder.", report.kept.len());
    }
    if let Some(backup) = report.prefix_backup {
        println!("Saves of its prefix copied to {}", shown(backup.display()));
    }
    if report.prefix_removed {
        println!("Its prefix was deleted.");
    }
    if let Some(copy) = report.game_backup {
        println!(
            "The game was in its prefix and went with it; its copy is in {}",
            shown(copy.display())
        );
    }
    Ok(())
}
