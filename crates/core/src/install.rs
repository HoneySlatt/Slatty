use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::gameinfo;
use crate::runner::Runner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Linux,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub game_id: String,
    pub title: String,
    pub platform: Platform,
    pub path: PathBuf,
    pub client_id: Option<String>,
    pub runner: Runner,
}

impl Install {
    pub fn save(&self, db: &Db) -> Result<()> {
        let runner = serde_json::to_string(&self.runner).map_err(|e| Error::parse("runner", e))?;
        let platform =
            serde_json::to_value(self.platform).map_err(|e| Error::parse("platform", e))?;
        db.conn().execute(
            "INSERT INTO installs (game_id, title, platform, path, client_id, runner, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(game_id) DO UPDATE SET title = excluded.title, platform = excluded.platform,
                path = excluded.path, client_id = excluded.client_id, runner = excluded.runner",
            params![
                self.game_id,
                self.title,
                platform.as_str(),
                self.path.to_string_lossy(),
                self.client_id,
                runner,
                Utc::now().timestamp()
            ],
        )?;
        Ok(())
    }

    pub fn get(db: &Db, game_id: &str) -> Result<Option<Install>> {
        db.conn()
            .query_row(&format!("{SELECT} WHERE game_id = ?1"), [game_id], from_row)
            .optional()?
            .transpose()
    }

    pub fn list(db: &Db) -> Result<Vec<Install>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY title"))?;
        let rows = stmt.query_map([], from_row)?;
        rows.map(|r| r?).collect()
    }

    pub fn remove(db: &Db, game_id: &str) -> Result<bool> {
        Ok(db
            .conn()
            .execute("DELETE FROM installs WHERE game_id = ?1", [game_id])?
            > 0)
    }
}

const SELECT: &str = "SELECT game_id, title, platform, path, client_id, runner FROM installs";

fn from_row(row: &Row<'_>) -> rusqlite::Result<Result<Install>> {
    let platform: String = row.get(2)?;
    let runner: String = row.get(5)?;
    let path: String = row.get(3)?;
    let (game_id, title, client_id) = (row.get(0)?, row.get(1)?, row.get(4)?);
    Ok((|| {
        Ok(Install {
            game_id,
            title,
            platform: serde_json::from_value(Value::String(platform))
                .map_err(|e| Error::parse("platform", e))?,
            path: PathBuf::from(path),
            client_id,
            runner: serde_json::from_str(&runner).map_err(|e| Error::parse("runner", e))?,
        })
    })())
}

/// Builds an install record from a directory that already contains the game.
pub fn from_dir(dir: &Path, game_id: Option<&str>, runner: Runner) -> Result<Install> {
    let path = dir
        .canonicalize()
        .map_err(|e| Error::io(format!("open {}", dir.display()), e))?;
    if path.join("start.sh").is_file() && path.join("gameinfo").is_file() {
        let id =
            game_id.ok_or_else(|| Error::Refused("native Linux installs need --game-id".into()))?;
        if runner != Runner::Native {
            return Err(Error::Refused(
                "native Linux installs must use the native runner".into(),
            ));
        }
        let title = std::fs::read_to_string(path.join("gameinfo"))
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| id.to_string());
        return Ok(Install {
            game_id: id.into(),
            title,
            platform: Platform::Linux,
            path,
            client_id: None,
            runner,
        });
    }
    let info = gameinfo::read(&path, game_id)?;
    if runner == Runner::Native {
        return Err(Error::Refused(
            "Windows builds need a Wine or Proton runner".into(),
        ));
    }
    Ok(Install {
        game_id: info.game_id,
        title: info.name,
        platform: Platform::Windows,
        path,
        client_id: info.client_id,
        runner,
    })
}

/// Reads (never writes) Heroic's records for a GOG game it installed.
pub fn from_heroic(heroic_config: &Path, game_id: &str) -> Result<Install> {
    let read = |p: PathBuf| -> Result<Value> {
        let bytes = std::fs::read(&p).map_err(|e| Error::io(format!("read {}", p.display()), e))?;
        serde_json::from_slice(&bytes).map_err(|e| Error::parse("Heroic configuration", e))
    };
    let installed = read(heroic_config.join("gog_store/installed.json"))?;
    let entry = installed["installed"]
        .as_array()
        .and_then(|a| a.iter().find(|g| g["appName"].as_str() == Some(game_id)))
        .ok_or_else(|| Error::NotFound(format!("Heroic has no GOG install of {game_id}")))?;
    let path = PathBuf::from(entry["install_path"].as_str().unwrap_or_default());
    match entry["platform"].as_str() {
        Some("linux") => from_dir(&path, Some(game_id), Runner::Native),
        Some("windows") => {
            let config = read(heroic_config.join(format!("GamesConfig/{game_id}.json")))?;
            let settings = &config[game_id];
            let prefix = PathBuf::from(settings["winePrefix"].as_str().unwrap_or_default());
            let bin = PathBuf::from(settings["wineVersion"]["bin"].as_str().unwrap_or_default());
            let runner = match settings["wineVersion"]["type"].as_str() {
                Some("proton") => Runner::Umu {
                    proton: bin.parent().map(Path::to_path_buf).unwrap_or_default(),
                    prefix,
                },
                Some("wine") => Runner::Wine { wine: bin, prefix },
                other => return Err(Error::Unsupported(format!("Heroic runner type {other:?}"))),
            };
            from_dir(&path, Some(game_id), runner)
        }
        other => Err(Error::Unsupported(format!("Heroic platform {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("slatty-inst-{name}-{}", std::process::id()));
        let game = root.join("Game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(
            game.join("goggame-42.info"),
            r#"{"gameId":"42","rootGameId":"42","clientId":"777","name":"Answer",
                "playTasks":[{"type":"FileTask","path":"answer.exe","isPrimary":true}]}"#,
        )
        .unwrap();
        std::fs::write(game.join("answer.exe"), b"").unwrap();
        root
    }

    #[test]
    fn imports_windows_dir_and_roundtrips_through_db() {
        let root = fixture("dir");
        let runner = Runner::Umu {
            proton: "/p".into(),
            prefix: "/x".into(),
        };
        let install = from_dir(&root.join("Game"), None, runner).unwrap();
        assert_eq!(
            (install.game_id.as_str(), install.client_id.as_deref()),
            ("42", Some("777"))
        );
        let db = Db::in_memory().unwrap();
        install.save(&db).unwrap();
        assert_eq!(Install::get(&db, "42").unwrap(), Some(install));
        assert_eq!(Install::list(&db).unwrap().len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn imports_from_heroic_records() {
        let root = fixture("heroic");
        let heroic = root.join("heroic");
        std::fs::create_dir_all(heroic.join("gog_store")).unwrap();
        std::fs::create_dir_all(heroic.join("GamesConfig")).unwrap();
        let game = root.join("Game").canonicalize().unwrap();
        std::fs::write(
            heroic.join("gog_store/installed.json"),
            serde_json::json!({"installed": [{"appName": "42", "platform": "windows", "install_path": game}]}).to_string(),
        )
        .unwrap();
        std::fs::write(
            heroic.join("GamesConfig/42.json"),
            r#"{"42":{"winePrefix":"/pfx","wineVersion":{"bin":"/tools/Proton-X/proton","type":"proton"}}}"#,
        )
        .unwrap();
        let install = from_heroic(&heroic, "42").unwrap();
        assert_eq!(
            install.runner,
            Runner::Umu {
                proton: "/tools/Proton-X".into(),
                prefix: "/pfx".into()
            }
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_native_runner_for_windows_build() {
        let root = fixture("native");
        assert!(from_dir(&root.join("Game"), None, Runner::Native).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
