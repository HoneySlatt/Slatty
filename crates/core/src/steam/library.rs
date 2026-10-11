//! The account's Steam games as the library shows them, read from the Steam client's own files:
//! their names and types from `appinfo.vdf`, their images from the client's image cache. Images
//! the client has not saved come from Steam's image servers once that is turned on
//! (`settings::steam_images`); nothing else is asked of the network.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use reqwest::Client;

use super::appinfo;
use super::local::{self, Steam, SteamUser};
use super::vdf::Vdf;
use crate::error::{Error, Result};
use crate::library::{LibraryGame, MetadataSource};
use crate::paths::Dirs;

/// Steam's image servers, which need no account.
const IMAGE_SERVER: &str = "https://shared.steamstatic.com/";

/// The Steam games of the account signed in to the client.
#[derive(Debug, Clone)]
pub struct SteamLibrary {
    pub user: SteamUser,
    pub games: Vec<LibraryGame>,
    /// Each game's cover, as the client cached it, by game id.
    pub covers: HashMap<String, PathBuf>,
    /// How long each game was played and when last, by game id.
    pub played: HashMap<String, local::Played>,
}

/// The library of the account signed in to Steam; none when no account ever signed in. With
/// `from_servers`, an image the client has not saved here is given by its address on Steam's
/// image servers instead, for `image` to download. Takes some tens of milliseconds
/// (`appinfo.vdf` is a few MB): not for the interface thread.
pub fn read(steam: &Steam, from_servers: bool) -> Option<SteamLibrary> {
    let user = local::signed_in_user(steam)?;
    let owned = local::owned(steam, &user);
    let wanted: HashSet<u32> = owned.iter().copied().collect();
    let apps = appinfo::read(&steam.root.join("appcache/appinfo.vdf"), &wanted)
        .inspect_err(|e| tracing::warn!("Steam's appinfo.vdf: {e}"))
        .unwrap_or_default();
    let images = steam.root.join("appcache/librarycache");
    let mut played = local::played(steam, &user);
    let mut library = SteamLibrary {
        user,
        games: Vec::new(),
        covers: HashMap::new(),
        played: HashMap::new(),
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
        let here = |asset: &str, legacy: &str| {
            cached_asset(&folder, common, asset).or_else(|| {
                let path = folder.join(legacy);
                path.is_file().then_some(path)
            })
        };
        let server = |asset: &str, legacy: &str| {
            from_servers.then(|| server_url(app_id, common, asset, legacy))
        };
        let image = |asset: &str, legacy: &str| {
            here(asset, legacy)
                .map(|p| p.display().to_string())
                .or_else(|| server(asset, legacy))
        };
        let id = crate::store::steam_id(app_id);
        if let Some(p) = played.remove(&app_id) {
            library.played.insert(id.clone(), p);
        }
        let cover = match here("library_capsule", "library_600x900.jpg") {
            Some(path) => {
                library.covers.insert(id.clone(), path);
                None
            }
            None => server("library_capsule", "library_600x900.jpg"),
        };
        let oslist = common.text("oslist").unwrap_or_default();
        library.games.push(LibraryGame {
            id,
            title: name.to_string(),
            cover,
            icon: None,
            background: image("library_hero", "library_hero.jpg"),
            logo: image("library_logo", "logo.png"),
            os: ["windows", "linux"]
                .into_iter()
                .filter(|os| oslist.split(',').any(|o| o.trim() == *os))
                .map(String::from)
                .collect(),
            metadata: MetadataSource::Product,
            steam: Vec::new(),
        });
    }
    Some(library)
}

/// The Steam game each GOG game is, by GOG id, among `gog` (id, title, Steam app ids GOG's
/// database gives) and `steam`: first by those app ids, then, for a GOG game GOG's database gives
/// none for, by title, written alike once kept to letters and digits, lower-cased. A GOG game
/// whose Steam apps are all elsewhere is not matched by title, nor a title two Steam games share;
/// each Steam game goes to one GOG game.
pub fn twins<'a>(
    gog: impl IntoIterator<Item = (&'a str, &'a str, &'a [u32])>,
    steam: &'a [LibraryGame],
) -> HashMap<String, String> {
    let alike = |title: &str| -> String {
        title
            .chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let by_app: HashMap<u32, &str> = steam
        .iter()
        .filter_map(|g| Some((crate::store::steam_app(&g.id)?, g.id.as_str())))
        .collect();
    // None for a title two of the Steam games share.
    let mut by_title: HashMap<String, Option<&str>> = HashMap::new();
    for g in steam {
        by_title
            .entry(alike(&g.title))
            .and_modify(|seen| *seen = None)
            .or_insert(Some(g.id.as_str()));
    }
    let mut taken = HashSet::new();
    let mut twins = HashMap::new();
    let mut by_name = Vec::new();
    for (id, title, apps) in gog {
        if apps.is_empty() {
            by_name.push((id, title));
        } else if let Some(steam_id) = apps.iter().find_map(|app| by_app.get(app))
            && taken.insert(*steam_id)
        {
            twins.insert(id.to_string(), steam_id.to_string());
        }
    }
    for (id, title) in by_name {
        let name = alike(title);
        if let Some(Some(steam_id)) = by_title.get(&name)
            && !name.is_empty()
            && taken.insert(*steam_id)
        {
            twins.insert(id.to_string(), steam_id.to_string());
        }
    }
    twins
}

/// How long the games of the account signed in to Steam were played and when last, by game id,
/// with that account's Steam id: what `read` gives of it, at a fraction of its cost.
pub fn played(steam: &Steam) -> Option<(u64, HashMap<String, local::Played>)> {
    let user = local::signed_in_user(steam)?;
    let played = local::played(steam, &user)
        .into_iter()
        .map(|(app, p)| (crate::store::steam_id(app), p))
        .collect();
    Some((user.id64, played))
}

/// Where Steam's image servers keep `asset` of `app_id`: the file `library_assets_full` names, else
/// the name every app's had before it. A name with anything but letters, digits and `._-` between
/// its slashes is not used in an address.
fn server_url(app_id: u32, common: &Vdf, asset: &str, legacy: &str) -> String {
    let named = common
        .get("library_assets_full")
        .and_then(|a| a.get(asset))
        .and_then(|a| a.get("image"))
        .and_then(|i| i.text("english"))
        .filter(|rel| {
            rel.split('/').all(|part| {
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
        });
    format!(
        "{IMAGE_SERVER}store_item_assets/steam/apps/{app_id}/{}",
        named.unwrap_or(legacy)
    )
}

/// Whether `url` is an image of Steam's image servers, which `image` downloads.
pub fn is_server_image(url: &str) -> bool {
    url.starts_with(IMAGE_SERVER)
}

/// The file of an image of Steam's image servers in SlattyLauncher's cache, downloaded when
/// missing. Shared by every account: it tells nothing of one. Any other address is refused.
pub async fn image(http: &Client, dirs: &Dirs, url: &str) -> Result<PathBuf> {
    if !is_server_image(url) {
        return Err(Error::Refused(
            "Steam images come from Steam's image servers only".into(),
        ));
    }
    let name = crate::fsutil::sha256_hex(url.as_bytes());
    crate::library::cached(
        http,
        dirs.cache.join("steam"),
        name,
        url,
        "downloading an image from Steam",
    )
    .await
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

        let library = read(&fake.steam, false).unwrap();
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
        let played: Vec<&str> = library.played.keys().map(String::as_str).collect();
        assert_eq!(played, ["steam-70"], "the games' alone");
        let (account, all) = super::played(&fake.steam).unwrap();
        assert_eq!(account, library.user.id64);
        assert_eq!(all["steam-70"], library.played["steam-70"]);
        assert_eq!(
            library.covers["steam-70"],
            cache.join("library_600x900.jpg")
        );
        assert!(!library.covers.contains_key("steam-220"));
        assert!(
            library
                .games
                .iter()
                .all(|g| g.cover.is_none() && g.background.is_none() && g.logo.is_none()),
            "nothing to download while Steam's servers are not to be asked"
        );

        let library = read(&fake.steam, true).unwrap();
        let (life, life2) = (&library.games[0], &library.games[1]);
        assert_eq!(life.cover, None, "the client saved it");
        assert_eq!(
            life2.cover.as_deref(),
            Some(
                "https://shared.steamstatic.com/store_item_assets/steam/apps/220/library_600x900.jpg"
            )
        );
        assert_eq!(
            life2.background.as_deref(),
            Some(
                "https://shared.steamstatic.com/store_item_assets/steam/apps/220/library_hero.jpg"
            )
        );
        assert!(library.games.iter().all(|g| {
            [&g.cover, &g.background, &g.logo]
                .into_iter()
                .flatten()
                .all(|url| is_server_image(url))
        }));
    }

    #[test]
    fn a_game_owned_on_gog_and_steam_is_matched_once() {
        let steam_game = |app: u32, title: &str| LibraryGame {
            id: crate::store::steam_id(app),
            title: title.into(),
            cover: None,
            icon: None,
            background: None,
            logo: None,
            os: Vec::new(),
            metadata: MetadataSource::Product,
            steam: Vec::new(),
        };
        let steam = [
            steam_game(10, "[FAKE] Another Name On Steam"),
            steam_game(20, "[FAKE] The Witcher 3: Wild Hunt"),
            steam_game(30, "[FAKE] Shared Name"),
            steam_game(31, "[FAKE] Shared  name"),
            steam_game(40, "[FAKE] Elsewhere"),
            steam_game(50, "[FAKE] Claimed"),
        ];
        let gog: [(&str, &str, &[u32]); 7] = [
            // GOG's database names the Steam app: matched whatever the titles.
            ("1", "[FAKE] Its GOG Name", &[9, 10]),
            ("2", "[FAKE] the witcher® 3 – WILD HUNT™", &[]),
            ("3", "[FAKE] Shared Name", &[]),
            ("4", "[FAKE] Elsewhere", &[99]),
            // A title that would claim what an app id claims, and a second GOG game alike.
            ("5", "[FAKE] Claimed", &[]),
            ("6", "[FAKE] Other", &[50]),
            ("7", "[FAKE] The Witcher 3 Wild Hunt", &[]),
        ];
        let twins = twins(gog, &steam);
        let mut found: Vec<(&str, &str)> = twins
            .iter()
            .map(|(g, s)| (g.as_str(), s.as_str()))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [("1", "steam-10"), ("2", "steam-20"), ("6", "steam-50")],
            "a title two Steam games share, and one whose Steam apps are elsewhere, stay apart"
        );
    }

    #[test]
    fn images_from_steams_servers_are_named_as_the_client_names_them() {
        let common = crate::steam::vdf::parse(
            r#""library_assets_full" {
                "library_capsule" { "image" { "english" "6cddc26/library_capsule.jpg" } }
                "library_hero" { "image" { "english" "../../../elsewhere/library_hero.jpg" } }
                "library_logo" { "image" { "english" "abc/logo.png?x=1" } }
            }"#,
        )
        .unwrap();
        let url = |asset, legacy| server_url(570, &common, asset, legacy);
        assert_eq!(
            url("library_capsule", "library_600x900.jpg"),
            "https://shared.steamstatic.com/store_item_assets/steam/apps/570/6cddc26/library_capsule.jpg"
        );
        assert!(url("library_hero", "library_hero.jpg").ends_with("/570/library_hero.jpg"));
        assert!(url("library_logo", "logo.png").ends_with("/570/logo.png"));
    }

    #[tokio::test]
    async fn only_steams_image_servers_are_asked() {
        let dirs = Dirs::under(Path::new("/nonexistent/slatty"));
        let http = Client::new();
        for url in [
            "https://example.com/store_item_assets/steam/apps/1/a.jpg",
            "http://shared.steamstatic.com/a.jpg",
            "https://shared.steamstatic.com.example.com/a.jpg",
        ] {
            assert!(
                matches!(image(&http, &dirs, url).await, Err(Error::Refused(_))),
                "{url}"
            );
        }
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
