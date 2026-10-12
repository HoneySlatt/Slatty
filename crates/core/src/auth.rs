use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::http;
use crate::secret::Secret;

// GOG Galaxy's own OAuth client, public: it ships with every Galaxy install, and heroic-gogdl and
// gogapidocs use the same. GOG accepts no other client for its sign-in page.
pub const GALAXY_CLIENT_ID: &str = "46899977096215655";
const GALAXY_CLIENT_SECRET: &str =
    "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
const REDIRECT_URI: &str = "https://embed.gog.com/on_login_success?origin=client";
const AUTH_URL: &str = "https://auth.gog.com/auth";
const TOKEN_URL: &str = "https://auth.gog.com/token";
const EXPIRY_MARGIN_SECS: i64 = 120;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub user_id: String,
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_at: i64,
}

impl Tokens {
    pub fn is_expired(&self) -> bool {
        Utc::now().timestamp() >= self.expires_at - EXPIRY_MARGIN_SECS
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Secret,
    refresh_token: Secret,
    expires_in: i64,
    user_id: String,
}

impl TryFrom<TokenResponse> for Tokens {
    type Error = Error;

    fn try_from(r: TokenResponse) -> Result<Self> {
        crate::paths::check_user_id(&r.user_id)?;
        Ok(Self {
            user_id: r.user_id,
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            expires_at: Utc::now().timestamp() + r.expires_in,
        })
    }
}

/// Opens the system browser without letting it write into our terminal.
pub fn open_in_browser(url: &Url) -> std::io::Result<()> {
    open_with_desktop(url.as_str().as_ref())
}

/// Opens an address or a file with the program the desktop chooses (`xdg-open`), without letting
/// it write into our terminal.
pub fn open_with_desktop(target: &std::ffi::OsStr) -> std::io::Result<()> {
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    spawn_reaped(&mut command).map(drop)
}

/// Starts `command` and reaps it once it exits, rather than leaving it a zombie for as long as
/// the launcher runs. Its process id.
fn spawn_reaped(command: &mut std::process::Command) -> std::io::Result<u32> {
    let mut child = command.spawn()?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(pid)
}

pub fn login_url() -> Url {
    Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", GALAXY_CLIENT_ID),
            ("redirect_uri", REDIRECT_URI),
            ("response_type", "code"),
            ("layout", "galaxy"),
        ],
    )
    .expect("static URL is valid")
}

/// Accepts either the full `on_login_success` URL or the bare code.
pub fn extract_code(input: &str) -> Result<Secret> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::InvalidLoginInput("nothing was pasted"));
    }
    if let Ok(url) = Url::parse(input) {
        let host_ok = url
            .host_str()
            .is_some_and(|h| h == "gog.com" || h.ends_with(".gog.com"));
        if !host_ok {
            return Err(Error::InvalidLoginInput("the URL is not a gog.com URL"));
        }
        if url.query_pairs().any(|(k, _)| k == "error") {
            return Err(Error::InvalidLoginInput("GOG reported a login error"));
        }
        return url
            .query_pairs()
            .find(|(k, _)| k == "code")
            .map(|(_, v)| Secret::new(v.into_owned()))
            .filter(|c| !c.expose().is_empty())
            .ok_or(Error::InvalidLoginInput("the URL has no `code` parameter"));
    }
    let looks_like_code = input.len() >= 16
        && input
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if looks_like_code {
        Ok(Secret::new(input))
    } else {
        Err(Error::InvalidLoginInput(
            "expected the final URL or the code",
        ))
    }
}

pub async fn exchange_code(http: &Client, code: &Secret) -> Result<Tokens> {
    let req = http.get(TOKEN_URL).query(&[
        ("client_id", GALAXY_CLIENT_ID),
        ("client_secret", GALAXY_CLIENT_SECRET),
        ("grant_type", "authorization_code"),
        ("code", code.expose()),
        ("redirect_uri", REDIRECT_URI),
    ]);
    match http::json::<TokenResponse>(req, "exchanging the login code").await {
        Err(Error::Http {
            status: 400 | 401 | 403,
            ..
        }) => Err(Error::InvalidLoginInput(
            "GOG rejected the code (expired, already used or mistyped)",
        )),
        other => other.and_then(Tokens::try_from),
    }
}

pub async fn refresh(http: &Client, tokens: &Tokens) -> Result<Tokens> {
    let req = http.get(TOKEN_URL).query(&[
        ("client_id", GALAXY_CLIENT_ID),
        ("client_secret", GALAXY_CLIENT_SECRET),
        ("grant_type", "refresh_token"),
        ("refresh_token", tokens.refresh_token.expose()),
    ]);
    token_request(req, "refreshing the session")
        .await
        .and_then(Tokens::try_from)
}

/// Token scoped to a game's own Galaxy client, used by cloud storage.
pub async fn game_access_token(
    http: &Client,
    galaxy: &Tokens,
    client_id: &str,
    client_secret: &Secret,
) -> Result<Secret> {
    let req = http.get(TOKEN_URL).query(&[
        ("client_id", client_id),
        ("client_secret", client_secret.expose()),
        ("grant_type", "refresh_token"),
        ("refresh_token", galaxy.refresh_token.expose()),
        ("without_new_session", "1"),
    ]);
    token_request(req, "obtaining a game token")
        .await
        .map(|r| r.access_token)
}

async fn token_request(
    req: reqwest::RequestBuilder,
    context: &'static str,
) -> Result<TokenResponse> {
    match http::json::<TokenResponse>(req, context).await {
        Err(Error::Http {
            status: 400 | 401 | 403,
            ..
        }) => Err(Error::SessionRejected),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_targets_galaxy_client() {
        let url = login_url();
        assert_eq!(url.host_str(), Some("auth.gog.com"));
        let pairs: Vec<_> = url.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("client_id".into(), GALAXY_CLIENT_ID.into())));
        assert!(pairs.contains(&("redirect_uri".into(), REDIRECT_URI.into())));
    }

    #[test]
    fn extracts_code_from_final_url() {
        let code = extract_code(
            " https://embed.gog.com/on_login_success?origin=client&code=AbC-123_xyzXYZ0987654 \n",
        )
        .unwrap();
        assert_eq!(code.expose(), "AbC-123_xyzXYZ0987654");
    }

    #[test]
    fn accepts_bare_code() {
        assert_eq!(
            extract_code("AbC-123_xyzXYZ0987654").unwrap().expose(),
            "AbC-123_xyzXYZ0987654"
        );
    }

    /// The account's id names its cache folder, its save backups, its lock and its keyring entry:
    /// GOG's ids are digits, and anything else in a token answer is refused.
    #[test]
    fn a_token_answer_for_an_odd_account_id_is_refused() {
        let answer = |id: &str| -> TokenResponse {
            serde_json::from_value(serde_json::json!({
                "access_token": "[FAKE] access",
                "refresh_token": "[FAKE] refresh",
                "expires_in": 3600,
                "user_id": id,
            }))
            .unwrap()
        };
        assert_eq!(
            Tokens::try_from(answer("1234567890")).unwrap().user_id,
            "1234567890"
        );
        for id in ["", "../x", "/home/x", "1#/../..", "12 34"] {
            assert!(
                matches!(Tokens::try_from(answer(id)), Err(Error::Refused(_))),
                "{id:?}"
            );
        }
    }

    #[test]
    fn rejects_foreign_or_incomplete_input() {
        assert!(
            extract_code("https://evil.example/on_login_success?code=abcdefabcdefabcdef").is_err()
        );
        assert!(extract_code("https://embed.gog.com/on_login_success?origin=client").is_err());
        assert!(
            extract_code("https://embed.gog.com/on_login_success?error=access_denied").is_err()
        );
        assert!(extract_code("my password").is_err());
        assert!(extract_code("").is_err());
    }

    /// `xdg-open` itself would open a page on the desktop running the tests: `true` stands in.
    #[test]
    fn an_opener_is_not_left_a_zombie() {
        let pid = spawn_reaped(&mut std::process::Command::new("true")).unwrap();
        let proc = std::path::PathBuf::from(format!("/proc/{pid}"));
        let gone = (0..500).any(|_| {
            std::thread::sleep(std::time::Duration::from_millis(10));
            !proc.exists()
        });
        assert!(gone, "still a zombie after 5 s");
    }
}
