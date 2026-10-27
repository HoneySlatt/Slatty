//! Starting an Epic game: its program and arguments, from its manifest and Epic's catalog, then
//! those the Epic Games Launcher adds to sign the game in to the account, as legendary does
//! (`LegendaryCore.get_launch_parameters`).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::Deserialize;

use super::install::EpicInstall;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::install::Install;
use crate::paths::Dirs;
use crate::secret::Secret;
use crate::{fsutil, gameinfo, http};

const EXCHANGE_URL: &str =
    "https://account-public-service-prod03.ol.epicgames.com/account/api/oauth/exchange";
const ECOMMERCE_HOST: &str =
    "https://ecommerceintegration-public-service-ecomprod02.ol.epicgames.com";
/// Where a game's ownership token is written for a launch, in its prefix's `C:`: seen there by an
/// isolated game too.
const OWNERSHIP_TOKEN: &str = "slatty-epic.ovt";

/// The program a launch starts, its arguments and its folder, as installed.
pub fn task(dirs: &Dirs, install: &Install) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    let epic = kept(dirs, install)?;
    let exe = epic.launch_exe.trim();
    if exe.is_empty() {
        return Err(Error::NotFound(format!(
            "Epic names no program to start {}",
            install.title
        )));
    }
    let exe = gameinfo::resolve_relative(&install.path, exe)?;
    if !exe.is_file() {
        return Err(Error::NotFound(format!("{} is missing", exe.display())));
    }
    let cwd = exe
        .parent()
        .map_or_else(|| install.path.clone(), Path::to_path_buf);
    let mut args = gameinfo::split_args(&epic.launch_command);
    args.extend(gameinfo::split_args(&epic.additional_args));
    Ok((exe, args, cwd))
}

fn kept(dirs: &Dirs, install: &Install) -> Result<EpicInstall> {
    EpicInstall::load(dirs, &install.game_id)?.ok_or_else(|| {
        Error::NotFound(format!(
            "what starting {} needs was not kept: install it again",
            install.title
        ))
    })
}

/// What signs a game in to the account for one launch.
pub struct SignIn {
    /// The arguments, one of them a code that signs in once, for five minutes.
    pub args: Vec<String>,
    /// The ownership token written for the game, to delete once it ends.
    pub ownership_token: Option<PathBuf>,
}

/// Signs `install` in for this launch: a fresh exchange code, the account, and, for a game that
/// asks for one, its ownership token, written in its prefix.
pub async fn sign_in(http: &Client, db: &Db, dirs: &Dirs, install: &Install) -> Result<SignIn> {
    let epic = kept(dirs, install)?;
    let tokens = super::auth::fresh(http, db, dirs).await?;
    let source = super::library::Epic {
        http,
        token: &tokens.access_token,
    };
    #[derive(Deserialize)]
    struct Exchange {
        code: Secret,
    }
    let exchange: Exchange = super::library::session_checked(
        http::json(source.get(EXCHANGE_URL), "asking Epic to sign the game in").await,
    )?;
    let ownership_token = if epic.ownership_token {
        let token = ownership_token(http, &tokens, &epic).await?;
        let prefix = install.runner.prefix().ok_or_else(|| {
            Error::Unsupported("an Epic game runs through Proton, in a prefix".into())
        })?;
        let path = prefix.join("drive_c").join(OWNERSHIP_TOKEN);
        crate::installer::refuse_outside(prefix, &path)?;
        fsutil::write_atomic(&path, &token)?;
        Some(path)
    } else {
        None
    };
    Ok(SignIn {
        args: launcher_args(
            &epic,
            &tokens.account_id,
            &tokens.display_name,
            exchange.code.expose(),
            ownership_token.is_some(),
            &system_locale(),
        ),
        ownership_token,
    })
}

/// The ownership token of a game that asks for one (`EPCAPI.get_ownership_token`). Sent once: it
/// is not a read.
async fn ownership_token(
    http: &Client,
    tokens: &super::auth::Tokens,
    epic: &EpicInstall,
) -> Result<Vec<u8>> {
    let context = "asking Epic for the game's ownership token";
    let url = format!(
        "{ECOMMERCE_HOST}/ecommerceintegration/api/public/platforms/EPIC/identities/{}/ownershipToken",
        tokens.account_id
    );
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(
            "nsCatalogItemId",
            &format!("{}:{}", epic.namespace, epic.catalog_item_id),
        )
        .finish();
    let req = http
        .post(url)
        .bearer_auth(tokens.access_token.expose())
        .header(reqwest::header::USER_AGENT, super::USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body);
    let resp = super::library::session_checked(http::send(req, context).await)?;
    http::read_up_to(resp, context, 1 << 20).await
}

/// The arguments the Epic Games Launcher gives a game.
fn launcher_args(
    epic: &EpicInstall,
    account_id: &str,
    name: &str,
    code: &str,
    ownership_token: bool,
    locale: &str,
) -> Vec<String> {
    let mut args = vec![
        "-AUTH_LOGIN=unused".to_string(),
        format!("-AUTH_PASSWORD={code}"),
        "-AUTH_TYPE=exchangecode".into(),
        format!("-epicapp={}", epic.app_name),
        "-epicenv=Prod".into(),
    ];
    if ownership_token {
        args.push(format!("-epicovt=C:\\{OWNERSHIP_TOKEN}"));
    }
    args.extend([
        "-EpicPortal".into(),
        format!("-epicusername={name}"),
        format!("-epicuserid={account_id}"),
        format!("-epiclocale={locale}"),
        format!("-epicsandboxid={}", epic.namespace),
    ]);
    if let Some(deployment) = &epic.deployment_id {
        args.push(format!("-epicdeploymentid={deployment}"));
    }
    args
}

/// The system's language, as Epic names it (`fr` for `fr_FR.UTF-8`), else English: a game is told
/// to use it, and Epic's texts are asked in it.
pub fn system_locale() -> String {
    let lang = std::env::var_os("LC_ALL")
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var_os("LANG"));
    locale(lang.as_deref())
}

fn locale(lang: Option<&OsStr>) -> String {
    let lang = lang.and_then(OsStr::to_str).unwrap_or_default();
    let code = lang.split(['_', '.', '@']).next().unwrap_or_default();
    if (2..=3).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphabetic()) {
        code.to_ascii_lowercase()
    } else {
        "en".into()
    }
}

/// Deletes the ownership token a launch wrote, once the game ended.
pub fn forget(sign_in: &SignIn) {
    if let Some(path) = &sign_in.ownership_token {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::Platform;
    use crate::runner::Runner;

    fn kept_install() -> EpicInstall {
        EpicInstall {
            app_name: "Fake".into(),
            namespace: "fakens".into(),
            catalog_item_id: "fakeitem".into(),
            build_version: "1.0-[FAKE]".into(),
            launch_exe: "Binaries\\Win64\\Game.exe".into(),
            launch_command: "-fake \"two words\"".into(),
            additional_args: "-extra".into(),
            ownership_token: true,
            deployment_id: Some("[FAKE]deployment".into()),
            secrets: Default::default(),
        }
    }

    /// The arguments legendary gives, in its order.
    #[test]
    fn a_game_is_signed_in_as_the_epic_launcher_signs_it_in() {
        let args = launcher_args(
            &kept_install(),
            "0123456789abcdef0123456789abcdef",
            "[FAKE] Player",
            "[FAKE]code",
            true,
            "fr",
        );
        assert_eq!(
            args,
            [
                "-AUTH_LOGIN=unused",
                "-AUTH_PASSWORD=[FAKE]code",
                "-AUTH_TYPE=exchangecode",
                "-epicapp=Fake",
                "-epicenv=Prod",
                "-epicovt=C:\\slatty-epic.ovt",
                "-EpicPortal",
                "-epicusername=[FAKE] Player",
                "-epicuserid=0123456789abcdef0123456789abcdef",
                "-epiclocale=fr",
                "-epicsandboxid=fakens",
                "-epicdeploymentid=[FAKE]deployment",
            ]
        );
    }

    #[test]
    fn the_locale_is_the_systems_language() {
        assert_eq!(locale(Some(OsStr::new("fr_FR.UTF-8"))), "fr");
        assert_eq!(locale(Some(OsStr::new("de_DE@euro"))), "de");
        assert_eq!(locale(Some(OsStr::new("C.UTF-8"))), "en");
        assert_eq!(locale(Some(OsStr::new("POSIX"))), "en");
        assert_eq!(locale(None), "en");
    }

    /// The program the manifest names, found whatever its case on disk, with the manifest's and
    /// the catalog's arguments; a game installed without what launching needs says so.
    #[test]
    fn the_program_is_the_manifests() {
        let root = std::env::temp_dir().join(format!("slatty-epic-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        let game = root.join("games/Fake");
        std::fs::create_dir_all(game.join("binaries/win64")).unwrap();
        std::fs::write(game.join("binaries/win64/Game.exe"), b"MZ").unwrap();
        let install = Install {
            umu_id: None,
            game_id: "epic-Fake".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: game.clone(),
            client_id: None,
            runner: Runner::Umu {
                proton: "/proton".into(),
                prefix: root.join("prefix"),
            },
            isolated: false,
        };
        assert!(matches!(task(&dirs, &install), Err(Error::NotFound(_))));
        kept_install().save(&dirs, b"[FAKE] manifest").unwrap();
        let (exe, args, cwd) = task(&dirs, &install).unwrap();
        assert_eq!(exe, game.join("binaries/win64/Game.exe"));
        assert_eq!(args, ["-fake", "two words", "-extra"]);
        assert_eq!(cwd, game.join("binaries/win64"));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// GOG's post-install setup never runs for an Epic game, whose record names no setup run.
    #[test]
    fn gogs_setup_is_not_an_epic_games() {
        let root = std::env::temp_dir().join(format!("slatty-epic-setup-{}", std::process::id()));
        let dirs = Dirs::under(&root);
        crate::installer::InstallRecord {
            build_id: "epic:1.0-[FAKE]".into(),
            version: "1.0-[FAKE]".into(),
            language: String::new(),
            path: None,
            dlcs: Vec::new(),
            setup_build: None,
            setup_failures: 0,
            files: Vec::new(),
        }
        .save(&dirs, "epic-Fake")
        .unwrap();
        assert!(!crate::setup::pending(&dirs, "epic-Fake").unwrap());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
