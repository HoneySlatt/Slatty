//! The local Steam installation, read and never written: where it is, who is signed in to it,
//! the games of that account, and those installed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::vdf::{self, Vdf};

/// Steam's text files are a few KB; `localconfig.vdf` grows to some MB. Larger is not Steam's.
const MAX_TEXT_FILE: u64 = 64 << 20;

/// What a Steam id adds to an account id (the individual account type, public universe).
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// A Steam client's data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steam {
    pub root: PathBuf,
    /// Steam installed as a Flatpak, whose files live in its sandbox.
    pub flatpak: bool,
}

/// The account signed in to the Steam client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamUser {
    pub id64: u64,
    /// The name shown to friends, not the one to sign in with.
    pub name: String,
}

impl SteamUser {
    /// The folder of this account under `userdata`.
    pub fn account_id(&self) -> u64 {
        self.id64.saturating_sub(STEAM_ID64_BASE)
    }
}

/// A game Steam installed in one of its library folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub app_id: u32,
    pub name: String,
    pub dir: PathBuf,
    pub size: u64,
    /// Whether it is fully installed, not being downloaded or updated.
    pub ready: bool,
}

/// The Steam installation in `home`: native, through the `~/.steam/steam` link, or the Flatpak's;
/// the one an account signed in to.
pub fn find(home: &Path) -> Option<Steam> {
    [
        (".local/share/Steam", false),
        (".steam/steam", false),
        (".var/app/com.valvesoftware.Steam/data/Steam", true),
        (".var/app/com.valvesoftware.Steam/.local/share/Steam", true),
    ]
    .into_iter()
    .map(|(path, flatpak)| (home.join(path), flatpak))
    .find(|(root, _)| root.join("config/loginusers.vdf").is_file())
    .map(|(root, flatpak)| Steam { root, flatpak })
}

/// The account last signed in to the Steam client, if one ever was.
pub fn signed_in_user(steam: &Steam) -> Option<SteamUser> {
    let users = read(&steam.root.join("config/loginusers.vdf"))?;
    users
        .get("users")?
        .entries()
        .iter()
        .filter_map(|(id, user)| {
            let id64: u64 = id.parse().ok().filter(|id| *id > STEAM_ID64_BASE)?;
            let at: u64 = user
                .text("Timestamp")
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            Some((
                at,
                id64,
                user.text("PersonaName").unwrap_or_default().to_string(),
            ))
        })
        .max()
        .map(|(_, id64, name)| SteamUser { id64, name })
}

/// The Steam app ids of the account's games: those Steam keeps details of for its library.
pub fn owned(steam: &Steam, user: &SteamUser) -> Vec<u32> {
    let folder = steam
        .root
        .join("userdata")
        .join(user.account_id().to_string())
        .join("config/librarycache");
    let mut ids: Vec<u32> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".json")?.parse().ok())
        .collect();
    ids.sort_unstable();
    ids
}

/// How long a game was played and when last, as the Steam client records them for the account.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Played {
    pub minutes: u64,
    /// Seconds since 1970; 0 when never.
    pub last_played: i64,
}

/// Play time and last session of the account's games, by app id, from the client's
/// `userdata/<account>/config/localconfig.vdf`. That file also holds the client's download tokens
/// and the account's friends: only these two numbers are kept from it.
pub fn played(steam: &Steam, user: &SteamUser) -> HashMap<u32, Played> {
    let path = steam
        .root
        .join("userdata")
        .join(user.account_id().to_string())
        .join("config/localconfig.vdf");
    let Some(config) = read(&path) else {
        return HashMap::new();
    };
    let apps = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
        .into_iter()
        .try_fold(&config, |section, key| section.get(key));
    apps.map_or(&[][..], Vdf::entries)
        .iter()
        .filter_map(|(id, app)| {
            let number = |key| {
                app.text(key)
                    .and_then(|t| t.parse::<i64>().ok())
                    .filter(|n| *n > 0)
                    .unwrap_or(0)
            };
            let played = Played {
                minutes: number("Playtime") as u64,
                last_played: number("LastPlayed"),
            };
            (played != Played::default()).then_some((id.parse().ok()?, played))
        })
        .collect()
}

/// Steam's library folders, the main one first.
pub fn libraries(steam: &Steam) -> Vec<PathBuf> {
    let mut found = vec![steam.root.clone()];
    if let Some(vdf) = read(&steam.root.join("steamapps/libraryfolders.vdf")) {
        let folders = vdf.get("libraryfolders").map_or(&[][..], Vdf::entries);
        for (_, folder) in folders {
            if let Some(path) = folder.text("path").map(PathBuf::from)
                && !found.contains(&path)
            {
                found.push(path);
            }
        }
    }
    found
}

/// The games installed in Steam's library folders. Reads each folder, which can be a network drive:
/// not for the interface thread.
pub fn installed(steam: &Steam) -> Vec<Installed> {
    let mut found = Vec::new();
    for library in libraries(steam) {
        let apps = library.join("steamapps");
        for entry in std::fs::read_dir(&apps).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            if let Some(game) = read(&entry.path()).and_then(|m| manifest(&apps, &m)) {
                found.push(game);
            }
        }
    }
    found.sort_by_key(|g| g.app_id);
    found.dedup_by_key(|g| g.app_id);
    found
}

fn manifest(apps: &Path, vdf: &Vdf) -> Option<Installed> {
    let state = vdf.get("AppState")?;
    let number = |key| state.text(key).and_then(|t| t.parse::<u64>().ok());
    let dir = state.text("installdir").filter(|d| safe_name(d))?;
    Some(Installed {
        app_id: number("appid").and_then(|id| u32::try_from(id).ok())?,
        name: state.text("name").unwrap_or_default().to_string(),
        dir: apps.join("common").join(dir),
        size: number("SizeOnDisk").unwrap_or(0),
        // Steam's "fully installed" state bit.
        ready: number("StateFlags").is_some_and(|f| f & 4 != 0),
    })
}

/// A folder name Steam wrote, kept inside `steamapps/common`.
fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

/// A text file of Steam's, if it is one.
fn read(path: &Path) -> Option<Vdf> {
    let file = std::fs::File::open(path).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(file, MAX_TEXT_FILE), &mut text).ok()?;
    vdf::parse(&text).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Steam installation made of the files SlattyLauncher reads, all `[FAKE]`.
    pub(crate) struct FakeSteam {
        pub home: PathBuf,
        pub steam: Steam,
    }

    impl FakeSteam {
        pub fn new(name: &str) -> FakeSteam {
            let home =
                std::env::temp_dir().join(format!("slatty-steam-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&home);
            let root = home.join(".local/share/Steam");
            let write = |rel: &str, text: &str| {
                let path = root.join(rel);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            };
            // Two accounts: the later sign-in is the one in use.
            write(
                "config/loginusers.vdf",
                r#""users" {
                    "76561198000000001" { "AccountName" "fake_old" "PersonaName" "[FAKE] Old" "Timestamp" "100" }
                    "76561198000000002" { "AccountName" "fake" "PersonaName" "[FAKE] Player" "Timestamp" "200" }
                }"#,
            );
            let other = home.join("other-library");
            write(
                "steamapps/libraryfolders.vdf",
                &format!(
                    r#""libraryfolders" {{ "0" {{ "path" "{}" }} "1" {{ "path" "{}" }} }}"#,
                    root.display(),
                    other.display()
                ),
            );
            write(
                "steamapps/appmanifest_440.acf",
                r#""AppState" { "appid" "440" "name" "[FAKE] Fortress" "installdir" "Fortress" "StateFlags" "4" "SizeOnDisk" "1000" }"#,
            );
            write(
                "steamapps/appmanifest_620.acf",
                r#""AppState" { "appid" "620" "name" "[FAKE] Portal" "installdir" "../escape" "StateFlags" "4" }"#,
            );
            std::fs::create_dir_all(other.join("steamapps")).unwrap();
            std::fs::write(
                other.join("steamapps/appmanifest_70.acf"),
                r#""AppState" { "appid" "70" "name" "[FAKE] Life" "installdir" "Life" "StateFlags" "1026" }"#,
            )
            .unwrap();
            for app in [70, 440, 220] {
                write(
                    &format!("userdata/39734274/config/librarycache/{app}.json"),
                    "[]",
                );
            }
            write(
                "userdata/39734274/config/librarycache/not-an-app.json",
                "[]",
            );
            write(
                "userdata/39734274/config/localconfig.vdf",
                r#""UserLocalConfigStore" {
                    "friends" { "76561198000000003" { "name" "[FAKE] Friend" } }
                    "depots" { "71" { "CDN" { "cdn.example" { "token" "[FAKE] token" } } } }
                    "Software" { "Valve" { "Steam" { "apps" {
                        "440" { "LastPlayed" "1760000000" "Playtime" "754" }
                        "70" { "LastPlayed" "1700000000" }
                        "220" { "Playtime" "0" "LastPlayed" "garbage" }
                        "not-an-app" { "Playtime" "5" }
                    } } } }
                }"#,
            );
            FakeSteam {
                steam: Steam {
                    root,
                    flatpak: false,
                },
                home,
            }
        }
    }

    impl Drop for FakeSteam {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    #[test]
    fn the_signed_in_account_and_its_games_are_read() {
        let fake = FakeSteam::new("local");
        assert_eq!(find(&fake.home), Some(fake.steam.clone()));
        assert_eq!(find(&fake.home.join("nowhere")), None);
        let user = signed_in_user(&fake.steam).unwrap();
        assert_eq!(user.id64, 76561198000000002);
        assert_eq!(user.name, "[FAKE] Player");
        assert_eq!(user.account_id(), 39734274);
        assert_eq!(owned(&fake.steam, &user), [70, 220, 440]);

        let installed = installed(&fake.steam);
        let ids: Vec<u32> = installed.iter().map(|g| g.app_id).collect();
        assert_eq!(
            ids,
            [70, 440],
            "the one leading out of steamapps is left out"
        );
        assert!(
            installed[1].ready && !installed[0].ready,
            "70 is being updated"
        );
        assert_eq!(
            installed[1].dir,
            fake.steam.root.join("steamapps/common/Fortress")
        );
        assert_eq!(installed[1].size, 1000);
    }

    #[test]
    fn play_time_and_last_session_come_from_the_clients_records() {
        let fake = FakeSteam::new("played");
        let user = signed_in_user(&fake.steam).unwrap();
        let played = played(&fake.steam, &user);
        assert_eq!(
            played.get(&440),
            Some(&Played {
                minutes: 754,
                last_played: 1_760_000_000
            })
        );
        assert_eq!(
            played.get(&70),
            Some(&Played {
                minutes: 0,
                last_played: 1_700_000_000
            })
        );
        assert_eq!(played.len(), 2, "nothing for a game never played");

        let other = SteamUser {
            id64: 76561198000000001,
            name: String::new(),
        };
        assert!(super::played(&fake.steam, &other).is_empty(), "no records");
    }
}
