use std::path::PathBuf;

use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::gameinfo::resolve_relative;
use crate::http;
use crate::install::{Install, Platform};
use crate::runner::Runner;
use crate::secret::Secret;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SaveLocation {
    pub name: String,
    pub location: String,
}

/// `None` when GOG does not enable cloud saves for this client.
pub async fn fetch(http: &Client, client_id: &str) -> Result<Option<Vec<SaveLocation>>> {
    let url = format!(
        "https://remote-config.gog.com/components/galaxy_client/clients/{client_id}?component_version=2.0.45"
    );
    let v: Value = http::json(http.get(url), "fetching cloud save locations").await?;
    parse(&v, client_id)
}

fn parse(v: &Value, client_id: &str) -> Result<Option<Vec<SaveLocation>>> {
    let storage = &v["content"]["Windows"]["cloudStorage"];
    if storage["enabled"] != Value::Bool(true) {
        return Ok(None);
    }
    let mut locations: Vec<SaveLocation> =
        serde_json::from_value(storage["locations"].clone()).unwrap_or_default();
    if locations.is_empty() {
        locations.push(SaveLocation {
            name: "__default".into(),
            location: format!("<?APPLICATION_DATA_LOCAL?>/GOG.com/Galaxy/Applications/{client_id}/Storage/Shared/Files"),
        });
    }
    Ok(Some(locations))
}

/// The folder a location syncs. Its name, from GOG's answer, names its backup folder and its
/// files in the cloud: anything but one plain name (`..`, a path) is refused, never joined.
pub fn root_of(location: &SaveLocation, install: &Install) -> Result<PathBuf> {
    let name = location.name.as_str();
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(Error::Unsupported(format!(
            "a save location named {name:?} was left out: not a plain name"
        )));
    }
    resolve(&location.location, install)
}

/// Maps a GOG location template to a folder inside the game's Wine prefix.
pub fn resolve(template: &str, install: &Install) -> Result<PathBuf> {
    if install.platform != Platform::Windows {
        return Err(Error::Unsupported(
            "cloud saves are only supported for Windows builds".into(),
        ));
    }
    let rest = template.trim_start();
    let (base, rest) = match rest.strip_prefix("<?").and_then(|r| r.split_once("?>")) {
        Some(("INSTALL", rest)) => (install.path.clone(), rest),
        Some((var, rest)) => (user_folder(install, var)?, rest),
        None => {
            return Err(Error::Unsupported(format!(
                "save location without a known root: {template}"
            )));
        }
    };
    if rest.contains("<?") || rest.contains('%') {
        return Err(Error::Unsupported(format!(
            "unsupported variable in save location: {template}"
        )));
    }
    let root = resolve_relative(&base, rest)?;
    // An isolated game writes its prefix and its folder, where a link could lead the sync, which
    // runs outside the container, to read or write the user's files for it. A game that is not
    // isolated sees them anyway (and Wine alone links the user folders to the real ones).
    if install.isolated {
        let anchor = match install.runner.prefix() {
            Some(prefix) if root.starts_with(prefix) => prefix,
            _ => install.path.as_path(),
        };
        if !crate::installer::leads_inside(anchor, &root) {
            return Err(Error::Refused(format!(
                "{} leads out of {} through a link; saves are not synced through it",
                root.display(),
                anchor.display()
            )));
        }
    }
    Ok(root)
}

fn user_folder(install: &Install, var: &str) -> Result<PathBuf> {
    let prefix = install
        .runner
        .prefix()
        .ok_or_else(|| Error::Unsupported("no Wine prefix".into()))?;
    let users = prefix.join("drive_c/users");
    let user = std::env::var_os("USER").unwrap_or_default();
    let existing = [std::ffi::OsStr::new("steamuser"), &user]
        .iter()
        .filter(|u| !u.is_empty())
        .map(|u| users.join(u))
        .find(|p| p.is_dir());
    // Before the first launch the prefix does not exist yet: the folder is the one Proton (always
    // `steamuser`) or Wine (the user's name) will create.
    let home = match (existing, &install.runner) {
        (Some(home), _) => home,
        (None, Runner::Umu { .. }) => users.join("steamuser"),
        (None, _) if !user.is_empty() => users.join(&user),
        (None, _) => {
            return Err(Error::NotFound(format!(
                "{} has no user folder; launch the game once to create it",
                prefix.display()
            )));
        }
    };
    let sub = match var {
        "SAVED_GAMES" => "Saved Games",
        "DOCUMENTS" => "Documents",
        "APPLICATION_DATA_LOCAL" => "AppData/Local",
        "APPLICATION_DATA_LOCAL_LOW" => "AppData/LocalLow",
        "APPLICATION_DATA_ROAMING" => "AppData/Roaming",
        other => {
            return Err(Error::Unsupported(format!(
                "unknown save location variable <?{other}?>"
            )));
        }
    };
    resolve_relative(&home, sub)
}

/// Galaxy client id and secret of a product, read from its newest generation 2 build.
pub async fn game_client(
    http: &Client,
    tokens: &Tokens,
    game_id: &str,
) -> Result<(String, Secret)> {
    let mut link = None;
    for os in ["windows", "osx"] {
        let url = format!(
            "https://content-system.gog.com/products/{game_id}/os/{os}/builds?generation=2"
        );
        let builds: Value = http::json(
            http.get(url).bearer_auth(tokens.access_token.expose()),
            "fetching build list",
        )
        .await?;
        if let Some(l) = builds["items"][0]["link"].as_str() {
            link = Some(l.to_string());
            break;
        }
    }
    let link = link.ok_or_else(|| {
        Error::Unsupported("this game has no Galaxy build, hence no Galaxy client".into())
    })?;
    let raw = http::bytes(http.get(link.as_str()), "fetching build metadata").await?;
    let meta: Value = crate::galaxy::decode_zlib_json(&raw, "build metadata")?;
    match (meta["clientId"].as_str(), meta["clientSecret"].as_str()) {
        (Some(id), Some(secret)) => Ok((id.to_string(), Secret::new(secret))),
        _ => Err(Error::parse(
            "fetching build metadata",
            "no Galaxy client credentials",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Runner;
    use serde_json::json;

    fn install(root: &std::path::Path) -> Install {
        Install {
            umu_id: None,
            isolated: false,
            game_id: "1".into(),
            title: "G".into(),
            platform: Platform::Windows,
            path: root.join("game"),
            client_id: Some("c".into()),
            runner: Runner::Umu {
                proton: "/p".into(),
                prefix: root.join("pfx"),
            },
        }
    }

    /// A location's name names its backup folder and its files in the cloud: one that is no plain
    /// name (`..`, a path, an absolute one) is left out, never followed out of the backups.
    #[test]
    fn a_location_named_like_a_path_is_left_out() {
        let root = std::env::temp_dir().join(format!("slatty-loc-name-{}", std::process::id()));
        let install = install(&root);
        let at = |name: &str| SaveLocation {
            name: name.into(),
            location: "<?INSTALL?>/saves".into(),
        };
        assert!(root_of(&at("saves"), &install).is_ok());
        for name in [
            "..",
            "../../../../escape",
            "/home/[FAKE]/x",
            "a/b",
            "a\\b",
            "",
            ".",
        ] {
            assert!(root_of(&at(name), &install).is_err(), "{name:?}");
        }
    }

    #[test]
    fn parses_remote_config() {
        let v = json!({"content": {"Windows": {"cloudStorage": {"enabled": true,
            "locations": [{"name": "saves", "location": "<?SAVED_GAMES?>/[FAKE] Studio/Game/base"}]}}}});
        assert_eq!(parse(&v, "c").unwrap().unwrap()[0].name, "saves");
        let off =
            json!({"content": {"Windows": {"cloudStorage": {"enabled": false, "locations": []}}}});
        assert_eq!(parse(&off, "c").unwrap(), None);
        let default =
            json!({"content": {"Windows": {"cloudStorage": {"enabled": true, "locations": []}}}});
        assert_eq!(parse(&default, "c").unwrap().unwrap()[0].name, "__default");
    }

    #[test]
    fn resolves_inside_prefix_case_insensitively() {
        let root = std::env::temp_dir().join(format!("slatty-loc-{}", std::process::id()));
        std::fs::create_dir_all(root.join("pfx/drive_c/users/steamuser/Saved Games/KingdomCome"))
            .unwrap();
        let i = install(&root);
        let p = resolve("<?SAVED_GAMES?>\\\\kingdomcome\\\\saves", &i).unwrap();
        assert_eq!(
            p,
            root.join("pfx/drive_c/users/steamuser/Saved Games/KingdomCome/saves")
        );
        assert_eq!(
            resolve("<?INSTALL?>/saves", &i).unwrap(),
            root.join("game/saves")
        );
        assert!(resolve("<?APPLICATION_SUPPORT?>/x", &i).is_err());
        assert!(resolve("<?SAVED_GAMES?>/../../escape", &i).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_isolated_games_saves_are_not_synced_through_a_link_out_of_its_prefix() {
        let root = std::env::temp_dir().join(format!("slatty-loc-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let users = root.join("pfx/drive_c/users");
        std::fs::create_dir_all(users.join("steamuser")).unwrap();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::fs::create_dir_all(root.join("game")).unwrap();
        // Planted by the game, which writes its prefix and its folder: the sync runs outside the
        // container and would follow it into the user's files.
        std::os::unix::fs::symlink(root.join("outside"), users.join("steamuser/AppData")).unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("game/saves")).unwrap();
        let mut i = install(&root);
        i.isolated = true;
        for template in [
            "<?APPLICATION_DATA_ROAMING?>/Game",
            "<?INSTALL?>/saves/profile",
        ] {
            assert!(
                matches!(resolve(template, &i), Err(Error::Refused(_))),
                "{template}"
            );
        }
        // umu links the user's name to `steamuser` inside the prefix: that one stays allowed.
        std::os::unix::fs::symlink("steamuser", users.join("someone")).unwrap();
        assert_eq!(
            resolve("<?SAVED_GAMES?>/Game", &i).unwrap(),
            users.join("steamuser/Saved Games/Game")
        );
        // Not isolated, the game sees the user's files anyway, and Wine alone links the user
        // folders to the real ones.
        i.isolated = false;
        assert_eq!(
            resolve("<?APPLICATION_DATA_ROAMING?>/Game", &i).unwrap(),
            users.join("steamuser/AppData/Roaming/Game")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn before_the_first_launch_saves_go_where_proton_will_put_them() {
        let root = std::env::temp_dir().join(format!("slatty-loc-empty-{}", std::process::id()));
        assert_eq!(
            resolve(
                "<?APPLICATION_DATA_LOCAL_LOW?>/[FAKE] Studio/Game",
                &install(&root)
            )
            .unwrap(),
            root.join("pfx/drive_c/users/steamuser/AppData/LocalLow/[FAKE] Studio/Game")
        );
        assert!(!root.exists(), "nothing is created");
    }
}
