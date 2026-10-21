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

    /// Signs out the account signed in, and with `forget` deletes what Slatty keeps of it
    /// (`forget_data`); the account signed out. Its tokens are not needed: an account whose keyring
    /// entry is gone is signed out all the same.
    pub async fn sign_out(db: &Db, dirs: &Dirs, forget: bool) -> Result<AccountInfo> {
        Self::sign_out_with(db, dirs, forget, |user_id| credentials::delete(&user_id)).await
    }

    async fn sign_out_with(
        db: &Db,
        dirs: &Dirs,
        forget: bool,
        delete: impl FnOnce(String) -> Result<()> + Send + 'static,
    ) -> Result<AccountInfo> {
        let info = Self::active(db)?.ok_or(Error::NotLoggedIn)?;
        let user_id = info.user_id.clone();
        // A refresh running now would store its tokens after they are deleted.
        let lock_path = dirs.locks().join(format!("auth-{user_id}.lock"));
        let _lock = lock::acquire(&lock_path, Duration::from_secs(30)).await?;
        let id = user_id.clone();
        tokio::task::spawn_blocking(move || delete(id))
            .await
            .expect("keyring task panicked")?;
        db.set_setting(ACTIVE_USER, None)?;
        if forget {
            forget_data(db, dirs, &user_id)?;
        }
        Ok(info)
    }
}

/// Deletes what Slatty keeps of an account: its name, its play sessions (those not yet
/// sent to GOG included), its cloud sync history, its library cache (covers, key art, avatar) and
/// Comet's record of its achievements. Saves are never deleted: the games, their prefixes and own
/// homes, and the save backups stay.
pub fn forget_data(db: &Db, dirs: &Dirs, user_id: &str) -> Result<()> {
    // It names the folders deleted below.
    crate::paths::check_user_id(user_id)?;
    // The sessions of games added by hand stay: they are no account's (`session::playtime`).
    db.conn().execute(
        "DELETE FROM sessions WHERE user_id = ?1 AND game_id NOT LIKE 'local-%'",
        [user_id],
    )?;
    for table in ["sync_baseline", "sync_roots", "accounts"] {
        db.conn().execute(
            &format!("DELETE FROM {table} WHERE user_id = ?1"),
            [user_id],
        )?;
    }
    let remove = |dir: &std::path::Path| match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(Error::io(format!("delete {}", dir.display()), e))
        }
        _ => Ok(()),
    };
    remove(&dirs.account_cache(user_id))?;
    // Comet keeps a database per game client and account.
    let comet = dirs.data.join("comet/comet/gameplay");
    for client in std::fs::read_dir(&comet).into_iter().flatten().flatten() {
        remove(&client.path().join(user_id))?;
    }
    for lock in ["auth", "playtime"] {
        let _ = std::fs::remove_file(dirs.locks().join(format!("{lock}-{user_id}.lock")));
    }
    Ok(())
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

    /// Signing out needs no token: an account whose keyring entry is gone (a keyring reset, the
    /// entry deleted by hand, a restored data folder) is still signed out, and forgotten when
    /// asked, rather than refused as "not logged in" for good.
    #[tokio::test]
    async fn an_account_whose_tokens_are_gone_can_still_be_signed_out() {
        let root = std::env::temp_dir().join(format!("slatty-sign-out-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO accounts VALUES ('111', '[FAKE] First', 0);
                 INSERT INTO sessions (game_id, user_id, started_at, state) VALUES
                   ('1', '111', 10, 'ended');",
            )
            .unwrap();
        db.set_setting(ACTIVE_USER, Some("111")).unwrap();
        let deleted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = deleted.clone();
        let info = Account::sign_out_with(&db, &dirs, true, move |id| {
            seen.lock().unwrap().push(id);
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(info.user_id, "111");
        assert!(Account::active(&db).unwrap().is_none());
        assert_eq!(*deleted.lock().unwrap(), ["111"]);
        let sessions: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 0, "forgotten");
        let _ = std::fs::remove_dir_all(root);
    }

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

    /// Forgetting an account deletes what the launcher keeps of it, and nothing of another account
    /// or of the games: saves, prefixes, the games' own homes and the save backups stay.
    #[test]
    fn forgetting_an_account_deletes_its_data_and_never_saves() {
        let root = std::env::temp_dir().join(format!("slatty-forget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        let db = Db::in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO accounts VALUES ('111', '[FAKE] First', 0), ('222', '[FAKE] Second', 0);
                 INSERT INTO sessions (game_id, user_id, started_at, state) VALUES
                   ('1', '111', 10, 'ended'), ('1', '222', 20, 'ended');
                 INSERT INTO sync_roots (user_id, game_id, location, root) VALUES
                   ('111', '1', 'saves', '/r'), ('222', '1', 'saves', '/r');",
            )
            .unwrap();
        let files = |id: &str| {
            [
                dirs.account_cache(id).join("library.json"),
                dirs.data
                    .join(format!("comet/comet/gameplay/9/{id}/gameplay.db")),
                dirs.backups(id, "1")
                    .join("20261011-000000/saves/local/a.sav"),
            ]
        };
        let kept = [
            dirs.data.join("prefixes/1/drive_c/users/steamuser/save"),
            dirs.data.join("homes/1/save"),
        ];
        for file in files("111").iter().chain(&files("222")).chain(&kept) {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, "[FAKE]").unwrap();
        }
        forget_data(&db, &dirs, "111").unwrap();
        let rows = |table: &str, id: &str| -> i64 {
            db.conn()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE user_id = ?1"),
                    [id],
                    |r| r.get(0),
                )
                .unwrap()
        };
        for table in ["accounts", "sessions", "sync_roots"] {
            assert_eq!((rows(table, "111"), rows(table, "222")), (0, 1), "{table}");
        }
        let [cache, comet, backup] = files("111");
        assert!(!cache.exists() && !comet.exists());
        assert!(backup.exists(), "save backups are never deleted");
        assert!(files("222").iter().chain(&kept).all(|f| f.exists()));
        assert!(matches!(
            forget_data(&db, &dirs, ".."),
            Err(Error::Refused(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
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
