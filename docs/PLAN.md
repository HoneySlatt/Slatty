# Plan

This file says what is being worked on now: the milestones of the current stage, their status, how
each one is checked, and the known debt. Direction beyond this stage is in
[`ROADMAP.md`](ROADMAP.md); decisions are in [`adr/`](adr/README.md). A *stage* is a step of the
roadmap; a *milestone* is a small, verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **Stage 3 — Windows** ([ADR-0022](adr/0022-windows-hosts.md)). A GOG game installs and plays
  on Windows from the command line ([M3.3](#m33--play-a-gog-game)): built and tested under Wine,
  its check by the owner on Windows 10 or 11 still to run. What Slatty depends on Linux for, and
  the port's open questions, are in [windows-port.md](windows-port.md).
- **The pause has begun** (2026-11-05,
  [ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)): no milestone starts until
  SlimeEngine's Stage 6d can run an application. CI's weekly run goes on. Only M3.3's check by
  hand on Windows, and the fixes it calls for, may come meanwhile.
- **At the resumption**, in order: M3.3's check on Windows if it has not run, its results in
  [compatibility.md](compatibility.md); then Stage 5. Read first the Known debt below and
  [windows-port.md](windows-port.md#open-questions).
- **Then Stage 5 — The interface on slime-ui**
  ([ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md)), released as 0.3: Iced replaced wholly.
  What slime-ui lacks and serves its other clients is built in slime-ui, in SlimeEngine. Until
  then Iced receives bug fixes only, and `slatty-app` stays free of any framework.
- **Then the rest of Stage 3** (M3.4 to M3.8), with the window, released as 0.4; then Stage 4.

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M3.1](#m31--the-windows-build-environment) | Slatty can be built for Windows from Linux | Done |
| [M3.2](#m32--everything-builds-for-windows) | Every crate builds for Windows, and the core's tests pass under Wine | Done |
| [M3.3](#m33--play-a-gog-game) | A person plays a GOG game on Windows from the command line | Built; its check on Windows waits |
| [M3.4](#m34--epic-and-steam) | A person plays Epic's and Steam's games on Windows | After Stage 5 |
| [M3.5](#m35--games-added-by-hand) | A person adds a game from no store on Windows | After Stage 5 |
| [M3.6](#m36--achievements) | Achievements through Comet on Windows | After Stage 5 |
| [M3.7](#m37--packaging) | A zip a person can download and run | After Stage 5 |
| [M3.8](#m38--real-checks) | Checked by a person on Windows 10 and 11, and documented | After Stage 5 |

## Stage 3 — Windows

Slatty on Windows 10 and 11 (x86-64), from the same code base, built and tested from Linux; Windows
games played directly, without Proton; what only Linux has stays Linux's
([ADR-0022](adr/0022-windows-hosts.md)). Up to M3.3 from the command line; M3.4 to M3.8 come after
Stage 5, which brings the window on slime-ui, and are planned again then
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
- **Where it stands.** Built 2026-11-05; the check on Windows has not run. Under Wine the core's
  428 tests and the command line's pass, `play_windows.rs` playing a fake GOG game through the real
  supervisor (started, followed past its launcher, stopped, its setup waiting for consent, Proton
  refused). The owner decided three questions: Linux's features refused where they start
  ([ADR-0027](adr/0027-linux-features-refused-on-windows-not-compiled-out.md)), GOG's setup asked
  for at launch ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)), every
  installed file flushed ([ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)).
  Found and fixed on the way: play time counted the computer's sleep on Windows; uninstalling a
  folder linked back with a junction left the junction; a read-only save, or a read-only file of
  the game, could not be replaced or deleted; the default games folder was relative to where
  Slatty started, as Windows sets no `HOME`. Not verified anywhere but on Windows itself: the
  administrator prompt, a real game's session and stop, saves in the real folders, the cost of
  flushing every file. Comet stays off on Windows (M3.6).

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

## Known debt

- **Iced's and zbus's unmaintained crates** (`rustybuzz`, `ttf-parser`, `zbus_names`, `zvariant`,
  `zvariant_derive`) are ignored in `deny.toml`, each with its reason; they go with Iced
  ([ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md)) and with zbus 6.
- **Not checked for real**: the couch mode on a Steam Deck (Stage 6) and on a television; emulated
  games in each emulator; the rest is listed in [compatibility.md](compatibility.md).
- **The lowest hardware tier and the Steam Deck are not measured**
  ([quality bar](ROADMAP.md#quality-bar)), nor Intel's and NVIDIA's GPUs: the budgets are measured
  on the high tier alone.
- **The performance figures are not reproducible by a command**: [testing.md](testing.md) measures
  start-up, scrolling and memory with a temporary probe that is not in the repository, and the rows
  of [measurements.md](measurements.md) on covers predate the copies made at the size drawn.
- **Wake-ups at rest**: one thread of zbus's pool, kept by the keyring's connection, wakes every
  500 ms (120 wake-ups a minute, against a budget of 0 in the
  [quality bar](ROADMAP.md#quality-bar)). To be fixed in Stage 4.
- **The Nix package sets the CI's pace**: CI runs `scripts/check.sh` in three parts at once, with
  a Cargo cache ([ADR-0026](adr/0026-ci-runs-the-checks-in-three-parts-at-once.md)), and a warm
  run took 9 min 13 s, within its 20 minutes, the package's 9 minutes being all of it, as no Cargo
  cache reaches its build. A run whose Linux cache is cold, after a change to `Cargo.lock` or
  `flake.lock`, took 18 min 55 s once: close to the budget. Building the package with crane is the
  step left if runs come closer ([measurements.md](measurements.md#ci-in-three-parts-2026-11-05)).
- **The download and no-change sync budgets are not measured** (set 2026-11-05): their benchmarks
  are to be written.
- **Video memory has no budget**: the owner decided on 2026-11-05 to measure it first (at rest,
  after scrolling the whole grid, after 200 game pages), then set it. The measurement needs the
  fake profile and the probe in the repository, and a headless compositor, so that no window opens
  on the desktop.
- **A crash leaves no report** ([quality bar](ROADMAP.md#quality-bar)): to be built in the core and
  the command line; the interface's notice waits for slime-ui (Stage 5), as Iced receives bug fixes
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
  the background starts lowers itself only if it calls it (`installer::verify_file`'s do); the
  cost of flushing every installed file
  ([ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md),
  `fsutil::tests::syncing_every_small_file`); a setup program started with administrator rights,
  whose output is not logged, which cannot be stopped and whose own programs are not followed
  ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)). To be checked on Windows
  with M3.3's check by hand.

## Ideas

Not planned work: each says what it is and what it would gain. The owner decides which become
milestones.

- **A Windows runner in CI** for the core's tests, beside Wine: Wine let through no false failure
  but is not Windows (NTFS's file index, POSIX renames, junctions behave as Windows does only
  there).

Finished stages move to [`history.md`](history.md), in a few lines each, so that this file holds the
current stage only, under about 2,000 words.
