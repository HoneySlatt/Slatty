use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Args, ValueEnum};
use serde::Serialize;
use slatty_core::account::Account;
use slatty_core::install::{self, Install};
use slatty_core::library::{self, LibraryGame};
use slatty_core::runner::{self, Runner};
use slatty_core::{settings, steam, store};

use crate::{Ctx, shown};

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
    /// GOG product id of the game, needed for a native Linux build
    #[arg(long)]
    game_id: Option<String>,
    /// How the game runs: native (a Linux build), umu (Proton) or wine
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
                (Some(RunnerKind::Umu), Some(proton), None, Some(prefix)) => {
                    Runner::Umu { proton, prefix }
                }
                (Some(RunnerKind::Wine), None, Some(wine), Some(prefix)) => {
                    Runner::Wine { wine, prefix }
                }
                _ => bail!(
                    "use --runner native, --runner umu --proton DIR --prefix DIR, or --runner wine --wine BIN --prefix DIR"
                ),
            };
            install::from_dir(&dir, args.game_id.as_deref(), runner)?
        }
        (None, None) => bail!("give a directory or --from-heroic GAME_ID"),
    };
    let install = install::import(&ctx.db, install)?;
    println!(
        "Imported {} ({}) from {}",
        install.title,
        install.game_id,
        install.path.display()
    );
    print_runner(&install);
    Ok(())
}

pub fn list(ctx: &Ctx) -> Result<()> {
    for i in Install::list(&ctx.db)? {
        println!(
            "{:>12}  {:<40} {:?}  {}",
            i.game_id,
            i.title,
            i.platform,
            i.path.display()
        );
    }
    Ok(())
}

/// A game of the library's Installed shelf, for other programs.
#[derive(Serialize)]
struct Installed {
    id: String,
    title: String,
    /// The cover the library shows, once it is in a local file.
    cover: Option<PathBuf>,
    /// The Steam app to start through Steam, for a game only Steam installed; any other starts
    /// with `slatty launch <id>`.
    steam_app: Option<u32>,
}

/// Prints as JSON the games the library's Installed shelf shows, with the titles and covers given
/// there, hidden ones left out. Reads local files only: a cover not downloaded yet is none.
pub fn list_json(ctx: &Ctx) -> Result<()> {
    let customs = slatty_core::custom::all(&ctx.db)?;
    let installs: HashSet<String> = Install::list(&ctx.db)?
        .into_iter()
        .map(|i| i.game_id)
        .collect();
    let (gog, mut covers) = match Account::active(&ctx.db)? {
        Some(info) => (
            library::load_cache(&ctx.dirs, &info.user_id)?
                .map(|c| c.games)
                .unwrap_or_default(),
            library::cached_covers(&ctx.dirs, &info.user_id),
        ),
        None => (Vec::new(), HashMap::new()),
    };
    let steam = settings::steam_games(&ctx.db)?
        .then(crate::steam::find)
        .flatten();
    let steam_library = match &steam {
        Some(s) => steam::library::read(s, settings::steam_images(&ctx.db)?),
        None => None,
    };
    let steam_games = steam_library.as_ref().map_or(&[][..], |l| &l.games);
    if let Some(l) = &steam_library {
        covers.extend(l.covers.clone());
        for g in steam_games {
            if let Some(path) = g
                .cover
                .as_deref()
                .and_then(|url| steam::library::downloaded_image(&ctx.dirs, url))
            {
                covers.insert(g.id.clone(), path);
            }
        }
    }
    // Steam's whole once installed, updates included; a first install still downloads.
    let ready: HashSet<u32> = steam
        .iter()
        .flat_map(steam::local::installed)
        .filter(|i| i.ready)
        .map(|i| i.app_id)
        .collect();
    let on_steam = |id: &str| store::steam_app(id).filter(|app| ready.contains(app));
    // A game owned on both shows once, as GOG's, matched with GOG's own titles.
    let twins = steam::library::twins(
        gog.iter()
            .map(|g| (g.id.as_str(), g.title.as_str(), g.steam.as_deref())),
        steam_games,
    );
    let twinned: HashSet<&String> = twins.values().collect();

    let mut shown: Vec<(&LibraryGame, Option<u32>)> = Vec::new();
    for g in &gog {
        let steam_app = twins.get(&g.id).and_then(|id| on_steam(id));
        if installs.contains(&g.id) {
            shown.push((g, None));
        } else if steam_app.is_some() {
            shown.push((g, steam_app));
        }
    }
    for g in steam_games.iter().filter(|g| !twinned.contains(&g.id)) {
        if let Some(app) = on_steam(&g.id) {
            shown.push((g, Some(app)));
        }
    }
    let added = slatty_core::added::library(&ctx.db)?;
    let emulated = slatty_core::emulated::library(&ctx.db, &ctx.dirs)?;
    shown.extend(added.iter().chain(&emulated).map(|g| (g, None)));

    let mut games: Vec<(String, Installed)> = shown
        .into_iter()
        .filter_map(|(g, steam_app)| {
            let custom = customs.get(&g.id);
            if custom.is_some_and(|c| c.hidden) {
                return None;
            }
            let title = custom
                .and_then(|c| c.title.clone())
                .unwrap_or_else(|| g.title.clone());
            let sort = custom
                .and_then(|c| c.sort_title.clone())
                .unwrap_or_else(|| title.clone())
                .to_lowercase();
            let cover = custom
                .and_then(|c| c.cover.clone())
                .or_else(|| covers.get(&g.id).cloned());
            Some((
                sort,
                Installed {
                    id: g.id.clone(),
                    title,
                    cover,
                    steam_app,
                },
            ))
        })
        .collect();
    games.sort_by(|a, b| a.0.cmp(&b.0));
    let games: Vec<Installed> = games.into_iter().map(|(_, g)| g).collect();
    println!("{}", serde_json::to_string_pretty(&games)?);
    Ok(())
}

pub fn print_spec(ctx: &Ctx, game_id: &str) -> Result<()> {
    let install = get(ctx, game_id)?;
    let spec = runner::game_launch_spec(&ctx.db, &ctx.dirs, &install)?;
    // The program, its arguments and its folder come from the game's own goggame-*.info.
    println!("program: {}", shown(spec.program.display()));
    println!("args:    {:?}", spec.args);
    println!("cwd:     {}", shown(spec.cwd.display()));
    for (k, v) in &spec.env {
        println!("env:     {k}={}", shown(v));
    }
    Ok(())
}

pub fn get(ctx: &Ctx, game_id: &str) -> Result<Install> {
    Install::get(&ctx.db, game_id)?
        .ok_or_else(|| anyhow::anyhow!("{game_id} is not imported; see `slatty import`"))
}

fn print_runner(install: &Install) {
    match &install.runner {
        Runner::Native => println!("Runner: native"),
        Runner::Umu { proton, prefix } => {
            println!(
                "Runner: umu + {}\nPrefix: {}",
                proton.display(),
                prefix.display()
            )
        }
        Runner::Wine { wine, prefix } => {
            println!("Runner: {}\nPrefix: {}", wine.display(), prefix.display())
        }
    }
}

#[derive(Args)]
pub struct IsolationArgs {
    game_id: String,
    /// Run the game without access to your files, from its next launch on
    #[arg(long, conflicts_with = "off")]
    on: bool,
    /// Give the game access to your files again, from its next launch on
    #[arg(long)]
    off: bool,
}

pub fn isolation(ctx: &Ctx, args: IsolationArgs) -> Result<()> {
    let install = if args.on || args.off {
        install::set_isolated(&ctx.db, &args.game_id, args.on)?
    } else {
        get(ctx, &args.game_id)?
    };
    if install.isolated {
        let home = runner::isolated_home(&ctx.dirs, &install.game_id)?;
        println!(
            "{} runs isolated: it sees its own folder, and {} as its home folder.",
            shown(&install.title),
            home.display()
        );
    } else {
        println!("{} runs with access to your files.", shown(&install.title));
    }
    Ok(())
}

pub fn forget(ctx: &Ctx, game_id: &str) -> Result<()> {
    if slatty_core::store::is_local(game_id) {
        bail!("{game_id} was added by hand: `slatty remove {game_id}` takes it out of the library");
    }
    get(ctx, game_id)?;
    let install = slatty_core::maintenance::forget(&ctx.db, &ctx.dirs, game_id)?;
    println!(
        "{} is no longer managed by Slatty: {} and its prefix are untouched.",
        shown(&install.title),
        shown(install.path.display())
    );
    Ok(())
}
