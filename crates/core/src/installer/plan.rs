//! What to install: the build, language and DLC chosen for a game.

use std::collections::HashSet;
use std::path::{Component, Path};

use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::galaxy::{self, Build, Depot, Meta};

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub game_id: String,
    pub title: String,
    pub build: Build,
    pub meta: Meta,
    pub language: String,
    pub languages: Vec<String>,
    pub depots: Vec<Depot>,
    pub dlcs: Vec<DlcChoice>,
    /// Resolved from GOG's dependency repository by `plan_for`.
    pub dependencies: Vec<galaxy::Dependency>,
    pub download_size: u64,
    pub disk_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DlcChoice {
    pub id: String,
    pub name: String,
    pub owned: bool,
    pub selected: bool,
    /// For the chosen language.
    pub download_size: u64,
    pub disk_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DlcSelection {
    AllOwned,
    Only(Vec<String>),
}

impl InstallPlan {
    pub fn selected_dlcs(&self) -> Vec<String> {
        self.dlcs
            .iter()
            .filter(|d| d.selected)
            .map(|d| d.id.clone())
            .collect()
    }

    /// Selects the depots of the base game and of the chosen owned DLC for one language.
    pub fn new(
        game_id: &str,
        build: Build,
        meta: Meta,
        language: Option<&str>,
        owned: &HashSet<String>,
        selection: &DlcSelection,
    ) -> Result<Self> {
        if build.generation != 2 || meta.version == Some(1) {
            return Err(Error::Unsupported(
                "only generation 2 (Galaxy) builds are supported".into(),
            ));
        }
        if meta.base_product_id != game_id {
            return Err(Error::Unsupported(format!(
                "build belongs to product {}",
                meta.base_product_id
            )));
        }
        let base: Vec<&Depot> = meta
            .depots
            .iter()
            .filter(|d| d.product_id == game_id)
            .collect();
        let mut languages: Vec<String> = Vec::new();
        for l in base.iter().flat_map(|d| &d.languages).filter(|l| *l != "*") {
            if !languages.iter().any(|x| x.eq_ignore_ascii_case(l)) {
                languages.push(l.clone());
            }
        }
        let language = match language {
            // A build without language packs (every depot is `*`) is planned as `*`; its install,
            // resumed job or update asks for `*` again.
            Some("*") if languages.is_empty() => "*".to_string(),
            Some(wanted) => languages
                .iter()
                .find(|l| l.eq_ignore_ascii_case(wanted))
                .cloned()
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "language `{wanted}` not offered; available: {}",
                        languages.join(", ")
                    ))
                })?,
            None => ["en-US", "en", "English"]
                .iter()
                .find_map(|p| languages.iter().find(|l| l.eq_ignore_ascii_case(p)))
                .or(languages.first())
                .cloned()
                .unwrap_or_else(|| "*".into()),
        };
        let speaks = |d: &&Depot| {
            d.languages
                .iter()
                .any(|l| l == "*" || l.eq_ignore_ascii_case(&language))
        };
        if !base.iter().any(speaks) {
            return Err(Error::NotFound("no depot for this language".into()));
        }
        let mut dlcs: Vec<DlcChoice> = meta
            .products
            .iter()
            .filter(|p| p.product_id != game_id)
            .filter(|p| meta.depots.iter().any(|d| d.product_id == p.product_id))
            .map(|p| {
                let depots = || {
                    meta.depots
                        .iter()
                        .filter(|d| d.product_id == p.product_id)
                        .filter(speaks)
                };
                DlcChoice {
                    id: p.product_id.clone(),
                    name: p.name.clone(),
                    owned: owned.contains(&p.product_id),
                    selected: false,
                    download_size: depots().map(|d| d.compressed_size).sum(),
                    disk_size: depots().map(|d| d.size).sum(),
                }
            })
            .collect();
        for dlc in &mut dlcs {
            dlc.selected = dlc.owned
                && match selection {
                    DlcSelection::AllOwned => true,
                    DlcSelection::Only(ids) => ids.contains(&dlc.id),
                };
        }
        if let DlcSelection::Only(ids) = selection
            && let Some(missing) = ids
                .iter()
                .find(|id| !dlcs.iter().any(|d| d.selected && &d.id == *id))
        {
            return Err(Error::Refused(format!(
                "DLC {missing} is not owned or not part of this game"
            )));
        }
        let depots: Vec<Depot> = meta
            .depots
            .iter()
            .filter(|d| {
                d.product_id == game_id || dlcs.iter().any(|c| c.selected && c.id == d.product_id)
            })
            .filter(speaks)
            .cloned()
            .collect();
        let title = meta
            .products
            .iter()
            .find(|p| p.product_id == game_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| meta.install_directory.clone());
        Ok(Self {
            game_id: game_id.to_string(),
            title,
            download_size: depots.iter().map(|d| d.compressed_size).sum(),
            disk_size: depots.iter().map(|d| d.size).sum(),
            build,
            meta,
            language,
            languages,
            depots,
            dlcs,
            dependencies: Vec::new(),
        })
    }

    pub fn directory_name(&self) -> Result<String> {
        let name = self.meta.install_directory.trim();
        match Path::new(name).components().collect::<Vec<_>>().as_slice() {
            [Component::Normal(_)] => Ok(name.to_string()),
            _ => Err(Error::Refused(format!("unsafe install directory `{name}`"))),
        }
    }
}

/// Public build by default; a pinned build id (from an interrupted job) must still exist.
pub async fn plan_for(
    http: &reqwest::Client,
    tokens: &Tokens,
    game_id: &str,
    language: Option<&str>,
    build_id: Option<&str>,
    dlcs: &DlcSelection,
) -> Result<InstallPlan> {
    let builds = galaxy::builds(http, tokens, game_id).await?;
    let build = match build_id {
        Some(id) => builds.iter().find(|b| b.build_id == id).ok_or_else(|| {
            Error::NotFound(format!(
                "build {id} is no longer offered; restart the install"
            ))
        })?,
        None => builds
            .iter()
            .find(|b| b.branch.is_none())
            .or(builds.first())
            .ok_or_else(|| Error::Unsupported("no Windows Galaxy build for this game".into()))?,
    }
    .clone();
    let meta = galaxy::meta(http, &build).await?;
    let has_dlc = meta.products.iter().any(|p| p.product_id != game_id);
    let owned = if has_dlc {
        galaxy::owned_products(http, tokens).await?
    } else {
        HashSet::new()
    };
    let mut plan = InstallPlan::new(game_id, build, meta, language, &owned, dlcs)?;
    if !plan.meta.dependencies.is_empty() {
        let repository = galaxy::dependencies(http, tokens).await?;
        plan.dependencies = plan
            .meta
            .dependencies
            .iter()
            .filter_map(|id| repository.iter().find(|d| &d.dependency_id == id).cloned())
            .collect();
        for dep in plan.dependencies.iter().filter(|d| !d.is_shared()) {
            plan.depots.push(dep.depot());
            plan.download_size += dep.compressed_size;
            plan.disk_size += dep.size;
        }
    }
    Ok(plan)
}
