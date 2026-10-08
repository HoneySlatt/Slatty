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
}
