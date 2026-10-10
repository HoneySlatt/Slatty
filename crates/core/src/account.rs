use std::time::Duration;

use chrono::Utc;
use reqwest::Client;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

use crate::auth::{self, Tokens};
use crate::credentials;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::http;
use crate::lock;
use crate::paths::Dirs;
use crate::secret::Secret;

const ACTIVE_USER: &str = "active_user";

#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub user_id: String,
    pub username: String,
}

/// Avatar picture of a GOG user, from the public profile (no token needed).
pub async fn avatar_url(http: &Client, user_id: &str) -> Result<Option<String>> {
    #[derive(Deserialize)]
    struct Profile {
        avatar: Option<Avatar>,
    }
    #[derive(Deserialize)]
    struct Avatar {
        medium_2x: Option<String>,
    }
    let profile: Profile = http::json(
        http.get(format!("https://users.gog.com/users/{user_id}")),
        "fetching the avatar",
    )
    .await?;
    Ok(profile.avatar.and_then(|a| a.medium_2x))
}

#[derive(Debug, Clone, Copy)]
pub struct RotationReport {
    pub rotated: bool,
    pub old_still_valid: bool,
}

pub struct Account {
    pub info: AccountInfo,
    tokens: Tokens,
    dirs: Dirs,
}

impl Account {
    pub async fn login(http: &Client, db: &Db, dirs: &Dirs, code: &Secret) -> Result<Account> {
        let tokens = auth::exchange_code(http, code).await?;
        let username = fetch_username(http, &tokens).await?;
        store_tokens(tokens.clone()).await?;
        db.conn().execute(
            "INSERT INTO accounts (user_id, username, added_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id) DO UPDATE SET username = excluded.username",
            params![tokens.user_id, username, Utc::now().timestamp()],
        )?;
        db.set_setting(ACTIVE_USER, Some(&tokens.user_id))?;
        Ok(Account {
            info: AccountInfo {
                user_id: tokens.user_id.clone(),
                username,
            },
            tokens,
            dirs: dirs.clone(),
        })
    }

    pub fn active(db: &Db) -> Result<Option<AccountInfo>> {
        let Some(user_id) = db.setting(ACTIVE_USER)? else {
            return Ok(None);
        };
        let username: Option<String> = db
            .conn()
            .query_row(
                "SELECT username FROM accounts WHERE user_id = ?1",
                [&user_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(username.map(|username| AccountInfo { user_id, username }))
    }

    pub async fn load(db: &Db, dirs: &Dirs) -> Result<Account> {
        let info = Self::active(db)?.ok_or(Error::NotLoggedIn)?;
        Self::load_info(info, dirs).await
    }

    /// Loads credentials only for the account that started the operation.
    pub async fn load_for(db: &Db, dirs: &Dirs, user_id: &str) -> Result<Account> {
        let info = Self::active(db)?.ok_or(Error::NotLoggedIn)?;
        if info.user_id != user_id {
            return Err(Error::Refused(
                "the signed-in account changed; try again".into(),
            ));
        }
        Self::load_info(info, dirs).await
    }

    async fn load_info(info: AccountInfo, dirs: &Dirs) -> Result<Account> {
        let tokens = load_tokens(info.user_id.clone())
            .await?
            .ok_or(Error::NotLoggedIn)?;
        Ok(Account {
            info,
            tokens,
            dirs: dirs.clone(),
        })
    }

    pub fn expires_at(&self) -> i64 {
        self.tokens.expires_at
    }

    /// Valid Galaxy tokens, refreshed under a cross-process lock when expired.
    pub async fn tokens(&mut self, http: &Client) -> Result<&Tokens> {
        if self.tokens.is_expired() {
            self.refresh(http, false).await?;
        }
        Ok(&self.tokens)
    }

    pub async fn refresh(&mut self, http: &Client, force: bool) -> Result<()> {
        self.tokens = fresh(http, &self.dirs, &self.tokens, force).await?;
        Ok(())
    }

    /// Characterises refresh-token rotation; always keeps the latest token issued.
    pub async fn probe_rotation(&mut self, http: &Client) -> Result<RotationReport> {
        let lock_path = self
            .dirs
            .locks()
            .join(format!("auth-{}.lock", self.info.user_id));
        let _lock = lock::acquire(&lock_path, Duration::from_secs(30)).await?;
        if let Some(stored) = load_tokens(self.info.user_id.clone()).await? {
            self.tokens = stored;
        }
        let old = self.tokens.clone();
        let first = auth::refresh(http, &old).await?;
        store_tokens(first.clone()).await?;
        let rotated = first.refresh_token != old.refresh_token;
        self.tokens = first;
        let old_still_valid = if rotated {
            match auth::refresh(http, &old).await {
                Ok(second) => {
                    store_tokens(second.clone()).await?;
                    self.tokens = second;
                    true
                }
                Err(Error::SessionRejected) => false,
                Err(e) => return Err(e),
            }
        } else {
            true
        };
        Ok(RotationReport {
            rotated,
            old_still_valid,
        })
    }

    pub async fn logout(self, db: &Db, dirs: &Dirs) -> Result<()> {
        let user_id = self.info.user_id.clone();
        // A refresh running now would store its tokens after they are deleted.
        let lock_path = dirs.locks().join(format!("auth-{user_id}.lock"));
        let _lock = lock::acquire(&lock_path, Duration::from_secs(30)).await?;
        tokio::task::spawn_blocking(move || credentials::delete(&user_id))
            .await
            .expect("keyring task panicked")?;
        db.set_setting(ACTIVE_USER, None)
    }
}

/// Valid tokens for the account `current` belongs to: the keyring copy when another process
/// already refreshed them, else a refresh, under a cross-process lock and stored.
pub async fn fresh(http: &Client, dirs: &Dirs, current: &Tokens, force: bool) -> Result<Tokens> {
    let lock_path = dirs.locks().join(format!("auth-{}.lock", current.user_id));
    let _lock = lock::acquire(&lock_path, Duration::from_secs(30)).await?;
    let (tokens, signed_in) = refresh_base(load_tokens(current.user_id.clone()).await?, current);
    if !force && !tokens.is_expired() {
        return Ok(tokens);
    }
    let fresh = auth::refresh(http, &tokens).await?;
    if fresh.user_id != current.user_id {
        return Err(Error::parse(
            "refreshing the session",
            "token belongs to another account",
        ));
    }
    if signed_in {
        store_tokens(fresh.clone()).await?;
    }
    Ok(fresh)
}

/// The tokens a refresh starts from, and whether the account is still signed in. With nothing in
/// the keyring, the account was signed out while work kept running: the work may refresh its own
/// copy, but storing it would sign the account back in.
fn refresh_base(stored: Option<Tokens>, current: &Tokens) -> (Tokens, bool) {
    match stored {
        Some(t) => (t, true),
        None => (current.clone(), false),
    }
}

async fn store_tokens(tokens: Tokens) -> Result<()> {
    tokio::task::spawn_blocking(move || credentials::save(&tokens))
        .await
        .expect("keyring task panicked")
}

async fn load_tokens(user_id: String) -> Result<Option<Tokens>> {
    tokio::task::spawn_blocking(move || credentials::load(&user_id))
        .await
        .expect("keyring task panicked")
}

async fn fetch_username(http: &Client, tokens: &Tokens) -> Result<String> {
    #[derive(Deserialize)]
    struct User {
        username: String,
    }
    let url = format!("https://users.gog.com/users/{}", tokens.user_id);
    let req = http.get(url).bearer_auth(tokens.access_token.expose());
    Ok(http::json::<User>(req, "fetching the user profile")
        .await?
        .username)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn account_work_refuses_a_switch_before_accessing_credentials() {
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute(
                "INSERT INTO accounts VALUES ('new', '[FAKE] New account', 0)",
                [],
            )
            .unwrap();
        db.set_setting(ACTIVE_USER, Some("new")).unwrap();
        let dirs = Dirs::under(
            &std::env::temp_dir().join(format!("slatty-account-switch-{}", std::process::id())),
        );
        assert!(matches!(
            Account::load_for(&db, &dirs, "old").await,
            Err(Error::Refused(_))
        ));
        db.set_setting(ACTIVE_USER, None).unwrap();
        assert!(matches!(
            Account::load_for(&db, &dirs, "old").await,
            Err(Error::NotLoggedIn)
        ));
    }

    #[test]
    fn a_refresh_after_signing_out_does_not_sign_the_account_back_in() {
        let tokens = |access: &str| Tokens {
            user_id: "1".into(),
            access_token: Secret::new(access),
            refresh_token: Secret::new("[FAKE] refresh"),
            expires_at: 0,
        };
        let (from, store) = refresh_base(Some(tokens("[FAKE] stored")), &tokens("[FAKE] mine"));
        assert!(store && from.access_token.expose() == "[FAKE] stored");
        let (from, store) = refresh_base(None, &tokens("[FAKE] mine"));
        assert!(!store && from.access_token.expose() == "[FAKE] mine");
    }
}
