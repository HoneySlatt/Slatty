use crate::auth::Tokens;
use crate::error::{Error, Result};

const SERVICE: &str = "slatty-launcher";

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
    entry(&tokens.user_id)?.set_password(&json).map_err(describe)
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
