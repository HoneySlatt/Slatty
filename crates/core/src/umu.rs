//! umu's database maps a store's game ids to umu ids, which select the game's fixes in umu's
//! protonfixes, as Heroic does. Only the GOG product id is sent.

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

/// `umu-` followed by a plain identifier: the value ends up in the game's environment.
fn valid(id: &str) -> bool {
    id.strip_prefix("umu-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    })
}

/// Looks the umu id up once for a game run through umu and saves it. When the database cannot be
/// reached the game starts without fixes, and the lookup is tried again at the next launch.
pub async fn resolve(db: &Db, http: &Client, install: &mut Install) {
    if install.umu_id.is_some() || !matches!(install.runner, Runner::Umu { .. }) {
        return;
    }
    // Turned off in Settings: nothing is sent, and the game runs without fixes.
    if !crate::settings::umu_lookup(db).unwrap_or(true) {
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
