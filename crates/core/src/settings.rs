use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::error::Result;
use crate::install::Platform;

const LIBRARY_ROOT: &str = "library_root";
const DEFAULT_PROTON: &str = "default_proton";
const FAVORITES: &str = "favorites";
const INTERFACE_FONT: &str = "interface_font";
const INTERFACE_THEME: &str = "interface_theme";
const COVER_WIDTH: &str = "cover_width";
const LIBRARY_SORT: &str = "library_sort";
const DEFAULT_PLATFORM: &str = "default_platform";
const DOWNLOAD_QUEUE: &str = "download_queue";
const UMU_LOOKUP: &str = "umu_lookup";
const REPORT_PLAYTIME: &str = "report_playtime";
const GAME_ACHIEVEMENTS: &str = "game_achievements";
const MANUAL_ACHIEVEMENTS: &str = "manual_achievements";
const STEAMGRIDDB: &str = "steamgriddb";
const ISOLATE_NEW_GAMES: &str = "isolate_new_games";
const PROTON_DOWNLOADS: &str = "proton_downloads";
const PROTON_UPDATES: &str = "proton_updates";
const PROTON_CHECKED_AT: &str = "proton_checked_at";
const SHARE_PROTONS: &str = "share_protons";
const AUTO_UPDATE: &str = "auto_update";
const VULKAN_ONLY: &str = "vulkan_only";

/// Folder that receives installed games (`~/Games/GOG` until chosen).
pub fn library_root(db: &Db) -> Result<PathBuf> {
    Ok(match db.setting(LIBRARY_ROOT)? {
        Some(p) => PathBuf::from(p),
        None => home().join("Games/GOG"),
    })
}

pub fn set_library_root(db: &Db, path: &Path) -> Result<()> {
    db.set_setting(LIBRARY_ROOT, Some(&path.to_string_lossy()))
}

pub fn default_proton(db: &Db) -> Result<Option<PathBuf>> {
    Ok(db.setting(DEFAULT_PROTON)?.map(PathBuf::from))
}

pub fn set_default_proton(db: &Db, path: &Path) -> Result<()> {
    db.set_setting(DEFAULT_PROTON, Some(&path.to_string_lossy()))
}

/// Build installed when a game has both: Windows until chosen.
pub fn default_platform(db: &Db) -> Result<Platform> {
    Ok(match db.setting(DEFAULT_PLATFORM)?.as_deref() {
        Some("linux") => Platform::Linux,
        _ => Platform::Windows,
    })
}

pub fn set_default_platform(db: &Db, platform: Platform) -> Result<()> {
    let value = match platform {
        Platform::Linux => "linux",
        Platform::Windows => "windows",
    };
    db.set_setting(DEFAULT_PLATFORM, Some(value))
}

/// Whether a game's GOG id may be sent to umu's public database, at its first launch, to pick
/// its Proton fixes. On until turned off.
pub fn umu_lookup(db: &Db) -> Result<bool> {
    Ok(db.setting(UMU_LOOKUP)?.as_deref() != Some("off"))
}

pub fn set_umu_lookup(db: &Db, on: bool) -> Result<()> {
    db.set_setting(UMU_LOOKUP, Some(if on { "on" } else { "off" }))
}

/// Whether play sessions are sent to GOG, so they count in the play time of its profile. On until
/// turned off.
pub fn report_playtime(db: &Db) -> Result<bool> {
    Ok(db.setting(REPORT_PLAYTIME)?.as_deref() != Some("off"))
}

pub fn set_report_playtime(db: &Db, on: bool) -> Result<()> {
    db.set_setting(REPORT_PLAYTIME, Some(if on { "on" } else { "off" }))
}

/// Whether Windows games installed from now on run isolated from the user's files (each game's
/// own setting changes it later). On until turned off.
pub fn isolate_new_games(db: &Db) -> Result<bool> {
    Ok(db.setting(ISOLATE_NEW_GAMES)?.as_deref() != Some("off"))
}

pub fn set_isolate_new_games(db: &Db, on: bool) -> Result<()> {
    db.set_setting(ISOLATE_NEW_GAMES, Some(if on { "on" } else { "off" }))
}

/// Whether Comet is started while a game that uses GOG's Galaxy runs, so achievements unlocked in
/// the game reach GOG. On until turned off.
pub fn game_achievements(db: &Db) -> Result<bool> {
    Ok(db.setting(GAME_ACHIEVEMENTS)?.as_deref() != Some("off"))
}

pub fn set_game_achievements(db: &Db, on: bool) -> Result<()> {
    db.set_setting(GAME_ACHIEVEMENTS, Some(if on { "on" } else { "off" }))
}

/// Whether the interface offers to unlock and clear achievements by hand. Off until turned on.
pub fn manual_achievements(db: &Db) -> Result<bool> {
    Ok(db.setting(MANUAL_ACHIEVEMENTS)?.as_deref() == Some("on"))
}

pub fn set_manual_achievements(db: &Db, on: bool) -> Result<()> {
    db.set_setting(MANUAL_ACHIEVEMENTS, Some(if on { "on" } else { "off" }))
}

/// Whether Edit game offers covers and backgrounds from SteamGridDB. Off until turned on.
pub fn steamgriddb(db: &Db) -> Result<bool> {
    Ok(db.setting(STEAMGRIDDB)?.as_deref() == Some("on"))
}

pub fn set_steamgriddb(db: &Db, on: bool) -> Result<()> {
    db.set_setting(STEAMGRIDDB, Some(if on { "on" } else { "off" }))
}

/// Whether Proton builds may be listed and downloaded from GitHub. Off until turned on.
pub fn proton_downloads(db: &Db) -> Result<bool> {
    Ok(db.setting(PROTON_DOWNLOADS)?.as_deref() == Some("on"))
}

pub fn set_proton_downloads(db: &Db, on: bool) -> Result<()> {
    db.set_setting(PROTON_DOWNLOADS, Some(if on { "on" } else { "off" }))
}

/// Whether the Proton builds games follow as `-latest` are updated at start, at most once a day.
/// Off until turned on.
pub fn proton_updates(db: &Db) -> Result<bool> {
    Ok(db.setting(PROTON_UPDATES)?.as_deref() == Some("on"))
}

pub fn set_proton_updates(db: &Db, on: bool) -> Result<()> {
    db.set_setting(PROTON_UPDATES, Some(if on { "on" } else { "off" }))
}

/// Whether the downloaded Proton builds are linked into Steam's `compatibilitytools.d`, where
/// Steam and Heroic find them. Off until turned on.
pub fn share_protons(db: &Db) -> Result<bool> {
    Ok(db.setting(SHARE_PROTONS)?.as_deref() == Some("on"))
}

pub fn set_share_protons(db: &Db, on: bool) -> Result<()> {
    db.set_setting(SHARE_PROTONS, Some(if on { "on" } else { "off" }))
}

/// When GitHub was last asked for newer Proton builds, in seconds since the epoch.
pub fn proton_checked_at(db: &Db) -> Result<Option<i64>> {
    Ok(db.setting(PROTON_CHECKED_AT)?.and_then(|v| v.parse().ok()))
}

pub fn set_proton_checked_at(db: &Db, at: i64) -> Result<()> {
    db.set_setting(PROTON_CHECKED_AT, Some(&at.to_string()))
}

/// Whether installed games on their newest build are updated without asking. On until turned off.
pub fn auto_update(db: &Db) -> Result<bool> {
    Ok(db.setting(AUTO_UPDATE)?.as_deref() != Some("off"))
}

pub fn set_auto_update(db: &Db, on: bool) -> Result<()> {
    db.set_setting(AUTO_UPDATE, Some(if on { "on" } else { "off" }))
}

/// Whether the interface starts its graphics with Vulkan alone: found, at the last start, to be
/// what it would pick among all its backends. Off until found.
pub fn vulkan_only(db: &Db) -> Result<bool> {
    Ok(db.setting(VULKAN_ONLY)?.as_deref() == Some("on"))
}

pub fn set_vulkan_only(db: &Db, on: bool) -> Result<()> {
    db.set_setting(VULKAN_ONLY, Some(if on { "on" } else { "off" }))
}

/// Whether notifications of a kind (`downloads`, `updates`, `achievements`, `cloud`, or `all`, the
/// switch for every kind) are shown: on until turned off.
pub fn notifications(db: &Db, kind: &str) -> Result<bool> {
    Ok(db.setting(&format!("notifications:{kind}"))?.as_deref() != Some("off"))
}

pub fn set_notifications(db: &Db, kind: &str, on: bool) -> Result<()> {
    db.set_setting(
        &format!("notifications:{kind}"),
        Some(if on { "on" } else { "off" }),
    )
}

/// Whether the user chose a build of the game older than GOG's newest: it is then not updated
/// without asking.
pub fn held_back(db: &Db, game_id: &str) -> Result<bool> {
    Ok(db.setting(&format!("held_back:{game_id}"))?.is_some())
}

pub fn set_held_back(db: &Db, game_id: &str, held: bool) -> Result<()> {
    db.set_setting(&format!("held_back:{game_id}"), held.then_some("on"))
}

/// The way a game is started, among its launch options, once chosen.
pub fn launch_choice(db: &Db, game_id: &str) -> Result<Option<String>> {
    db.setting(&format!("launch_task:{game_id}"))
}

pub fn set_launch_choice(db: &Db, game_id: &str, choice: &str) -> Result<()> {
    db.set_setting(&format!("launch_task:{game_id}"), Some(choice))
}

/// Game ids waiting in the download queue, in order.
pub fn download_queue(db: &Db) -> Result<Vec<String>> {
    Ok(db
        .setting(DOWNLOAD_QUEUE)?
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default())
}

pub fn set_download_queue(db: &Db, ids: &[String]) -> Result<()> {
    db.set_setting(DOWNLOAD_QUEUE, Some(&ids.join(",")))
}

/// Font family the interface uses; `None` for the system's default.
pub fn interface_font(db: &Db) -> Result<Option<String>> {
    db.setting(INTERFACE_FONT)
}

pub fn set_interface_font(db: &Db, family: Option<&str>) -> Result<()> {
    db.set_setting(INTERFACE_FONT, family)
}

/// Width of the covers in the library grid, in pixels.
pub fn cover_width(db: &Db) -> Result<Option<f32>> {
    Ok(db.setting(COVER_WIDTH)?.and_then(|v| v.parse().ok()))
}

pub fn set_cover_width(db: &Db, width: f32) -> Result<()> {
    db.set_setting(COVER_WIDTH, Some(&width.round().to_string()))
}

/// Order of the library grid, by name.
pub fn library_sort(db: &Db) -> Result<Option<String>> {
    db.setting(LIBRARY_SORT)
}

pub fn set_library_sort(db: &Db, sort: &str) -> Result<()> {
    db.set_setting(LIBRARY_SORT, Some(sort))
}

/// Built-in theme the interface starts from, by name; `None` for its own.
pub fn interface_theme(db: &Db) -> Result<Option<String>> {
    db.setting(INTERFACE_THEME)
}

pub fn set_interface_theme(db: &Db, name: &str) -> Result<()> {
    db.set_setting(INTERFACE_THEME, Some(name))
}

/// Game ids marked as favorites.
pub fn favorites(db: &Db) -> Result<Vec<String>> {
    Ok(db
        .setting(FAVORITES)?
        .map(|v| {
            v.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default())
}

pub fn set_favorites(db: &Db, ids: &[String]) -> Result<()> {
    db.set_setting(FAVORITES, Some(&ids.join(",")))
}

/// Proton builds that can run games: those SlattyLauncher downloaded, custom ones in Steam's
/// `compatibilitytools.d`, Valve's own (Proton Experimental, stable, Hotfix) in every Steam
/// library, and those umu downloaded. A build found twice (`~/.steam/steam` is the same folder, or
/// the same name in two places) is listed once.
pub fn proton_candidates(dirs: &crate::paths::Dirs) -> Vec<PathBuf> {
    proton_candidates_in(&crate::protons::dir(dirs), &home())
}

fn proton_candidates_in(downloaded: &Path, home: &Path) -> Vec<PathBuf> {
    let mut places = Vec::new();
    for steam in steam_roots(home) {
        places.push(steam.join("compatibilitytools.d"));
    }
    for steam in steam_roots(home) {
        places.extend(
            steam_libraries(&steam)
                .into_iter()
                .map(|l| l.join("steamapps/common")),
        );
    }
    places.push(home.join(".local/share/umu/compatibilitytools"));

    let builds_in = |place: &Path| {
        let mut builds: Vec<PathBuf> = std::fs::read_dir(place)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .filter(|p| p.join("proton").is_file())
            .collect();
        builds.sort();
        builds
    };
    // Downloaded builds each, even the one a `-latest` link leads to, which is listed first.
    let mut found = builds_in(downloaded);
    let mut seen_dirs: std::collections::HashSet<PathBuf> = found
        .iter()
        .map(|b| b.canonicalize().unwrap_or_else(|_| b.clone()))
        .collect();
    let mut seen_names: std::collections::HashSet<_> = found
        .iter()
        .map(|b| b.file_name().map(|n| n.to_owned()))
        .collect();
    for place in places {
        for build in builds_in(&place) {
            let real = build.canonicalize().unwrap_or_else(|_| build.clone());
            if seen_dirs.insert(real) && seen_names.insert(build.file_name().map(|n| n.to_owned()))
            {
                found.push(build);
            }
        }
    }
    found
}

/// Steam's client library folders (`linux64`, `linux32`, each with a `steamclient.so`), which
/// Valve's Proton builds need, where Steam is installed.
pub fn steam_client_libs() -> Option<[PathBuf; 2]> {
    steam_client_libs_in(&home())
}

fn steam_client_libs_in(home: &Path) -> Option<[PathBuf; 2]> {
    steam_roots(home)
        .into_iter()
        .map(|steam| [steam.join("linux64"), steam.join("linux32")])
        .find(|libs| libs.iter().all(|l| l.join("steamclient.so").is_file()))
}

/// Steam's `compatibilitytools.d`, where Steam and Heroic look for custom Proton builds, when
/// Steam is installed natively. The Flatpak's cannot see a folder of this user's.
pub fn shared_protons_dir() -> Option<PathBuf> {
    shared_protons_dir_in(&home())
}

fn shared_protons_dir_in(home: &Path) -> Option<PathBuf> {
    [".local/share/Steam", ".steam/steam"]
        .iter()
        .map(|p| home.join(p))
        .find(|p| p.is_dir())
        .map(|p| p.join("compatibilitytools.d"))
}

/// Where Steam keeps its data: native, the `~/.steam/steam` link, and the Flatpak.
fn steam_roots(home: &Path) -> Vec<PathBuf> {
    [
        ".local/share/Steam",
        ".steam/steam",
        ".var/app/com.valvesoftware.Steam/data/Steam",
    ]
    .iter()
    .map(|p| home.join(p))
    .filter(|p| p.is_dir())
    .collect()
}

/// Library folders listed in Steam's `libraryfolders.vdf` (the main one included).
fn steam_libraries(steam: &Path) -> Vec<PathBuf> {
    let vdf =
        std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
    let mut libraries = vec![steam.to_path_buf()];
    for line in vdf.lines() {
        let mut quoted = line.split('"').skip(1).step_by(2);
        if quoted.next() == Some("path")
            && let Some(path) = quoted.next()
        {
            libraries.push(PathBuf::from(path.replace("\\\\", "\\")));
        }
    }
    libraries
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_are_on_until_turned_off_kind_by_kind() {
        let db = Db::in_memory().unwrap();
        assert!(notifications(&db, "all").unwrap());
        assert!(notifications(&db, "downloads").unwrap());
        set_notifications(&db, "downloads", false).unwrap();
        assert!(!notifications(&db, "downloads").unwrap());
        assert!(notifications(&db, "updates").unwrap(), "the others stay on");
        set_notifications(&db, "downloads", true).unwrap();
        assert!(notifications(&db, "downloads").unwrap());
    }

    #[test]
    fn graphics_start_with_every_backend_until_vulkan_is_found() {
        let db = Db::in_memory().unwrap();
        assert!(!vulkan_only(&db).unwrap());
        set_vulkan_only(&db, true).unwrap();
        assert!(vulkan_only(&db).unwrap());
        set_vulkan_only(&db, false).unwrap();
        assert!(!vulkan_only(&db).unwrap());
    }

    fn build(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("proton"), b"").unwrap();
    }

    #[test]
    fn steam_gives_its_client_libraries_and_its_custom_builds_folder() {
        let home = std::env::temp_dir().join(format!("slatty-steam-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(steam_client_libs_in(&home), None);
        assert_eq!(shared_protons_dir_in(&home), None);
        let steam = home.join(".local/share/Steam");
        std::fs::create_dir_all(steam.join("linux64")).unwrap();
        assert_eq!(steam_client_libs_in(&home), None, "both libraries or none");
        assert_eq!(
            shared_protons_dir_in(&home),
            Some(steam.join("compatibilitytools.d"))
        );
        std::fs::create_dir_all(steam.join("linux32")).unwrap();
        std::fs::write(steam.join("linux64/steamclient.so"), b"").unwrap();
        std::fs::write(steam.join("linux32/steamclient.so"), b"").unwrap();
        assert_eq!(
            steam_client_libs_in(&home),
            Some([steam.join("linux64"), steam.join("linux32")])
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn proton_builds_come_from_steam_libraries_and_umu_once_each() {
        let home = std::env::temp_dir().join(format!("slatty-protons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let steam = home.join(".local/share/Steam");
        let nas = home.join("games/SteamLibrary");
        build(&steam.join("compatibilitytools.d/GE-Proton"));
        build(&steam.join("steamapps/common/Proton - Experimental"));
        build(&nas.join("steamapps/common/Proton 10.0"));
        std::fs::create_dir_all(nas.join("steamapps/common/Some Game")).unwrap();
        build(&home.join(".local/share/umu/compatibilitytools/GE-Proton"));
        build(&home.join(".local/share/umu/compatibilitytools/UMU-Proton-10.0-4"));
        std::fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
                steam.display(),
                nas.display()
            ),
        )
        .unwrap();
        std::fs::create_dir_all(home.join(".steam")).unwrap();
        std::os::unix::fs::symlink(&steam, home.join(".steam/steam")).unwrap();
        // Downloaded by SlattyLauncher: listed first; a download half done is not a build.
        let downloaded = home.join("slatty/protons");
        build(&downloaded.join("GE-Proton11-7-x86_64"));
        std::fs::create_dir_all(downloaded.join(".staging/GE-Proton11-8-x86_64")).unwrap();
        // The link to the newest is listed, and so is the build it leads to.
        std::os::unix::fs::symlink("GE-Proton11-7-x86_64", downloaded.join("GE-Proton-latest"))
            .unwrap();
        // A link being swapped in when a crash cut it off is not offered.
        std::os::unix::fs::symlink(
            "GE-Proton11-7-x86_64",
            downloaded.join(".GE-Proton-latest.slatty-tmp-1-0"),
        )
        .unwrap();

        let names: Vec<String> = proton_candidates_in(&downloaded, &home)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "GE-Proton-latest",
                "GE-Proton11-7-x86_64",
                "GE-Proton",
                "Proton - Experimental",
                "Proton 10.0",
                "UMU-Proton-10.0-4"
            ]
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
