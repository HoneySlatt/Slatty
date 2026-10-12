//! Images enlarged twice by Real-ESRGAN's compact model (`realesr-animevideov3`) through
//! `realesrgan-ncnn-vulkan`, on the graphics card, once Enhance images with AI is on. Chosen over
//! Real-ESRGAN x4plus, nearly as clean on the covers and key art compared but ten times as quick
//! (about 0.5 s for key art of 1920 pixels on a Radeon RX 6900 XT, 4.7 s) and taking a sixth of its
//! graphics memory (0.4 GB), and over Real-CUGAN, which made up textures on realistic art.
//!
//! The program never reads a downloaded file: it is given PNGs written here from images decoded
//! here. It runs in bubblewrap, without network, seeing only its working folder and its shader
//! cache writable and the home folder not at all.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const PROGRAM: &str = "realesrgan-ncnn-vulkan";
const MODEL: &str = "realesr-animevideov3";
/// Stopped past this: about 0.1 s a cover and 0.5 s a key art, at most 32 at once.
const TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upscaler {
    program: PathBuf,
    models: PathBuf,
    bwrap: PathBuf,
}

/// What enlarges images twice: the upscaler, or a fake in tests.
pub trait Enlarge: Send + Sync {
    /// Enlarges twice each PNG in `work/in` into `work/out`, under the same names; `stop` is asked
    /// now and then whether a game started, which stops it.
    fn enlarge(&self, work: &Path, shaders: &Path, stop: &dyn Fn() -> bool) -> Result<(), Stopped>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Stopped {
    /// A game started: what was not made is made once it ends.
    ForAGame,
    Failed(String),
}

impl Upscaler {
    /// The program and its model, as nixpkgs installs them (`share/models` beside `bin`), and
    /// bubblewrap.
    pub fn find() -> Option<Upscaler> {
        let program = slatty_core::doctor::find_in_path(PROGRAM)?;
        let models = std::fs::canonicalize(&program)
            .ok()?
            .parent()?
            .parent()?
            .join("share/models");
        if !models.join(format!("{MODEL}-x2.param")).is_file() {
            return None;
        }
        let bwrap = slatty_core::doctor::find_in_path("bwrap")?;
        Some(Upscaler {
            program,
            models,
            bwrap,
        })
    }

    fn command(&self, work: &Path, shaders: &Path) -> Command {
        let mut c = Command::new(&self.bwrap);
        c.args(["--ro-bind", "/", "/", "--dev", "/dev"])
            .args(["--dev-bind-try", "/dev/dri", "/dev/dri"])
            .args(["--proc", "/proc", "--tmpfs", "/tmp"]);
        if let Some(home) = std::env::var_os("HOME") {
            c.arg("--tmpfs").arg(home);
        }
        c.arg("--bind").arg(work).arg(work);
        c.arg("--bind").arg(shaders).arg(shaders);
        c.args([
            "--unshare-net",
            "--unshare-pid",
            "--die-with-parent",
            "--new-session",
        ])
        .arg("--")
        .arg(&self.program)
        .arg("-i")
        .arg(work.join("in"))
        .arg("-o")
        .arg(work.join("out"))
        .args(["-n", MODEL, "-s", "2", "-f", "png"])
        .arg("-m")
        .arg(&self.models)
        // Mesa compiles the model's shaders once, kept here rather than in the hidden home.
        .env("MESA_SHADER_CACHE_DIR", shaders)
        // A temporary folder under /tmp the environment names is not in the sandbox's.
        .env("TMPDIR", "/tmp");
        c
    }
}

impl Enlarge for Upscaler {
    /// Asks `stop` every 100 ms.
    fn enlarge(&self, work: &Path, shaders: &Path, stop: &dyn Fn() -> bool) -> Result<(), Stopped> {
        let failed = |e: std::io::Error| Stopped::Failed(e.to_string());
        std::fs::create_dir_all(work.join("out")).map_err(failed)?;
        std::fs::create_dir_all(shaders).map_err(failed)?;
        let mut child = self
            .command(work, shaders)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(failed)?;
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().map_err(failed)? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(Stopped::Failed(format!("{PROGRAM} ended with {status}")))
                };
            }
            let over = started.elapsed() > TIMEOUT;
            if over || stop() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if over {
                    Stopped::Failed(format!("{PROGRAM} took more than {}s", TIMEOUT.as_secs()))
                } else {
                    Stopped::ForAGame
                });
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upscaler() -> Upscaler {
        Upscaler {
            program: "/nix/store/x/bin/realesrgan-ncnn-vulkan".into(),
            models: "/nix/store/x/share/models".into(),
            bwrap: "/bin/bwrap".into(),
        }
    }

    /// The program runs without network, its working folder and shader cache alone writable, the
    /// home folder hidden, and the compact model asked for.
    #[test]
    fn the_upscaler_runs_sandboxed_with_the_compact_model() {
        let c = upscaler().command(Path::new("/c/work"), Path::new("/c/shaders"));
        let args: Vec<String> = c.get_args().map(|a| a.to_string_lossy().into()).collect();
        let has = |pair: &[&str]| args.windows(pair.len()).any(|w| w == pair);
        assert!(has(&["--ro-bind", "/", "/"]));
        assert!(has(&["--bind", "/c/work", "/c/work"]));
        assert!(has(&["--bind", "/c/shaders", "/c/shaders"]));
        assert!(args.contains(&"--unshare-net".to_string()));
        assert!(has(&["-n", "realesr-animevideov3", "-s", "2"]));
        assert!(has(&["-i", "/c/work/in", "-o", "/c/work/out"]));
        if let Some(home) = std::env::var_os("HOME") {
            assert!(has(&["--tmpfs", &home.to_string_lossy()]));
        }
        let program = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[program + 1], "/nix/store/x/bin/realesrgan-ncnn-vulkan");
    }

    /// The real program on the graphics card: ignored, as it needs it (`nix develop` provides the
    /// program).
    #[test]
    #[ignore]
    fn the_compact_model_enlarges_twice() {
        let Some(upscaler) = Upscaler::find() else {
            panic!("{PROGRAM} not found");
        };
        let dir = std::env::temp_dir().join(format!("slatty-upscale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("in")).unwrap();
        ::image::RgbImage::from_fn(64, 48, |x, y| ::image::Rgb([x as u8 * 4, y as u8 * 5, 90]))
            .save(dir.join("in/1.png"))
            .unwrap();
        upscaler
            .enlarge(&dir, &dir.join("shaders"), &|| false)
            .unwrap();
        assert_eq!(
            ::image::image_dimensions(dir.join("out/1.png")).unwrap(),
            (128, 96)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
