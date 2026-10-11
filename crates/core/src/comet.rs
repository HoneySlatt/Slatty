use std::collections::HashSet;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::process::Child;

use crate::achievements::{Achievement, Unlock};
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
        // The log first: what can fail before Comet starts fails before the tokens are written.
        let log_path = dirs.logs().join("comet.log");
        crate::paths::ensure_dir(&dirs.logs())?;
        crate::logs::start_run(&log_path);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&log_path)
            .map_err(|e| Error::io(format!("open {}", log_path.display()), e))?;
        let log_err = log.try_clone().map_err(|e| Error::io("comet log", e))?;

        let private_dir = private_dir(runtime)?;
        let token_file = private_dir.join("cache/lutris/.gog.token");
        if let Err(e) = write_private(&private_dir, &token_file, tokens) {
            let _ = std::fs::remove_dir_all(&private_dir);
            return Err(e);
        }

        let mut command = tokio::process::Command::new(bin);
        command
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
            // Out of the terminal's reach: the launcher stops it once the game has ended, or
            // when it ends itself (`tied_to_this_thread`, `Drop`).
            .process_group(0);
        tied_to_this_thread(&mut command);
        let spawned = command.spawn();
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
            // Another user of the computer could take the port between the check that it is free
            // and Comet's start, and the game would talk to them: Comet then fails to start.
            if tokio::net::TcpStream::connect(("127.0.0.1", PORT))
                .await
                .is_ok()
                && std::fs::read_to_string("/proc/net/tcp").is_ok_and(|t| listening_as_me(&t))
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
    /// A Comet not stopped (the launcher quitting during a session, a failed start) is asked to
    /// stop as `stop` does, so that it sends what it holds.
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT);
        }
        let _ = std::fs::remove_dir_all(&self.private_dir);
    }
}

/// Has the program `command` starts stopped (SIGINT, so that Comet sends what it holds) when the
/// thread starting it ends: Tokio's worker threads live as long as the launcher, and a launcher
/// killed outright (a crash, the system out of memory) runs no code that could stop it.
fn tied_to_this_thread(command: &mut tokio::process::Command) {
    // SAFETY: the closure only makes one system call (prctl), which is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            nix::sys::prctl::set_pdeathsig(Signal::SIGINT).map_err(std::io::Error::from)
        });
    }
}

/// Whether, in the kernel's table of TCP sockets (`/proc/net/tcp`), Comet's port on 127.0.0.1 is
/// listened on by the user this process runs as.
fn listening_as_me(table: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let me = std::fs::metadata("/proc/self").map(|m| m.uid().to_string());
    let address = format!("0100007F:{PORT:04X}");
    table.lines().skip(1).any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // Local address, state (0A: listening) and owner.
        fields.get(1) == Some(&address.as_str())
            && fields.get(3) == Some(&"0A")
            && me
                .as_ref()
                .is_ok_and(|me| fields.get(7) == Some(&me.as_str()))
    })
}

/// The folder of this process's handoff, made anew in `runtime` (`$XDG_RUNTIME_DIR/slatty`). The
/// tokens go there, so the folders on the way must be the user's alone: some sessions point
/// `XDG_RUNTIME_DIR` at a folder others can write. Handoffs of launchers no longer running (killed
/// while Comet started) are removed on the way.
fn private_dir(runtime: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    // The process's own entry belongs to the user it runs as.
    let me = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
    let theirs = |p: &Path| {
        !std::fs::symlink_metadata(p)
            .is_ok_and(|m| m.is_dir() && Some(m.uid()) == me && m.mode() & 0o077 == 0)
    };
    let refused = || {
        Error::Refused(
            "XDG_RUNTIME_DIR is not a folder of yours alone; refusing to write tokens there".into(),
        )
    };
    if runtime.parent().is_none_or(theirs) {
        return Err(refused());
    }
    match std::fs::DirBuilder::new().mode(0o700).create(runtime) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => {
            return Err(Error::io(format!("create {}", runtime.display()), e));
        }
        _ if theirs(runtime) => return Err(refused()),
        _ => {}
    }
    for entry in std::fs::read_dir(runtime).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let pid = name.to_str().and_then(|n| n.strip_prefix("comet-"));
        if pid.is_some_and(|p| p.parse::<u32>().is_ok() && !Path::new("/proc").join(p).exists()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    let dir = runtime.join(format!("comet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|e| Error::io(format!("create {}", dir.display()), e))?;
    Ok(dir)
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
/// (on GOG, or in the database, dated before) are not new.
pub struct Unlocks {
    /// `None` when an id cannot name a folder of Comet's.
    file: Option<PathBuf>,
    connection: Option<rusqlite::Connection>,
    known: HashSet<String>,
    /// Unix time a minute before the session. Comet copies GOG's list into the database during
    /// the session, with GOG's dates: an unlock dated earlier is not new, whenever it appears.
    since: i64,
    /// SQLite's `data_version` at the last reading.
    version: Option<i64>,
}

impl Unlocks {
    pub fn new(dirs: &Dirs, client_id: &str, user_id: &str, unlocked: &[Achievement]) -> Unlocks {
        // GOG's ids are digits; a game imported from a folder brings its client id from its own
        // `goggame-*.info`.
        let digits = |id: &str| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
        Unlocks {
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
            since: chrono::Utc::now().timestamp() - 60,
            version: None,
        }
    }

    /// The achievements unlocked since the last look. Never waits on Comet: a database being
    /// written is read at the next look.
    pub fn new_ones(&mut self) -> Vec<Unlock> {
        let Some(rows) = self.read() else {
            return Vec::new();
        };
        let since = self.since;
        rows.into_iter()
            .filter(|(.., time)| unlock_time(time).is_some_and(|t| t >= since))
            .filter(|(key, ..)| self.known.insert(key.clone()))
            .map(|(key, name, _)| Unlock { key, name })
            .collect()
    }

    fn read(&mut self) -> Option<Vec<(String, String, String)>> {
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
            .prepare_cached(
                "SELECT key, name, unlock_time FROM achievement WHERE unlock_time IS NOT NULL",
            )
            .ok()?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .ok()?
            .collect::<rusqlite::Result<_>>()
            .ok()?;
        self.version = Some(version);
        Some(rows)
    }
}

/// Unix time of an unlock as Comet stores it: RFC 3339 for its own, GOG's own form (`+0000`) for
/// those it copies from GOG.
fn unlock_time(time: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(time)
        .or_else(|_| chrono::DateTime::parse_from_str(time, "%Y-%m-%dT%H:%M:%S%z"))
        .ok()
        .map(|t| t.timestamp())
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

    /// The account's tokens are written for Comet only once its log is open: a log that cannot be
    /// opened (a full disk, a logs folder that cannot be made) leaves no token file behind.
    #[tokio::test]
    async fn no_token_file_is_left_when_comets_log_cannot_be_opened() {
        if !crate::doctor::comet_port_free() {
            return;
        }
        let root = std::env::temp_dir().join(format!("slatty-comet-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dirs = Dirs::under(&root);
        std::fs::create_dir_all(&dirs.state).unwrap();
        std::fs::write(dirs.logs(), b"not a folder").unwrap();
        let tokens = Tokens {
            user_id: "1".into(),
            access_token: Secret::new("[FAKE]"),
            refresh_token: Secret::new("[FAKE]"),
            expires_at: i64::MAX,
        };
        let started = Comet::start(Path::new("/bin/true"), &tokens, "[FAKE]", &dirs).await;
        assert!(started.is_err());
        let mut left = Vec::new();
        let mut folders = vec![root.join("runtime")];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).into_iter().flatten().flatten() {
                match entry.file_type() {
                    Ok(t) if t.is_dir() => folders.push(entry.path()),
                    _ => left.push(entry.path()),
                }
            }
        }
        assert!(left.is_empty(), "{left:?}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The tokens go through `$XDG_RUNTIME_DIR`, which some sessions point at a folder others can
    /// write: there, anyone could plant the handoff's folders beforehand and swap what Comet reads.
    #[test]
    fn a_runtime_folder_others_can_open_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("slatty-comet-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (mode, refused) in [(0o777, true), (0o755, true), (0o700, false)] {
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(mode)).unwrap();
            let r = private_dir(&root.join("slatty"));
            assert_eq!(
                matches!(r, Err(Error::Refused(_))),
                refused,
                "{mode:o}: {r:?}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Comet holds the account's tokens and listens for any program while it runs: it must not
    /// outlive a launcher killed during a session (a crash, the system out of memory).
    #[test]
    fn comet_does_not_outlive_the_launcher() {
        let (tx, rx) = std::sync::mpsc::channel();
        // Stands for the launcher: the thread that started Comet, gone without stopping it.
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let mut command = tokio::process::Command::new("sleep");
                command.arg("30");
                tied_to_this_thread(&mut command);
                let child = command.spawn().unwrap();
                tx.send(child.id().unwrap()).unwrap();
                std::mem::forget(child);
            });
        })
        .join()
        .unwrap();
        let pid = rx.recv().unwrap();
        let gone = || {
            std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |s| {
                s.rsplit_once(')')
                    .is_some_and(|(_, r)| r.trim_start().starts_with('Z'))
            })
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !gone() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let survived = !gone();
        let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        assert!(!survived, "sleep {pid} outlived the thread that started it");
    }

    /// Comet's port taken by another user is not Comet: the game would talk to them.
    #[test]
    fn only_a_listener_of_this_user_is_taken_for_comet() {
        use std::os::unix::fs::MetadataExt;
        let me = std::fs::metadata("/proc/self").unwrap().uid();
        let table = |uid: u32, state: &str| {
            format!(
                "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
                 \x20  0: 0100007F:26F9 00000000:0000 {state} 00000000:00000000 00:00000000 00000000 {uid:>5}        0 4242 1 0000000000000000 100 0 0 10 0\n"
            )
        };
        assert!(listening_as_me(&table(me, "0A")));
        assert!(!listening_as_me(&table(me + 1, "0A")), "another user");
        assert!(!listening_as_me(&table(me, "01")), "not listening");
        assert!(!listening_as_me(&table(me, "0A").replace("26F9", "26FA")));
    }

    /// A launcher killed while Comet started left the tokens in its handoff folder: the next
    /// Comet start removes those of processes no longer running.
    #[test]
    fn handoffs_left_by_a_killed_launcher_are_removed() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("slatty-comet-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let runtime = root.join("slatty");
        // Above the kernel's highest process id: never running.
        let stale = runtime.join("comet-4194305/cache/lutris");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join(".gog.token"), "[FAKE]").unwrap();
        std::fs::create_dir_all(runtime.join("other")).unwrap();
        for dir in [&root, &runtime] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mine = private_dir(&runtime).unwrap();
        assert!(mine.is_dir());
        assert!(!runtime.join("comet-4194305").exists());
        assert!(runtime.join("other").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

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
                    "UPDATE achievement SET unlock_time = ?2 WHERE id = ?1",
                    rusqlite::params![id, chrono::Utc::now().to_rfc3339()],
                )
                .unwrap()
        };
        unlock(2);
        assert_eq!(names(unlocks.new_ones()), ["[FAKE] b"]);
        assert!(unlocks.new_ones().is_empty(), "said once");
        unlock(4);
        assert_eq!(names(unlocks.new_ones()), ["[FAKE] d"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn names(unlocks: Vec<Unlock>) -> Vec<String> {
        unlocks.into_iter().map(|u| u.name).collect()
    }

    /// GOG could not be asked at the start, and the first reading found Comet's database still
    /// empty: the unlocks Comet then copies from GOG, with GOG's dates, are still not new.
    #[test]
    fn unlocks_copied_from_gog_during_the_session_are_not_new() {
        let root = std::env::temp_dir().join(format!("slatty-unlocks-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dirs = Dirs::under(&root);
        let file = dirs.data.join("comet/comet/gameplay/123/456/gameplay.db");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let comet = rusqlite::Connection::open(&file).unwrap();
        comet
            .execute_batch(
                "CREATE TABLE achievement (id INTEGER PRIMARY KEY, key TEXT, name TEXT, unlock_time TEXT);",
            )
            .unwrap();
        let mut unlocks = Unlocks::new(&dirs, "123", "456", &[]);
        assert!(unlocks.new_ones().is_empty());
        comet
            .execute_batch(
                "INSERT INTO achievement VALUES
                   (1, 'a', '[FAKE] a', '2024-10-10T10:00:00+0000'),
                   (2, 'b', '[FAKE] b', '2024-10-11T10:00:00+00:00'),
                   (3, 'c', '[FAKE] c', NULL);",
            )
            .unwrap();
        assert!(unlocks.new_ones().is_empty());
        comet
            .execute(
                "UPDATE achievement SET unlock_time = ?1 WHERE id = 3",
                [chrono::Utc::now().to_rfc3339()],
            )
            .unwrap();
        assert_eq!(
            unlocks.new_ones(),
            [Unlock {
                key: "c".into(),
                name: "[FAKE] c".into()
            }]
        );
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
            "UPDATE achievement SET unlock_time = ?1",
            [chrono::Utc::now().to_rfc3339()],
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
