use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{Error, Result};

const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE accounts (
    user_id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    added_at INTEGER NOT NULL
);
CREATE TABLE installs (
    game_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    platform TEXT NOT NULL,
    path TEXT NOT NULL,
    client_id TEXT,
    runner TEXT NOT NULL,
    added_at INTEGER NOT NULL
);
CREATE TABLE sessions (
    id INTEGER PRIMARY KEY,
    game_id TEXT NOT NULL,
    user_id TEXT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    exit_code INTEGER,
    state TEXT NOT NULL
);
CREATE TABLE sync_roots (
    user_id TEXT NOT NULL,
    game_id TEXT NOT NULL,
    location TEXT NOT NULL,
    root TEXT NOT NULL,
    last_sync_at INTEGER,
    PRIMARY KEY (user_id, game_id, location)
);
CREATE TABLE sync_baseline (
    user_id TEXT NOT NULL,
    game_id TEXT NOT NULL,
    location TEXT NOT NULL,
    path TEXT NOT NULL,
    local_sha256 TEXT NOT NULL,
    local_size INTEGER NOT NULL,
    remote_hash TEXT NOT NULL,
    synced_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, game_id, location, path)
);
"#,
    r#"
CREATE TABLE install_jobs (
    game_id TEXT PRIMARY KEY,
    build_id TEXT NOT NULL,
    language TEXT NOT NULL,
    root TEXT NOT NULL,
    directory TEXT NOT NULL,
    state TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
"#,
    r#"
ALTER TABLE install_jobs ADD COLUMN dlcs TEXT NOT NULL DEFAULT '';
"#,
    r#"
ALTER TABLE sessions ADD COLUMN reported INTEGER NOT NULL DEFAULT 0;
"#,
    r#"
ALTER TABLE installs ADD COLUMN umu_id TEXT;
"#,
    r#"
CREATE TABLE game_custom (
    game_id TEXT PRIMARY KEY,
    title TEXT,
    sort_title TEXT,
    cover TEXT,
    background TEXT
);
"#,
];

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(parent) = path.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let db = Self::init(Connection::open(path)?)?;
        // Readable by the user only, with the journal SQLite keeps beside it.
        for suffix in ["", "-wal", "-shm"] {
            let file = PathBuf::from(format!("{}{suffix}", path.display()));
            if file.exists() {
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))
                    .map_err(|e| Error::io(format!("protect {}", file.display()), e))?;
            }
        }
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Never hold the guard across an `.await`.
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        let conn = self.conn();
        match value {
            Some(v) => conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, v],
            )?,
            None => conn.execute("DELETE FROM settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > MIGRATIONS.len() as i64 {
        return Err(Error::Unsupported(format!(
            "state database version {version} is newer than this build supports"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_stores_settings() {
        let db = Db::in_memory().unwrap();
        assert_eq!(db.setting("x").unwrap(), None);
        db.set_setting("x", Some("1")).unwrap();
        db.set_setting("x", Some("2")).unwrap();
        assert_eq!(db.setting("x").unwrap().as_deref(), Some("2"));
        db.set_setting("x", None).unwrap();
        assert_eq!(db.setting("x").unwrap(), None);
    }
}
