use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub game_id: String,
    pub root_game_id: Option<String>,
    pub client_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub play_tasks: Vec<PlayTask>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayTask {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: Option<String>,
    pub arguments: Option<String>,
    pub working_dir: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
    pub category: Option<String>,
}

impl GameInfo {
    pub fn primary_task(&self) -> Option<&PlayTask> {
        let file_tasks = || self.play_tasks.iter().filter(|t| t.kind == "FileTask" && t.path.is_some());
        file_tasks()
            .find(|t| t.is_primary)
            .or_else(|| file_tasks().find(|t| t.category.as_deref() == Some("game")))
    }
}

/// Reads the base game's `goggame-<id>.info` from an install directory.
pub fn read(dir: &Path, game_id: Option<&str>) -> Result<GameInfo> {
    let entries = std::fs::read_dir(dir).map_err(|e| Error::io(format!("read {}", dir.display()), e))?;
    let mut infos = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("goggame-") && name.ends_with(".info") {
            let bytes = std::fs::read(entry.path()).map_err(|e| Error::io(format!("read {name}"), e))?;
            let info: GameInfo = serde_json::from_slice(&bytes).map_err(|e| Error::parse("goggame info file", e))?;
            infos.push(info);
        }
    }
    let wanted = |i: &GameInfo| match game_id {
        Some(id) => i.game_id == id,
        None => i.root_game_id.as_ref().is_none_or(|r| *r == i.game_id),
    };
    let mut matching: Vec<_> = infos.into_iter().filter(wanted).collect();
    match matching.len() {
        1 => Ok(matching.remove(0)),
        0 => Err(Error::NotFound(format!("no matching goggame-*.info in {}", dir.display()))),
        _ => Err(Error::Refused(format!("several base games in {}; pass the game id", dir.display()))),
    }
}

/// Resolves a Windows-style relative path under `base`, matching each component case-insensitively.
pub fn resolve_relative(base: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = base.to_path_buf();
    for part in rel.split(['\\', '/']).filter(|p| !p.is_empty() && *p != ".") {
        if Path::new(part).components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(Error::Refused(format!("unsafe path component `{part}`")));
        }
        let exact = out.join(part);
        out = if exact.exists() {
            exact
        } else {
            find_case_insensitive(&out, part).unwrap_or(exact)
        };
    }
    Ok(out)
}

fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let lower = name.to_lowercase();
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_lowercase() == lower)
        .map(|e| e.path())
}

/// Splits a GOG `arguments` string: whitespace-separated, double quotes group.
pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    for c in s.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_primary_file_task() {
        let info: GameInfo = serde_json::from_str(
            r#"{"gameId":"1","name":"G","playTasks":[
                {"type":"URLTask","link":"x","category":"document"},
                {"type":"FileTask","path":"Other.exe","category":"tool"},
                {"type":"FileTask","path":"bin\\Game.exe","isPrimary":true,"arguments":"-nolauncher"}]}"#,
        )
        .unwrap();
        assert_eq!(info.primary_task().unwrap().path.as_deref(), Some("bin\\Game.exe"));
    }

    #[test]
    fn splits_quoted_arguments() {
        assert_eq!(split_args(r#" -a  "b c" d"" "#), vec!["-a", "b c", "d"]);
        assert_eq!(split_args(r#""""#), vec![""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn resolves_case_insensitively_and_rejects_escapes() {
        let root = std::env::temp_dir().join(format!("slatty-gi-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Bin/X64")).unwrap();
        std::fs::write(root.join("Bin/X64/Game.EXE"), b"").unwrap();
        assert_eq!(resolve_relative(&root, "bin\\x64\\game.exe").unwrap(), root.join("Bin/X64/Game.EXE"));
        assert!(resolve_relative(&root, "..\\outside").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_base_game_info_and_ignores_dlc() {
        let root = std::env::temp_dir().join(format!("slatty-gi2-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("goggame-1.info"), r#"{"gameId":"1","rootGameId":"1","name":"Base"}"#).unwrap();
        std::fs::write(root.join("goggame-2.info"), r#"{"gameId":"2","rootGameId":"1","name":"DLC"}"#).unwrap();
        assert_eq!(read(&root, None).unwrap().name, "Base");
        assert_eq!(read(&root, Some("2")).unwrap().name, "DLC");
        std::fs::remove_dir_all(root).unwrap();
    }
}
