# Plan

This file says what is being worked on now: the milestones of the current stage, their status, how
each one is checked, and the known debt. Direction beyond this stage is in
[`ROADMAP.md`](ROADMAP.md); decisions are in [`adr/`](adr/README.md). A *stage* is a step of the
roadmap; a *milestone* is a small, verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **Stage 3 — Windows, without the interface** ([ADR-0022](adr/0022-windows-hosts.md)), to be
  released as 0.3. Every crate builds for Windows and the core's tests pass under Wine. Next:
  playing a GOG game from the command line ([M3.3](#m33--play-a-gog-game)). What Slatty depends
  on Linux for, and the port's open questions, are in [windows-port.md](windows-port.md).
- **Stage 5 — The interface on slime-ui, Windows included**
  ([ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md)): waits for SlimeEngine's Stage 6d to run
  applications. Until then every piece of interface work waits; Iced receives bug fixes only, and
  `slatty-app` stays free of any framework.

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M3.1](#m31--the-windows-build-environment) | Slatty can be built for Windows from Linux | Done |
| [M3.2](#m32--everything-builds-for-windows) | Every crate builds for Windows, and the core's tests pass under Wine | Done |
| [M3.3](#m33--play-a-gog-game) | A person plays a GOG game on Windows from the command line | Planned |
| [M3.4](#m34--epic-and-steam) | A person plays Epic's and Steam's games on Windows | Planned |
| [M3.5](#m35--games-added-by-hand) | A person adds a game from no store on Windows | Planned |
| [M3.6](#m36--achievements) | Achievements through Comet on Windows | Planned |
| [M3.7](#m37--packaging) | A zip a person can download and run | Planned |
| [M3.8](#m38--real-checks) | Checked by a person on Windows 10 and 11, and documented | Planned |

## Stage 3 — Windows, without the interface

Slatty's core and command line on Windows 10 and 11 (x86-64), from the same code base, built and
tested from Linux; Windows games played directly, without Proton; what only Linux has stays Linux's
([ADR-0022](adr/0022-windows-hosts.md)). The window comes with Stage 5, on slime-ui
([ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md)). Linux behaves exactly as before at every
milestone.

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
  modules (Proton, umu, isolation, Linux installers, Proton builds, the Galaxy service in a prefix)
  built on Linux alone, their callers on Windows getting `Error::Unsupported`: today they build for
  Windows, and a game set to umu fails there on a missing program.
- **Done when.** The flow's tests pass under Wine, and a person ran it on Windows.
- **How to test.** `nix develop .#windows -c cargo test --workspace`.
- **By hand.** Install, play and sync a GOG game on Windows 10 or 11.

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
  [ROADMAP.md](ROADMAP.md#stage-3--windows-without-the-interface--03) holds.

The stage closes with a stage audit, every dimension of the project's method, before 0.3 is
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
- **CI is over its budget**: 30 min 44 s on 2026-11-05, against 20
  ([quality bar](ROADMAP.md#quality-bar)), every build starting cold; each step's time is in
  [measurements.md](measurements.md#a-ci-run-every-build-cold-2026-11-05). Next, and started:
  the owner chose a cache first, then the Windows build in a job of its own if needed. What the
  figures showed since, to put to the owner before building: a Cargo cache cannot reach the Nix
  package's 10 minutes, which rebuilds every crate in Nix's sandbox (`buildRustPackage`), and a
  cache of `target/` for both systems may not fit GitHub's 10 GB per repository. Jobs in parallel,
  each running a part of `scripts/check.sh` (Linux, Windows, the package), would take about
  16 minutes cold with no cache; a Cargo cache would then shorten the Linux and Windows jobs.
- **The download and no-change sync budgets are not measured** (set 2026-11-05): their benchmarks
  are to be written.
- **Video memory has no budget**: the owner decided on 2026-11-05 to measure it first (at rest,
  after scrolling the whole grid, after 200 game pages), then set it. The measurement needs the
  fake profile and the probe in the repository, and a headless compositor, so that no window opens
  on the desktop; after the CI's cache.
- **A crash leaves no report** ([quality bar](ROADMAP.md#quality-bar)): to be built in the core and
  the command line after the CI's cache; the interface's notice waits for slime-ui (Stage 5), as
  Iced receives bug fixes only.
- **CI runs on GitHub's Linux machines only**: Windows is checked by building from Linux, not by a
  Windows runner. The core's tests under Wine are run by hand; the owner decided on 2026-11-05 that
  `scripts/check.sh` runs them, done with the CI's cache so that it stays within budget. Clippy for
  Windows is not run either, as the `windows` shell lacks it.
- **Links are tested on Linux only**: the tests that make symbolic links are `#[cfg(unix)]`, as
  making one on Windows needs a right users lack. Windows' refusal of reparse points planted by a
  game has no test yet, nor a game folder a person moved and linked back with a junction
  (`maintenance::a_game_folder_linked_from_elsewhere_is_uninstalled`), which Windows may refuse to
  uninstall. To be written with junctions in M3.3.
- **Some tests of shared behaviour run on Linux only**, as their setup uses file modes or links:
  `added::a_folder_too_wide_or_a_program_out_of_its_folder_is_refused`,
  `installer::tests::a_restart_that_cannot_delete_the_download_keeps_it`,
  `maintenance::a_game_whose_emptied_folder_cannot_be_removed_is_uninstalled`,
  `epic::download::an_unreadable_file_is_repaired`,
  `steam::local::what_steam_wrote_of_a_download_is_measured_in_its_folder`. To be made for Windows
  (a file held open, a lock) with the milestone of each area.
- **Windows gaps found while testing, not fixed**: `added::check_folder` reads `HOME`, which
  Windows does not set, so the profile folder is not refused there (M3.5);
  `platform::lowest_priority` may lower the calling thread only on Windows, not the threads
  `fsutil::in_background` work starts, unverified (M3.3); a save Windows marks read-only is backed
  up but not replaced, the sync reporting it (M3.3).
- **Files are not flushed to the disk on Windows**: on Linux the installers, and games added by
  hand, flush the whole filesystem once, after many files (`fsutil::sync_filesystem`); Windows has
  no such call for a user, and files smaller than `installer::SYNCED_FROM` are not flushed one by
  one. A power cut during an install or update on Windows could leave a file published but not
  written. To be settled in M3.3.

## Ideas

Not planned work: each says what it is and what it would gain. The owner decides which become
milestones.

- **A Windows runner in CI** for the core's tests, beside Wine: Wine let through no false failure
  but is not Windows (NTFS's file index, POSIX renames, junctions behave as Windows does only
  there).

Finished stages move to [`history.md`](history.md), in a few lines each, so that this file holds the
current stage only, under about 2,000 words.
