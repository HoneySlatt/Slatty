use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use reqwest::Client;
use tokio_util::sync::CancellationToken;

use crate::auth::Tokens;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::galaxy::GogContent;
use crate::install::{Install, Platform};
use crate::installer::{self, DlcSelection, Download, InstallJob, InstallRecord, Progress, linux};
use crate::paths::Dirs;
use crate::session;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UninstallReport {
    pub removed_files: usize,
    /// Files that slatty did not install (saves, mods, configs), left in place.
    pub kept: Vec<PathBuf>,
    pub folder_removed: bool,
    pub prefix_backup: Option<PathBuf>,
    pub prefix_removed: bool,
}

fn installed_by_slatty(db: &Db, dirs: &Dirs, game_id: &str) -> Result<(Install, InstallRecord)> {
    let install = Install::get(db, game_id)?
        .ok_or_else(|| Error::NotFound(format!("{game_id} is not installed")))?;
    let record = InstallRecord::load(dirs, game_id)?.ok_or_else(|| {
        Error::Refused(format!(
            "{} was not installed by slatty; use `forget` to drop it without touching its files",
            install.title
        ))
    })?;
    if record.path.as_ref().is_some_and(|p| p != &install.path) {
        return Err(Error::Refused(
            "the install record points to another folder".into(),
        ));
    }
    if session::unfinished(db)?
        .iter()
        .any(|s| s.game_id == game_id)
    {
        return Err(Error::Refused(format!("{} is running", install.title)));
    }
    Ok((install, record))
}

/// Removes the files slatty installed and keeps everything else. The Wine prefix (where most
/// saves live) is deleted only on request, after its `users` folder is copied to the backups.
pub fn uninstall(
    db: &Db,
    dirs: &Dirs,
    game_id: &str,
    delete_prefix: bool,
) -> Result<UninstallReport> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let (install, record) = installed_by_slatty(db, dirs, game_id)?;
    let prefix = install.runner.prefix().filter(|_| delete_prefix);
    if let Some(p) = prefix
        && !p.starts_with(dirs.data.join("prefixes"))
    {
        return Err(Error::Refused(format!(
            "{} was not created by slatty; nothing was deleted",
            p.display()
        )));
    }
    let files = record
        .files
        .iter()
        .map(|f| installer::safe_relative(&f.path))
        .collect::<Result<Vec<_>>>()?;

    let mut report = UninstallReport::default();
    let root = &install.path;
    // Validate the whole list before deleting anything: a refused uninstall stays intact.
    for rel in &files {
        installer::refuse_outside(root, &root.join(rel))?;
    }
    // The saves in the prefix are copied first too: a backup that fails deletes nothing.
    if let Some(prefix) = prefix {
        let users = prefix.join("drive_c/users");
        // A game writes its prefix: a link there would have the backup copy where it leads.
        if users.is_dir() && !installer::leads_inside(prefix, &users) {
            return Err(Error::Refused(format!(
                "{} leads out of the prefix through a link; nothing was deleted",
                users.display()
            )));
        }
        if users.is_dir() {
            let backup = dirs
                .data
                .join("backups/prefixes")
                .join(game_id)
                .join(Utc::now().format("%Y%m%d-%H%M%S").to_string());
            copy_dir(&users, &backup)?;
            report.prefix_backup = Some(backup);
        }
    }
    for rel in files {
        let path = root.join(rel);
        installer::refuse_outside(root, &path)?;
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() || m.is_symlink()) {
            std::fs::remove_file(&path)
                .map_err(|e| Error::io(format!("delete {}", path.display()), e))?;
            report.removed_files += 1;
        }
    }
    if root.is_dir() {
        remove_empty_dirs(root)?;
        report.kept = leftovers(root, root);
        if report.kept.is_empty() {
            std::fs::remove_dir(root)
                .map_err(|e| Error::io(format!("delete {}", root.display()), e))?;
            report.folder_removed = true;
        }
    } else {
        report.folder_removed = true;
    }

    if let Some(prefix) = prefix
        && prefix.exists()
    {
        std::fs::remove_dir_all(prefix)
            .map_err(|e| Error::io(format!("delete {}", prefix.display()), e))?;
        report.prefix_removed = true;
    }

    Install::remove(db, game_id)?;
    InstallJob::delete(db, game_id)?;
    crate::settings::set_held_back(db, game_id, false)?;
    let _ = std::fs::remove_file(InstallRecord::file(dirs, game_id));
    let _ = std::fs::remove_dir_all(installer::support_dir(dirs, game_id));
    Ok(report)
}

/// Verifies an installed game against the build it was installed from; with `repair`,
/// bad or missing files are downloaded again. Returns the files that were not right.
#[allow(clippy::too_many_arguments)]
pub async fn check(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
    repair: bool,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<installer::Checked> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let (install, record) = installed_by_slatty(db, dirs, game_id)?;
    if update_pending(db, game_id)?.is_some() {
        return Err(Error::Refused(
            "an update is unfinished; run the update again first".into(),
        ));
    }
    let dlcs = DlcSelection::Only(record.dlcs.clone());
    let plan = installer::plan_for(
        http,
        tokens,
        game_id,
        install.platform,
        Some(&record.language),
        Some(&record.build_id),
        &dlcs,
    )
    .await
    .map_err(installed_build_gone)?;
    if plan.platform == Platform::Linux {
        let source = linux_source(http, tokens, dirs, &plan);
        let set = linux::LinuxSet::new(&plan.linux)?;
        return linux::LinuxDownload {
            source: &source,
            cancel,
            progress,
            free_space: &installer::free_space,
        }
        .check_installed(&set, &install.path, repair)
        .await;
    }
    let source = GogContent::new(http.clone(), tokens.clone(), dirs);
    let set = installer::collect_files(&source, &plan.depots).await?;
    Download {
        source: &source,
        cancel,
        progress,
        free_space: &installer::free_space,
    }
    .check_installed(&set, &install.path, repair)
    .await
}

fn linux_source(
    http: &Client,
    tokens: &Tokens,
    dirs: &Dirs,
    plan: &installer::InstallPlan,
) -> linux::GogInstallers {
    linux::GogInstallers::new(
        http.clone(),
        tokens.clone(),
        Some(dirs),
        plan.linux
            .iter()
            .map(|p| p.installer.downlink.clone())
            .collect(),
    )
}

pub(crate) const UPDATING: &str = "updating";
/// An unfinished change paused on request: the game still waits for it, but it is not resumed
/// by itself.
pub(crate) const UPDATE_PAUSED: &str = "updating-paused";

/// Build id of an update, language or DLC change that started but did not finish.
pub fn update_pending(db: &Db, game_id: &str) -> Result<Option<String>> {
    Ok(InstallJob::load(db, game_id)?
        .filter(InstallJob::is_update)
        .map(|j| j.build_id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheck {
    pub installed_version: String,
    pub available_version: String,
    pub available_build: String,
}

/// The newest public build for the installed language and DLC, or `None` when up to date.
pub async fn check_update(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
) -> Result<Option<UpdateCheck>> {
    let (install, record) = installed_by_slatty(db, dirs, game_id)?;
    // A Linux build's version is in GOG's list of installers: no need to read their files.
    if install.platform == Platform::Linux {
        let offer = linux::offer(http, tokens, game_id).await?;
        let version = linux::pick(&offer.installers, &record.language)
            .map(|i| i.version.clone())
            .ok_or_else(|| Error::Unsupported("GOG no longer offers a Linux build".into()))?;
        let build_id = format!("{}{version}", linux::BUILD_PREFIX);
        return Ok((build_id != record.build_id).then(|| UpdateCheck {
            installed_version: record.version.clone(),
            available_version: version,
            available_build: build_id,
        }));
    }
    // The list of builds is enough to tell, in one request: games are checked by the dozen.
    let builds = crate::galaxy::builds(http, tokens, game_id).await?;
    let newest = installer::newest_public(&builds)
        .ok_or_else(|| Error::Unsupported("GOG no longer offers a Windows build".into()))?;
    Ok((newest.build_id != record.build_id).then(|| UpdateCheck {
        installed_version: record.version.clone(),
        available_version: newest.version_name.clone(),
        available_build: newest.build_id.clone(),
    }))
}

/// Whether a game is held back from updates once `change` is made to a build that is (`newest`)
/// or is not GOG's newest; `None` leaves it as it was.
fn held_after(change: &Change, newest: bool) -> Option<bool> {
    match change {
        Change::Update => Some(false),
        Change::Build(_) => Some(!newest),
        Change::Language(_) | Change::Dlcs(_) => None,
    }
}

/// The games to update without asking: installed by slatty, on the build they were given as the
/// newest (not an older one chosen), with automatic updates on.
pub fn auto_update_candidates(db: &Db, dirs: &Dirs) -> Result<Vec<String>> {
    if !crate::settings::auto_update(db)? {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for install in Install::list(db)? {
        if InstallRecord::load(dirs, &install.game_id)?.is_some()
            && !crate::settings::held_back(db, &install.game_id)?
        {
            ids.push(install.game_id);
        }
    }
    Ok(ids)
}

/// Languages and DLC that can be chosen for an installed game, on its installed build.
pub async fn content_options(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
) -> Result<installer::InstallPlan> {
    let (install, record) = installed_by_slatty(db, dirs, game_id)?;
    let dlcs = DlcSelection::Only(record.dlcs.clone());
    installer::plan_for(
        http,
        tokens,
        game_id,
        install.platform,
        Some(&record.language),
        Some(&record.build_id),
        &dlcs,
    )
    .await
    .map_err(installed_build_gone)
}

fn installed_build_gone(e: Error) -> Error {
    match e {
        Error::NotFound(m) if m.contains("no longer offered") => {
            Error::Refused("the installed build is no longer offered; update the game first".into())
        }
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Update,
    /// Another build GOG offers, older or newer, with the installed language and DLC.
    Build(String),
    Language(String),
    Dlcs(Vec<String>),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UpdateReport {
    pub from_version: String,
    pub to_version: String,
    pub downloaded: Vec<PathBuf>,
    /// Bytes of changed files copied from their installed version instead of downloaded.
    pub reused_bytes: u64,
    /// Files rebuilt from GOG's binary patches, and the size of those patches.
    pub patched: Vec<PathBuf>,
    pub patch_bytes: u64,
    pub removed: Vec<PathBuf>,
    /// An earlier unfinished change was completed instead of the requested one.
    pub resumed: bool,
}

/// Changes an installed game in place (newer build, other language, other DLC): files that are
/// missing or differ are downloaded and atomically replaced, files slatty installed that are no
/// longer needed are removed, and nothing else in the folder is touched. An unfinished change is
/// always completed first, with the build, language and DLC it started with.
#[allow(clippy::too_many_arguments)]
pub async fn reconfigure(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
    change: Change,
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<UpdateReport> {
    let _busy = crate::lock::game(dirs, game_id)?;
    let (install, record) = installed_by_slatty(db, dirs, game_id)?;
    let pending = InstallJob::load(db, game_id)?.filter(InstallJob::is_update);
    let (build, language, dlcs) = match (&pending, &change) {
        (Some(j), _) => (Some(j.build_id.clone()), j.language.clone(), j.dlcs.clone()),
        (None, Change::Update) => (None, record.language.clone(), record.dlcs.clone()),
        (None, Change::Build(b)) => (
            Some(b.clone()),
            record.language.clone(),
            record.dlcs.clone(),
        ),
        (None, Change::Language(l)) => (
            Some(record.build_id.clone()),
            l.clone(),
            record.dlcs.clone(),
        ),
        (None, Change::Dlcs(d)) => (
            Some(record.build_id.clone()),
            record.language.clone(),
            d.clone(),
        ),
    };
    let plan = installer::plan_for(
        http,
        tokens,
        game_id,
        install.platform,
        Some(&language),
        build.as_deref(),
        &DlcSelection::Only(dlcs),
    )
    .await
    .map_err(installed_build_gone)?;
    let mut selected = plan.selected_dlcs();
    selected.sort();
    let mut current = record.dlcs.clone();
    current.sort();
    if pending.is_none()
        && plan.build.build_id == record.build_id
        && plan.language.eq_ignore_ascii_case(&record.language)
        && selected == current
    {
        return Err(Error::Refused(match change {
            Change::Update => format!("{} is already up to date", install.title),
            _ => "nothing to change".into(),
        }));
    }
    // Noted as the change starts, so a change cut off and finished later keeps it: an older build
    // chosen is not updated without asking; the newest is. A language or DLC change leaves it.
    if pending.is_none()
        && let Some(held) = held_after(&change, plan.is_newest())
    {
        crate::settings::set_held_back(db, game_id, held)?;
    }
    InstallJob {
        game_id: game_id.to_string(),
        build_id: plan.build.build_id.clone(),
        language: plan.language.clone(),
        root: install
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
        directory: install
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        state: UPDATING.into(),
        dlcs: selected.clone(),
    }
    .save(db)?;
    let (applied, files) = if plan.platform == Platform::Linux {
        let source = linux_source(http, tokens, dirs, &plan);
        let set = linux::LinuxSet::new(&plan.linux)?;
        let dl = linux::LinuxDownload {
            source: &source,
            cancel,
            progress,
            free_space: &installer::free_space,
        };
        let applied = async {
            let checked = dl.check_installed(&set, &install.path, true).await?;
            let kept: HashSet<&Path> = set
                .files
                .iter()
                .chain(&set.links)
                .map(|f| f.path.as_path())
                .collect();
            let removed = remove_obsolete(&record, &install.path, |rel| kept.contains(rel))?;
            Ok::<_, Error>(UpdateReport {
                downloaded: checked.bad,
                removed,
                ..Default::default()
            })
        }
        .await;
        (applied, set.recorded_files())
    } else {
        windows_reconfigure(
            dirs, http, tokens, game_id, &plan, &record, &install, &selected, progress, cancel,
        )
        .await?
    };
    let report = match applied {
        Err(Error::Cancelled) => {
            if let Some(mut job) = InstallJob::load(db, game_id)? {
                job.state = UPDATE_PAUSED.into();
                job.save(db)?;
            }
            return Err(Error::Cancelled);
        }
        other => other?,
    };
    InstallRecord {
        build_id: plan.build.build_id.clone(),
        version: plan.build.version_name.clone(),
        language: plan.language.clone(),
        path: Some(install.path.clone()),
        dlcs: selected,
        setup_build: None,
        files,
    }
    .save(dirs, game_id)?;
    InstallJob::delete(db, game_id)?;
    Ok(UpdateReport {
        from_version: record.version,
        to_version: plan.build.version_name,
        resumed: pending.is_some(),
        ..report
    })
}

/// The Windows part of `reconfigure`: GOG's binary patches, then the files that still differ.
/// Returns what was applied (or why not) and the files the game now has.
#[allow(clippy::too_many_arguments)]
async fn windows_reconfigure(
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
    plan: &installer::InstallPlan,
    record: &InstallRecord,
    install: &Install,
    selected: &[String],
    progress: &(dyn Fn(Progress) + Send + Sync),
    cancel: CancellationToken,
) -> Result<(Result<UpdateReport>, Vec<installer::RecordedFile>)> {
    let source = GogContent::new(http.clone(), tokens.clone(), dirs);
    let set = installer::collect_files(&source, &plan.depots).await?;
    let dl = Download {
        source: &source,
        cancel,
        progress,
        free_space: &installer::free_space,
    };
    let patches = if plan.build.build_id == record.build_id {
        Vec::new()
    } else {
        let mut products = vec![game_id.to_string()];
        products.extend(selected.iter().cloned());
        crate::patches::find(
            http,
            tokens,
            game_id,
            &record.build_id,
            &plan.build.build_id,
            &plan.language,
            &products,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("no binary patch used: {e}");
            None
        })
        .unwrap_or_default()
    };
    let applied = async {
        let report = apply_update(&dl, &set, record, &install.path, &patches).await?;
        dl.check_installed(
            &set.support_set(),
            &installer::support_dir(dirs, game_id),
            true,
        )
        .await?;
        Ok::<_, Error>(report)
    }
    .await;
    Ok((applied, installer::recorded_files(&set)))
}

pub(crate) async fn apply_update<S: crate::galaxy::ContentSource>(
    dl: &Download<'_, S>,
    set: &installer::FileSet,
    old: &InstallRecord,
    dir: &Path,
    patches: &[crate::patches::FilePatch],
) -> Result<UpdateReport> {
    let patched = crate::patches::apply_all(dl.source, patches, set, old, dir, &dl.cancel).await?;
    let checked = dl.check_installed(set, dir, true).await?;
    // Windows sees differently cased paths as one file.
    let kept: HashSet<String> = set
        .files
        .iter()
        .map(|(p, _)| p.to_string_lossy().to_lowercase())
        .collect();
    let removed = remove_obsolete(old, dir, |rel| {
        kept.contains(&rel.to_string_lossy().to_lowercase())
    })?;
    Ok(UpdateReport {
        downloaded: checked.bad,
        reused_bytes: checked.reused_bytes,
        patched: patched.files,
        patch_bytes: patched.delta_bytes,
        removed,
        ..Default::default()
    })
}

/// Deletes the files slatty installed before that the game no longer has, and the folders they
/// leave empty.
fn remove_obsolete(
    old: &InstallRecord,
    dir: &Path,
    kept: impl Fn(&Path) -> bool,
) -> Result<Vec<PathBuf>> {
    let files = old
        .files
        .iter()
        .map(|f| installer::safe_relative(&f.path))
        .collect::<Result<Vec<_>>>()?;
    let files: Vec<_> = files.into_iter().filter(|rel| !kept(rel)).collect();
    for rel in &files {
        installer::refuse_outside(dir, &dir.join(rel))?;
    }
    let mut removed = Vec::new();
    for rel in files {
        let path = dir.join(&rel);
        installer::refuse_outside(dir, &path)?;
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file() || m.is_symlink()) {
            std::fs::remove_file(&path)
                .map_err(|e| Error::io(format!("delete {}", path.display()), e))?;
            removed.push(rel.clone());
            let mut parent = path.parent();
            while let Some(p) = parent.filter(|p| *p != dir) {
                if std::fs::remove_dir(p).is_err() {
                    break;
                }
                parent = p.parent();
            }
        }
    }
    Ok(removed)
}

fn remove_empty_dirs(dir: &Path) -> Result<bool> {
    let mut empty = true;
    for entry in
        std::fs::read_dir(dir).map_err(|e| Error::io(format!("list {}", dir.display()), e))?
    {
        let entry = entry.map_err(|e| Error::io(format!("list {}", dir.display()), e))?;
        let ft = entry.file_type().map_err(|e| Error::io("stat", e))?;
        if ft.is_dir() && remove_empty_dirs(&entry.path())? {
            std::fs::remove_dir(entry.path())
                .map_err(|e| Error::io(format!("delete {}", entry.path().display()), e))?;
        } else {
            empty = false;
        }
    }
    Ok(empty)
}

fn leftovers(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            out.extend(leftovers(root, &path));
        } else {
            out.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
    }
    out.sort();
    out
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    crate::paths::ensure_dir(dst)?;
    for entry in
        std::fs::read_dir(src).map_err(|e| Error::io(format!("list {}", src.display()), e))?
    {
        let entry = entry.map_err(|e| Error::io(format!("list {}", src.display()), e))?;
        let ft = entry.file_type().map_err(|e| Error::io("stat", e))?;
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), &to)
                .map_err(|e| Error::io(format!("copy {}", entry.path().display()), e))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::Platform;
    use crate::installer::RecordedFile;
    use crate::runner::Runner;

    struct Env {
        root: PathBuf,
        dirs: Dirs,
        db: Db,
    }

    impl Env {
        fn new(name: &str, record: bool) -> Env {
            let root =
                std::env::temp_dir().join(format!("slatty-maint-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let dirs = Dirs::under(&root.join("app"));
            let db = Db::in_memory().unwrap();
            let game = root.join("games/Game");
            std::fs::create_dir_all(game.join("data")).unwrap();
            std::fs::create_dir_all(game.join("saves")).unwrap();
            std::fs::write(game.join("Game.exe"), b"exe").unwrap();
            std::fs::write(game.join("data/a.pak"), b"pak").unwrap();
            std::fs::write(game.join("saves/slot1.sav"), b"my progress").unwrap();
            let prefix = dirs.data.join("prefixes/1");
            std::fs::create_dir_all(prefix.join("drive_c/users/steamuser/Documents")).unwrap();
            std::fs::write(
                prefix.join("drive_c/users/steamuser/Documents/save.dat"),
                b"prefix save",
            )
            .unwrap();
            Install {
                umu_id: None,
                isolated: false,
                game_id: "1".into(),
                title: "[FAKE] Game".into(),
                platform: Platform::Windows,
                path: game.clone(),
                client_id: None,
                runner: Runner::Umu {
                    proton: "/p".into(),
                    prefix,
                },
            }
            .save(&db)
            .unwrap();
            if record {
                InstallRecord {
                    build_id: "b".into(),
                    version: "1".into(),
                    language: "en-US".into(),
                    path: Some(game),
                    dlcs: vec![],
                    setup_build: None,
                    files: ["Game.exe", "data/a.pak"]
                        .iter()
                        .map(|p| RecordedFile {
                            path: p.to_string(),
                            size: 3,
                        })
                        .collect(),
                }
                .save(&dirs, "1")
                .unwrap();
            }
            Env { root, dirs, db }
        }

        fn game(&self) -> PathBuf {
            self.root.join("games/Game")
        }
    }

    #[test]
    fn uninstall_refuses_an_escaping_parent_before_deleting_any_file() {
        let env = Env::new("escaping-uninstall", true);
        let outside = env.root.join("outside");
        std::fs::rename(env.game().join("data"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, env.game().join("data")).unwrap();
        let result = uninstall(&env.db, &env.dirs, "1", false);
        assert!(matches!(result, Err(Error::Refused(_))));
        assert!(env.game().join("Game.exe").exists());
        assert_eq!(std::fs::read(outside.join("a.pak")).unwrap(), b"pak");
        assert!(Install::get(&env.db, "1").unwrap().is_some());
    }

    #[test]
    fn obsolete_files_cannot_be_deleted_through_an_escaping_parent() {
        let env = Env::new("escaping-obsolete", true);
        let outside = env.root.join("outside");
        std::fs::rename(env.game().join("data"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, env.game().join("data")).unwrap();
        let record = InstallRecord::load(&env.dirs, "1").unwrap().unwrap();
        assert!(remove_obsolete(&record, &env.game(), |_| false).is_err());
        assert!(env.game().join("Game.exe").exists());
        assert_eq!(std::fs::read(outside.join("a.pak")).unwrap(), b"pak");
    }

    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn removes_installed_files_but_keeps_saves_in_the_game_folder() {
        let env = Env::new("keep", true);
        let r = uninstall(&env.db, &env.dirs, "1", false).unwrap();
        assert_eq!(r.removed_files, 2);
        assert_eq!(r.kept, vec![PathBuf::from("saves/slot1.sav")]);
        assert!(!r.folder_removed);
        assert_eq!(
            std::fs::read(env.game().join("saves/slot1.sav")).unwrap(),
            b"my progress"
        );
        assert!(!env.game().join("data").exists());
        assert!(
            env.dirs.data.join("prefixes/1/drive_c").exists(),
            "prefix kept by default"
        );
        assert!(Install::get(&env.db, "1").unwrap().is_none());
    }

    #[test]
    fn deleting_the_prefix_backs_up_its_user_folder_first() {
        let env = Env::new("prefix", true);
        std::fs::remove_dir_all(env.game().join("saves")).unwrap();
        let r = uninstall(&env.db, &env.dirs, "1", true).unwrap();
        assert!(r.folder_removed && r.prefix_removed);
        let backup = r.prefix_backup.unwrap();
        assert_eq!(
            std::fs::read(backup.join("steamuser/Documents/save.dat")).unwrap(),
            b"prefix save"
        );
        assert!(!env.dirs.data.join("prefixes/1").exists());
    }

    /// The prefix's user folder is a link out of the prefix (a game writes its prefix): the
    /// backup would copy where it leads, and the uninstall stops before deleting anything.
    #[test]
    fn a_prefix_whose_user_folder_leads_out_of_it_is_not_backed_up_nor_deleted() {
        let env = Env::new("prefix-link", true);
        let prefix = env.dirs.data.join("prefixes/1");
        let outside = env.root.join("outside");
        std::fs::rename(prefix.join("drive_c/users"), &outside).unwrap();
        std::os::unix::fs::symlink(&outside, prefix.join("drive_c/users")).unwrap();
        let result = uninstall(&env.db, &env.dirs, "1", true);
        assert!(matches!(result, Err(Error::Refused(_))), "{result:?}");
        assert!(!env.dirs.data.join("backups").exists());
        assert!(env.game().join("Game.exe").exists());
        assert!(prefix.join("drive_c").exists());
        assert!(Install::get(&env.db, "1").unwrap().is_some());
    }

    #[test]
    fn games_on_their_newest_build_are_updated_without_asking_not_older_ones_chosen() {
        let env = Env::new("auto", true);
        // Not installed by slatty (no record): never.
        Install {
            game_id: "2".into(),
            title: "[FAKE] Imported".into(),
            ..Install::get(&env.db, "1").unwrap().unwrap()
        }
        .save(&env.db)
        .unwrap();
        assert_eq!(auto_update_candidates(&env.db, &env.dirs).unwrap(), ["1"]);
        crate::settings::set_held_back(&env.db, "1", true).unwrap();
        assert!(
            auto_update_candidates(&env.db, &env.dirs)
                .unwrap()
                .is_empty()
        );
        crate::settings::set_held_back(&env.db, "1", false).unwrap();
        crate::settings::set_auto_update(&env.db, false).unwrap();
        assert!(
            auto_update_candidates(&env.db, &env.dirs)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn choosing_an_older_build_holds_a_game_back_and_updating_releases_it() {
        assert_eq!(held_after(&Change::Build("old".into()), false), Some(true));
        assert_eq!(held_after(&Change::Build("new".into()), true), Some(false));
        assert_eq!(held_after(&Change::Update, true), Some(false));
        // A language or DLC change on the build installed leaves the choice as it was, even
        // when GOG has a newer build by then.
        assert_eq!(held_after(&Change::Language("fr".into()), false), None);
        assert_eq!(held_after(&Change::Dlcs(vec![]), false), None);
    }

    #[test]
    fn an_uninstalled_game_is_no_longer_held_back() {
        let env = Env::new("held", true);
        crate::settings::set_held_back(&env.db, "1", true).unwrap();
        uninstall(&env.db, &env.dirs, "1", false).unwrap();
        assert!(!crate::settings::held_back(&env.db, "1").unwrap());
    }

    #[test]
    fn the_home_folder_of_an_isolated_game_outlives_its_uninstall() {
        let env = Env::new("home", true);
        let home = crate::runner::isolated_home(&env.dirs, "1").unwrap();
        std::fs::create_dir_all(home.join(".local/share/Game")).unwrap();
        std::fs::write(home.join(".local/share/Game/save"), b"linux save").unwrap();
        uninstall(&env.db, &env.dirs, "1", true).unwrap();
        assert_eq!(
            std::fs::read(home.join(".local/share/Game/save")).unwrap(),
            b"linux save"
        );
    }

    /// The prefix's user folder cannot be backed up (a full disk, here a file in the way): the
    /// uninstall stops before deleting anything, rather than leaving the game half removed.
    #[test]
    fn a_prefix_that_cannot_be_backed_up_leaves_the_game_installed() {
        let env = Env::new("prefix-backup-fails", true);
        std::fs::create_dir_all(env.dirs.data.join("backups")).unwrap();
        std::fs::write(env.dirs.data.join("backups/prefixes"), b"in the way").unwrap();
        assert!(uninstall(&env.db, &env.dirs, "1", true).is_err());
        assert!(env.game().join("Game.exe").exists());
        assert!(env.game().join("data/a.pak").exists());
        assert!(env.dirs.data.join("prefixes/1/drive_c/users").is_dir());
        assert!(Install::get(&env.db, "1").unwrap().is_some());
    }

    #[test]
    fn games_not_installed_by_slatty_are_never_deleted() {
        let env = Env::new("imported", false);
        assert!(matches!(
            uninstall(&env.db, &env.dirs, "1", true),
            Err(Error::Refused(_))
        ));
        assert!(env.game().join("Game.exe").exists());
        assert!(Install::get(&env.db, "1").unwrap().is_some());
    }

    #[test]
    fn running_game_cannot_be_uninstalled() {
        let env = Env::new("running", true);
        session::record_start(&env.db, "1", None).unwrap();
        assert!(matches!(
            uninstall(&env.db, &env.dirs, "1", false),
            Err(Error::Refused(_))
        ));
        assert!(env.game().join("Game.exe").exists());
    }

    #[test]
    fn a_game_busy_with_another_operation_is_left_alone() {
        let env = Env::new("busy", true);
        let busy = crate::lock::game(&env.dirs, "1").unwrap();
        assert!(matches!(
            uninstall(&env.db, &env.dirs, "1", false),
            Err(Error::Refused(_))
        ));
        assert!(env.game().join("Game.exe").exists());
        drop(busy);
        uninstall(&env.db, &env.dirs, "1", false).unwrap();
    }

    #[test]
    fn foreign_prefix_is_kept_even_when_asked() {
        let env = Env::new("foreign", true);
        let mut install = Install::get(&env.db, "1").unwrap().unwrap();
        let foreign = env.root.join("heroic-prefix");
        std::fs::create_dir_all(&foreign).unwrap();
        install.runner = Runner::Umu {
            proton: "/p".into(),
            prefix: foreign.clone(),
        };
        install.save(&env.db).unwrap();
        assert!(uninstall(&env.db, &env.dirs, "1", true).is_err());
        assert!(foreign.exists());
        assert!(
            env.game().join("Game.exe").exists(),
            "a refusal must not leave a half-done uninstall"
        );
    }
}
