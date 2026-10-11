//! The post-install setup GOG Galaxy runs: the GOG script interpreter or each product's temporary
//! executable, then shared redistributables. Order and arguments follow Heroic
//! (GPL-3.0, https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher).

use std::path::{Path, PathBuf};

use reqwest::Client;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::galaxy::{self, Dependency, GogContent, Meta};
use crate::gameinfo::{resolve_relative, split_args};
use crate::install::Install;
use crate::installer::{self, DlcSelection, Download, InstallRecord};
use crate::paths::Dirs;
use crate::runner;
use crate::session::SessionHandle;

const SCRIPT_INTERPRETER: &str = "ISI";
const SCRIPT_INTERPRETER_EXE: &str = "__redist/ISI/scriptinterpreter.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupCommand {
    pub label: String,
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupEvent {
    Downloading,
    Running(String),
    /// A command failed: it exited with a code other than those meaning done (`DONE_CODES`).
    NonZeroExit {
        label: String,
        code: Option<i32>,
    },
    /// A step failed: the setup runs again at the next launch (`again`), or it was the last try
    /// and only `slatty setup --force` runs it again.
    Unfinished {
        again: bool,
    },
}

/// Exit codes of installers that mean done: 1638 found a newer version already installed, 1641
/// and 3010 ask for a restart (Windows Installer codes, used by Microsoft's redistributables).
const DONE_CODES: [i32; 4] = [0, 1638, 1641, 3010];
/// Runs of the setup for one build that may fail before it is left as it is: a step that fails
/// for good would otherwise run at every launch.
pub const SETUP_RUNS: u32 = 3;

/// Whether a step exited as done; one that did not exit by itself failed.
fn done(code: Option<i32>) -> bool {
    code.is_some_and(|c| DONE_CODES.contains(&c))
}

/// Notes how a run of the setup went. A run with a failed step leaves the setup pending, so it
/// runs again at the next launch, until the last of `SETUP_RUNS`. Whether it runs again, when a
/// step failed.
fn finish(record: &mut InstallRecord, failed: bool) -> Option<bool> {
    if failed {
        record.setup_failures += 1;
    }
    let again = failed && record.setup_failures < SETUP_RUNS;
    if !again {
        record.setup_build = Some(record.build_id.clone());
        record.setup_failures = 0;
    }
    failed.then_some(again)
}

pub struct SetupPaths<'a> {
    pub game: &'a Path,
    pub support: &'a Path,
    pub redist: &'a Path,
}

/// Wine sees the host file system as drive Z:.
pub fn wine_path(p: &Path) -> String {
    format!("Z:{}", p.display().to_string().replace('/', "\\"))
}

/// English name of a language code (`fr-FR` → `French`), as Galaxy passes it to installers.
pub fn language_name(code: &str) -> &'static str {
    match code
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "pl" => "Polish",
        "ru" => "Russian",
        // GOG's offline installers use its own codes for some.
        "pt" | "br" => "Portuguese",
        "ja" | "jp" => "Japanese",
        "ko" => "Korean",
        "zh" | "cn" => "Chinese",
        "cs" | "cz" => "Czech",
        "hu" => "Hungarian",
        "nl" => "Dutch",
        "tr" => "Turkish",
        "uk" => "Ukrainian",
        "sv" => "Swedish",
        "da" => "Danish",
        "fi" => "Finnish",
        "no" | "nb" => "Norwegian",
        "ar" => "Arabic",
        "ro" => "Romanian",
        "el" => "Greek",
        "th" => "Thai",
        _ => "English",
    }
}

/// Shared dependencies to download before running the commands.
pub fn needed_dependencies(meta: &Meta, repository: &[Dependency]) -> Vec<Dependency> {
    let mut ids: Vec<&str> = meta.dependencies.iter().map(String::as_str).collect();
    if meta.script_interpreter {
        ids.push(SCRIPT_INTERPRETER);
    }
    let mut out: Vec<Dependency> = Vec::new();
    for id in ids {
        if let Some(d) = repository
            .iter()
            .find(|d| d.dependency_id == id && d.is_shared())
            && !out.iter().any(|o| o.dependency_id == d.dependency_id)
        {
            out.push(d.clone());
        }
    }
    out
}

/// Commands Galaxy would run after installing `record`'s build.
pub fn commands(
    meta: &Meta,
    dependencies: &[Dependency],
    record: &InstallRecord,
    paths: &SetupPaths<'_>,
) -> Result<Vec<SetupCommand>> {
    let language = language_name(&record.language);
    let products = meta
        .products
        .iter()
        .filter(|p| p.product_id == meta.base_product_id || record.dlcs.contains(&p.product_id));
    let common = |product_id: &str| {
        vec![
            "/VERYSILENT".to_string(),
            format!("/DIR={}", wine_path(paths.game)),
            format!("/Language={language}"),
            format!("/LANG={language}"),
            format!("/ProductId={product_id}"),
            "/galaxyclient".to_string(),
            format!("/buildId={}", record.build_id),
            format!("/versionName={}", record.version),
            format!("/lang-code={}", record.language),
        ]
    };
    let mut out = Vec::new();
    for product in products {
        if meta.script_interpreter {
            let mut args = common(&product.product_id);
            args.push(format!("/supportDir={}", wine_path(paths.support)));
            args.extend([
                "/nodesktopshorctut".to_string(),
                "/nodesktopshortcut".to_string(),
            ]);
            out.push(SetupCommand {
                label: format!("GOG setup script ({})", product.name),
                program: resolve_relative(paths.redist, SCRIPT_INTERPRETER_EXE)?,
                args,
            });
        } else if let Some(exe) = product.temp_executable.as_deref().filter(|e| !e.is_empty()) {
            let mut args = common(&product.product_id);
            args.extend([
                "/nodesktopshorctut".to_string(),
                "/nodesktopshortcut".to_string(),
            ]);
            out.push(SetupCommand {
                label: format!("GOG setup ({})", product.name),
                program: resolve_relative(
                    &paths
                        .support
                        .join(crate::installer::safe_relative(&product.product_id)?),
                    exe,
                )?,
                args,
            });
        }
    }
    for dep in dependencies
        .iter()
        .filter(|d| d.dependency_id != SCRIPT_INTERPRETER)
    {
        if dep.executable.path.is_empty() {
            continue;
        }
        let exe = resolve_relative(paths.redist, &dep.executable.path)?;
        let mut args = split_args(&dep.executable.arguments);
        let program = if dep.dependency_id == "PHYSXLEGACY" {
            args = ["/i".to_string(), exe.display().to_string()]
                .into_iter()
                .chain(args)
                .chain(["/qb".into()])
                .collect();
            PathBuf::from("msiexec")
        } else {
            exe
        };
        let label = if dep.readable_name.is_empty() {
            dep.dependency_id.clone()
        } else {
            dep.readable_name.clone()
        };
        out.push(SetupCommand {
            label,
            program,
            args,
        });
    }
    Ok(out)
}

pub fn redist_dir(dirs: &Dirs) -> PathBuf {
    dirs.data.join("redist")
}

/// True when the installed build still needs its setup. Linux builds have none.
pub fn pending(dirs: &Dirs, game_id: &str) -> Result<bool> {
    Ok(InstallRecord::load(dirs, game_id)?.is_some_and(|r| {
        r.setup_build.as_deref() != Some(&r.build_id)
            && !installer::linux::is_linux_build(&r.build_id)
    }))
}

pub struct Preview {
    pub dependencies: Vec<Dependency>,
    pub commands: Vec<SetupCommand>,
}

/// What the setup would download and run; changes nothing.
pub async fn preview(
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    install: &Install,
) -> Result<Preview> {
    let record = slatty_record(dirs, install)?;
    let (plan, dependencies) = resolve(http, tokens, install, &record).await?;
    let support = installer::support_dir(dirs, &install.game_id);
    let redist = redist_dir(dirs);
    let paths = SetupPaths {
        game: &install.path,
        support: &support,
        redist: &redist,
    };
    let commands = commands(&plan.meta, &dependencies, &record, &paths)?;
    Ok(Preview {
        dependencies,
        commands,
    })
}

fn slatty_record(dirs: &Dirs, install: &Install) -> Result<InstallRecord> {
    if install.runner.prefix().is_none() {
        return Err(Error::Unsupported("setup needs a Wine prefix".into()));
    }
    InstallRecord::load(dirs, &install.game_id)?.ok_or_else(|| {
        Error::Refused("setup is only available for games installed by slatty".into())
    })
}

async fn resolve(
    http: &Client,
    tokens: &Tokens,
    install: &Install,
    record: &InstallRecord,
) -> Result<(installer::InstallPlan, Vec<Dependency>)> {
    let plan = installer::plan_for(
        http,
        tokens,
        &install.game_id,
        install.platform,
        Some(&record.language),
        Some(&record.build_id),
        &DlcSelection::Only(record.dlcs.clone()),
    )
    .await?;
    let repository = if plan.meta.dependencies.is_empty() && !plan.meta.script_interpreter {
        Vec::new()
    } else {
        galaxy::dependencies(http, tokens).await?
    };
    let needed = needed_dependencies(&plan.meta, &repository);
    Ok((plan, needed))
}

/// Downloads what the setup needs, runs every command under the session supervisor, and marks
/// the build as set up. Only for games installed by slatty with a Wine prefix. Holds the game, so
/// that no update changes its files and record meanwhile.
pub async fn run(
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    install: &Install,
    supervisor: &Path,
    force: bool,
    emit: &(dyn Fn(SetupEvent) + Send + Sync),
) -> Result<Vec<SetupCommand>> {
    let _busy = crate::lock::game(dirs, &install.game_id)?;
    run_held(dirs, http, tokens, install, supervisor, force, emit).await
}

/// `run`, for a caller already holding the game (a launch).
pub(crate) async fn run_held(
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    install: &Install,
    supervisor: &Path,
    force: bool,
    emit: &(dyn Fn(SetupEvent) + Send + Sync),
) -> Result<Vec<SetupCommand>> {
    let mut record = slatty_record(dirs, install)?;
    if !force && record.setup_build.as_deref() == Some(&record.build_id) {
        return Ok(Vec::new());
    }
    let (plan, needed) = resolve(http, tokens, install, &record).await?;
    let redist = redist_dir(dirs);
    let support = installer::support_dir(dirs, &install.game_id);
    emit(SetupEvent::Downloading);
    let source = GogContent::new(http.clone(), tokens.clone(), dirs);
    let dl = Download {
        source: &source,
        cancel: tokio_util::sync::CancellationToken::new(),
        progress: &|_| {},
        free_space: &installer::free_space,
    };
    let game_files = installer::collect_files(&source, &plan.depots).await?;
    dl.check_installed(&game_files.support_set(), &support, true)
        .await?;
    if !needed.is_empty() {
        let depots: Vec<_> = needed.iter().map(Dependency::depot).collect();
        let set = installer::collect_files(&source, &depots).await?;
        dl.check_installed(&set, &redist, true).await?;
    }
    let meta = plan.meta;
    let paths = SetupPaths {
        game: &install.path,
        support: &support,
        redist: &redist,
    };
    let commands = commands(&meta, &needed, &record, &paths)?;
    let log = dirs.logs().join(format!("setup-{}.log", install.game_id));
    let mut failed = false;
    for command in &commands {
        emit(SetupEvent::Running(command.label.clone()));
        let mut args = vec![command.program.display().to_string()];
        args.extend(command.args.iter().cloned());
        let spec = runner::windows_command(dirs, install, args, install.path.clone())?;
        let busy = crate::lock::session(dirs, &install.game_id);
        let outcome = SessionHandle::start(supervisor, &spec, &log, Some(&busy))
            .await?
            .wait()
            .await?;
        if !done(outcome.main_code) {
            failed = true;
            emit(SetupEvent::NonZeroExit {
                label: command.label.clone(),
                code: outcome.main_code,
            });
        }
    }
    let again = finish(&mut record, failed);
    record.save(dirs, &install.game_id)?;
    if let Some(again) = again {
        emit(SetupEvent::Unfinished { again });
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{DependencyExecutable, Product};

    fn dep(id: &str, path: &str, args: &str) -> Dependency {
        Dependency {
            dependency_id: id.into(),
            executable: DependencyExecutable {
                path: path.into(),
                arguments: args.into(),
            },
            manifest: format!("m-{id}"),
            readable_name: format!("[FAKE] {id}"),
            size: 1,
            compressed_size: 1,
        }
    }

    fn meta(script_interpreter: bool, temp_exe: Option<&str>) -> Meta {
        let product = |id: &str, exe: Option<&str>| Product {
            product_id: id.into(),
            name: format!("[FAKE] {id}"),
            temp_executable: exe.map(String::from),
            temp_arguments: None,
        };
        Meta {
            version: Some(2),
            base_product_id: "1".into(),
            client_id: None,
            install_directory: "Game".into(),
            depots: vec![],
            dependencies: vec!["MSVC2019".into(), "GAMEDIR".into()],
            products: vec![
                product("1", temp_exe),
                product("2", temp_exe),
                product("3", temp_exe),
            ],
            script_interpreter,
        }
    }

    fn record() -> InstallRecord {
        InstallRecord {
            build_id: "b1".into(),
            version: "1.0".into(),
            language: "fr-FR".into(),
            path: None,
            dlcs: vec!["2".into()],
            setup_build: None,
            setup_failures: 0,
            files: vec![],
        }
    }

    /// A setup with a failed step (DOOM's DirectX setup given an argument it refused) runs again
    /// at the next launches, not at every launch forever; an installer saying it is already
    /// installed, or asking for a restart, has not failed.
    #[test]
    fn a_failed_setup_runs_again_a_few_times() {
        assert!(done(Some(0)) && done(Some(3010)) && done(Some(1638)) && done(Some(1641)));
        assert!(!done(Some(1)) && !done(None));

        let mut r = record();
        for run in 1..SETUP_RUNS {
            assert_eq!(finish(&mut r, true), Some(true), "run {run}");
            assert_eq!(r.setup_build, None, "still to run");
        }
        assert_eq!(finish(&mut r, true), Some(false), "the last try");
        assert_eq!(
            (r.setup_build.as_deref(), r.setup_failures),
            (Some("b1"), 0)
        );

        let mut r = record();
        assert_eq!(finish(&mut r, true), Some(true));
        assert_eq!(finish(&mut r, false), None, "passed the next time");
        assert_eq!(
            (r.setup_build.as_deref(), r.setup_failures),
            (Some("b1"), 0)
        );
    }

    fn repository() -> Vec<Dependency> {
        vec![
            dep(
                "MSVC2019",
                "__redist/MSVC2019/VC_redist.x86.exe",
                "/install /quiet /norestart",
            ),
            dep("GAMEDIR", "", ""),
            dep("ISI", "__redist/ISI/scriptinterpreter.exe", ""),
            dep("UNUSED", "__redist/x/y.exe", ""),
        ]
    }

    fn paths() -> (PathBuf, PathBuf, PathBuf) {
        (
            "/games/Game".into(),
            "/data/support/1".into(),
            "/data/redist".into(),
        )
    }

    /// `slatty setup` rewrites the game's install record once done: it waits for no update, which
    /// would have its record overwritten, and is refused while the game is busy.
    #[tokio::test]
    async fn setup_is_refused_while_the_game_is_busy() {
        let root = std::env::temp_dir().join(format!("slatty-setup-busy-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: crate::install::Platform::Windows,
            path: root.join("Game"),
            client_id: None,
            runner: runner::Runner::Umu {
                proton: "/proton".into(),
                prefix: root.join("prefix"),
            },
            umu_id: None,
            isolated: true,
        };
        let tokens = Tokens {
            user_id: "1".into(),
            access_token: crate::secret::Secret::new("[FAKE]"),
            refresh_token: crate::secret::Secret::new("[FAKE]"),
            expires_at: i64::MAX,
        };
        let _update = crate::lock::game(&dirs, "1").unwrap();
        let busy = run(
            &dirs,
            &crate::http::client().unwrap(),
            &tokens,
            &install,
            Path::new("/nonexistent"),
            true,
            &|_| {},
        )
        .await
        .unwrap_err();
        assert!(busy.to_string().contains("another operation"), "{busy}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn script_interpreter_runs_for_base_game_and_installed_dlc_only() {
        let (game, support, redist) = paths();
        let m = meta(true, None);
        let needed = needed_dependencies(&m, &repository());
        let ids: Vec<_> = needed.iter().map(|d| d.dependency_id.as_str()).collect();
        assert_eq!(ids, ["MSVC2019", "ISI"]);
        let cmds = commands(
            &m,
            &needed,
            &record(),
            &SetupPaths {
                game: &game,
                support: &support,
                redist: &redist,
            },
        )
        .unwrap();
        assert_eq!(cmds.len(), 3);
        assert!(
            cmds[0]
                .program
                .ends_with("__redist/ISI/scriptinterpreter.exe")
        );
        assert!(cmds[0].args.contains(&"/ProductId=1".to_string()));
        assert!(cmds[1].args.contains(&"/ProductId=2".to_string()));
        assert!(cmds[0].args.contains(&"/DIR=Z:\\games\\Game".to_string()));
        assert!(
            cmds[0]
                .args
                .contains(&"/supportDir=Z:\\data\\support\\1".to_string())
        );
        assert!(cmds[0].args.contains(&"/Language=French".to_string()));
        assert_eq!(cmds[2].args, ["/install", "/quiet", "/norestart"]);
    }

    #[test]
    fn temporary_executables_run_from_the_support_folder() {
        let (game, support, redist) = paths();
        let m = meta(false, Some("setup_helper.exe"));
        let needed = needed_dependencies(&m, &repository());
        assert!(!needed.iter().any(|d| d.dependency_id == "ISI"));
        let cmds = commands(
            &m,
            &needed,
            &record(),
            &SetupPaths {
                game: &game,
                support: &support,
                redist: &redist,
            },
        )
        .unwrap();
        assert_eq!(
            cmds[0].program,
            PathBuf::from("/data/support/1/1/setup_helper.exe")
        );
        assert_eq!(
            cmds[1].program,
            PathBuf::from("/data/support/1/2/setup_helper.exe")
        );
        assert_eq!(cmds.len(), 3);
    }

    #[test]
    fn unsafe_executable_paths_are_refused() {
        let (game, support, redist) = paths();
        let m = meta(false, Some("..\\..\\evil.exe"));
        assert!(
            commands(
                &m,
                &[],
                &record(),
                &SetupPaths {
                    game: &game,
                    support: &support,
                    redist: &redist
                }
            )
            .is_err()
        );
    }

    /// A DLC's id from the build metadata names the folder its setup program is run from.
    #[test]
    fn a_product_id_that_climbs_out_of_the_support_folder_is_refused() {
        let (game, support, redist) = paths();
        let mut m = meta(false, Some("setup.exe"));
        m.products[1].product_id = "../../../etc".into();
        let mut r = record();
        r.dlcs = vec!["../../../etc".into()];
        let paths = SetupPaths {
            game: &game,
            support: &support,
            redist: &redist,
        };
        assert!(matches!(
            commands(&m, &[], &r, &paths),
            Err(Error::Refused(_))
        ));
    }

    #[test]
    fn language_names_follow_galaxy() {
        assert_eq!(language_name("en-US"), "English");
        assert_eq!(language_name("zh-Hans"), "Chinese");
        assert_eq!(language_name("pt-BR"), "Portuguese");
        assert_eq!(wine_path(Path::new("/a/b c")), "Z:\\a\\b c");
    }
}
