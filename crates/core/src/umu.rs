//! umu's database maps a store's game ids to umu ids, which select the game's fixes in umu's
//! protonfixes, as Heroic does. It is read from the copy the Proton build carries; online, only
//! once turned on, and then only the GOG product id is sent.

use std::path::Path;
use std::time::Duration;

use reqwest::Client;
use serde_json::Value;

use crate::db::Db;
use crate::error::Result;
use crate::http;
use crate::install::Install;
use crate::runner::Runner;

const API: &str = "https://umu.openwinecomponents.org/umu_api.php";

/// The id umu uses for a game it has no fixes for.
pub const UNKNOWN: &str = "umu-0";

/// The game's umu id, or [`UNKNOWN`] when the database does not list it.
pub async fn lookup(http: &Client, game_id: &str) -> Result<String> {
    let req = http
        .get(API)
        .query(&[("codename", game_id), ("store", "gog")]);
    let body: Value = http::json(req, "looking up the game in umu's database").await?;
    Ok(parse(&body))
}

fn parse(body: &Value) -> String {
    body.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["umu_id"].as_str())
        .find(|id| valid(id))
        .map_or_else(|| UNKNOWN.to_string(), String::from)
}

/// The umu id of a GOG game in the copy of umu's database `proton` carries
/// (`protonfixes/umu-database.csv`: title, store, store's id, umu id, …).
fn in_proton(proton: &Path, game_id: &str) -> Option<String> {
    let csv = std::fs::read_to_string(proton.join("protonfixes/umu-database.csv")).ok()?;
    csv.lines()
        .map(csv_fields)
        .find(|f| {
            f.get(1).map(String::as_str) == Some("gog")
                && f.get(2).map(String::as_str) == Some(game_id)
        })
        .and_then(|f| f.get(3).cloned())
        .filter(|id| valid(id))
}

/// The fields of a CSV line; a title holding a comma comes quoted, its quotes doubled.
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            c => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

/// `umu-` followed by a plain identifier: the value ends up in the game's environment.
fn valid(id: &str) -> bool {
    id.strip_prefix("umu-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

/// The umu id of a game run through umu. It is read in the copy of umu's database that the game's
/// Proton build carries (UMU-Proton, GE-Proton and Proton-CachyOS have one in their protonfixes),
/// at each launch, so a newer build's copy counts. Not listed there, it is asked of umu's database
/// online once turned on in Settings, once, and kept. When neither knows the game, or the database
/// online cannot be reached, the game starts without fixes.
pub async fn resolve(db: &Db, http: &Client, install: &mut Install) {
    let Runner::Umu { proton, .. } = &install.runner else {
        return;
    };
    if install.umu_id.is_some() {
        return;
    }
    if let Some(id) = in_proton(proton, &install.game_id) {
        install.umu_id = Some(id);
        return;
    }
    if !crate::settings::umu_lookup(db).unwrap_or(false) {
        return;
    }
    match tokio::time::timeout(Duration::from_secs(5), lookup(http, &install.game_id)).await {
        Ok(Ok(id)) => {
            install.umu_id = Some(id);
            if let Err(e) = install.save(db) {
                tracing::warn!("umu id not saved: {e}");
            }
        }
        Ok(Err(e)) => tracing::warn!("umu id not looked up: {e}"),
        Err(_) => tracing::warn!("umu id lookup timed out"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn listed_games_get_their_id_and_others_the_unknown_one() {
        assert_eq!(
            parse(&json!([{"title": "Cyberpunk 2077", "umu_id": "umu-1091500"}])),
            "umu-1091500"
        );
        assert_eq!(parse(&json!([])), UNKNOWN);
        assert_eq!(parse(&json!({"error": "no such game"})), UNKNOWN);
    }

    #[test]
    fn ids_that_are_not_plain_identifiers_are_ignored() {
        for bad in ["1091500", "umu-", "umu-1; rm -rf ~", "umu-1\nX=1"] {
            assert_eq!(parse(&json!([{"umu_id": bad}])), UNKNOWN, "{bad:?}");
        }
    }

    /// The Proton builds made for umu (UMU-Proton, GE-Proton, Proton-CachyOS) carry umu's
    /// database in their protonfixes: the id is read there, and nothing leaves the computer.
    #[tokio::test]
    async fn the_id_is_read_from_the_proton_builds_own_copy_of_the_database() {
        let proton = std::env::temp_dir().join(format!("slatty-umu-csv-{}", std::process::id()));
        std::fs::create_dir_all(proton.join("protonfixes")).unwrap();
        std::fs::write(
            proton.join("protonfixes/umu-database.csv"),
            "TITLE,STORE,CODENAME,UMU_ID,COMMON ACRONYM (Optional),NOTE (Optional),EXE_STRINGS (Optional)\n\
             \"[FAKE] Space, the \"\"Game\"\"\",gog,1234567890,umu-42,,,\n\
             [FAKE] Other,steam,1234567891,umu-7,,,\n\
             [FAKE] Odd,gog,1234567892,umu-1; rm -rf ~,,,\n",
        )
        .unwrap();
        assert_eq!(in_proton(&proton, "1234567890").as_deref(), Some("umu-42"));
        assert_eq!(in_proton(&proton, "1234567891"), None, "another store's id");
        assert_eq!(in_proton(&proton, "1234567892"), None, "not a plain id");
        assert_eq!(in_proton(&proton, "9"), None);

        let db = Db::in_memory().unwrap();
        let mut install = crate::install::Install {
            game_id: "1234567890".into(),
            title: "[FAKE] Game".into(),
            platform: crate::install::Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Umu {
                proton: proton.clone(),
                prefix: "/prefix".into(),
            },
            umu_id: None,
            isolated: false,
        };
        // Every request goes through a local proxy, which would see a lookup.
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        proxy.set_nonblocking(true).unwrap();
        let http = Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", proxy.local_addr().unwrap())).unwrap())
            .build()
            .unwrap();
        resolve(&db, &http, &mut install).await;
        assert_eq!(install.umu_id.as_deref(), Some("umu-42"));
        assert!(proxy.accept().is_err(), "no request made");
        // Not listed there, and the database online not turned on: nothing is sent either.
        install.game_id = "9".into();
        install.umu_id = None;
        resolve(&db, &http, &mut install).await;
        assert_eq!(install.umu_id, None);
        assert!(proxy.accept().is_err(), "no request made");
        std::fs::remove_dir_all(proton).unwrap();
    }

    #[tokio::test]
    async fn nothing_is_looked_up_once_turned_off() {
        let db = Db::in_memory().unwrap();
        crate::settings::set_umu_lookup(&db, false).unwrap();
        let mut install = crate::install::Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: crate::install::Platform::Windows,
            path: "/games/Game".into(),
            client_id: None,
            runner: Runner::Umu {
                proton: "/proton".into(),
                prefix: "/prefix".into(),
            },
            umu_id: None,
            isolated: false,
        };
        // Every request goes through a local proxy, which would see a lookup.
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        proxy.set_nonblocking(true).unwrap();
        let http = Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://{}", proxy.local_addr().unwrap())).unwrap())
            .build()
            .unwrap();
        resolve(&db, &http, &mut install).await;
        assert_eq!(install.umu_id, None);
        assert!(proxy.accept().is_err(), "no request made");
    }
}
