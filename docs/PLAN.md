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
  private folders, opening without following links, known folders, finding programs); Linux-only
  modules
  (Proton, umu, isolation, Linux installers, Proton builds, the Galaxy service in a prefix) built on
  Linux alone, their callers on Windows getting `Error::Unsupported`; the session supervisor's
  Windows version (a job object).
- **Done when.** `slatty`, `slatty-gui` and the core build for Windows, and the core's tests that do
  not need Linux pass under Wine.
- **How to test.** `nix develop .#windows -c cargo check --workspace --locked`;
  `nix develop .#windows -c cargo test -p slatty-core` ([testing.md](testing.md#the-windows-build)).
- **Where it stands.** Done 2026-11-05: every crate passes `cargo check` for Windows, and the
  core's 420 tests that do not need Linux pass under Wine. Running them found four bugs, fixed:
  no cloud save was ever downloaded, and no install or update finished, on Windows
  ([windows-port.md](windows-port.md#what-windows-does-differently)). Comet and the games Steam
  runs are not available on Windows yet.

### M3.3 — Play a GOG game

- **Goal.** A person installs and plays a GOG game on Windows, from the command line.
- **Delivers.** A Windows build installed (the download code is shared), started directly, its
  session followed and stopped through a job object, play time recorded, GOG's setup run with the
  person's consent for administrator rights, cloud saves in the real folders.
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

## Known debt

- **Iced's and zbus's unmaintained crates** (`rustybuzz`, `ttf-parser`, `zbus_names`, `zvariant`,
  `zvariant_derive`) are ignored in `deny.toml`, each with its reason; they go with Iced
  ([ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md)) and with zbus 6.
- **Not checked for real**: the couch mode on a Steam Deck (Stage 6) and on a television; emulated
  games in each emulator; the rest is listed in [compatibility.md](compatibility.md).
- **The performance figures are not reproducible by a command**: [testing.md](testing.md) measures
  start-up, scrolling and memory with a temporary probe that is not in the repository, and the rows
  of [measurements.md](measurements.md) on covers predate the copies made at the size drawn.
- **Wake-ups at rest**: one thread of zbus's pool, kept by the keyring's connection, wakes every
  500 ms (120 wake-ups a minute, against a budget of 0 in the
  [quality bar](ROADMAP.md#quality-bar)). To be fixed in Stage 4.
- **CI runs on GitHub's Linux machines only**: Windows is checked by building from Linux, not by a
  Windows runner. The core's tests under Wine are run by hand, not by `scripts/check.sh`; nor is
  Clippy for Windows, which the `windows` shell lacks.
- **Links planted by a game are tested on Linux only**: the tests that plant symbolic links are
  `#[cfg(unix)]`, as making one on Windows needs a right users lack. Windows' refusal of reparse
  points (junctions, links) has no test yet; to be written with junctions in M3.3.
- **Installs are not flushed to the disk on Windows**: on Linux the installers flush the whole
  filesystem once, after many files (`fsutil::sync_filesystem`); Windows has no such call for a
  user, and files smaller than `installer::SYNCED_FROM` are not flushed one by one. A power cut during an install or update on
  Windows could leave a file published but not written. To be settled in M3.3.

Finished stages move to [`history.md`](history.md), in a few lines each, so that this file holds the
current stage only, under about 2,000 words.
