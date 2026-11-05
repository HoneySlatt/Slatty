# Roadmap

> Living document, set by the owner on 2026-11-04. It defines direction, ordering and exit criteria.
> It is not a task list: the current plan lives in [PLAN.md](PLAN.md), and decisions live in
> [adr/](adr/README.md). Stages will be merged, reduced, reordered or dropped as the project evolves;
> each such change is recorded in an ADR.

## 1. Vision

Slatty is a native launcher for the games a person owns on GOG, and beside them their Steam and Epic
games, the games they added by hand and their emulated games, in one library. It installs, updates,
runs, isolates and saves those games so that a person never has to think about Wine, Proton or
where a save went, and never loses one.

It handles people's accounts, saves and disks: **safety comes before features**. Excellent means a
library of 10,000 games that scrolls at every frame of a fast display, that costs nothing while it
waits, that keeps every save, that tells no company more than the person allowed, and that is as
easy to use from a sofa with a gamepad, or on a Steam Deck, as at a desk.

It runs on Linux today. Its interface moves to uwui (formerly slime-ui), the owner's interface
framework, built in SlimeEngine. Slatty is the second of four programs ported onto it before its API
is declared stable ([Stage 5](#stage-5--the-interface-on-uwui-windows-included--03)). Every piece
of interface work waits for that move. Slatty pauses once a GOG game plays on Windows from the
command line, until its turn comes; then its interface moves first, Windows and packages after
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0030](adr/0030-ported-second-onto-uwui.md)).

## 2. Principles

1. **Safety before features.** A destructive action refuses when something looks wrong, or keeps a
   copy first; nothing is written through a link out of a game folder; saves are never lost
   ([AGENTS.md](../AGENTS.md#files-and-data-safety)).
2. **Honest about GOG and the others.** Nothing is called verified before a check against the real
   service, recorded in [compatibility.md](compatibility.md).
3. **Private by default.** Every third party is off until turned on; no telemetry
   ([ADR-0005](adr/0005-every-third-party-off-until-turned-on.md)).
4. **Measured, at 10,000 games.** Speed, memory and rest are measured in the release build on a
   fake library of 10,000 games ([testing.md](testing.md)); a change that costs them is measured
   before and after.
5. **The core holds the behaviour.** The command line and the interface share `slatty-core`; the
   interface's state names no framework ([ADR-0019](adr/0019-the-interfaces-state-free-of-any-framework.md)).

## Quality bar

Set by the owner on 2026-11-05, from the figures measured with Iced.

References:

- **Steam**: its library and its Game Mode, the bar for the couch mode and the Steam Deck.
- **GOG Galaxy 2.0**: one library for every store.
- **Playnite**: libraries of thousands of games, many sources, speed.
- **Heroic**: GOG and Epic on Linux, the closest in purpose.
- For the interface on uwui (Stage 5): the most refined current interfaces, in motion,
  materials, type and responsiveness.

Hardware: aiming at the top never narrows who can run Slatty. On the lowest tier it holds its frame
rate, answers at once and every feature works, at 10,000 games; only detail is lost (blur, shadows,
rich animation). A tier, or a GPU maker, never measured is said so, never assumed. Tiers set by the
owner on 2026-11-05 ([ADR-0024](adr/0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md)).
The lowest is the same class of machine as uwui's lower desktop tier for applications (the same
GPU, cores, memory and display); Slatty's adds a hard disk
([ADR-0030](adr/0030-ported-second-onto-uwui.md)):

| Tier | Reference machine | Display | Measured |
|---|---|---|---|
| Lowest | A 2016 laptop: Core i5-6200U (2 cores, 4 threads), Intel HD 520, 4 GB of memory, a 5,400 rpm hard disk; Linux and Windows 10. Chosen as the lowest GPU with Vulkan on both systems | 1920×1080 at 60 Hz, scale 1 | No |
| Middle | Steam Deck (Stage 6): Zen 2, 4 cores; RDNA 2; 16 GB; its own storage | 1280×800 at 60 Hz | No; its budgets are set at Stage 6, the lowest tier's apply until then |
| High | Ryzen 7 5800X, Radeon RX 6900 XT, NVMe | 3840×2160 at 240 Hz, scale 1.5 | Yes, the budgets below |

GPU makers: AMD measured; Intel and NVIDIA not.

Budgets on the high tier, with the fake library of 10,000 games
([testing.md](testing.md#the-application-at-10000-games)), release build. The figures measured are
in [measurements.md](measurements.md).

| Budget | Target | Measured by | Checked in |
|---|---|---|---|
| Library shown after start | ≤ 95 ms warm, ≤ 135 ms cold | The probe of testing.md | By hand, when start-up changes |
| Scrolling the grid | No frame missed at 240 Hz: p99 of the time between frames ≤ 5 ms, none above 6.25 ms (1.5 refresh intervals) | The probe, 1,200 frames | By hand, when the library or the grid changes |
| `view` at 10,000 games | p99 ≤ 0.5 ms, library and Achievements | `library_at_10000_games` | By hand; in CI from Stage 4 |
| Lists made again | ≤ 2.5 ms on the desktop, ≤ 3.5 ms in the couch mode | `library_at_10000_games` | As above |
| A couch mode move | Shown in the next frame: message and view ≤ 0.1 ms | `library_at_10000_games` | As above |
| At rest | No frame drawn, 0 CPU ticks, 0 wake-ups of Slatty's own in 60 s | `top`, `voluntary_ctxt_switches` of every thread | By hand, at each milestone that touches the interface or a background task |
| Memory | ≤ 140 MB resident with every cover in; ≤ 200 MB after 200 game pages with key art | `VmRSS` in `/proc/<pid>/status` | By hand, when images change |
| GPU power at rest | No more than the desktop without Slatty | `power1_average` of the card | By hand, once per stage |
| Command line start-up | `slatty library list` at 10,000 games ≤ 10 ms warm | `hyperfine` | By hand, when the command line changes |
| Verifying a file | 1 GB in ≤ 0.45 s on four threads | `verifying_a_large_file` | By hand, when verification changes |
| Bad input | Never a crash on a file or answer from outside (VDF, `appinfo.vdf`, GOG manifests, installers, archives) | Tests of hostile input; `cargo fuzz` from Stage 4 | `scripts/check.sh` (tests); fuzzing by hand and briefly in CI from Stage 4 |
| Downloading a game (set 2026-11-05, not measured yet) | A GOG or Epic install takes ≥ 90 % of the rate of one plain download from the same server, with at most one core of CPU on average, unpacking and verifying included | A fake local server at a fixed rate; no account, no real download | By hand, when downloads change |
| A cloud sync with nothing changed (set 2026-11-05, not measured yet) | Sends nothing, and ends in ≤ 50 ms off the network for 100 saves: it runs before each game | The tests' fake cloud | By hand, when the sync changes |

Iteration, set by the owner on 2026-11-05: a CI run ≤ 20 min; `scripts/check.sh` ≤ 10 min warm on
the high tier; `cargo test -p slatty-core` ≤ 1 min. Measured from GitHub's run times and `time`;
checked at each change to CI or to the checks.

Crashes, set by the owner on 2026-11-05: a crash writes a report in the logs folder (version,
system, message, stack trace; never a token or the sign-in address), which the person may attach to
an issue; nothing leaves the machine. The command line prints its path; the interface tells it at
the next start. Checked by a test that makes a crash and looks for secrets in its report.

Budgets on the lowest tier, with the same library and build. None is measured yet: no such machine
has run Slatty.

| Budget | Target |
|---|---|
| Library shown after start | ≤ 250 ms warm, ≤ 1.5 s cold (hard disk) |
| Scrolling the grid | No frame missed at 60 Hz: p99 of the time between frames ≤ 16.7 ms, none above 25 ms |
| `view` at 10,000 games | p99 ≤ 2 ms, library and Achievements |
| Lists made again | ≤ 8 ms |
| A couch mode move | Shown in the next frame |
| At rest, memory | As on the high tier |
| Command line start-up | `slatty library list` at 10,000 games ≤ 30 ms warm |
| Verifying a file | Bound by the disk: at most 10 % over reading the file alone |

Left for the interface on uwui (Stage 5), since Iced receives bug fixes only: input to visible
response within one frame, animations at the display's rate judged at the 99th percentile,
rendering checked against reference images at scales 1 and 2, screen readers, full keyboard use,
reduced motion and contrast.

## 3. Decisions

### Fixed

| Topic | Decision |
|---|---|
| Language | Rust for the whole application; Proton, umu and Comet stay external tools ([ADR-0001](adr/0001-rust-with-proton-umu-and-comet-outside.md)) |
| Licence | GPL-3.0-or-later ([ADR-0002](adr/0002-gpl-3-0-or-later.md)) |
| Storage | SQLite through rusqlite; migrations only appended ([ADR-0003](adr/0003-sqlite-through-rusqlite-behind-a-mutex.md)) |
| Secrets and privacy | Tokens only in the system keyring; every third party off until turned on; no telemetry ([ADR-0004](adr/0004-tokens-only-in-the-system-keyring.md), [ADR-0005](adr/0005-every-third-party-off-until-turned-on.md)) |
| Running Windows games on Linux | umu and Proton builds Slatty downloads; isolation through umu's container ([ADR-0009](adr/0009-isolation-through-umus-container.md), [ADR-0010](adr/0010-proton-builds-downloaded-by-slatty.md)) |
| Other stores | Steam read from its files and played through its client; GOG never told of other stores' games ([ADR-0013](adr/0013-steam-read-from-its-files-played-through-its-client.md), [ADR-0014](adr/0014-gog-never-told-of-other-stores-games.md)) |
| Interface | Iced, with bug fixes only, until uwui runs Slatty; every piece of interface work waits for uwui; the state free of any framework ([ADR-0015](adr/0015-iced-for-the-interface.md), [ADR-0019](adr/0019-the-interfaces-state-free-of-any-framework.md), [ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md), [ADR-0030](adr/0030-ported-second-onto-uwui.md)) |
| Platforms | Linux (Wayland and X11, x86-64); Windows 10 and 11 from one code base, built from Linux, its window coming with uwui ([ADR-0022](adr/0022-windows-hosts.md)); no macOS; unchanged by uwui's other targets ([ADR-0030](adr/0030-ported-second-onto-uwui.md)) |

### Open

Each is decided by an ADR at the stage where it becomes necessary, not earlier.

| Topic | Candidates | Decide at |
|---|---|---|
| Windows toolchain | MinGW (now), Microsoft's MSVC | Stage 3, if the MinGW build shows a problem MSVC has not |
| Packages for other distributions | Flatpak, AppImage, distribution packages | Stage 4 |
| Slatty on a Steam Deck | The Nix package, a Flatpak, an AppImage | Stage 6 |
| The theme file people already have, on uwui | Kept as Slatty's format, migrated to uwui's themes | Stage 5 |
| The couch mode's gamepads and focus | Slatty's own (gilrs, `slatty-app`'s `pad`), uwui's | Stage 5 |
| Translations | Fluent, gettext, a format of Slatty's own | Stage 7 |

## 4. Stages

The stages are listed by number but run in this order: Stages 1 and 2; Stage 3 up to M3.3; a
pause; Stage 5; the rest of Stage 3; Stages 4, 6 and 7
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)).

The pause lasts until Slatty's turn in SlimeEngine's Stage 6d
([ADR-0030](adr/0030-ported-second-onto-uwui.md)). Only M3.3's check on Windows and M5.0, Iced's
figures measured again, run meanwhile
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)).

### Stage 1 — GOG on Linux → 0.1.0 (done, 2026-10-21)

**Goal.** Sign in to GOG, see the library, install, update, run, isolate and sync the saves of GOG's
games on Linux, from the command line and the interface.

**Exit.** Met: the features of 0.1.0 in [CHANGELOG.md](../CHANGELOG.md), checked against GOG in
[compatibility.md](compatibility.md).

### Stage 2 — Beyond GOG → 0.2.0 (done, 2026-10-27)

**Goal.** One library for every game a person owns: Steam's, Epic's, games added by hand, emulated
games, and a couch mode for a gamepad.

**Exit.** Met: the features of 0.2.0 in [CHANGELOG.md](../CHANGELOG.md). The couch mode is not yet
tried on a Steam Deck (Stage 6), nor emulated games played in each emulator
([PLAN.md](PLAN.md#known-debt)).

### Stage 3 — Windows → 0.4

**Goal.** Slatty on Windows 10 and 11: GOG's, Epic's and Steam's games and games added by hand,
played directly, with the same safety as on Linux ([ADR-0022](adr/0022-windows-hosts.md)). Up to
M3.3 from the command line; the rest after Stage 5, which brings the window, planned again then
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md)).

- Everything builds for Windows; the core's tests pass under Wine.
- GOG games installed, played, followed and synced on Windows; Epic and Steam; games added by hand;
  achievements through Comet.
- A zip with the command line and the licences.

**Exit.** A person installed, played and synced a GOG game, an Epic game and a Steam game on a real
Windows 10 or 11, recorded in [compatibility.md](compatibility.md); Linux
unchanged.

### Stage 4 — Packages and smaller work, outside the interface → 0.5

**Goal.** Slatty installable outside NixOS, and the core's smaller items waiting:

- packages for other distributions;
- Steam's downloads followed more exactly while the client downloads;
- Wine without Proton for the games Slatty installs (it exists for imported games);
- nothing that wakes at rest: the keyring's connection open only while a secret is read or
  written ([quality bar](#quality-bar));
- the parsers of outside data (Steam's VDF and `appinfo.vdf`, GOG's manifests, Linux installers,
  archives) fuzzed with `cargo fuzz`, from a nightly toolchain in the development shell, decided
  in an ADR; with the same toolchain, Miri run on the tests of `unsafe` code it can run, those it
  cannot listed with the reason (the owner's decision, 2026-11-05);
- the budgets `library_at_10000_games` measures checked by `scripts/check.sh` in CI, with limits
  for CI's machines written in the quality bar.

**Exit.** Slatty installed and run from a package on a distribution other than NixOS, recorded in
[compatibility.md](compatibility.md).

### Stage 5 — The interface on uwui, Windows included → 0.3

**Goal.** Slatty's interface drawn by uwui, on Linux and on Windows, as the second of the four
programs that SlimeEngine's Stage 6d ports before uwui's API is declared stable
([ADR-0030](adr/0030-ported-second-onto-uwui.md)). Starts when that stage's plan opens Slatty's
port, after the music player's port on the desktop. 0.3 is released for Linux once the exit
holds, on a pinned revision of uwui, without waiting for the API to be declared stable;
Windows' first release is Stage 3's, 0.4.

- Every page and the couch mode on uwui, on its desktop host, over the same state; the interface's
  tests ported. `slatty-core` and `slatty-app` never depend on uwui.
- The interface on Windows: Linux-only settings hidden, the file chooser, gamepads, a window icon,
  paths shown the Windows way.
- A checkpoint where the owner uses it day to day.
- Iced removed.

**Exit.** Slatty on uwui at least as good as on Iced on the library shown, scrolling, rest under
Wayland and Xwayland, and memory, both measured side by side by M5.0's command
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)), and within the
[quality bar](#quality-bar); every page at parity; the window checked by a person on Windows; Iced
gone from `Cargo.lock`.

### Stage 6 — On a Steam Deck

**Goal.** The couch mode on uwui as the Steam Deck's Game Mode needs it, checked on a real Deck.

- Slatty installed on SteamOS, started from Steam and by itself in Game Mode.
- What the checks find fixed: scale, gamepad, full screen, suspending and resuming.

**Exit.** A person browsed, installed, played and synced a game on a real Steam Deck in Game Mode,
recorded in [compatibility.md](compatibility.md).

### Stage 7 — Translations

**Goal.** The interface in the languages people ask for, beginning with French.

**Exit.** The interface wholly translated into one language besides English, and a way for others
to add one.

### Out of scope

Store purchases, friends and chat, multiplayer services, an in-game overlay, mods, plugins. Slatty
has no friends or chat feature; Comet, which it starts for achievements once turned on, still
subscribes to those topics on GOG for the account ([SECURITY.md](../SECURITY.md#comet)).

## 5. Release map

| Version | Stage | Content |
|---|---|---|
| 0.1.0 | 1 | GOG on Linux (released 2026-10-21) |
| 0.2.0 | 2 | Beyond GOG (released 2026-10-27) |
| 0.3 | 5 | The interface on uwui, Windows' window included |
| 0.4 | 3 | Windows |
| 0.5 | 4 | Packages and smaller work |
| 0.6 | 6 | On a Steam Deck |
| 0.7 | 7 | Translations |
