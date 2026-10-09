use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::error::Result;

const LIBRARY_ROOT: &str = "library_root";
const DEFAULT_PROTON: &str = "default_proton";
const FAVORITES: &str = "favorites";
const INTERFACE_FONT: &str = "interface_font";
const INTERFACE_THEME: &str = "interface_theme";
const COVER_WIDTH: &str = "cover_width";
const LIBRARY_SORT: &str = "library_sort";

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

/// Proton builds already on disk (Steam compatibility tools folder).
/// Proton builds that can run games: custom ones in Steam's `compatibilitytools.d`, Valve's own
/// (Proton Experimental, stable, Hotfix) in every Steam library, and those umu downloaded. A build
/// found twice (`~/.steam/steam` is the same folder, or the same name in two places) is listed once.
pub fn proton_candidates() -> Vec<PathBuf> {
    proton_candidates_in(&home())
}

fn proton_candidates_in(home: &Path) -> Vec<PathBuf> {
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

    let mut seen_dirs = std::collections::HashSet::new();
    let mut seen_names = std::collections::HashSet::new();
    let mut found = Vec::new();
    for place in places {
        let mut builds: Vec<PathBuf> = std::fs::read_dir(&place)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("proton").is_file())
            .collect();
        builds.sort();
        for build in builds {
            let real = build.canonicalize().unwrap_or_else(|_| build.clone());
            if seen_dirs.insert(real) && seen_names.insert(build.file_name().map(|n| n.to_owned()))
            {
                found.push(build);
            }
        }
    }
    found
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

    fn build(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("proton"), b"").unwrap();
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

        let names: Vec<String> = proton_candidates_in(&home)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "GE-Proton",
                "Proton - Experimental",
                "Proton 10.0",
                "UMU-Proton-10.0-4"
            ]
        );
        std::fs::remove_dir_all(home).unwrap();
    }
}
