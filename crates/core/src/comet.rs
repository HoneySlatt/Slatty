use std::collections::HashSet;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::process::Child;

use crate::achievements::Achievement;
use crate::auth::Tokens;
use crate::error::{Error, Result};
use crate::paths::Dirs;

pub const PORT: u16 = 9977;
const READY_TIMEOUT: Duration = Duration::from_secs(15);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

/// A Comet process serving the Galaxy SDK of one game session.
pub struct Comet {
    child: Child,
    private_dir: PathBuf,
}

impl Comet {
    /// Tokens reach Comet through its Lutris importer: a 0600 file in a private runtime
    /// directory, removed as soon as Comet listens. They never appear in argv.
    pub async fn start(bin: &Path, tokens: &Tokens, username: &str, dirs: &Dirs) -> Result<Comet> {
        if !crate::doctor::comet_port_free() {
            return Err(Error::Refused(format!(
                "port {PORT} is already used, probably by another Comet (Heroic?); achievements would go elsewhere"
            )));
        }
        let runtime = dirs.runtime.as_ref().ok_or_else(|| {
            Error::Unsupported(
                "XDG_RUNTIME_DIR is not set; refusing to write tokens to disk".into(),
            )
        })?;
        let private_dir = runtime.join(format!("comet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&private_dir);
        let token_file = private_dir.join("cache/lutris/.gog.token");
        write_private(&private_dir, &token_file, tokens)?;

        let log_path = dirs.logs().join("comet.log");
        crate::paths::ensure_dir(&dirs.logs())?;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&log_path)
            .map_err(|e| Error::io(format!("open {}", log_path.display()), e))?;
        let log_err = log.try_clone().map_err(|e| Error::io("comet log", e))?;

        let spawned = tokio::process::Command::new(bin)
            .args(["--from-lutris", "--username", username])
            .env("HOME", private_dir.join("home"))
            .env("XDG_CACHE_HOME", private_dir.join("cache"))
            .env("XDG_CONFIG_HOME", dirs.config.join("comet"))
            .env("XDG_DATA_HOME", dirs.data.join("comet"))
            // Comet 0.3.2's handlers log a failed token request with its address, whose query
            // holds the account's refresh token, and each game's client secret.
            .env("COMET_LOG", "info,comet::api::handlers=error")
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log_err)
            .kill_on_drop(true)
            .spawn();
        let child = match spawned {
            Ok(c) => c,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&private_dir);
                return Err(Error::io(format!("start {}", bin.display()), e));
            }
        };
        let mut comet = Comet { child, private_dir };
        let ready = comet.wait_ready().await;
        let _ = std::fs::remove_file(&token_file);
        ready.map(|()| comet)
    }

    async fn wait_ready(&mut self) -> Result<()> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().map_err(|e| Error::io("comet", e))? {
                return Err(Error::Refused(format!(
                    "Comet exited during startup ({status}); see comet.log"
                )));
            }
            if tokio::net::TcpStream::connect(("127.0.0.1", PORT))
                .await
                .is_ok()
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Refused(
                    "Comet did not start listening in time; see comet.log".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// SIGINT lets Comet finish pending requests before exiting.
    pub async fn stop(mut self) -> Result<()> {
        if let Some(pid) = self.child.id() {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT);
        }
        if tokio::time::timeout(STOP_TIMEOUT, self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.kill().await;
        }
        Ok(())
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Comet {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.private_dir);
    }
}

fn write_private(private_dir: &Path, token_file: &Path, tokens: &Tokens) -> Result<()> {
    let mkdir = |p: &Path| {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(p)
            .map_err(|e| Error::io(format!("create {}", p.display()), e))
    };
    mkdir(&private_dir.join("home"))?;
    mkdir(token_file.parent().expect("token file has a parent"))?;
    let json = serde_json::json!({
        "access_token": tokens.access_token.expose(),
        "refresh_token": tokens.refresh_token.expose(),
        "user_id": tokens.user_id,
    });
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(token_file)
        .map_err(|e| Error::io("create the Comet token file", e))?;
    std::io::Write::write_all(&mut f, json.to_string().as_bytes())
        .map_err(|e| Error::io("write the Comet token file", e))
}

/// Achievements unlocked while the game runs, read from Comet's own database: Comet records an
/// unlock there at once and sends it to GOG later. Those already unlocked when the session started
/// (on GOG, or in the database at its first reading) are not new.
pub struct Unlocks {
    /// `None` when an id cannot name a folder of Comet's.
    file: Option<PathBuf>,
    connection: Option<rusqlite::Connection>,
    known: HashSet<String>,
    read_once: bool,
    /// SQLite's `data_version` at the last reading.
    version: Option<i64>,
}

impl Unlocks {
    pub fn new(dirs: &Dirs, client_id: &str, user_id: &str, unlocked: &[Achievement]) -> Unlocks {
        // GOG's ids are digits; a game imported from a folder brings its client id from its own
        // `goggame-*.info`.
        let digits = |id: &str| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
        let mut unlocks = Unlocks {
            // Comet adds its own folder to the XDG_DATA_HOME it is given.
            file: (digits(client_id) && digits(user_id)).then(|| {
                dirs.data
                    .join("comet/comet/gameplay")
                    .join(client_id)
                    .join(user_id)
                    .join("gameplay.db")
            }),
            connection: None,
            known: unlocked
                .iter()
                .filter(|a| a.date_unlocked.is_some())
                .map(|a| a.achievement_key.clone())
                .collect(),
            read_once: false,
            version: None,
        };
        unlocks.new_ones();
        unlocks
    }

    /// Names of the achievements unlocked since the last look. Never waits on Comet: a database
    /// being written is read at the next look.
    pub fn new_ones(&mut self) -> Vec<String> {
        let Some(rows) = self.read() else {
            return Vec::new();
        };
        let new: Vec<String> = rows
            .into_iter()
            .filter_map(|(key, name)| self.known.insert(key).then_some(name))
            .collect();
        if std::mem::replace(&mut self.read_once, true) {
            new
        } else {
            Vec::new()
        }
    }

    fn read(&mut self) -> Option<Vec<(String, String)>> {
        if self.connection.is_none() {
            let file = self.file.as_ref().filter(|f| f.is_file())?;
            let connection = rusqlite::Connection::open_with_flags(
                file,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .ok()?;
            connection.busy_timeout(Duration::ZERO).ok()?;
            self.connection = Some(connection);
        }
        let connection = self.connection.as_ref()?;
        // Changes only when Comet has written: the rows are read again only then.
        let version = connection
            .query_row("PRAGMA data_version", [], |row| row.get(0))
            .ok()?;
        if self.version == Some(version) {
            return None;
        }
        let mut statement = connection
            .prepare_cached("SELECT key, name FROM achievement WHERE unlock_time IS NOT NULL")
            .ok()?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok()?
            .collect::<rusqlite::Result<_>>()
            .ok()?;
        self.version = Some(version);
        Some(rows)
    }
}

/// Files of GOG's Galaxy SDK, the only way a game reports achievements to Comet.
const GALAXY_SDK: [&str; 4] = [
    "galaxy.dll",
    "galaxy64.dll",
    "galaxypeer.dll",
    "galaxypeer64.dll",
];

/// Whether a game ships GOG's Galaxy SDK: Comet is started for those only. The files slatty
/// installed are looked through; a game installed elsewhere is searched a few folders deep.
pub fn uses_galaxy(dirs: &Dirs, install: &crate::install::Install) -> bool {
    let is_sdk = |name: &str| {
        let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
        GALAXY_SDK.iter().any(|sdk| name.eq_ignore_ascii_case(sdk))
    };
    if let Ok(Some(record)) = crate::installer::InstallRecord::load(dirs, &install.game_id) {
        return record.files.iter().any(|f| is_sdk(&f.path));
    }
    fn search(dir: &Path, depth: u32, is_sdk: &dyn Fn(&str) -> bool) -> bool {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                match e.file_type() {
                    Ok(t) if t.is_dir() => depth > 0 && search(&e.path(), depth - 1, is_sdk),
                    Ok(_) => is_sdk(&name),
                    Err(_) => false,
                }
            })
    }
    search(&install.path, 4, &is_sdk)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::secret::Secret;

    #[test]
    fn token_file_is_private() {
        let dir = std::env::temp_dir().join(format!("slatty-comet-{}", std::process::id()));
        let file = dir.join("cache/lutris/.gog.token");
        let tokens = Tokens {
            user_id: "1".into(),
            access_token: Secret::new("a"),
            refresh_token: Secret::new("r"),
            expires_at: 0,
        };
        write_private(&dir, &file, &tokens).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(dir.join("cache/lutris"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(v["refresh_token"], "r");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// An unlock is seen as soon as Comet records it, once; those unlocked before the session (on
    /// GOG, or in Comet's database from an earlier one) are not.
    #[test]
    fn unlocks_are_read_from_comets_database_as_they_come() {
        let root = std::env::temp_dir().join(format!("slatty-unlocks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        let gog = |key: &str, unlocked: bool| Achievement {
            achievement_id: key.into(),
            achievement_key: key.into(),
            name: format!("[FAKE] {key}"),
            description: String::new(),
            visible: true,
            date_unlocked: unlocked.then(|| "2026-10-10T10:00:00+00:00".into()),
            rarity: 0.0,
            image_url_unlocked: String::new(),
            image_url_locked: String::new(),
        };
        // Not created yet: Comet makes it when the game first asks for achievements.
        let mut unlocks = Unlocks::new(&dirs, "123", "456", &[gog("a", true), gog("b", false)]);
        assert!(unlocks.new_ones().is_empty());

        let file = dirs.data.join("comet/comet/gameplay/123/456/gameplay.db");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let comet = rusqlite::Connection::open(&file).unwrap();
        comet
            .execute_batch(
                "CREATE TABLE achievement (id INTEGER PRIMARY KEY, key TEXT, name TEXT, unlock_time TEXT);
                 INSERT INTO achievement VALUES
                   (1, 'a', '[FAKE] a', '2026-10-10T10:00:00+00:00'),
                   (2, 'b', '[FAKE] b', NULL),
                   (3, 'c', '[FAKE] c', '2026-10-09T10:00:00+00:00'),
                   (4, 'd', '[FAKE] d', NULL);",
            )
            .unwrap();
        assert!(
            unlocks.new_ones().is_empty(),
            "what the first reading finds was there before"
        );

        let unlock = |id: u32| {
            comet
                .execute(
                    "UPDATE achievement SET unlock_time = '2026-10-10T11:00:00+00:00' WHERE id = ?1",
                    [id],
                )
                .unwrap()
        };
        unlock(2);
        assert_eq!(unlocks.new_ones(), ["[FAKE] b"]);
        assert!(unlocks.new_ones().is_empty(), "said once");
        unlock(4);
        assert_eq!(unlocks.new_ones(), ["[FAKE] d"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A game imported from a folder brings its client id from its own `goggame-*.info`: an id
    /// that is not GOG's digits never leads the reading to a database elsewhere.
    #[test]
    fn unlocks_are_never_read_from_outside_comets_folder() {
        let root = std::env::temp_dir().join(format!("slatty-unlocks-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root.join("app"));
        let planted = root.join("elsewhere/456/gameplay.db");
        std::fs::create_dir_all(planted.parent().unwrap()).unwrap();
        let db = rusqlite::Connection::open(&planted).unwrap();
        db.execute_batch(
            "CREATE TABLE achievement (id INTEGER PRIMARY KEY, key TEXT, name TEXT, unlock_time TEXT);
             INSERT INTO achievement VALUES (1, 'a', '[FAKE] a', NULL);",
        )
        .unwrap();
        let client_id = root.join("elsewhere").display().to_string();
        let mut unlocks = Unlocks::new(&dirs, &client_id, "456", &[]);
        db.execute(
            "UPDATE achievement SET unlock_time = '2026-10-10T11:00:00+00:00'",
            [],
        )
        .unwrap();
        assert!(unlocks.new_ones().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn comet_is_for_games_that_ship_the_galaxy_sdk() {
        use crate::install::{Install, Platform};
        use crate::installer::{InstallRecord, RecordedFile};
        let root = std::env::temp_dir().join(format!("slatty-galaxy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root.join("app"));
        let game = root.join("Game");
        std::fs::create_dir_all(game.join("bin/x64")).unwrap();
        let install = Install {
            game_id: "1".into(),
            title: "[FAKE] Game".into(),
            platform: Platform::Windows,
            path: game.clone(),
            client_id: None,
            runner: crate::runner::Runner::Native,
            umu_id: None,
            isolated: false,
        };
        // Installed elsewhere: its folder is searched.
        assert!(!uses_galaxy(&dirs, &install));
        std::fs::write(game.join("bin/x64/Galaxy64.dll"), b"").unwrap();
        assert!(uses_galaxy(&dirs, &install));
        // Installed by slatty: its record says.
        let record = |path: &str| InstallRecord {
            build_id: "b".into(),
            version: "1".into(),
            language: "en".into(),
            path: Some(game.clone()),
            dlcs: Vec::new(),
            setup_build: None,
            setup_failures: 0,
            files: vec![RecordedFile {
                path: path.into(),
                size: 1,
            }],
        };
        record("Game_Data\\Plugins\\GalaxyCSharp.dll")
            .save(&dirs, "1")
            .unwrap();
        assert!(!uses_galaxy(&dirs, &install), "a wrapper is not the SDK");
        record("Game_Data\\Plugins\\x86_64\\galaxy.dll")
            .save(&dirs, "1")
            .unwrap();
        assert!(uses_galaxy(&dirs, &install));
        std::fs::remove_dir_all(root).unwrap();
    }
}
