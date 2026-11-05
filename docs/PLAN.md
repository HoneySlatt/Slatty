# Plan

This file says what is being worked on now: the milestones of the current stage, their status, how
each one is checked, and the known debt. Direction beyond this stage is in
[`ROADMAP.md`](ROADMAP.md); decisions are in [`adr/`](adr/README.md). A *stage* is a step of the
roadmap; a *milestone* is a small, verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **Stage 3 — Windows** ([ADR-0022](adr/0022-windows-hosts.md)). A GOG game installs and plays
  on Windows from the command line ([M3.3](#m33--play-a-gog-game)): checked by the owner on
  Windows 11 on 2026-11-05, one failure left to understand (a launch's sync downloaded every save
  just after a sync found them unchanged). What Slatty depends on Linux for, and
  the port's open questions, are in [windows-port.md](windows-port.md).
- **The pause has begun** (2026-11-05,
  [ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)): no milestone starts until Slatty's
  turn in SlimeEngine's Stage 6d ([ADR-0030](adr/0030-ported-second-onto-uwui.md)). CI's weekly
  run goes on. Only M3.3's check by hand on Windows, its fixes, and
  [M5.0](#m50--iceds-figures-measured-again) may come meanwhile.
- **[M5.0](#m50--iceds-figures-measured-again) is done** (2026-11-05): Iced's figures measured
  again by `slatty-measure` ([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)),
  the reference uwui's port must match.
- **At the resumption**, in order: M3.3's failure on Windows if it is still open
  ([compatibility.md](compatibility.md#windows)); then Stage 5. Read first the Known debt below and
  [windows-port.md](windows-port.md#open-questions).
- **Then Stage 5 — The interface on uwui** (formerly slime-ui;
  [ADR-0030](adr/0030-ported-second-onto-uwui.md)), released as 0.3 once its exit holds: Iced
  replaced wholly. What uwui lacks and serves its other clients is built in uwui. Until then Iced
  receives bug fixes only, and `slatty-app` stays free of any framework.
- **Then the rest of Stage 3** (M3.4 to M3.8), with the window, released as 0.4; then Stage 4.

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M3.1](#m31--the-windows-build-environment) | Slatty can be built for Windows from Linux | Done |
| [M3.2](#m32--everything-builds-for-windows) | Every crate builds for Windows, and the core's tests pass under Wine | Done |
| [M3.3](#m33--play-a-gog-game) | A person plays a GOG game on Windows from the command line | Checked on Windows; one failure open |
| [M5.0](#m50--iceds-figures-measured-again) | Iced's figures measured again by a command, the reference for uwui's port | Done |
| [M3.4](#m34--epic-and-steam) | A person plays Epic's and Steam's games on Windows | After Stage 5 |
| [M3.5](#m35--games-added-by-hand) | A person adds a game from no store on Windows | After Stage 5 |
| [M3.6](#m36--achievements) | Achievements through Comet on Windows | After Stage 5 |
| [M3.7](#m37--packaging) | A zip a person can download and run | After Stage 5 |
| [M3.8](#m38--real-checks) | Checked by a person on Windows 10 and 11, and documented | After Stage 5 |

## Stage 3 — Windows

Slatty on Windows 10 and 11 (x86-64), from the same code base, built and tested from Linux; Windows
games played directly, without Proton; what only Linux has stays Linux's
([ADR-0022](adr/0022-windows-hosts.md)). Up to M3.3 from the command line; M3.4 to M3.8 come after
Stage 5, which brings the window on uwui, and are planned again then
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)). Linux behaves exactly as before at
every milestone.

### M3.1 — The Windows build environment

- **Goal.** Slatty can be built for Windows from Linux.
- **Delivers.** The `windows` development shell in `flake.nix` (MinGW-w64, Wine as the test
  runner) and [windows-port.md](windows-port.md).
- **Done when.** `nix develop .#windows -c cargo check` runs for the Windows target. Done
  2026-10-28.

### M3.2 — Everything builds for Windows

- **Goal.** Every crate builds for Windows, and the core's tests that do not need Linux pass under
  Wine.
- **Delivers.** The core's `platform` module gathering what differs by system (processes, priority,
  private folders, opening without following links, known folders, finding programs); the session
  supervisor's Windows version (a job object). Keeping the Linux-only modules on Linux moved to
  M3.3 (the owner's decision, 2026-11-05).
- **Done when.** `slatty`, `slatty-gui` and the core build for Windows, and the core's tests that do
  not need Linux pass under Wine.
- **How to test.** `nix develop .#windows -c cargo check --workspace --locked`;
  `nix develop .#windows -c cargo test -p slatty-core` ([testing.md](testing.md#the-windows-build)).
- **Where it stands.** Done 2026-11-05: every crate passes `cargo check` for Windows, and the
  core's 421 tests that do not need Linux pass under Wine. Running them found four bugs, fixed:
  no cloud save was ever downloaded, and no install or update finished, on Windows
  ([windows-port.md](windows-port.md#what-windows-does-differently)); a fifth, which the first
  fix brought to Linux's read-only saves, was fixed after the closing review. Comet and the games
  Steam runs are not available on Windows yet.

### M3.3 — Play a GOG game

- **Goal.** A person installs and plays a GOG game on Windows, from the command line.
- **Delivers.** A Windows build installed (the download code is shared), started directly, its
  session followed and stopped through a job object, play time recorded, GOG's setup run with the
  person's consent for administrator rights, cloud saves in the real folders. The Linux-only
  features (Proton, umu, isolation, Linux installers, Proton builds, the Galaxy service in a prefix)
  refused on Windows where they start, with `Error::Unsupported`
  ([ADR-0027](adr/0027-linux-features-refused-on-windows-not-compiled-out.md), the owner's choice
  over compiling them out).
- **Done when.** The flow's tests pass under Wine, and a person ran it on Windows.
- **How to test.** `nix develop .#windows -c cargo test -p slatty-core -p slatty-cli`, in
  `scripts/check.sh`'s `windows` part.
- **By hand.** Install, play and sync a GOG game on Windows 10 or 11: the steps are in
  [testing.md](testing.md#checks-on-windows).
- **Where it stands.** Built 2026-11-05, checked by the owner on Windows 11 the same day
  ([compatibility.md](compatibility.md#windows)): install, pause and resume, the setup and its
  administrator prompt, a session quit and stopped, cloud saves, verify, repair and uninstall, and
  the refusals work. Fixed since, not checked again on Windows: `doctor` failing on Comet's port,
  held by GOG Galaxy; `setup` saying it was done after a step failed; `verify` succeeding with
  files damaged; exit codes shown as `Some(2)`; a last progress line of 0 files of 0. Open: a
  launch right after `slatty cloud sync` downloaded all 10 of Hollow Knight's saves, which that
  sync had found unchanged; Slatty's log of it is needed to find why. Not checked: play time sent
  (the session lasted under a minute, which is not sent) and a session across the computer's sleep.
  Under Wine the core's
  428 tests and the command line's pass, `play_windows.rs` playing a fake GOG game through the real
  supervisor (started, followed past its launcher, stopped, its setup waiting for consent, Proton
  refused). The owner decided three questions: Linux's features refused where they start
  ([ADR-0027](adr/0027-linux-features-refused-on-windows-not-compiled-out.md)), GOG's setup asked
  for at launch ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)), every
  installed file flushed ([ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)).
  Found and fixed on the way: play time counted the computer's sleep on Windows; uninstalling a
  folder linked back with a junction left the junction; a read-only save, or a read-only file of
  the game, could not be replaced or deleted; the default games folder was relative to where
  Slatty started, as Windows sets no `HOME`. Comet stays off on Windows (M3.6).

### M3.4 — Epic and Steam

- **Goal.** A person plays Epic's and Steam's games on Windows.
- **Delivers.** Epic's games launched directly and signed in; Steam found from the registry, its
  games listed, played, installed and uninstalled through the client, the game running found.
- **Done when.** The tests pass under Wine, and a person ran both on Windows.
- **By hand.** Play an Epic game and a Steam game on Windows.

### M3.5 — Games added by hand

- **Goal.** A person adds a game from no store on Windows.
- **Delivers.** Programs, archives and installers added on Windows.
- **Done when.** The tests pass under Wine, and a person added and played one on Windows.

### M3.6 — Achievements

- **Goal.** Achievements through Comet on Windows.
- **Delivers.** Comet on Windows, and whether GOG Galaxy's service is needed there
  ([windows-port.md](windows-port.md#open-questions)).
- **Done when.** A person unlocked an achievement in a game on Windows, reported to GOG.

### M3.7 — Packaging

- **Goal.** A zip a person can download and run.
- **Delivers.** A zip with `slatty.exe` and the licences, built by Nix. A release for Windows is
  published only once M3.8 passes.
- **Done when.** The zip built by Nix runs on Windows.

### M3.8 — Real checks

- **Goal.** Slatty checked by a person on Windows 10 and 11, and documented.
- **Delivers.** The checks recorded in [compatibility.md](compatibility.md); the command line's and
  the user guide's Windows parts; SECURITY.md (where secrets and files live on Windows); the README.
- **Done when.** The stage's exit in
  [ROADMAP.md](ROADMAP.md#stage-3--windows--04) holds.

The stage closes with a stage audit, every dimension of the project's method, before 0.4 is
released: mutation testing (`cargo mutants`) first on cloud sync, installs and uninstalls
([ADR-0024](adr/0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md)). Audits are recorded in
[audits/](audits/).

## During the pause

### M5.0 — Iced's figures measured again

- **Goal.** Iced's figures taken again by one command, on the owner's machine as it is now
  (Umbriel), as the reference that uwui's port must match
  ([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)).
- **Delivers.**
  - A generator of the fake profile of 10,000 games
    ([testing.md](testing.md#the-application-at-10000-games)), its covers made with `image`
    (already in `slatty-app`), 1.1 GB, in a throwaway folder it refuses if not empty.
  - The probe in `slatty-gui`, behind a feature off by default, built by `scripts/check.sh`'s lints
    so that it keeps compiling during the pause. It writes a log whose format does not name Iced,
    so that uwui's port writes the same.
  - One command that sets the isolated profile itself (its XDG folders, no keyring, no network),
    runs the measurements, and reads the log into figures with their spread. It does not depend on
    the framework, so that it measures the port too.
  - A headless compositor with Xwayland in the development shell (`flake.nix`).
  - testing.md describing it; the figures in [measurements.md](measurements.md), with the commit.
- **Measured**, release build, 10 runs where it repeats:
  - the library shown after start, warm and cold;
  - scrolling the grid by the probe, 1,200 frames: the time between frames (median, p99, max,
    frames over 4.17, 6.25 and 16.7 ms) and `view`'s time;
  - at rest for 60 s: frames drawn, CPU ticks, wake-ups of every thread, under Wayland and
    Xwayland;
  - resident memory after start, with every cover in, after scrolling the whole grid, after 200
    game pages with key art;
  - video memory at the same points, from the card's `fdinfo` (`drm-memory-vram`), measured first
    as the owner decided;
  - the GPU's power at rest against the desktop without Slatty.
- **Where.** Start, rest and memory in the headless compositor; scrolling at 240 Hz and power on
  the owner's display, with their agreement. The probe scrolls by itself: no input enters the
  session.
- **Done when.** The command ran twice and the second run's medians fall within the first run's
  range; measurements.md holds the figures; the debt below on figures and video memory is closed or
  restated.
- **Not measured.** Input to visible response, which Iced does not report; the lowest tier, which no
  machine stands for.
- **Where it stands.** Done 2026-11-05: `slatty-measure` (`crates/measure`), the probe, sway and
  Xwayland in the development shell. Each command ran twice, 10 runs a series; the figures are in
  [measurements.md](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05).
  Every second series' median falls within the first's range but three: video memory after the
  game pages, and on the desktop the first frame and every cover shown, the wait for the first
  frame having varied; the owner accepted them. Measuring found the start-up wait, the stalls of
  scrolling, and memory over its budgets, written in the debt below; Iced receiving bug fixes only,
  none was fixed here. The owner then set video memory's budget (≤ 256 MB), the start-up budget
  measured at the first frame shown, and wake-ups counted voluntary and not
  ([quality bar](ROADMAP.md#quality-bar)).

## Known debt

- **Iced's and zbus's unmaintained crates** (`rustybuzz`, `ttf-parser`, `zbus_names`, `zvariant`,
  `zvariant_derive`) are ignored in `deny.toml`, each with its reason; they go with Iced
  (Stage 5) and with zbus 6.
- **Not checked for real**: the couch mode on a Steam Deck (Stage 6) and on a television; emulated
  games in each emulator; the rest is listed in [compatibility.md](compatibility.md).
- **The lowest hardware tier and the Steam Deck are not measured**
  ([quality bar](ROADMAP.md#quality-bar)), nor Intel's and NVIDIA's GPUs: the budgets are measured
  on the high tier alone.
- **Wake-ups at rest**: one thread of zbus's pool (`blocking-1`) wakes every 500 ms (120 wake-ups
  a minute, against a budget of 0 in the [quality bar](ROADMAP.md#quality-bar)). It was thought
  kept by the keyring's connection, but it wakes so with no session bus at all (M5.0): what keeps
  it is to be found. To be fixed in Stage 4.
- **Start-up misses its budget**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): the first
  frame shows 370–630 ms after the library is in the state, of which about 230 ms is the
  compositor's answer to it; the rest is not explained. A cold start also takes 155 ms to put the
  library in the state, past the 135 ms budget. To be understood before uwui's port is measured
  against it.
- **Scrolling stalls on each new row of covers**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): a
  frame of about 30 ms each time, for a reason not yet found, and the median frame misses 240 Hz
  too. For uwui's port.
- **Memory over its budgets**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): resident
  memory with every cover in and after 200 game pages; video memory at all three points (about
  300 MB, 452 MB and 388 MB, against 256 MB). For uwui's port.
- **The Nix package sets the CI's pace**: CI runs `scripts/check.sh` in three parts at once, with
  a Cargo cache ([ADR-0026](adr/0026-ci-runs-the-checks-in-three-parts-at-once.md)), and a warm
  run took 9 min 13 s, within its 20 minutes, the package's 9 minutes being all of it, as no Cargo
  cache reaches its build. A run whose Linux cache is cold, after a change to `Cargo.lock` or
  `flake.lock`, took 18 min 55 s once: close to the budget. Building the package with crane is the
  step left if runs come closer ([measurements.md](measurements.md#ci-in-three-parts-2026-11-05)).
- **The download and no-change sync budgets are not measured** (set 2026-11-05): their benchmarks
  are to be written.
- **A crash leaves no report** ([quality bar](ROADMAP.md#quality-bar)): to be built in the core and
  the command line; the interface's notice waits for uwui (Stage 5), as Iced receives bug fixes
  only.
- **CI runs on GitHub's Linux machines only**: Windows is checked by building from Linux and by
  the core's and the command line's tests under Wine, in `scripts/check.sh`'s `windows` part, not
  by a Windows runner.
  Clippy for Windows is not run, as the `windows` shell lacks it.
- **Links to files are tested on Linux only**: making a symbolic link on Windows needs a right
  users lack, so the tests of a link to a file planted by a game
  (`installer::tests::downloading_cannot_truncate_a_temporary_symlink_target`) are `#[cfg(unix)]`.
  Links to folders are tested on both, as junctions on Windows (`platform::link_folder`).
- **Some tests of shared behaviour run on Linux only**, as their setup uses file modes or links:
  `added::a_folder_too_wide_or_a_program_out_of_its_folder_is_refused`,
  `installer::tests::a_restart_that_cannot_delete_the_download_keeps_it`,
  `maintenance::a_game_whose_emptied_folder_cannot_be_removed_is_uninstalled`,
  `epic::download::an_unreadable_file_is_repaired`,
  `steam::local::what_steam_wrote_of_a_download_is_measured_in_its_folder`. To be made for Windows
  (a file held open, a lock) with the milestone of each area.
- **Windows gaps found while testing, not fixed**: `added::check_folder` reads `HOME`, which
  Windows does not set, so the profile folder is not refused there (M3.5); `import --from-heroic`
  looks for Heroic's folder from `HOME` or `XDG_CONFIG_HOME` too, so on Windows it reads a folder
  relative to where Slatty started (it only reads, and finds nothing); games added by hand are not
  flushed to the disk on Windows, which has no `fsutil::sync_filesystem` (M3.5, as installs do in
  [ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)).
- **Windows behaviour checked under Wine only**: `platform::lowest_priority`'s background mode (its
  disk priority shows nowhere under Wine; its processor priority does), and a thread that work in
  the background starts lowers itself only if it calls it (`installer::verify_file`'s do); a
  setup program started with administrator rights, whose output is not logged, which cannot be
  stopped and whose own programs are not followed
  ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)): M3.3's check on Windows saw
  its prompt come to the front, not these. Flushing every installed file was measured there, in a
  virtual machine only
  ([measurements.md](measurements.md#flushing-every-installed-file-on-windows-2026-11-05)).

## Ideas

Not planned work: each says what it is and what it would gain. The owner decides which become
milestones.

- **A Windows runner in CI** for the core's tests, beside Wine: Wine let through no false failure
  but is not Windows (NTFS's file index, POSIX renames, junctions behave as Windows does only
  there).

Finished stages move to [`history.md`](history.md), in a few lines each, so that this file holds the
current stage only, under about 2,000 words.
