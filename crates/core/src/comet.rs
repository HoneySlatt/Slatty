use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::process::Child;

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
            .env("COMET_LOG", "info")
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
