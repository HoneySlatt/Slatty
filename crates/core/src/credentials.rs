use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::secret::Secret;

const SERVICE: &str = "slatty";

fn entry(user_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("gog:{user_id}")).map_err(describe)
}

fn describe(e: keyring::Error) -> Error {
    let msg = match e {
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            "stored credential is unreadable".to_string()
        }
        other => other.to_string(),
    };
    Error::Keyring(msg)
}

pub fn save(tokens: &Tokens) -> Result<()> {
    let json = serde_json::to_string(tokens).map_err(|e| Error::Keyring(e.to_string()))?;
    entry(&tokens.user_id)?
        .set_password(&json)
        .map_err(describe)
}

pub fn load(user_id: &str) -> Result<Option<Tokens>> {
    match entry(user_id)?.get_password() {
        Ok(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|_| Error::Keyring("stored credential is unreadable".into())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(describe(e)),
    }
}

pub fn delete(user_id: &str) -> Result<()> {
    match entry(user_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(describe(e)),
    }
}

/// The Epic Games account's tokens, under its account id.
fn epic_entry(account_id: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("epic:{account_id}")).map_err(describe)
}

pub fn save_epic(tokens: &crate::epic::auth::Tokens) -> Result<()> {
    let json = serde_json::to_string(tokens).map_err(|e| Error::Keyring(e.to_string()))?;
    epic_entry(&tokens.account_id)?
        .set_password(&json)
        .map_err(describe)
}

pub fn load_epic(account_id: &str) -> Result<Option<crate::epic::auth::Tokens>> {
    match epic_entry(account_id)?.get_password() {
        Ok(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|_| Error::Keyring("stored credential is unreadable".into())),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(describe(e)),
    }
}

pub fn delete_epic(account_id: &str) -> Result<()> {
    match epic_entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(describe(e)),
    }
}

/// Another secret of slatty's, kept under its own name (an API key).
fn key_entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("key:{name}")).map_err(describe)
}

pub fn save_key(name: &str, key: &Secret) -> Result<()> {
    key_entry(name)?
        .set_password(key.expose())
        .map_err(describe)
}

pub fn load_key(name: &str) -> Result<Option<Secret>> {
    match key_entry(name)?.get_password() {
        Ok(key) => Ok(Some(Secret::new(key))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(describe(e)),
    }
}

pub fn delete_key(name: &str) -> Result<()> {
    match key_entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(describe(e)),
    }
}

#[cfg(test)]
mod tests {
    /// The keyring finds a sign-in and the keys by this name, given in SECURITY.md and the user
    /// guide: changing it makes everyone sign in and enter their keys again.
    #[test]
    fn secrets_are_kept_under_the_programs_name() {
        assert_eq!(super::SERVICE, "slatty");
    }
}
