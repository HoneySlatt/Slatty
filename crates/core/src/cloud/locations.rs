use std::io::Read;
use std::path::PathBuf;

use flate2::read::ZlibDecoder;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::gameinfo::resolve_relative;
use crate::http;
use crate::install::{Install, Platform};
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
    resolve_relative(&base, rest)
}

fn user_folder(install: &Install, var: &str) -> Result<PathBuf> {
    let prefix = install
        .runner
        .prefix()
        .ok_or_else(|| Error::Unsupported("no Wine prefix".into()))?;
    let users = prefix.join("drive_c/users");
    let user = std::env::var("USER").unwrap_or_default();
    let home = ["steamuser", user.as_str()]
        .iter()
        .filter(|u| !u.is_empty())
        .map(|u| users.join(u))
        .find(|p| p.is_dir())
        .ok_or_else(|| {
            Error::NotFound(format!(
                "{} has no user folder; launch the game once to create it",
                prefix.display()
            ))
        })?;
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
    let resp = http::send(http.get(link.as_str()), "fetching build metadata").await?;
    let raw = resp
        .bytes()
        .await
        .map_err(|e| Error::network("fetching build metadata", e))?;
    let meta: Value = decode_meta(&raw)?;
    match (meta["clientId"].as_str(), meta["clientSecret"].as_str()) {
        (Some(id), Some(secret)) => Ok((id.to_string(), Secret::new(secret))),
        _ => Err(Error::parse(
            "fetching build metadata",
            "no Galaxy client credentials",
        )),
    }
}

fn decode_meta(raw: &[u8]) -> Result<Value> {
    let mut text = Vec::new();
    match ZlibDecoder::new(raw).read_to_end(&mut text) {
        Ok(_) => serde_json::from_slice(&text),
        Err(_) => serde_json::from_slice(raw),
    }
    .map_err(|e| Error::parse("build metadata", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Runner;
    use serde_json::json;

    fn install(root: &std::path::Path) -> Install {
        Install {
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

    #[test]
    fn parses_remote_config() {
        let v = json!({"content": {"Windows": {"cloudStorage": {"enabled": true,
            "locations": [{"name": "saves", "location": "<?SAVED_GAMES?>/id Software/DOOM/base"}]}}}});
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
    fn uninitialised_prefix_is_an_error() {
        let root = std::env::temp_dir().join(format!("slatty-loc-empty-{}", std::process::id()));
        assert!(matches!(
            resolve("<?DOCUMENTS?>/x", &install(&root)),
            Err(Error::NotFound(_))
        ));
    }
}
