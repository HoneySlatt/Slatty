//! What to install: the build, language and DLC chosen for a game.

use std::collections::HashSet;
use std::path::{Component, Path};

use super::linux::{self, Part};
use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::galaxy::{self, Build, Depot, Meta};
use crate::install::Platform;

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
    /// Every build GOG offers for the game, newest first; filled by `plan_for`.
    pub builds: Vec<Build>,
    pub platform: Platform,
    /// For a Linux build: the game's installer, then the chosen DLC's.
    pub linux: Vec<Part>,
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
        select(&mut dlcs, selection)?;
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
            builds: Vec::new(),
            platform: Platform::Windows,
            linux: Vec::new(),
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
    platform: Platform,
    language: Option<&str>,
    build_id: Option<&str>,
    dlcs: &DlcSelection,
) -> Result<InstallPlan> {
    if platform == Platform::Linux {
        return plan_linux(http, tokens, game_id, language, build_id, dlcs).await;
    }
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
    plan.builds = builds;
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

/// Marks the owned DLC to install; a DLC asked for by id must be owned and part of the game.
fn select(dlcs: &mut [DlcChoice], selection: &DlcSelection) -> Result<()> {
    for dlc in dlcs.iter_mut() {
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
    Ok(())
}

/// GOG's Linux installer of the game in one language, and those of its owned DLC. Their zip
/// directories are read now: the installers' sizes say little of the size on disk.
async fn plan_linux(
    http: &reqwest::Client,
    tokens: &Tokens,
    game_id: &str,
    language: Option<&str>,
    build_id: Option<&str>,
    selection: &DlcSelection,
) -> Result<InstallPlan> {
    let offer = linux::offer(http, tokens, game_id).await?;
    let mut languages: Vec<String> = Vec::new();
    for i in &offer.installers {
        if !languages
            .iter()
            .any(|l| l.eq_ignore_ascii_case(&i.language))
        {
            languages.push(i.language.clone());
        }
    }
    let language = match language {
        Some(wanted) => languages
            .iter()
            .find(|l| l.eq_ignore_ascii_case(wanted))
            .cloned()
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "language `{wanted}` not offered for Linux; available: {}",
                    languages.join(", ")
                ))
            })?,
        None => languages
            .iter()
            .find(|l| *l == "en")
            .or(languages.first())
            .cloned()
            .ok_or_else(|| Error::Unsupported("GOG offers no Linux build of this game".into()))?,
    };
    let base = linux::pick(&offer.installers, &language)
        .expect("a language comes from an installer")
        .clone();
    let build = Build {
        build_id: format!("{}{}", linux::BUILD_PREFIX, base.version),
        version_name: base.version.clone(),
        link: String::new(),
        branch: None,
        generation: 0,
        date_published: None,
    };
    if let Some(id) = build_id
        && id != build.build_id
    {
        return Err(Error::NotFound(format!(
            "build {id} is no longer offered; restart the install"
        )));
    }
    let owned = if offer.dlcs.is_empty() {
        HashSet::new()
    } else {
        galaxy::owned_products(http, tokens).await?
    };
    let mut dlcs: Vec<DlcChoice> = offer
        .dlcs
        .iter()
        .map(|(id, name, _)| DlcChoice {
            id: id.clone(),
            name: name.clone(),
            owned: owned.contains(id),
            selected: false,
            download_size: 0,
            disk_size: 0,
        })
        .collect();
    select(&mut dlcs, selection)?;
    // The game's installer, then the owned DLC's: GOG gives no link for a DLC not owned.
    let mut installers = vec![base];
    for (id, _, list) in &offer.dlcs {
        if owned.contains(id) {
            installers.push(linux::pick(list, &language).expect("never empty").clone());
        }
    }
    let source = linux::GogInstallers::new(
        http.clone(),
        tokens.clone(),
        None,
        installers.iter().map(|i| i.downlink.clone()).collect(),
    );
    let entries = futures::future::try_join_all(
        (0..installers.len()).map(|i| linux::read_entries(&source, i)),
    )
    .await?;
    let mut parts: Vec<Part> = installers
        .into_iter()
        .zip(entries)
        .map(|(installer, entries)| Part { installer, entries })
        .collect();
    for d in &mut dlcs {
        if let Some(p) = parts.iter().find(|p| p.installer.product_id == d.id) {
            d.download_size = p.download_size();
            d.disk_size = p.disk_size();
        }
    }
    parts.retain(|p| {
        p.installer.product_id == game_id
            || dlcs
                .iter()
                .any(|d| d.selected && d.id == p.installer.product_id)
    });
    Ok(InstallPlan {
        game_id: game_id.to_string(),
        title: offer.title.clone(),
        download_size: parts.iter().map(Part::download_size).sum(),
        disk_size: parts.iter().map(Part::disk_size).sum(),
        meta: Meta {
            version: None,
            base_product_id: game_id.to_string(),
            client_id: None,
            install_directory: folder_name(&offer.title),
            depots: Vec::new(),
            dependencies: Vec::new(),
            products: Vec::new(),
            script_interpreter: false,
        },
        builds: vec![build.clone()],
        build,
        language,
        languages,
        depots: Vec::new(),
        dlcs,
        dependencies: Vec::new(),
        platform: Platform::Linux,
        linux: parts,
    })
}

/// A folder name from a title: without the characters file systems or shells mind.
fn folder_name(title: &str) -> String {
    title
        .chars()
        .filter(|c| {
            !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_names_drop_what_file_systems_mind() {
        assert_eq!(super::folder_name("Hollow Knight"), "Hollow Knight");
        assert_eq!(
            super::folder_name("The Witcher 3: Wild Hunt"),
            "The Witcher 3 Wild Hunt"
        );
        assert_eq!(super::folder_name("../AC/DC?"), "ACDC");
    }
}
