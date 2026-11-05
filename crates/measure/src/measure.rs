//! The measurements, in two series: those a headless compositor can take (start, rest, memory,
//! video memory), and those that need the person's display (scrolling at its rate, the graphics
//! card's power). Each repeats `runs` times and gives its figures with their spread.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::log::{self, Log};
use crate::profile::{self, Profile};
use crate::session::{self, Display, Headless, Running};
use crate::stats::{Spread, percentile};
use crate::system;

/// How long the first start may take: it makes a copy of every cover at the size drawn, about
/// 20 s at 10,000 games.
const FIRST_START: Duration = Duration::from_secs(300);
/// How long a later start may take.
const START: Duration = Duration::from_secs(60);
/// How long a step of the probe may take: 200 pages wait up to 5 s each for their key art.
const STEP: Duration = Duration::from_secs(1200);
/// How long memory is left to settle before it is read.
const MEMORY_SETTLE: Duration = Duration::from_secs(3);
/// How long the program is left after every cover shows before its rest is measured: the start's
/// last work (the graphics check, 5 s after start) is over.
const REST_SETTLE: Duration = Duration::from_secs(10);
/// How long rest, and the card's power, are measured.
const REST: Duration = Duration::from_secs(60);
/// How often the card's power is read.
const POWER_EVERY: Duration = Duration::from_secs(1);

/// What both series need.
pub struct Setup {
    pub profile: Profile,
    pub program: PathBuf,
    pub runs: usize,
    /// Where this measurement writes its logs and its report.
    pub folder: PathBuf,
}

impl Setup {
    /// Makes or opens the profile in `folder`, and builds the interface with its probe.
    pub fn new(folder: &Path, runs: usize) -> Result<Self> {
        if runs == 0 {
            bail!("--runs must be at least 1");
        }
        let profile = Profile::open_or_make(folder)?;
        let program = build()?;
        let stamp = stamp();
        let folder = profile.runs().join(stamp);
        std::fs::create_dir_all(&folder)?;
        Ok(Self {
            profile,
            program,
            runs,
            folder,
        })
    }

    fn run(
        &self,
        display: &Display,
        runtime: &Path,
        step: Option<&str>,
        name: &str,
    ) -> Result<Running> {
        Running::start(
            &self.program,
            &self.profile,
            runtime,
            display,
            step,
            &self.folder.join(name),
        )
    }
}

/// Builds the interface with its probe, in release, from this workspace, into `target/probe`: the
/// release build people run stays without it. Returns its path.
fn build() -> Result<PathBuf> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    eprintln!("building slatty-gui with its probe, in release");
    let status = Command::new(cargo)
        .current_dir(&workspace)
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "slatty-gui",
            "--features",
            "probe",
            "--target-dir",
            "target/probe",
        ])
        .status()
        .context("running cargo")?;
    if !status.success() {
        bail!("building slatty-gui failed");
    }
    Ok(workspace
        .join("target/probe/release/slatty-gui")
        .canonicalize()?)
}

/// Seconds since the Unix epoch: a folder name that sorts by time.
fn stamp() -> String {
    (session::now() / 1_000_000_000).to_string()
}

/// What one start found, in milliseconds after the program was started.
struct Start {
    library: f64,
    shown: f64,
    covers: f64,
}

impl Start {
    fn read(run: &mut Running, timeout: Duration) -> Result<Self> {
        let covers = run.wait_for("covers", None, timeout)?;
        let library = run.wait_for("library", None, timeout)?;
        let shown = run.wait_for("shown", None, timeout)?;
        Ok(Self {
            library: run.since_start(&library),
            shown: run.since_start(&shown),
            covers: run.since_start(&covers),
        })
    }
}

/// Memory read at one point of a run, in bytes.
#[derive(Clone, Copy)]
struct Memory {
    resident: u64,
    video: Option<u64>,
}

impl Memory {
    fn read(pid: u32) -> Result<Self> {
        Ok(Self {
            resident: system::resident(pid)?,
            video: system::video_memory(pid)?,
        })
    }
}

/// What 60 s at rest cost.
struct Rest {
    frames: usize,
    ticks: u64,
    wakeups: u64,
}

/// Measures 60 s of rest of a program whose covers all show.
fn rest(run: &mut Running) -> Result<Rest> {
    std::thread::sleep(REST_SETTLE);
    run.read_log()?;
    let frames_before = run.log.count("frame");
    let (ticks, wakeups) = (system::ticks(run.pid)?, system::wakeups(run.pid)?);
    std::thread::sleep(REST);
    let (ticks_after, wakeups_after) = (system::ticks(run.pid)?, system::wakeups(run.pid)?);
    run.read_log()?;
    Ok(Rest {
        frames: run.log.count("frame") - frames_before,
        ticks: ticks_after - ticks,
        wakeups: system::wakeups_between(&wakeups, &wakeups_after),
    })
}

/// The headless series: start warm and cold, memory and video memory at three points, rest under
/// Wayland and Xwayland. Returns the report.
pub fn headless(setup: &Setup) -> Result<String> {
    let runs = setup.runs;
    let runtime = setup.profile.root.join("runtime/headless");
    let sway = Headless::start(&runtime)?;
    let wayland = Display::Wayland(sway.wayland.clone());
    let x11 = Display::X11(sway.x11.clone());

    // Its game pages make the key art's copies too, about 3 s each the first time: every run of
    // the series finds them made.
    eprintln!("first start: covers and key art made at the size drawn, caches filled");
    let mut first = setup.run(&wayland, &runtime, Some("pages"), "first")?;
    first.wait_for("covers", None, FIRST_START)?;
    first.wait_for("end", Some("pages"), STEP)?;
    let window = first.stop()?.window();

    eprintln!("start, warm, {runs} runs; memory after start");
    let mut warm = Vec::new();
    let mut after_start = Vec::new();
    let mut mapped = std::collections::HashSet::new();
    for i in 0..runs {
        let mut run = setup.run(&wayland, &runtime, None, &format!("warm-{i}"))?;
        warm.push(Start::read(&mut run, START)?);
        std::thread::sleep(MEMORY_SETTLE);
        after_start.push(Memory::read(run.pid)?);
        mapped.extend(system::mapped_files(run.pid)?);
        run.stop()?;
    }

    eprintln!("start, cold, {runs} runs");
    let mut dropped: Vec<PathBuf> = system::files_under(&setup.profile.home());
    dropped.extend(mapped);
    dropped.push(setup.program.clone());
    let mut cold = Vec::new();
    for i in 0..runs {
        system::drop_from_cache(&dropped)?;
        let mut run = setup.run(&wayland, &runtime, None, &format!("cold-{i}"))?;
        cold.push(Start::read(&mut run, START)?);
        run.stop()?;
    }

    eprintln!("the whole grid scrolled, {runs} runs");
    let mut after_grid = Vec::new();
    for i in 0..runs {
        let mut run = setup.run(&wayland, &runtime, Some("grid"), &format!("grid-{i}"))?;
        run.wait_for("end", Some("grid"), STEP)?;
        std::thread::sleep(MEMORY_SETTLE);
        after_grid.push(Memory::read(run.pid)?);
        run.stop()?;
    }

    eprintln!("{} game pages opened, {runs} runs", profile::WITH_KEY_ART);
    let mut after_pages = Vec::new();
    let mut no_art = Vec::new();
    for i in 0..runs {
        let mut run = setup.run(&wayland, &runtime, Some("pages"), &format!("pages-{i}"))?;
        run.wait_for("end", Some("pages"), STEP)?;
        std::thread::sleep(MEMORY_SETTLE);
        after_pages.push(Memory::read(run.pid)?);
        no_art.push(run.stop()?.count("no-art"));
    }

    let mut rests = Vec::new();
    for (name, display) in [("Wayland", &wayland), ("Xwayland", &x11)] {
        eprintln!("rest under {name}, {runs} runs of {} s", REST.as_secs());
        if name == "Xwayland" {
            // Its scale differs: the covers' copies at its size are made once, first.
            let mut run = setup.run(display, &runtime, None, "first-xwayland")?;
            run.wait_for("covers", None, FIRST_START)?;
            run.stop()?;
        }
        let mut series = Vec::new();
        for i in 0..runs {
            let mut run = setup.run(display, &runtime, None, &format!("rest-{name}-{i}"))?;
            run.wait_for("covers", None, START)?;
            series.push(rest(&mut run)?);
            run.stop()?;
        }
        rests.push((name, series));
    }
    let sway_version = sway.version.clone();
    drop(sway);

    let mut report = header(
        setup,
        &format!(
            "Headless: {sway_version}, one output of {}, the window tiled",
            session::HEADLESS_OUTPUT
        ),
        window,
    );
    report.push_str("| Figure | Median (range) |\n|---|---|\n");
    starts(&mut report, "warm", &warm);
    starts(&mut report, "cold", &cold);
    memories(&mut report, "after start, every cover shown", &after_start);
    memories(&mut report, "after scrolling the whole grid", &after_grid);
    memories(
        &mut report,
        &format!("after {} game pages with key art", profile::WITH_KEY_ART),
        &after_pages,
    );
    let missing: usize = no_art.iter().sum();
    if missing > 0 {
        writeln!(
            report,
            "| Game pages whose key art did not show within 5 s | {missing} in all |"
        )?;
    }
    for (name, series) in &rests {
        let spread = |f: &dyn Fn(&Rest) -> f64| show(&series.iter().map(f).collect::<Vec<_>>(), 0);
        writeln!(
            report,
            "| At rest for {} s, {name}: frames drawn / CPU ticks / wake-ups of every thread | {} / {} / {} |",
            REST.as_secs(),
            spread(&|r| r.frames as f64),
            spread(&|r| r.ticks as f64),
            spread(&|r| r.wakeups as f64),
        )?;
    }
    Ok(report)
}

/// The series on the person's display: start (to compare with the headless compositor's),
/// scrolling, and the card's power at rest against the desktop without the program.
pub fn desktop(setup: &Setup) -> Result<String> {
    let runs = setup.runs;
    let (Some(runtime_dir), Some(socket)) = (
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("WAYLAND_DISPLAY"),
    ) else {
        bail!("no Wayland session here: WAYLAND_DISPLAY and XDG_RUNTIME_DIR must be set");
    };
    // The session's socket by its full path: the program gets a runtime folder of its own.
    let wayland = Display::Wayland(Path::new(&runtime_dir).join(socket));
    let runtime = setup.profile.root.join("runtime/desktop");
    session::empty_private_folder(&runtime)?;

    eprintln!("first start: caches filled");
    let mut first = setup.run(&wayland, &runtime, None, "desktop-first")?;
    // A screen turned off sends a window no frame: said at once rather than after five minutes.
    first
        .wait_for("shown", None, START)
        .context("the window drew nothing: is the screen on, and kept on?")?;
    first.wait_for("covers", None, FIRST_START)?;
    let window = first.stop()?.window();

    eprintln!("start, warm, {runs} runs");
    let mut warm = Vec::new();
    for i in 0..runs {
        let mut run = setup.run(&wayland, &runtime, None, &format!("desktop-warm-{i}"))?;
        warm.push(Start::read(&mut run, START)?);
        run.stop()?;
    }

    eprintln!("scrolling, {runs} runs");
    let mut scrolls = Vec::new();
    for i in 0..runs {
        let mut run = setup.run(&wayland, &runtime, Some("scroll"), &format!("scroll-{i}"))?;
        let begin = run.wait_for("begin", Some("scroll"), STEP)?;
        let end = run.wait_for("end", Some("scroll"), STEP)?;
        let log = run.stop()?;
        scrolls.push(Scroll::read(&log, begin.at, end.at));
    }

    let power = match system::gpu_power() {
        None => None,
        Some((_, sensor)) => {
            eprintln!(
                "the card's power: {} s without, with, and without the program",
                REST.as_secs()
            );
            let before = power_over(REST);
            let mut run = setup.run(&wayland, &runtime, None, "power")?;
            run.wait_for("covers", None, START)?;
            std::thread::sleep(REST_SETTLE);
            let with = power_over(REST);
            run.stop()?;
            let after = power_over(REST);
            Some((sensor, before, with, after))
        }
    };

    let mut report = header(setup, "The desktop's own display", window);
    report.push_str("| Figure | Median (range) |\n|---|---|\n");
    starts(&mut report, "warm", &warm);
    let spread = |f: &dyn Fn(&Scroll) -> f64, decimals| {
        show(&scrolls.iter().map(f).collect::<Vec<_>>(), decimals)
    };
    writeln!(
        report,
        "| Scrolling by the probe ({} frames): time between frames, p50 / p99 / max (ms) | {} / {} / {} |",
        spread(&|s| s.frames as f64, 0),
        spread(&|s| s.p50, 2),
        spread(&|s| s.p99, 2),
        spread(&|s| s.max, 2),
    )?;
    writeln!(
        report,
        "| Frames over 4.17 / 6.25 / 16.7 ms | {} / {} / {} |",
        spread(&|s| s.over[0] as f64, 0),
        spread(&|s| s.over[1] as f64, 0),
        spread(&|s| s.over[2] as f64, 0),
    )?;
    writeln!(
        report,
        "| `view` while scrolling, p50 / p99 (ms) | {} / {} |",
        spread(&|s| s.view_p50, 3),
        spread(&|s| s.view_p99, 3),
    )?;
    if let Some((sensor, before, with, after)) = power {
        writeln!(
            report,
            "| The card's power at rest (W), without / with the program / without | {} / {} / {} |",
            show(&before, 1),
            show(&with, 1),
            show(&after, 1),
        )?;
        writeln!(
            report,
            "\nPower read from `{}` every second.",
            sensor.display()
        )?;
    } else {
        report.push_str("\nNo graphics card here reports its power: not measured.\n");
    }
    Ok(report)
}

/// What one scroll of the probe found.
struct Scroll {
    frames: usize,
    p50: f64,
    p99: f64,
    max: f64,
    /// Frames over 4.17, 6.25 and 16.7 ms.
    over: [usize; 3],
    view_p50: f64,
    view_p99: f64,
}

impl Scroll {
    fn read(log: &Log, begin: u128, end: u128) -> Self {
        let frames = log.frames(begin, end);
        let intervals = log::intervals(&frames);
        let views = log.views(begin, end);
        let over = |limit: f64| intervals.iter().filter(|i| **i > limit).count();
        Self {
            frames: frames.len(),
            p50: percentile(&intervals, 50.0).unwrap_or(f64::NAN),
            p99: percentile(&intervals, 99.0).unwrap_or(f64::NAN),
            max: intervals.iter().copied().fold(f64::NAN, f64::max),
            over: [
                over(1000.0 / 240.0),
                over(1000.0 / 160.0),
                over(1000.0 / 60.0),
            ],
            view_p50: percentile(&views, 50.0).unwrap_or(f64::NAN),
            view_p99: percentile(&views, 99.0).unwrap_or(f64::NAN),
        }
    }
}

/// The card's power over `span`, read every `POWER_EVERY`, in watts.
fn power_over(span: Duration) -> Vec<f64> {
    let end = Instant::now() + span;
    let mut readings = Vec::new();
    while Instant::now() < end {
        if let Some((watts, _)) = system::gpu_power() {
            readings.push(watts);
        }
        std::thread::sleep(POWER_EVERY);
    }
    readings
}

/// `median (min–max)` of `values`, or a dash when there is none.
fn show(values: &[f64], decimals: usize) -> String {
    Spread::of(values).map_or_else(|| "–".into(), |s| s.show(decimals))
}

fn starts(report: &mut String, which: &str, starts: &[Start]) {
    let spread = |f: &dyn Fn(&Start) -> f64| show(&starts.iter().map(f).collect::<Vec<_>>(), 0);
    let _ = writeln!(
        report,
        "| Start, {which}: library in the state / shown / every cover shown (ms after start) | {} / {} / {} |",
        spread(&|s| s.library),
        spread(&|s| s.shown),
        spread(&|s| s.covers),
    );
}

fn memories(report: &mut String, when: &str, memories: &[Memory]) {
    let megabytes = |bytes: u64| bytes as f64 / 1e6;
    let resident: Vec<f64> = memories.iter().map(|m| megabytes(m.resident)).collect();
    let video: Vec<f64> = memories
        .iter()
        .filter_map(|m| m.video.map(megabytes))
        .collect();
    let _ = writeln!(
        report,
        "| Memory {when}: resident / video (MB) | {} / {} |",
        show(&resident, 0),
        show(&video, 0),
    );
}

/// The report's heading: the machine, the build, the window.
fn header(setup: &Setup, display: &str, window: Option<(f64, f64, f64)>) -> String {
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let changed = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
    let window = window.map_or_else(
        || "unknown".into(),
        |(w, h, scale)| {
            format!(
                "{:.0}×{:.0} at scale {scale} ({:.0}×{:.0} pixels)",
                w,
                h,
                w * scale,
                h * scale
            )
        },
    );
    format!(
        "Machine: {}, Linux {}. Build: release, commit {commit}{}. {display}. Window: {window}. \
         {} runs each; the logs are in `{}`.\n\n",
        system::processor(),
        system::kernel(),
        if changed {
            " with changes not committed"
        } else {
            ""
        },
        setup.runs,
        setup.folder.display(),
    )
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
