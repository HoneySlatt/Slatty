//! Signing in to Epic Games with the Epic Games Launcher's own OAuth client, as legendary does
//! (`legendary/api/egs.py`): the person signs in on Epic's page in their browser, which then shows
//! a single-use authorization code; that code is exchanged for tokens, kept in the keyring.

use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::http;
use crate::paths::Dirs;
use crate::secret::Secret;
use crate::{credentials, lock, settings};

// The Epic Games Launcher's OAuth client, public: it ships with the launcher, and legendary and
// Heroic use the same.
const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";
const TOKEN_URL: &str =
    "https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/token";
const EXPIRY_MARGIN_SECS: i64 = 120;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub account_id: String,
    pub display_name: String,
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
    account_id: String,
    #[serde(rename = "displayName", default)]
    display_name: String,
}

impl TryFrom<TokenResponse> for Tokens {
    type Error = Error;

    fn try_from(r: TokenResponse) -> Result<Self> {
        check_account_id(&r.account_id)?;
        Ok(Self {
            account_id: r.account_id,
            display_name: r.display_name,
            access_token: r.access_token,
            refresh_token: r.refresh_token,
            expires_at: Utc::now().timestamp() + r.expires_in,
        })
    }
}

/// Refuses an account id that could not safely name a folder, a lock or a keyring entry: Epic's
/// are 32 hexadecimal digits.
pub fn check_account_id(account_id: &str) -> Result<()> {
    if account_id.len() != 32 || !account_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::Refused(format!(
            "unexpected Epic account id `{account_id}`"
        )));
    }
    Ok(())
}

/// Epic's sign-in page, which ends on a page showing the authorization code.
pub fn login_url() -> Url {
    let redirect =
        format!("https://www.epicgames.com/id/api/redirect?clientId={CLIENT_ID}&responseType=code");
    Url::parse_with_params(
        "https://www.epicgames.com/id/login",
        &[("redirectUrl", redirect)],
    )
    .expect("static URL is valid")
}

/// Accepts the whole page Epic shows after signing in (`{"authorizationCode": "…", …}`) or the
/// bare code.
pub fn extract_code(input: &str) -> Result<Secret> {
    let input = input.trim();
    if input.is_empty() {
        return Err(Error::InvalidLoginInput("nothing was pasted"));
    }
    if input.starts_with('{') {
        #[derive(Deserialize)]
        struct Page {
            #[serde(rename = "authorizationCode")]
            code: Option<String>,
        }
        let page: Page = serde_json::from_str(input)
            .map_err(|_| Error::InvalidLoginInput("the pasted page is incomplete"))?;
        return page
            .code
            .filter(|c| looks_like_code(c))
            .map(Secret::new)
            .ok_or(Error::InvalidLoginInput(
                "the page has no authorization code: sign in again",
            ));
    }
    if looks_like_code(input) {
        Ok(Secret::new(input))
    } else {
        Err(Error::InvalidLoginInput(
            "expected the page Epic shows, or its authorizationCode",
        ))
    }
}

fn looks_like_code(code: &str) -> bool {
    code.len() >= 16 && code.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub async fn exchange_code(http: &Client, code: &Secret) -> Result<Tokens> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.expose()),
        ("token_type", "eg1"),
    ];
    match token_request(http, &form, "exchanging the Epic sign-in code").await {
        Err(Error::Http {
            status: 400 | 401 | 403,
            ..
        }) => Err(Error::InvalidLoginInput(
            "Epic rejected the code (expired, already used or mistyped)",
        )),
        other => other.and_then(Tokens::try_from),
    }
}

pub async fn refresh(http: &Client, tokens: &Tokens) -> Result<Tokens> {
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", tokens.refresh_token.expose()),
        ("token_type", "eg1"),
    ];
    match token_request(http, &form, "refreshing the Epic session").await {
        Err(Error::Http {
            status: 400 | 401 | 403,
            ..
        }) => Err(Error::EpicSessionRejected),
        other => other.and_then(Tokens::try_from),
    }
}

/// Sent once: a code or a refresh token is used up by the first answer, even one lost on the way.
async fn token_request(
    http: &Client,
    form: &[(&str, &str)],
    context: &'static str,
) -> Result<TokenResponse> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form)
        .finish();
    let req = http
        .post(TOKEN_URL)
        .basic_auth(CLIENT_ID, Some(CLIENT_SECRET))
        .header(reqwest::header::USER_AGENT, super::USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body);
    let raw = http::read_up_to(http::send(req, context).await?, context, http::MAX_BODY).await?;
    serde_json::from_slice(&raw).map_err(|e| Error::parse(context, e))
}

/// Signs in with the code Epic showed: the tokens go to the keyring, the account id to the
/// settings.
pub async fn sign_in(http: &Client, db: &Db, code: &Secret) -> Result<Tokens> {
    let tokens = exchange_code(http, code).await?;
    store(tokens.clone()).await?;
    settings::set_epic_account(db, Some(&tokens.account_id))?;
    Ok(tokens)
}

/// Signs the Epic account out: its tokens leave the keyring. Its library cache stays.
pub fn sign_out(db: &Db) -> Result<Option<String>> {
    let Some(account_id) = settings::epic_account(db)? else {
        return Ok(None);
    };
    credentials::delete_epic(&account_id)?;
    settings::set_epic_account(db, None)?;
    Ok(Some(account_id))
}

/// Valid tokens of the account signed in: the keyring's, refreshed when they expired, under a
/// cross-process lock (the command line and the window may both ask).
pub async fn fresh(http: &Client, db: &Db, dirs: &Dirs) -> Result<Tokens> {
    let account_id = settings::epic_account(db)?.ok_or(Error::NotLoggedIn)?;
    check_account_id(&account_id)?;
    let lock_path = dirs.locks().join(format!("epic-auth-{account_id}.lock"));
    let _lock = lock::acquire(&lock_path, Duration::from_secs(30)).await?;
    let id = account_id.clone();
    let tokens = tokio::task::spawn_blocking(move || credentials::load_epic(&id))
        .await
        .expect("keyring task panicked")?
        .ok_or(Error::NotLoggedIn)?;
    if !tokens.is_expired() {
        return Ok(tokens);
    }
    let fresh = refresh(http, &tokens).await?;
    if fresh.account_id != account_id {
        return Err(Error::parse(
            "refreshing the Epic session",
            "token belongs to another account",
        ));
    }
    store(fresh.clone()).await?;
    Ok(fresh)
}

async fn store(tokens: Tokens) -> Result<()> {
    tokio::task::spawn_blocking(move || credentials::save_epic(&tokens))
        .await
        .expect("keyring task panicked")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_url_ends_on_the_launchers_code_page() {
        let url = login_url();
        assert_eq!(url.host_str(), Some("www.epicgames.com"));
        let (_, redirect) = url.query_pairs().find(|(k, _)| k == "redirectUrl").unwrap();
        let redirect = Url::parse(&redirect).unwrap();
        assert_eq!(redirect.path(), "/id/api/redirect");
        let pairs: Vec<_> = redirect.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("clientId".into(), CLIENT_ID.into())));
        assert!(pairs.contains(&("responseType".into(), "code".into())));
    }

    #[test]
    fn extracts_the_code_from_the_page_or_alone() {
        let page = r#" {"warning":"[FAKE]","redirectUrl":"https://localhost/launcher/authorized?code=0123456789abcdef0123456789abcdef","authorizationCode":"0123456789abcdef0123456789abcdef","exchangeCode":null,"sid":null} "#;
        assert_eq!(
            extract_code(page).unwrap().expose(),
            "0123456789abcdef0123456789abcdef"
        );
        assert_eq!(
            extract_code("0123456789abcdef0123456789abcdef\n")
                .unwrap()
                .expose(),
            "0123456789abcdef0123456789abcdef"
        );
    }

    #[test]
    fn rejects_a_page_without_a_code_or_anything_else() {
        for input in [
            "",
            r#"{"authorizationCode":null,"exchangeCode":null}"#,
            r#"{"authorizationCode":"../../etc"}"#,
            r#"{"authorizationCode":"#,
            "my password",
            "https://www.epicgames.com/id/login",
        ] {
            assert!(extract_code(input).is_err(), "{input:?}");
        }
    }

    /// The account's id names its cache folder, its lock and its keyring entry: anything but 32
    /// hexadecimal digits in a token answer is refused.
    #[test]
    fn a_token_answer_for_an_odd_account_id_is_refused() {
        let answer = |id: &str| -> TokenResponse {
            serde_json::from_value(serde_json::json!({
                "access_token": "[FAKE] access",
                "refresh_token": "[FAKE] refresh",
                "expires_in": 7200,
                "account_id": id,
                "displayName": "[FAKE] player",
            }))
            .unwrap()
        };
        let tokens = Tokens::try_from(answer("0123456789abcdef0123456789abcdef")).unwrap();
        assert_eq!(tokens.display_name, "[FAKE] player");
        assert!(!tokens.is_expired());
        for id in ["", "../x", "0123456789abcdef0123456789abcde/", "12 34"] {
            assert!(
                matches!(Tokens::try_from(answer(id)), Err(Error::Refused(_))),
                "{id:?}"
            );
        }
    }

    #[test]
    fn signing_out_with_nobody_signed_in_changes_nothing() {
        let db = Db::in_memory().unwrap();
        assert_eq!(sign_out(&db).unwrap(), None);
    }
}
