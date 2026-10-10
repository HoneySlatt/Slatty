//! Covers and backgrounds from SteamGridDB (steamgriddb.com), a community database of game art:
//! a game is searched by name, then its grids (covers) or heroes (backgrounds) are listed. Used
//! only once the user turns it on and gives their own API key, which stays in the keyring and goes
//! in a header, never in a URL.

use reqwest::Client;
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::http;
use crate::secret::Secret;

/// Name of the API key in the keyring.
pub const KEY: &str = "steamgriddb";
/// Where a user creates their key.
pub const KEY_PAGE: &str = "https://www.steamgriddb.com/profile/preferences/api";
const API: &str = "https://www.steamgriddb.com/api/v2";

pub fn key_page() -> url::Url {
    url::Url::parse(KEY_PAGE).expect("a valid address")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Upright, like GOG's covers: SteamGridDB's grids.
    Cover,
    /// Wide, for the key art of the game page: SteamGridDB's heroes.
    Background,
}

/// A game as SteamGridDB knows it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Game {
    pub id: u64,
    pub name: String,
    /// Unix time of its first release, when known.
    #[serde(default)]
    pub release_date: Option<i64>,
}

/// One image SteamGridDB offers: the full one and a small preview.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Art {
    pub url: String,
    pub thumb: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

/// The games whose name matches `name`, the closest first.
pub async fn search(http: &Client, key: &Secret, name: &str) -> Result<Vec<Game>> {
    search_at(http, API, key, name).await
}

async fn search_at(http: &Client, api: &str, key: &Secret, name: &str) -> Result<Vec<Game>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    let mut url = url::Url::parse(api).map_err(|e| Error::parse("SteamGridDB address", e))?;
    url.path_segments_mut()
        .map_err(|()| Error::parse("SteamGridDB address", "no path"))?
        .extend(["search", "autocomplete", name]);
    let req = http.get(url).bearer_auth(key.expose());
    read(req, "searching SteamGridDB").await
}

/// The covers or backgrounds of a SteamGridDB game, the community's favourites first. Still images
/// only, without nudity or jokes.
pub async fn art(http: &Client, key: &Secret, game: u64, kind: Kind) -> Result<Vec<Art>> {
    art_at(http, API, key, game, kind).await
}

async fn art_at(http: &Client, api: &str, key: &Secret, game: u64, kind: Kind) -> Result<Vec<Art>> {
    let (images, dimensions) = match kind {
        Kind::Cover => ("grids", "600x900,342x482,660x930"),
        Kind::Background => ("heroes", "1920x620,3840x1240,1600x650"),
    };
    let req = http
        .get(format!("{api}/{images}/game/{game}"))
        .query(&[
            ("dimensions", dimensions),
            ("types", "static"),
            ("nsfw", "false"),
            ("humor", "false"),
        ])
        .bearer_auth(key.expose());
    read(req, "listing SteamGridDB art").await
}

/// The list a request answers; none when SteamGridDB has nothing for it.
async fn read<T: serde::de::DeserializeOwned>(
    req: reqwest::RequestBuilder,
    context: &'static str,
) -> Result<Vec<T>> {
    match http::json::<Page<T>>(req, context).await {
        Ok(page) => Ok(page.data),
        Err(Error::Http { status: 404, .. }) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// The full image behind `url`, fetched only from SteamGridDB's own servers.
pub async fn download(http: &Client, url: &str) -> Result<Vec<u8>> {
    if !from_steamgriddb(url) {
        return Err(Error::Refused(format!("{url} is not on SteamGridDB")));
    }
    http::bytes(http.get(url), "downloading SteamGridDB art").await
}

fn from_steamgriddb(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str()
                .is_some_and(|h| h == "steamgriddb.com" || h.ends_with(".steamgriddb.com"))
    })
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// Answers one request with `status` and `body`, and hands back the request it received.
    async fn serve(
        status: &'static str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap();
            let reply = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (format!("http://{addr}/api/v2"), handle)
    }

    const GAMES: &str = r#"{"success":true,"data":[{"id":5254,"name":"[FAKE] Hollow Knight",
        "release_date":1488326400,"types":["steam","gog"],"verified":true},
        {"id":77,"name":"[FAKE] Hollow Knight: Silksong","types":[],"verified":true}]}"#;

    const GRIDS: &str = r#"{"success":true,"data":[{"id":1,"score":3,"style":"alternate",
        "width":600,"height":900,"nsfw":false,"humor":false,"mime":"image/png","language":"en",
        "url":"https://cdn2.steamgriddb.com/grid/fake.png",
        "thumb":"https://cdn2.steamgriddb.com/thumb/fake.jpg","lock":false,"epilepsy":false,
        "upvotes":0,"downvotes":0,"author":{"name":"[FAKE]"}}]}"#;

    #[tokio::test]
    async fn a_game_is_searched_by_its_name_with_the_key_in_a_header() {
        let (api, request) = serve("200 OK", GAMES).await;
        let key = Secret::new("[FAKE]-key");
        let games = search_at(&http::client().unwrap(), &api, &key, " Hollow Knight/2 ")
            .await
            .unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[0].id, 5254);
        assert_eq!(games[0].release_date, Some(1488326400));
        assert_eq!(games[1].release_date, None);
        let request = request.await.unwrap();
        let first = request.lines().next().unwrap();
        assert_eq!(
            first, "GET /api/v2/search/autocomplete/Hollow%20Knight%2F2 HTTP/1.1",
            "the name is one part of the path, whatever it holds"
        );
        assert!(
            request
                .to_lowercase()
                .contains("authorization: bearer [fake]-key")
        );
    }

    #[tokio::test]
    async fn covers_are_grids_of_the_chosen_game() {
        let (api, request) = serve("200 OK", GRIDS).await;
        let art = art_at(
            &http::client().unwrap(),
            &api,
            &Secret::new("k"),
            5254,
            Kind::Cover,
        )
        .await
        .unwrap();
        assert_eq!(art.len(), 1);
        assert_eq!(art[0].thumb, "https://cdn2.steamgriddb.com/thumb/fake.jpg");
        assert_eq!((art[0].width, art[0].height), (600, 900));
        let request = request.await.unwrap();
        let first = request.lines().next().unwrap();
        assert!(
            first.starts_with("GET /api/v2/grids/game/5254?dimensions=600x900"),
            "{first}"
        );
        assert!(first.contains("nsfw=false") && first.contains("types=static"));
    }

    #[tokio::test]
    async fn backgrounds_are_heroes_and_nothing_found_is_an_empty_list() {
        let (api, request) = serve("404 Not Found", r#"{"success":false}"#).await;
        let art = art_at(
            &http::client().unwrap(),
            &api,
            &Secret::new("k"),
            42,
            Kind::Background,
        )
        .await
        .unwrap();
        assert!(art.is_empty());
        assert!(
            request
                .await
                .unwrap()
                .starts_with("GET /api/v2/heroes/game/42?")
        );
        let none = search_at(&http::client().unwrap(), API, &Secret::new("k"), "  ").await;
        assert_eq!(none.unwrap(), Vec::new(), "nothing asked for an empty name");
    }

    #[tokio::test]
    async fn only_steamgriddb_images_are_downloaded() {
        let http = http::client().unwrap();
        assert!(from_steamgriddb("https://cdn2.steamgriddb.com/grid/a.png"));
        assert!(!from_steamgriddb("http://cdn2.steamgriddb.com/grid/a.png"));
        assert!(!from_steamgriddb(
            "https://steamgriddb.com.example.org/a.png"
        ));
        assert!(!from_steamgriddb(
            "https://example.org/steamgriddb.com/a.png"
        ));
        assert!(matches!(
            download(&http, "https://example.org/a.png").await,
            Err(Error::Refused(_))
        ));
    }
}
