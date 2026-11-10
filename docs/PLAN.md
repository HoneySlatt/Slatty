# Plan

This file says what is being worked on now: the milestones of the current stage, their status and
how each one is checked. The known debt is in [`debt.md`](debt.md); direction beyond this stage is
in [`ROADMAP.md`](ROADMAP.md); decisions are in [`adr/`](adr/README.md). A *stage* is a step of the
roadmap; a *milestone* is a small, verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **Stage 3 — Windows** ([ADR-0022](adr/0022-windows-hosts.md)): a GOG game installs and plays on
  Windows from the command line ([M3.3](#m33--play-a-gog-game)), checked by the owner on Windows 11
  with one failure open. What Slatty depends on Linux for, and the port's open questions, are in
  [windows-port.md](windows-port.md).
- **Paused** ([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)) until uwui's plan opens
  Slatty's port, whole, in uwui's first stage ([ADR-0030](adr/0030-ported-second-onto-uwui.md),
  [ADR-0034](adr/0034-what-slatty-is-to-become.md)); until then Iced receives bug fixes only. What
  Slatty asks of uwui is in [uwui-needs.md](uwui-needs.md).
- **The bar, set against the market** on 2026-11-08
  ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)): Steam and Heroic measured on the
  moments of play, the lowest tier measured before 0.3, features placed in their stages
  ([ROADMAP.md](ROADMAP.md#quality-bar)). Waiting for the owner's go: Slatty's present build times
  measured as a floor, and the laptop's display, storage and driver read.
- **At the resumption**: Stage 5, the interface on uwui, released as 0.3 once its exit holds. Its
  first milestone, under Iced, moves the rules of `slatty-app`'s `update` into pure models with
  their tests; then the views on uwui, in signals over them, with French and accessibility at
  level AA. Read first [the known debt](#known-debt) and the open decisions of
  [ROADMAP.md](ROADMAP.md#open).
- **Then** Stage 6, the Steam Deck (0.4); Stage 8, saves everywhere (0.5); the rest of Stage 3,
  Windows (0.6), M3.3's open checks first; then Stages 4, 10, 9 and 11
  ([ROADMAP.md](ROADMAP.md#4-stages)).

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M3.1](history.md#m31--the-windows-build-environment) | Slatty can be built for Windows from Linux | Done 2026-10-28 |
| [M3.2](history.md#m32--everything-builds-for-windows) | Every crate builds for Windows, and the core's tests pass under Wine | Done 2026-11-05 |
| [M3.3](#m33--play-a-gog-game) | A person plays a GOG game on Windows from the command line | Checked on Windows; one failure open |
| [M5.0](history.md#m50--iceds-figures-measured-again) | Iced's figures measured again by a command, the reference for uwui's port | Done 2026-11-05 |
| [M3.4](#m34--epic-and-steam) | A person plays Epic's and Steam's games on Windows | After Stages 5, 6 and 8 |
| [M3.5](#m35--games-added-by-hand) | A person adds a game from no store on Windows | After Stages 5, 6 and 8 |
| [M3.6](#m36--achievements) | Achievements through Comet on Windows | After Stages 5, 6 and 8 |
| [M3.7](#m37--packaging) | A zip a person can download and run | After Stages 5, 6 and 8 |
| [M3.8](#m38--real-checks) | Checked by a person on Windows 10 and 11, and documented | After Stages 5, 6 and 8 |

## Stage 3 — Windows

Slatty on Windows 10 and 11 (x86-64), from the same code base, built and tested from Linux; Windows
games played directly, without Proton; what only Linux has stays Linux's
([ADR-0022](adr/0022-windows-hosts.md)). Up to M3.3 from the command line; M3.4 to M3.8 come after
Stage 5, which brings the window on uwui, and Stages 6 and 8, and are planned again then, with what
Stage 8 adds that needs no Linux ([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0034](adr/0034-what-slatty-is-to-become.md)). Linux behaves exactly as before at
every milestone.

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
- **Where it stands.** Open: a launch right after `slatty cloud sync` downloaded all 10 of Hollow
  Knight's saves, which that sync had found unchanged. Slatty logs no sync: the backups it made
  then, kept in the virtual machine under `%APPDATA%\slatty\data\backups`, are what can show why.
  Not checked: play time sent (the session lasted under a minute, which is not sent), a session
  across the computer's sleep, and whether [the fixes made since](history.md#m33--play-a-gog-game)
  work on Windows itself. These wait for the window (Stage 5). Comet stays off on Windows (M3.6).

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
  [ROADMAP.md](ROADMAP.md#stage-3--windows--06) holds.

The stage closes with a stage audit, every dimension of the project's method, before 0.6 is
released: mutation testing (`cargo mutants`) first on cloud sync, installs and uninstalls
([ADR-0024](adr/0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md)). Audits are recorded in
[audits/](audits/).

## Known debt

What is known to fall short, and where each item is to be paid, is in
[debt.md](debt.md).

## Ideas

Not planned work: each says what it is and what it would gain. The owner decides which become
milestones.

- **A Windows runner in CI** for the core's tests, beside Wine: Wine let through no false failure
  but is not Windows (NTFS's file index, POSIX renames, junctions behave as Windows does only
  there).

A finished milestone moves to [`history.md`](history.md), leaving its row in the status table; a
finished stage moves there in a few lines. This file holds the current stage only, within 2,000 words
(`scripts/check.sh` refuses more).
