# Roadmap

> Living document, set by the owner on 2026-11-04; its direction set on 2026-11-07
> ([ADR-0034](adr/0034-what-slatty-is-to-become.md)), its bar against the market on 2026-11-08
> ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)). It defines direction, ordering and exit
> criteria. It is not a task list: the current plan lives in [PLAN.md](PLAN.md), and decisions live in
> [adr/](adr/README.md). Stages will be merged, reduced, reordered or dropped as the project evolves;
> each such change is recorded in an ADR.

## 1. Vision

Slatty is a native launcher for the games a person owns on GOG, and beside them their Steam and Epic
games, the games they added by hand and their emulated games, in one library. It installs, updates,
runs, isolates, mods and saves those games so that a person never has to think about Wine, Proton
or where a save went, and never loses one. It is for the general public, made first for its owner
([ADR-0034](adr/0034-what-slatty-is-to-become.md)).

It handles people's accounts, saves and disks: **safety comes before features**. Excellent means a
library of 10,000 games that scrolls at every frame of a fast display, that costs nothing while it
waits, that keeps every save, that tells no company more than the person allowed, and that is as
easy to use from a sofa with a gamepad, or on a Steam Deck, as at a desk.

It is first a showcase of uwui, the owner's interface framework: its couch mode, its motion and
materials, its speed at 10,000 games. A person opens it beside Steam for four reasons: all their
games in one place, the couch mode and the Steam Deck, saves that follow between the computer and
the Deck, and Windows games isolated. No other launcher does the last three.

It runs on Linux today. Its interface moves to uwui, built in SlimeEngine and in its own
repository: Slatty is one of the programs of uwui's first stage, ported whole
([Stage 5](#stage-5--the-interface-on-uwui-windows-included--03)). Every piece of interface work
waits for that move. Slatty pauses until its turn comes; then its interface moves first, the Steam
Deck and saves everywhere next, Windows after
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0030](adr/0030-ported-second-onto-uwui.md), [ADR-0034](adr/0034-what-slatty-is-to-become.md)).

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

Set by the owner on 2026-11-05 from the figures measured with Iced, then against the market on
2026-11-08 ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)). Each figure missed is debt
until met ([debt.md](debt.md)).

Rivals:

- **The interface** is judged by uwui's screen of reference "Slatty's library", against Steam and
  the native gThumb and Nautilus, with uwui's lead of three times on its *own cost*
  ([glossary](glossary.md)) ([uwui's quality bar](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md#2-quality-bar)).
- **The moments of play** are measured against **Steam** and **Heroic**, and GOG Galaxy for Linux
  once it ships: Play to the game's first frame, installs and updates end to end, download
  throughput, Slatty's cost while a game runs, the couch mode's input to photon against Steam's
  Game Mode. Each measure on the owner's accounts waits for the owner's consent.
- **Features** are compared with Steam, GOG Galaxy 2, Playnite, Heroic, Lutris, Bottles, ES-DE and
  LaunchBox, without measures.
- **The look**: the most refined current interfaces, in motion, materials, type and
  responsiveness; judged blind at Stage 5's exit.

The lead, by family. Each launcher's own cost is what it adds above the *bare tool*: the same game
started by `umu-run` on the same Proton, or one plain download. On launching and while a game runs,
Slatty's own cost is at most a third of the best rival's, at equal features: isolation, which the
rivals lack, has a budget of its own. On downloads and installs, Slatty does at least as well as the
best rival and reaches 90 % of a plain download. Beating each line is the least. The rivals'
figures are not measured yet; each ratio becomes a figure when they are.

Hardware: aiming at the top never narrows who can run Slatty. On the lowest tier it holds its frame
rate, answers at once and every feature works, at 10,000 games; only detail is lost (blur, shadows,
rich animation). A tier, or a GPU maker, never measured is said so, never assumed. Tiers set by the
owner on 2026-11-05 ([ADR-0024](adr/0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md)).
The lowest is the owner's laptop, the same machine as uwui's and Opium's lowest desktop
([ADR-0034](adr/0034-what-slatty-is-to-become.md)):

| Tier | Reference machine | Display | Measured |
|---|---|---|---|
| Lowest | The owner's laptop: AMD Athlon Silver 3050U (2 cores, 2 threads), its Radeon Vega, 4 GB of memory in one channel shared with the GPU, about 3.25 GB left to programs; Linux. Its storage and Vulkan driver are not yet read; the cold start's budget on a hard disk is measured once a hard disk is at hand; Windows on this tier is not measured | Assumed 1920×1080 at 60 Hz, scale 1; read before its figures are set | No; measured and met as Stage 5's exit |
| Middle | The owner's Steam Deck OLED (Stage 6): Zen 2, 4 cores; RDNA 2; 16 GB; its own storage | 1280×800 at 90 Hz, HDR | No; its budgets are set at Stage 6, the lowest tier's apply until then |
| High | Ryzen 7 5800X, Radeon RX 6900 XT, NVMe | 3840×2160 at 240 Hz, scale 1.5 | Yes, the budgets below |

GPU makers: AMD measured; Intel and NVIDIA not, until one is at hand. On a GPU that shares main
memory, video memory counts what it borrows from it (GTT) as well as `drm-memory-vram`.

Budgets on the high tier, with the fake library of 10,000 games
([testing.md](testing.md#the-application-at-10000-games)), release build. The figures measured are
in [measurements.md](measurements.md).

| Budget | Target | Measured by | Checked in |
|---|---|---|---|
| Library shown after start | The first frame drawn with the library in (the probe's `shown`, the compositor's wait included; the owner's choice, 2026-11-05) ≤ 95 ms warm, ≤ 135 ms cold | `slatty-measure` ([testing.md](testing.md#the-application-at-10000-games)) | By hand, when start-up changes |
| Scrolling the grid | No frame missed at 240 Hz: p99 of the time between frames ≤ 5 ms, none above 6.25 ms (1.5 refresh intervals) | `slatty-measure desktop`, 1,200 frames | By hand, when the library or the grid changes |
| Everything that moves, on uwui (Stage 5) | No refresh missed: scrolling, pages, drawers, the cover that flies to its page, the couch mode's focus; counted from the compositor's presentation feedback, variable refresh off | The port's probe | By hand, when motion changes |
| Input to photon, on uwui (Stage 5) | Equal to uwui's blank window at the 99th percentile, within a tolerance and a sample count set at the first measure; desktop and couch mode | The light sensor's bench (SlimeEngine's Stage 6e) | By hand, once per stage |
| Launching a game (set 2026-11-08, not measured yet) | Slatty's own cost above `umu-run` on the same game and Proton, isolation apart, at most a third of the best of Steam and Heroic; isolation within a budget of its own, its runtime copied once per version | Launches timed from outside, to the game's first frame | By hand, when launches change |
| While a game runs (set 2026-11-08) | A blank window's video memory, a few tens of MB resident, no wake-up but the session's events; at most a third of the best rival's own cost | `slatty-measure`, with a fake game | By hand, when sessions change |
| Back from a game (set 2026-11-08) | Slatty shown again within 100 ms of the game's end, the library in place and its first screen's covers shown | As above | As above |
| Download controls (set 2026-11-08) | A speed cap held within 5 % over a stated window; Slatty says why each waiting download waits | A fake local server | By hand, when downloads change |
| At 100,000 games (set 2026-11-08) | Lists made again, start and memory: figures set at Stage 5 by measure, on a fake profile of 100,000 games | `library_at_10000_games` and `slatty-measure`, on that profile | By hand, at Stage 5 |
| `view` at 10,000 games | p99 ≤ 0.5 ms, library and Achievements | `library_at_10000_games` | By hand; in CI from Stage 4 |
| Lists made again | ≤ 2.5 ms on the desktop, ≤ 3.5 ms in the couch mode | `library_at_10000_games` | As above |
| A couch mode move | Shown in the next frame: message and view ≤ 0.1 ms | `library_at_10000_games` | As above |
| At rest | No frame drawn, 0 CPU ticks, 0 wake-ups of Slatty's own in 60 s | `slatty-measure headless`: the probe's frames, `/proc/<pid>/stat`, context switches of every thread, voluntary and not (set 2026-11-05) | By hand, at each milestone that touches the interface or a background task |
| Memory | ≤ 140 MB resident with every cover in; ≤ 200 MB after 200 game pages with key art | `slatty-measure headless`: `VmRSS` in `/proc/<pid>/status` | By hand, when images change |
| Video memory (set 2026-11-05) | ≤ 256 MB with every cover in, after scrolling the whole grid, and after 200 game pages with key art | `slatty-measure headless`: `drm-memory-vram` in `/proc/<pid>/fdinfo` | As above |
| GPU power at rest | No more than the desktop without Slatty | `slatty-measure desktop`: `power1_average` of the card | By hand, once per stage |
| Command line start-up | `slatty library list` at 10,000 games ≤ 10 ms warm | `hyperfine` | By hand, when the command line changes |
| Verifying a file | 1 GB in ≤ 0.45 s on four threads | `verifying_a_large_file` | By hand, when verification changes |
| Bad input | Never a crash on a file or answer from outside (VDF, `appinfo.vdf`, GOG manifests, installers, archives) | Tests of hostile input; `cargo fuzz` from Stage 4 | `scripts/check.sh` (tests); fuzzing by hand and briefly in CI from Stage 4 |
| Downloading a game (set 2026-11-05, not measured yet) | A GOG or Epic install takes ≥ 90 % of the rate of one plain download from the same server, with at most one core of CPU on average, unpacking and verifying included; at least as fast as the best rival: Heroic on the same build, run in turn; Steam by its share of its own servers' rate | A fake local server at a fixed rate; no account, no real download; the rivals with the owner's consent | By hand, when downloads change |
| A cloud sync with nothing changed (set 2026-11-05, not measured yet) | Sends nothing, and ends in ≤ 50 ms off the network for 100 saves: it runs before each game | The tests' fake cloud | By hand, when the sync changes |

On uwui, where a line of the screen of reference "Slatty's library" is stricter than a budget
above, that line is the budget: today gThumb's 4.19 ms at p99 while scrolling, and its 149.9 to
153.8 MB of video memory
([uwui's measurements](https://github.com/HoneySlatt/uwui/blob/main/docs/measurements.md#slattys-library)).

Iteration, set by the owner on 2026-11-05: a CI run ≤ 20 min; `scripts/check.sh` ≤ 10 min warm on
the high tier; `cargo test -p slatty-core` ≤ 1 min. Measured from GitHub's run times and `time`;
checked at each change to CI or to the checks. Compiling, set 2026-11-08: uwui's budgets for an
application once SlimeEngine's M6d.10 sets them, a change of the interface seen within 2 s already
among them; Slatty's present build times measured during the pause, as a floor, once the owner
agrees.

Saves, set 2026-11-08: a bench in CI interrupts every sync and every save written at each step (the
process killed, power cut through a file-system fault layer, a full disk, the network cut, two
devices at once) and finds no version lost and no conflict resolved by itself. On GOG's and Epic's
clouds, which take no conditional upload, the two-device window is measured and kept as debt; the
person's own storage (Stage 8) allows none. Every mutant of cloud sync, installs and uninstalls is
killed or explained (`cargo mutants`), weekly and at each audit.

Crashes, set by the owner on 2026-11-05 and 2026-11-08: a crash writes a report in the logs folder
(version, system, message, stack trace; never a token or the sign-in address), which the person may
attach to an issue; nothing leaves the machine. The command line prints its path; the interface
tells it at the next start. Every crash leaves one: a panic, an abort, a signal, the GPU lost; a kill
for lack of memory is found at the next start. Secrets are kept out of core dumps. A local journal
counts hours of use and crashes: 99.9 % of hours without a crash, published with its statistical
bound, on the owner's and testers' use. Checked by a test that makes a crash and looks for secrets in
its report, and a test in CI that kills Slatty at random and finds nothing lost, which gates a
release.

Budgets on the lowest tier, with the same library and build. None is measured yet: the owner's
laptop has not run Slatty.

| Budget | Target |
|---|---|
| Library shown after start | ≤ 250 ms warm, ≤ 1.5 s cold (hard disk) |
| Scrolling the grid | No frame missed at 60 Hz: p99 of the time between frames ≤ 16.7 ms, none above 25 ms |
| `view` at 10,000 games | p99 ≤ 2 ms, library and Achievements |
| Lists made again | ≤ 8 ms |
| A couch mode move | Shown in the next frame |
| At rest, memory | As on the high tier |
| Everything that moves, input to photon, while a game runs | As on the high tier, at 60 Hz, effects reduced |
| Power, at rest and while scrolling (set 2026-11-08) | Figures set at the first measure, on the laptop (Stage 5) and on the Deck (Stage 6) |
| Command line start-up | `slatty library list` at 10,000 games ≤ 30 ms warm |
| Verifying a file | Bound by the disk: at most 10 % over reading the file alone |

Left for the interface on uwui (Stage 5), since Iced receives bug fixes only: rendering checked
against reference images at scales 1 and 2; WCAG 2.2 at level AA on the desktop and in the couch
mode (screen readers, full keyboard and gamepad use, reduced motion and transparency, contrast);
captures and recordings judged blind against the most refined current interfaces, by judges who
never saw Slatty, on the same fake content, by forced choice between pairs, against a threshold
written before the test; uwui's test of signature, which checks that Slatty is not recognised as an
application of uwui by its look. HDR as soon as uwui offers it, said not supported until then. Beside
its own budgets, Slatty on uwui beats Steam, gThumb and Nautilus on each criterion of uwui's screen
of reference "Slatty's library"; uwui's lead of three times there is measured, and a figure missed
is debt ([uwui's quality bar](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md#2-quality-bar)).

Agents, set 2026-11-08: from Stage 5, at each stage's end, tester agents drive Slatty end to end
through uwui's MCP server, on the fake profile and fake GOG and Epic servers, and report what
blocks, crashes or misses a budget. A person's own agents get read-only actions first, on local data
alone ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).

## 3. Decisions

### Fixed

| Topic | Decision |
|---|---|
| Direction | A showcase of uwui, for the general public; GOG, Steam, Epic, games added by hand and emulated; its own GOG engine, never depending on GOG Galaxy; native and optional pieces ([ADR-0034](adr/0034-what-slatty-is-to-become.md)) |
| Language | Rust for the whole application; Proton, umu and Comet stay external tools ([ADR-0001](adr/0001-rust-with-proton-umu-and-comet-outside.md)) |
| Licence | GPL-3.0-or-later; uwui, a dependency, is MIT OR Apache-2.0 ([ADR-0002](adr/0002-gpl-3-0-or-later.md), [ADR-0032](adr/0032-gpl-kept-on-a-permissive-framework.md)) |
| Storage | SQLite through rusqlite; migrations only appended ([ADR-0003](adr/0003-sqlite-through-rusqlite-behind-a-mutex.md)) |
| Secrets and privacy | Tokens only in the system keyring; every third party off until turned on; no telemetry ([ADR-0004](adr/0004-tokens-only-in-the-system-keyring.md), [ADR-0005](adr/0005-every-third-party-off-until-turned-on.md)) |
| Running Windows games on Linux | umu and Proton builds Slatty downloads; isolation through umu's container ([ADR-0009](adr/0009-isolation-through-umus-container.md), [ADR-0010](adr/0010-proton-builds-downloaded-by-slatty.md)) |
| Other stores | Steam read from its files and played through its client, Slatty's installed games written into its shortcuts while it is closed; GOG never told of other stores' games ([ADR-0013](adr/0013-steam-read-from-its-files-played-through-its-client.md), [ADR-0014](adr/0014-gog-never-told-of-other-stores-games.md), [ADR-0034](adr/0034-what-slatty-is-to-become.md)) |
| Interface | Iced, with bug fixes only, until uwui runs Slatty; every piece of interface work waits for uwui; two interfaces, the desktop's and the couch mode's, with uwui's gamepads and focus; the interface's state in uwui's signals over pure models free of any framework ([ADR-0015](adr/0015-iced-for-the-interface.md), [ADR-0018](adr/0018-a-couch-mode-of-its-own.md), [ADR-0019](adr/0019-the-interfaces-state-free-of-any-framework.md), [ADR-0020](adr/0020-gpui-set-aside-for-slime-ui.md), [ADR-0030](adr/0030-ported-second-onto-uwui.md), [ADR-0034](adr/0034-what-slatty-is-to-become.md)) |
| Themes | Slatty's themes in uwui's format; a person's `theme.toml` converted, the old file kept ([ADR-0034](adr/0034-what-slatty-is-to-become.md)) |
| Translations | Through uwui's (Fluent); French with the port ([ADR-0034](adr/0034-what-slatty-is-to-become.md)) |
| Platforms | Linux (Wayland and X11, x86-64); Windows 10 and 11 from one code base, built from Linux, its window coming with uwui ([ADR-0022](adr/0022-windows-hosts.md)); unchanged by uwui's other targets ([ADR-0030](adr/0030-ported-second-onto-uwui.md)); the interface on uwui holds no line written for one platform ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) |
| The bar | Steam and Heroic measured on the moments of play; a lead of three times on Slatty's own cost where only Slatty decides it; Slatty steps back while a game runs ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) |
| Packages | An AppImage and the Nix package, with the whole isolation; Flathub only if Flatpak's sandbox gives the same guarantees ([ADR-0034](adr/0034-what-slatty-is-to-become.md)) |

### Open

Each is decided by an ADR at the stage where it becomes necessary, not earlier.

| Topic | Candidates | Decide at |
|---|---|---|
| Windows toolchain | MinGW (now), Microsoft's MSVC | Stage 3, if the MinGW build shows a problem MSVC has not |
| What GOG Galaxy on Linux changes in the order of stages | A short review once its beta opens | When its beta opens |
| Where each part of `slatty-app` goes | Pure models, signals, the core | Stage 5's start |
| The look Slatty's themes start from | uwui's own look, an official theme, a structure of Slatty's own | Stage 5's start |
| The actions a person's agents may call | Read-only on local data first; acting ones left out or confirmed each time ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)) | Stage 5's start |
| Flathub | Only if Flatpak's sandbox gives Slatty's guarantees | Stage 6, by a prototype |
| How the AppImage updates itself | uwui's crate for updates, Slatty's own, AppImage's tools | Stage 6 |
| The couch mode's sounds | Made for Slatty, or found under a free licence | Stage 6 |
| A session of Slatty's own on a living room's computer | gamescope at login | After Stage 6 |
| The format of the person's storage for saves and play time, and its retention | Files never written over, a log per device | Stage 8 |
| Ludusavi's manifest | Used as compiled from PCGamingWiki, and how it is stored | Stage 8 |
| Epic's achievements | Read, if it can be done safely | Stage 8 |
| How Windows updates Slatty | uwui's crate, Slatty's own | Stage 3, M3.7 |
| Packages for other distributions | Distribution packages beside the AppImage and Nix | Stage 4 |
| Where emulated games' metadata comes from | ScreenScraper, IGDB or another, with the person's own key where asked | Stage 4 |
| How the network cut keeps the game's link to Comet | — | Stage 9 |
| How native Linux games are isolated | The Steam Linux Runtime's container, bubblewrap of Slatty's own | Stage 9 |
| Mods laid over a game | An overlay inside umu's container; hard links or copies on Windows | Stage 10, by a prototype |
| The pairing and trust between devices | Shared with Opium Connect | Stage 11 |
| Another store | itch.io, Amazon | After Stage 10 |
| Games made with SlimeEngine | Recognised and launched by Slatty | When SlimeEngine ships a game |
| macOS | GOG's native Mac games, without Proton | After Stage 11 |
| What 1.0 holds | — | Later |

## 4. Stages

The stages are listed by number but run in this order: Stages 1 and 2; Stage 3 up to M3.3; a
pause; Stages 5, 6 and 8; the rest of Stage 3; Stages 4, 10, 9 and 11
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0034](adr/0034-what-slatty-is-to-become.md)). Stage 7 is folded into Stage 5.

The pause lasts until Slatty's turn in uwui's first stage
([ADR-0030](adr/0030-ported-second-onto-uwui.md),
[SlimeEngine's ADR-0206](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0206-slatty-ported-again-the-daw-later.md)).
Only M3.3's check on Windows and M5.0, Iced's figures measured again, ran meanwhile
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
([debt.md](debt.md)).

### Stage 3 — Windows → 0.6

**Goal.** Slatty on Windows 10 and 11: GOG's, Epic's and Steam's games and games added by hand,
played directly, with the same safety as on Linux ([ADR-0022](adr/0022-windows-hosts.md)). Up to
M3.3 from the command line; the rest after Stages 5, 6 and 8, planned again then
([ADR-0025](adr/0025-paused-after-m3-3-until-slime-ui.md),
[ADR-0034](adr/0034-what-slatty-is-to-become.md)).

- Everything builds for Windows; the core's tests pass under Wine.
- GOG games installed, played, followed and synced on Windows; Epic and Steam; games added by hand;
  achievements through Comet.
- What Stage 8 brought that needs no Linux: the person's storage for saves, play time and
  achievements.
- A zip with the command line and the licences.

**Exit.** A person installed, played and synced a GOG game, an Epic game and a Steam game on a real
Windows 10 or 11, recorded in [compatibility.md](compatibility.md); Linux
unchanged.

### Stage 4 — Smaller work, outside the interface → 0.7

**Goal.** The core's smaller items waiting, before mods bring archives from outside:

- packages for other distributions, beside the AppImage and Nix of Stage 6;
- Steam's downloads followed more exactly while the client downloads;
- Wine without Proton for the games Slatty installs (it exists for imported games);
- the parsers of outside data (Steam's VDF and `appinfo.vdf`, GOG's manifests, Linux installers,
  archives) fuzzed with `cargo fuzz`, from a nightly toolchain in the development shell, decided
  in an ADR; with the same toolchain, Miri run on the tests of `unsafe` code it can run, those it
  cannot listed with the reason (the owner's decision, 2026-11-05);
- the budgets `library_at_10000_games` measures checked by `scripts/check.sh` in CI, with limits
  for CI's machines written in the quality bar;
- an open, documented export of the library, and existing installs adopted from Lutris and
  Minigalaxy, beside Heroic's import;
- emulated games' metadata fetched once turned on, an optional piece, from a source decided by an
  ADR ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).

**Exit.** The parsers fuzzed and the budgets checked in CI, and Slatty installed and run from a
distribution's own package, recorded in [compatibility.md](compatibility.md).

### Stage 5 — The interface on uwui, Windows included → 0.3

**Goal.** Slatty's interface drawn by uwui, on Linux and on Windows, as one of the programs of
uwui's first stage ([ADR-0030](adr/0030-ported-second-onto-uwui.md),
[ADR-0034](adr/0034-what-slatty-is-to-become.md)). Starts when uwui's plan opens Slatty's port.
0.3 is released for Linux once the exit holds, on a pinned revision of uwui, without waiting for
the API to be declared stable; Windows' first release is Stage 3's, 0.6.

- First, under Iced: the rules of `slatty-app`'s `update` moved into pure models, free of any
  framework, with their tests; Iced's figures measured unchanged.
- Then every page and the couch mode on uwui, on its desktop host, the interface's state in uwui's
  signals over the pure models; the interface's tests ported. `slatty-core` and the pure models
  never depend on uwui.
- The couch mode on uwui's gamepads, focus and on-screen keyboard.
- The showcase: the cover that flies from the grid to its game's page, Slatty's own title bar,
  glass over the desktop where the compositor allows it, the tray while work runs after the window
  is closed.
- Slatty's themes in uwui's format, a person's `theme.toml` converted, the old file kept.
- The new emblem in the window and the package's icons
  ([ADR-0035](adr/0035-slattys-emblem.md)).
- French, through uwui's translation; WCAG 2.2 at level AA on the desktop and in the couch mode.
- Nothing that wakes at rest: the thread of zbus's pool that wakes every 500 ms found and stopped
  ([quality bar](#quality-bar), [known debt](debt.md)).
- The interface on Windows: Linux-only settings hidden, the file chooser, gamepads, a window icon,
  paths shown the Windows way; no line of the interface written for one platform, what differs in
  the core's `platform` module, a check refusing the rest
  ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- Slatty stepping back while a game runs, and shown again within 100 ms after it.
- Once at parity: collections of the person's own, and saved filters that update themselves; a
  download speed cap changed live and a window of hours for automatic updates, Slatty saying why
  each waiting download waits.
- The budgets at 100,000 games set; power measured on the laptop; tester agents from this stage.
- A checkpoint where the owner uses it day to day.
- Iced removed.

**Exit.** Slatty on uwui at least as good as on Iced on the library shown, scrolling, rest under
Wayland and Xwayland, and memory, both measured side by side by M5.0's command
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)); within the
[quality bar](#quality-bar), the lowest tier's budgets measured and met; better than Steam, gThumb
and Nautilus on each criterion of uwui's screen of reference "Slatty's library", uwui's lead of
three times measured, a figure missed written as debt without holding 0.3 back; the blind tests and
the test of signature passed; every page at parity, in English and French; the window checked by a
person on Windows; Iced gone from `Cargo.lock`.

### Stage 6 — On a Steam Deck → 0.4

**Goal.** The couch mode on uwui as the Steam Deck's Game Mode needs it, checked on a real Deck, and
Slatty installable outside NixOS.

- An AppImage, and the Nix package, both with the whole isolation; Flathub only if a prototype
  shows that Flatpak's sandbox gives the same guarantees.
- Slatty started from Steam and by itself in Game Mode.
- Slatty's installed games written into Steam's library, once turned on, while Steam is closed,
  its file copied first, only Slatty's entries changed.
- A disk chosen for each game, games moved between disks, an absent SD card's games shown absent.
- The couch mode's sounds; on a computer, the Guide button bringing Slatty to the front during a
  game, where the compositor allows it.
- Launches ready before the click: the prefix made and the game's setup run at install, the saves
  watched from a page's opening, the container's runtime copied once per version (it is written at
  each launch on ext4, the Deck's file system); launches timed against Steam and Heroic.
- What the checks find fixed: scale, gamepad, full screen, suspending and resuming.
- A poster mode in the couch mode: one still picture of the game in focus when nothing is touched,
  never changing by itself.
- Controller mappings per game, given to SDL inside the game's sandbox, with no virtual input
  device; Steam Input keeps the games written into Steam.
- The Deck's budgets, its power among them, set and measured on the owner's Deck OLED at 90 Hz.

**Exit.** A person browsed, installed, played and synced a game on a real Steam Deck in Game Mode,
from the AppImage, and found it in Steam's library, recorded in
[compatibility.md](compatibility.md).

### Stage 7 — Translations

Folded into Stage 5 ([ADR-0034](adr/0034-what-slatty-is-to-become.md)): French ships with the
port, through uwui's translation, whose files are the way for others to add a language.

### Stage 8 — Saves, play time and achievements everywhere → 0.5

**Goal.** A game played on the computer goes on on the Deck, whatever its store.

- Games without a store's cloud synced through the person's storage, a folder or a WebDAV server,
  in a format decided by an ADR, with the rules of [cloud-saves.md](cloud-saves.md).
- Save locations from Ludusavi's manifest, once turned on, or chosen by hand.
- Each game's save history shown, an older version brought back in one gesture.
- Play time per game, sessions and simple statistics, followed between devices.
- GOG's, Steam's and, if it can be done safely, Epic's achievements on one page;
  RetroAchievements for emulated games, once turned on.
- A game's page with its store's screenshots and videos, and HowLongToBeat's times once turned on.
- Emulated games checked emulator by emulator.

**Exit.** A person played a GOG game, an added game and an emulated game on the computer, then went
on on the Deck from the same save, recorded in [compatibility.md](compatibility.md); every new case
of sync tested, interrupted, concurrent and offline.

### Stage 9 — Playing further on Linux → 0.9

**Goal.** More control over how a game runs, at no cost to safety.

- A game's network cut by a switch, its link to Comet kept.
- GOG's native Linux games isolated.
- Per game: MangoHud, gamescope, GameMode and a few launch options, with defaults that work.
- ProtonDB's rating, once turned on, beside a local mark, "runs well here", from the person's own
  sessions.
- Updates of GOG's and Epic's games automatic by a setting, never during a game.
- GOG's offline installers kept on demand in a folder of the person's, and installed from.

**Exit.** Each checked against real games, recorded in [compatibility.md](compatibility.md); the
isolation tests extended to Linux games and to the network cut.

### Stage 10 — Mods → 0.8

**Goal.** Mods installed, ordered and turned off without ever changing a game's installed files.

- A prototype first: mods laid over a game folder and its prefix inside umu's container.
- Mods from local archives; then Nexus Mods (through its site's `nxm://` links, as a registered
  application) and Thunderstore, each once turned on.
- On Windows, hard links or copies, with a copy of what they cover.
- An update or a verification warns before touching a file a mod covers.
- A snapshot of the game's Wine prefix before a mod or a change of Proton, rolled back in one
  gesture, the current saves kept across the rollback.

**Exit.** A person modded a game from each source on Linux and on Windows, then updated the game and
turned the mods off, its files verified unchanged, recorded in [compatibility.md](compatibility.md).

### Stage 11 — Devices that exchange saves

**Goal.** Saves and play time exchanged between the computer and the Deck directly, on the local
network, without a storage between them.

- The pairing and trust, shared with Opium Connect, decided by an ADR.

**Exit.** Two devices paired, a save exchanged both ways, every case tested.

### Out of scope

Store purchases, deals, friends and chat, presence in chat services, multiplayer services, an
overlay drawn inside games, recording and clips, streaming, parental controls, installing
emulators, moving a save between two stores' versions of a game, plugins
([ADR-0036](adr/0036-slattys-bar-against-the-market.md)). Slatty has no friends or
chat feature; Comet, which it starts for achievements once turned on, still subscribes to those
topics on GOG for the account ([SECURITY.md](../SECURITY.md#comet)).

## 5. Release map

| Version | Stage | Content |
|---|---|---|
| 0.1.0 | 1 | GOG on Linux (released 2026-10-21) |
| 0.2.0 | 2 | Beyond GOG (released 2026-10-27) |
| 0.3 | 5 | The interface on uwui, Windows' window included, in English and French |
| 0.4 | 6 | On a Steam Deck, with the AppImage and Nix |
| 0.5 | 8 | Saves, play time and achievements everywhere |
| 0.6 | 3 | Windows |
| 0.7 | 4 | Smaller work outside the interface |
| 0.8 | 10 | Mods |
| 0.9 | 9 | Playing further on Linux |
| Later | 11 | Devices that exchange saves |
