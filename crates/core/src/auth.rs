use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::http;
use crate::secret::Secret;

pub const GALAXY_CLIENT_ID: &str = "46899977096215655";
const GALAXY_CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
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

impl From<TokenResponse> for Tokens {
    fn from(r: TokenResponse) -> Self {
        Self {
            user_id: r.user_id,
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            expires_at: Utc::now().timestamp() + r.expires_in,
        }
    }
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
        && input.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if looks_like_code {
        Ok(Secret::new(input))
    } else {
        Err(Error::InvalidLoginInput("expected the final URL or the code"))
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
        Err(Error::Http { status: 400 | 401 | 403, .. }) => Err(Error::InvalidLoginInput(
            "GOG rejected the code (expired, already used or mistyped)",
        )),
        other => other.map(Tokens::from),
    }
}

pub async fn refresh(http: &Client, tokens: &Tokens) -> Result<Tokens> {
    let req = http.get(TOKEN_URL).query(&[
        ("client_id", GALAXY_CLIENT_ID),
        ("client_secret", GALAXY_CLIENT_SECRET),
        ("grant_type", "refresh_token"),
        ("refresh_token", tokens.refresh_token.expose()),
    ]);
    token_request(req, "refreshing the session").await.map(Tokens::from)
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
    token_request(req, "obtaining a game token").await.map(|r| r.access_token)
}

async fn token_request(req: reqwest::RequestBuilder, context: &'static str) -> Result<TokenResponse> {
    match http::json::<TokenResponse>(req, context).await {
        Err(Error::Http { status: 400 | 401 | 403, .. }) => Err(Error::SessionRejected),
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
        assert_eq!(extract_code("AbC-123_xyzXYZ0987654").unwrap().expose(), "AbC-123_xyzXYZ0987654");
    }

    #[test]
    fn rejects_foreign_or_incomplete_input() {
        assert!(extract_code("https://evil.example/on_login_success?code=abcdefabcdefabcdef").is_err());
        assert!(extract_code("https://embed.gog.com/on_login_success?origin=client").is_err());
        assert!(extract_code("https://embed.gog.com/on_login_success?error=access_denied").is_err());
        assert!(extract_code("my password").is_err());
        assert!(extract_code("").is_err());
    }
}
