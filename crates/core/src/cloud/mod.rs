pub mod locations;
pub mod plan;
pub mod scan;
pub mod sync;
pub mod transport;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use reqwest::Client;

use crate::auth::{self, Tokens};
use crate::error::{Error, Result};
use crate::install::Install;
use locations::SaveLocation;
use transport::GogCloud;

pub struct GameCloud {
    pub transport: GogCloud,
    pub locations: Vec<(SaveLocation, Result<PathBuf>)>,
}

/// `None` when GOG has no cloud saves for this game.
pub async fn open(http: &Client, tokens: &Tokens, install: &Install) -> Result<Option<GameCloud>> {
    let client_id = install
        .client_id
        .as_deref()
        .ok_or_else(|| Error::Unsupported("the install has no Galaxy client id".into()))?;
    let Some(locations) = locations::fetch(http, client_id).await? else {
        return Ok(None);
    };
    let secret = locations::game_client_secret(http, tokens, &install.game_id, client_id).await?;
    let token = auth::game_access_token(http, tokens, client_id, &secret).await?;
    let transport = GogCloud::new(http.clone(), &tokens.user_id, client_id, token)?;
    let locations = locations
        .into_iter()
        .map(|l| {
            let root = locations::resolve(&l.location, install);
            (l, root)
        })
        .collect();
    Ok(Some(GameCloud {
        transport,
        locations,
    }))
}
