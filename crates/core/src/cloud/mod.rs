pub mod inspect;
pub mod locations;
pub mod plan;
pub mod scan;
pub mod sync;
pub mod transport;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use reqwest::Client;

use crate::auth::Tokens;
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
    let (_, token) = crate::achievements::game_token(http, tokens, install).await?;
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

use crate::db::Db;
use crate::paths::Dirs;
use sync::{SyncOptions, SyncReport, SyncTarget};

pub struct LocationOutcome {
    pub name: String,
    pub template: String,
    pub root: Option<PathBuf>,
    pub result: Result<SyncReport>,
}

impl LocationOutcome {
    pub fn is_clean(&self) -> bool {
        matches!(&self.result, Ok(r) if r.is_clean())
    }
}

/// Syncs every save location of a game. `None` when GOG has no cloud saves for it.
pub async fn sync_game(
    db: &Db,
    dirs: &Dirs,
    http: &Client,
    tokens: &Tokens,
    install: &Install,
    opts: SyncOptions,
) -> Result<Option<Vec<LocationOutcome>>> {
    if !opts.dry_run && !crate::runner::prefix_ready(install) {
        return Err(Error::Refused(format!(
            "{} has not been launched yet: its cloud saves are downloaded when it first starts, \
             once Proton has created its Wine prefix",
            install.title
        )));
    }
    let Some(game_cloud) = open(http, tokens, install).await? else {
        return Ok(None);
    };
    let mut outcomes = Vec::new();
    for (location, root) in game_cloud.locations {
        let result = match &root {
            Err(e) => Err(Error::Unsupported(format!(
                "cannot resolve {}: {e}",
                location.location
            ))),
            Ok(root) => {
                let target = SyncTarget {
                    db,
                    dirs,
                    user_id: &tokens.user_id,
                    game_id: &install.game_id,
                    location: &location.name,
                    root,
                };
                sync::sync(&game_cloud.transport, &target, opts).await
            }
        };
        outcomes.push(LocationOutcome {
            name: location.name,
            template: location.location,
            root: root.ok(),
            result,
        });
    }
    Ok(Some(outcomes))
}
