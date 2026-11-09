# History

The stages finished, newest first, and, for the current stage, the milestones finished: what each
delivered and found, its release where it had one, and the ADRs that hold its decisions. The
current work is in [`PLAN.md`](PLAN.md); what changed for users is in
[`CHANGELOG.md`](../CHANGELOG.md).

## The pause, from 2026-11-05

On 2026-11-05, with M3.2 done, the owner paused Slatty after M3.3 until its port onto uwui
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0030](adr/0030-ported-second-onto-uwui.md)). CI's weekly run goes on. On 2026-11-06 the owner
chose to run M3.3's checks left open on Windows first in Stage 3's rest, with the window that
Stage 5 brings.

GOG's announcement of a GOG Galaxy for Linux led SlimeEngine's ADR-0203 (2026-11-06) to drop
Slatty's port; the owner changed their mind the next day: Slatty is ported whole in uwui's first
stage, with Opium and Pluto's client
([SlimeEngine's ADR-0206](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0206-slatty-ported-again-the-daw-later.md)).
No stop was ever recorded in the PLAN. On 2026-11-07, in an interview that built nothing, the owner
decided what Slatty is to become ([ADR-0034](adr/0034-what-slatty-is-to-become.md), which tells the
story): a showcase of uwui, for the general public; the couch mode and the Steam Deck, saves on
every device, mods; a new order of stages ([ROADMAP.md](ROADMAP.md#4-stages)). What it asks of uwui
is in [uwui-needs.md](uwui-needs.md), for ADR-0206's check of uwui's signals; the owner carries it
there.

One milestone ran during the pause
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)):

### M5.0 — Iced's figures measured again

**Goal.** Iced's figures taken again by one command, on the owner's machine as it is now (Umbriel),
as the reference that uwui's port must match
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)).

**Delivered.** A generator of the fake profile of 10,000 games
([testing.md](testing.md#the-application-at-10000-games)), its covers made with `image` (already in
`slatty-app`), 1.1 GB, in a throwaway folder it refuses if not empty. The probe in `slatty-gui`,
behind a feature off by default, built by `scripts/check.sh`'s lints so that it keeps compiling
during the pause; it writes a log whose format does not name Iced, so that uwui's port writes the
same. One command, `slatty-measure` (`crates/measure`), that sets the isolated profile itself (its
XDG folders, no keyring, no network), runs the measurements and reads the log into figures with
their spread; it does not depend on the framework, so that it measures the port too. A headless
compositor, sway, with Xwayland in the development shell (`flake.nix`), testing.md describing it,
and the figures in [measurements.md](measurements.md), with the commit.

**Measured**, release build, 10 runs where it repeats: the library shown after start, warm and cold;
scrolling the grid by the probe over 1,200 frames (the time between frames: median, p99, max, frames
over 4.17, 6.25 and 16.7 ms; and `view`'s time); at rest for 60 s (frames drawn, CPU ticks, wake-ups
of every thread, under Wayland and Xwayland); resident memory after start, with every cover in,
after scrolling the whole grid, and after 200 game pages with key art; video memory at the same
points, from the card's `fdinfo` (`drm-memory-vram`), measured first as the owner decided; and the
GPU's power at rest against the desktop without Slatty.

**Where.** Start, rest and memory in the headless compositor; scrolling at 240 Hz and power on the
owner's display, with their agreement. The probe scrolls by itself: no input enters the session.

**Done when.** The command ran twice and the second run's medians fall within the first run's range;
measurements.md holds the figures; the debt on figures and video memory is closed or restated.

**Not measured.** Input to visible response, which Iced does not report; the lowest tier, which no
machine stands for.

**Where it stands.** Done 2026-11-05: each command ran twice, 10 runs a series, and the figures are
in [measurements.md](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05).
Every second series' median falls within the first's range but three: video memory after the game
pages, and on the desktop the first frame and every cover shown, the wait for the first frame having
varied; the owner accepted them. Measuring found the start-up wait, the stalls of scrolling, and
memory over its budgets, written in [the known debt](debt.md); Iced receiving bug fixes
only, none was fixed here. The owner then set video memory's budget (≤ 256 MB), the start-up budget
measured at the first frame shown, and wake-ups counted voluntary and not
([quality bar](ROADMAP.md#quality-bar)).

## Stage 3 — Windows, so far

The milestones of Windows' port done under Stage 3
([ADR-0022](adr/0022-windows-hosts.md)); the stage's goal, what stays open and the rest of its
milestones are in [PLAN.md](PLAN.md#stage-3--windows).

### M3.1 — The Windows build environment

**Goal.** Slatty can be built for Windows from Linux. **Delivered:** the `windows` development shell
in `flake.nix` (MinGW-w64, Wine as the test runner) and [windows-port.md](windows-port.md). **Done
2026-10-28**, when `nix develop .#windows -c cargo check` ran for the Windows target.

### M3.2 — Everything builds for Windows

**Goal.** Every crate builds for Windows, and the core's tests that do not need Linux pass under
Wine. **Delivered:** the core's `platform` module gathering what differs by system (processes,
priority, private folders, opening without following links, known folders, finding programs), and
the session supervisor's Windows version (a job object); keeping the Linux-only modules on Linux
moved to M3.3 (the owner's decision, 2026-11-05). **Done 2026-11-05:** `slatty`, `slatty-gui` and
the core build for Windows, every crate passing `cargo check` for Windows, and the core's 421 tests
that do not need Linux pass under Wine. The commands:
`nix develop .#windows -c cargo check --workspace --locked` and
`nix develop .#windows -c cargo test -p slatty-core` ([testing.md](testing.md#the-windows-build)).
Running them found four bugs, fixed: no cloud save was ever downloaded, and no install or update
finished, on Windows ([windows-port.md](windows-port.md#what-windows-does-differently)); a fifth,
which the first fix brought to Linux's read-only saves, was fixed after the closing review. Comet
and the games Steam runs are not available on Windows yet.

### M3.3 — Play a GOG game

**Built 2026-11-05 and checked by the owner on Windows 11 the same day**
([compatibility.md](compatibility.md#windows)): install, pause and resume, the setup and its
administrator prompt, a session quit and stopped, cloud saves, verify, repair and uninstall, and the
refusals work. **Fixed since, not checked again on Windows:** `doctor` failing on Comet's port, held
by GOG Galaxy; `setup` saying it was done after a step failed; `verify` succeeding with files
damaged; exit codes shown as `Some(2)`; a last progress line of 0 files of 0. **Under Wine**, the
core's 428 tests and the command line's pass, `play_windows.rs` playing a fake GOG game through the
real supervisor (started, followed past its launcher, stopped, its setup waiting for consent, Proton
refused). **The owner decided three questions:** Linux's features refused where they start
([ADR-0027](adr/0027-linux-features-refused-on-windows-not-compiled-out.md)), GOG's setup asked for
at launch ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)), every installed file
flushed ([ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)). **Found and fixed on the
way:** play time counted the computer's sleep on Windows; uninstalling a folder linked back with a
junction left the junction; a read-only save, or a read-only file of the game, could not be replaced
or deleted; the default games folder was relative to where Slatty started, as Windows sets no
`HOME`. What remained open after the check, and what was not checked, are in
[M3.3](PLAN.md#m33--play-a-gog-game).

## Stage 2 — Beyond GOG → 0.2.0 (2026-10-27)

One library for every game a person owns: Steam's and Epic's games, games added by hand, emulated
games, the list of games beside the library, and the couch mode for a gamepad. Not yet tried on a
Steam Deck (Stage 6), nor emulated games in each emulator ([debt.md](debt.md)).
Decisions: [ADR-0013](adr/0013-steam-read-from-its-files-played-through-its-client.md),
[ADR-0014](adr/0014-gog-never-told-of-other-stores-games.md),
[ADR-0018](adr/0018-a-couch-mode-of-its-own.md).

## Stage 1 — GOG on Linux → 0.1.0 (2026-10-21)

Sign-in, library, installs of Windows and Linux builds, updates, isolation, cloud saves and
achievements, from the command line and the interface, checked against GOG in
[compatibility.md](compatibility.md). Decisions: [ADR-0001](adr/0001-rust-with-proton-umu-and-comet-outside.md)
to [ADR-0012](adr/0012-cloud-saves-decided-by-content-hashes.md), and the interface's
[ADR-0015](adr/0015-iced-for-the-interface.md) to [ADR-0017](adr/0017-only-the-rows-in-view-are-built.md).
