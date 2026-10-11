//! The account's Steam games as the library shows them, read from the Steam client's own files:
//! their names and types from `appinfo.vdf`, their images from the client's image cache. Nothing
//! is asked of the network.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use super::appinfo;
use super::local::{self, Steam, SteamUser};
use super::vdf::Vdf;
use crate::library::{LibraryGame, MetadataSource};

/// The Steam games of the account signed in to the client.
#[derive(Debug, Clone)]
pub struct SteamLibrary {
    pub user: SteamUser,
    pub games: Vec<LibraryGame>,
    /// Each game's cover, as the client cached it, by game id.
    pub covers: HashMap<String, PathBuf>,
}

/// The library of the account signed in to Steam; none when no account ever signed in. Takes
/// some tens of milliseconds (`appinfo.vdf` is a few MB): not for the interface thread.
pub fn read(steam: &Steam) -> Option<SteamLibrary> {
    let user = local::signed_in_user(steam)?;
    let owned = local::owned(steam, &user);
    let wanted: HashSet<u32> = owned.iter().copied().collect();
    let apps = appinfo::read(&steam.root.join("appcache/appinfo.vdf"), &wanted)
        .inspect_err(|e| tracing::warn!("Steam's appinfo.vdf: {e}"))
        .unwrap_or_default();
    let images = steam.root.join("appcache/librarycache");
    let mut library = SteamLibrary {
        user,
        games: Vec::new(),
        covers: HashMap::new(),
    };
    for app_id in owned {
        let Some(common) = apps
            .get(&app_id)
            .and_then(|a| a.get("appinfo"))
            .and_then(|a| a.get("common"))
        else {
            continue;
        };
        let kind = common.text("type").unwrap_or_default();
        let name = common.text("name").unwrap_or_default().trim();
        if !(kind.eq_ignore_ascii_case("game") || kind.eq_ignore_ascii_case("demo"))
            || name.is_empty()
        {
            continue;
        }
        let folder = images.join(app_id.to_string());
        let image = |asset: &str, legacy: &str| {
            cached_asset(&folder, common, asset).or_else(|| {
                let path = folder.join(legacy);
                path.is_file().then_some(path)
            })
        };
        let id = crate::store::steam_id(app_id);
        if let Some(cover) = image("library_capsule", "library_600x900.jpg") {
            library.covers.insert(id.clone(), cover);
        }
        let text = |p: Option<PathBuf>| p.map(|p| p.display().to_string());
        let oslist = common.text("oslist").unwrap_or_default();
        library.games.push(LibraryGame {
            id,
            title: name.to_string(),
            cover: None,
            icon: None,
            background: text(image("library_hero", "library_hero.jpg")),
            logo: text(image("library_logo", "logo.png")),
            os: ["windows", "linux"]
                .into_iter()
                .filter(|os| oslist.split(',').any(|o| o.trim() == *os))
                .map(String::from)
                .collect(),
            metadata: MetadataSource::Product,
        });
    }
    Some(library)
}

/// The file the client cached for `asset` (`library_capsule`, `library_hero`, `library_logo`), as
/// `library_assets_full` names it: English first, else any language.
fn cached_asset(folder: &Path, common: &Vdf, asset: &str) -> Option<PathBuf> {
    let image = common
        .get("library_assets_full")?
        .get(asset)?
        .get("image")?;
    let english = image.text("english").into_iter();
    let others = image.entries().iter().filter_map(|(_, v)| match v {
        Vdf::Text(t) => Some(t.as_str()),
        Vdf::Section(_) => None,
    });
    english
        .chain(others)
        .filter(|rel| {
            Path::new(rel)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        })
        .map(|rel| folder.join(rel))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::steam::appinfo::tests::fake_appinfo;
    use crate::steam::local::tests::FakeSteam;

    #[test]
    fn the_accounts_games_are_named_and_illustrated_from_steams_files() {
        let fake = FakeSteam::new("library");
        let root = &fake.steam.root;
        std::fs::create_dir_all(root.join("appcache")).unwrap();
        std::fs::write(
            root.join("appcache/appinfo.vdf"),
            fake_appinfo(&[
                (70, "[FAKE] Life", "game", "windows,linux,macos"),
                (220, "[FAKE] Life 2", "Game", "windows"),
                (440, "[FAKE] Redistributables", "Tool", "windows"),
                (999, "[FAKE] Not owned", "Game", "windows"),
            ]),
        )
        .unwrap();
        let cache = root.join("appcache/librarycache/70");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("library_600x900.jpg"), b"jpg").unwrap();

        let library = read(&fake.steam).unwrap();
        assert_eq!(library.user.name, "[FAKE] Player");
        let games: Vec<(&str, &str)> = library
            .games
            .iter()
            .map(|g| (g.id.as_str(), g.title.as_str()))
            .collect();
        assert_eq!(
            games,
            [("steam-70", "[FAKE] Life"), ("steam-220", "[FAKE] Life 2")],
            "games of the account only, not its tools"
        );
        assert_eq!(library.games[0].os, ["windows", "linux"]);
        assert_eq!(
            library.covers["steam-70"],
            cache.join("library_600x900.jpg")
        );
        assert!(!library.covers.contains_key("steam-220"));
    }

    #[test]
    fn cached_images_are_found_by_name_and_never_outside_their_folder() {
        let dir = std::env::temp_dir().join(format!("slatty-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app/abc")).unwrap();
        std::fs::write(dir.join("app/abc/library_capsule.jpg"), b"jpg").unwrap();
        std::fs::write(dir.join("outside.jpg"), b"jpg").unwrap();
        let common = crate::steam::vdf::parse(
            r#""library_assets_full" {
                "library_capsule" { "image" { "schinese" "../outside.jpg" "english" "abc/library_capsule.jpg" } }
                "library_hero" { "image" { "english" "../outside.jpg" } }
            }"#,
        )
        .unwrap();
        let folder = dir.join("app");
        assert_eq!(
            cached_asset(&folder, &common, "library_capsule"),
            Some(folder.join("abc/library_capsule.jpg"))
        );
        assert_eq!(cached_asset(&folder, &common, "library_hero"), None);
        assert_eq!(cached_asset(&folder, &common, "library_logo"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
