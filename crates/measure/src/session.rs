//! Where the program measured shows its window, and the program itself: started in the fake
//! profile, isolated from the person's own (no keyring, no network), followed through its log.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::log::{Event, Log};
use crate::profile::Profile;

/// A closed port: every request through it fails at once.
const NO_NETWORK: &str = "http://127.0.0.1:9";
/// How often a log is read while waiting for an event.
const POLL: Duration = Duration::from_millis(10);

/// The window's server.
#[derive(Clone, Debug)]
pub enum Display {
    /// A Wayland compositor's socket, by its full path.
    Wayland(PathBuf),
    /// An X server, as `DISPLAY` names it.
    X11(String),
}

/// sway without a screen, at the size, rate and scale of the owner's display (3840×2160, 240 Hz,
/// 1.5), with Xwayland; stopped when dropped.
pub struct Headless {
    sway: Child,
    pub wayland: PathBuf,
    pub x11: String,
    pub version: String,
}

/// The output of the headless compositor.
pub const HEADLESS_OUTPUT: &str = "3840x2160@240Hz scale 1.5";

impl Headless {
    /// Starts sway in `runtime`, a folder of its own made empty first.
    pub fn start(runtime: &Path) -> Result<Self> {
        empty_private_folder(runtime)?;
        let display_file = runtime.join("display");
        let config = runtime.join("sway.conf");
        std::fs::write(
            &config,
            format!(
                "output HEADLESS-1 mode --custom {HEADLESS_OUTPUT}\n\
                 xwayland enable\n\
                 exec sh -c 'printf %s \"$DISPLAY\" > {}'\n",
                display_file.display()
            ),
        )?;
        let version = Command::new("sway")
            .arg("--version")
            .output()
            .context("sway is missing: run in the development shell (nix develop)")?;
        // A process group of its own: Nix's `sway` is a wrapper that starts sway under
        // `dbus-run-session`, and both must stop with it (`Drop`).
        let sway = Command::new("sway")
            .arg("--config")
            .arg(&config)
            .process_group(0)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("XDG_RUNTIME_DIR", runtime)
            .env("WLR_BACKENDS", "headless")
            .env("WLR_RENDERER", "gles2")
            .env("WLR_LIBINPUT_NO_DEVICES", "1")
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(runtime.join("sway.log"))?)
            .stderr(std::fs::File::create(runtime.join("sway.err"))?)
            .spawn()
            .context("starting sway")?;
        let mut headless = Self {
            sway,
            wayland: PathBuf::new(),
            x11: String::new(),
            version: String::from_utf8_lossy(&version.stdout).trim().to_string(),
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = headless.sway.try_wait()? {
                bail!(
                    "sway stopped ({status}): see {}",
                    runtime.join("sway.err").display()
                );
            }
            let socket = std::fs::read_dir(runtime)?
                .flatten()
                .map(|e| e.path())
                .find(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("wayland-") && !n.ends_with(".lock"))
                });
            let x11 = std::fs::read_to_string(&display_file).unwrap_or_default();
            if let Some(socket) = socket
                && !x11.is_empty()
            {
                headless.wayland = socket;
                headless.x11 = x11;
                return Ok(headless);
            }
            if Instant::now() > deadline {
                bail!("sway did not start within 20 s");
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Headless {
    /// Stops sway, Xwayland and everything sway started: its whole process group.
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.sway.id())])
            .status();
        let _ = self.sway.wait();
    }
}

/// The program measured, running in the fake profile; killed when dropped.
pub struct Running {
    child: Child,
    pub pid: u32,
    pub log: Log,
    log_path: PathBuf,
    /// When it was started, in nanoseconds since the Unix epoch, as the probe's times are.
    pub started: u128,
}

impl Running {
    /// Starts `program` on `display`, in `profile`, the probe running `step`, its log and output in
    /// `folder`. Its environment holds nothing of the person's but the paths to programs and
    /// libraries: its home and XDG folders are the profile's, its session bus does not exist (no
    /// keyring, no notifications) and every request goes through a closed port.
    pub fn start(
        program: &Path,
        profile: &Profile,
        runtime: &Path,
        display: &Display,
        step: Option<&str>,
        folder: &Path,
    ) -> Result<Self> {
        std::fs::create_dir_all(folder)?;
        let log_path = folder.join("probe.log");
        let mut command = Command::new(program);
        command.env_clear();
        for kept in ["PATH", "LD_LIBRARY_PATH", "LANG"] {
            if let Some(value) = std::env::var_os(kept) {
                command.env(kept, value);
            }
        }
        command
            .env("HOME", profile.home())
            .envs(profile.xdg())
            .env("XDG_RUNTIME_DIR", runtime)
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .env("SLATTY_PROBE", &log_path)
            .env("SLATTY_PROBE_STEP", step.unwrap_or(""));
        for proxy in ["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"] {
            command
                .env(proxy, NO_NETWORK)
                .env(proxy.to_lowercase(), NO_NETWORK);
        }
        match display {
            Display::Wayland(socket) => command.env("WAYLAND_DISPLAY", socket),
            Display::X11(name) => command.env("DISPLAY", name),
        };
        command
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(folder.join("program.log"))?)
            .stderr(std::fs::File::create(folder.join("program.err"))?);
        let started = now();
        let child = command
            .spawn()
            .with_context(|| format!("starting {}", program.display()))?;
        Ok(Self {
            pid: child.id(),
            child,
            log: Log::new(),
            log_path,
            started,
        })
    }

    /// Waits for the probe's event `name` (with `value`, when given), at most `timeout`.
    pub fn wait_for(
        &mut self,
        name: &str,
        value: Option<&str>,
        timeout: Duration,
    ) -> Result<Event> {
        let deadline = Instant::now() + timeout;
        loop {
            self.log.read_more(&self.log_path)?;
            if let Some(event) = self.log.first(name, value) {
                return Ok(event.clone());
            }
            if let Some(status) = self.child.try_wait()? {
                bail!(
                    "the program stopped ({status}) before `{name}`: see {}",
                    self.log_path.with_file_name("program.err").display()
                );
            }
            if Instant::now() > deadline {
                bail!(
                    "no `{name}` within {timeout:?}: see {}",
                    self.log_path.display()
                );
            }
            std::thread::sleep(POLL);
        }
    }

    /// Reads what the probe added to its log.
    pub fn read_log(&mut self) -> Result<()> {
        self.log.read_more(&self.log_path)
    }

    /// Milliseconds from the start to `event`.
    pub fn since_start(&self, event: &Event) -> f64 {
        (event.at.saturating_sub(self.started)) as f64 / 1e6
    }

    /// Stops the program, by its process, and reads the rest of its log.
    pub fn stop(mut self) -> Result<Log> {
        self.kill();
        self.log.read_more(&self.log_path)?;
        Ok(std::mem::replace(&mut self.log, Log::new()))
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Makes `folder` empty and readable by the user only, as a runtime folder must be. It is always
/// inside the fake profile.
pub fn empty_private_folder(folder: &Path) -> Result<()> {
    if folder.exists() {
        std::fs::remove_dir_all(folder)?;
    }
    std::fs::create_dir_all(folder)?;
    std::fs::set_permissions(folder, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

/// Now, in nanoseconds since the Unix epoch.
pub fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
