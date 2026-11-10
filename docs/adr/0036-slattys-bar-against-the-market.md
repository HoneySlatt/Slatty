# 0036. Slatty's bar set against the market

- Status: Accepted; amends [0009](0009-isolation-through-umus-container.md) (isolation measured
  apart; no virtual input device inside the sandbox),
  [0018](0018-a-couch-mode-of-its-own.md) (a poster mode; controller mappings per game),
  [0023](0023-a-quality-bar-close-to-what-is-measured.md) (rivals measured on the moments of play,
  with a lead), [0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md) (a launch budget;
  the lowest tier measured before 0.3; the Deck at 90 Hz; battery; mutants weekly),
  [0030](0030-ported-second-onto-uwui.md) (build times measured during the pause; macOS an open
  decision) and [0034](0034-what-slatty-is-to-become.md) (the port's exit; features added; a new
  optional piece; agents)
- Date: 2026-11-10

## Context

[ADR-0023](0023-a-quality-bar-close-to-what-is-measured.md) set Slatty's budgets close to what
Iced measured. Its references, Steam, GOG Galaxy 2, Playnite and Heroic, are named but never
measured. On 2026-11-08 the owner asked that Slatty be the best launcher on the market, and held an
interview on its quality, budgets, ambitions and features. A fresh review then attacked the
answers; what it found changed several of them.

What the decisions start from:

- **uwui's decisions of 2026-11-07.** SlimeEngine's
  [ADR-0213](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0213-uwuis-reach-its-lead-its-themes-and-agents.md),
  [ADR-0215](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0215-the-engine-measured-against-the-market.md)
  and [ADR-0218](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0218-one-core-for-agents-in-uwui.md)
  ask of every application on uwui a lead on its rivals, measured as its *own cost* above a
  *blank window*; input timed to the photon by a light sensor; blind tests; a test of signature; budgets
  of compiling; 99.9 % of hours without a crash; actions declared once for users' agents; and no
  line written for one platform. uwui's [roadmap](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md#2-quality-bar)
  holds them.
- **What is measured.** On the screen of reference "Slatty's library", Slatty on Iced beats Steam
  on start, memory and rest, but scrolls less steadily than Steam (p99 16.5 ms against 8.2 ms), and
  a native program, gThumb or Nautilus, beats it on start, scrolling, video memory and wake-ups
  ([uwui's measurements](https://github.com/HoneySlatt/uwui/blob/main/docs/measurements.md#slattys-library)).
  Being lighter than a launcher built on Chromium is not being the best.
- **What nobody measures.** A survey of the field on 2026-11-08 found no independent figure, from
  any launcher, for the time from Play to the game, for download throughput, or for a library of
  10,000 games; and no launcher that publishes budgets with their method and machine. Epic and
  Microsoft announced faster launchers that autumn, on their own tests.
- **What rivals do better.** Steam caps its downloads and sets a window for updates; Galaxy 2 and
  Playnite have collections and saved filters; Bottles and Faugus snapshot a Wine prefix; ES-DE and
  LaunchBox fetch emulated games' metadata; Steam Input remaps controllers per game.
- **Limits found by the review.** Isolation copies the container's runtime at each launch, about
  a second on btrfs and about 750 MB written on ext4, unmeasured ([SECURITY.md](../../SECURITY.md));
  the rivals do not isolate. A window given back at once cannot be rebuilt from nothing: the
  compositor's answer to a first frame alone took about 230 ms. A window behind others cannot raise
  itself on Wayland without the compositor's token. A virtual input device (`/dev/uinput`) reachable
  from a game lets it type into the desktop. GOG's and Epic's storage APIs take no conditional
  upload ([cloud-saves.md](../cloud-saves.md)). A core dump after a crash may hold session tokens.

## Decision

The owner's decisions of 2026-11-08, with the review's corrections the owner took on 2026-11-10.
Each figure missed is debt until met. Beating each line is the least; the lead is a budget of its
own.

**Rivals, on the moments of play only.** Slatty is not compared broadly with other launchers'
interfaces: its grid is judged by uwui's screen of reference. It is measured against Steam and
Heroic on the *moments of play*: Play to the game's first frame, the same game on the same Proton;
an install and an update, end to end; download throughput; Slatty's cost while a game runs; the
couch mode's input to photon against Steam's Game Mode. GOG Galaxy for Linux joins them when it
ships, in the short review already planned when its beta opens: it downloads the same builds from
the same servers. Playnite, Galaxy 2 and the others serve for features only. Each measure that
uses the owner's accounts waits for the owner's consent.

**The lead, by family.** Each launcher's *own cost* is what it adds above the *bare tool*: the
same game started by `umu-run` with the same Proton, or one plain download. On launching and while
a game runs, Slatty's own cost is at most a third of the best rival's, compared at equal features:
isolation, which the rivals lack, has a budget of its own, and its runtime is copied once per
version rather than at each launch. On downloads and installs, Slatty does at least as well as the
best rival and reaches 90 % of a plain download; against Heroic on the same build, run in turn;
Steam is judged by its share of its own servers' rate.

**While a game runs, Slatty steps back.** It keeps its process but frees what the game could use:
the covers' textures and its caches. Its video memory falls to a blank window's, its resident memory
to a few tens of MB, and it wakes only for the session's events. The covers of the first screen stay
decoded in memory, so that Slatty is shown again within 100 ms of the game's end. The Guide button
brings it to the front only where the compositor allows it. Comet is watched through its files
rather than polled every second; the Guide button is heard through a handle that sees only that
button; downloads still pause during play by default (**Pause downloads while playing**).

**Ready before the click, nothing sent.** At install, the Wine prefix is created, Proton checked and
the game's setup run. From a game's page opening to the click, Slatty watches the save folder for
changes rather than hashing it again. The cloud is asked only at the click, and a save is still
checked again before it is replaced.

**Downloads under the person's control.** A speed cap changed live, held within 5 % over a stated
window of time; a window of hours for automatic updates; priority by the queue's order. Slatty
always says why a download waits. All of it is local.

**Fluidity and response.** Nothing that moves misses a refresh: scrolling, pages, drawers, the cover
that flies to its page, the couch mode's focus. Missed refreshes are counted from the compositor's
presentation feedback, with variable refresh off. Input to photon equals the blank window's at the
99th percentile, within a tolerance and a sample count set with the first measure, timed by the
light sensor on the desktop and in the couch mode. The lowest tier follows the same rules at 60 Hz,
effects reduced.

**Machines and power.** The lowest tier's budgets are measured and met as Stage 5's exit, before
0.3. Its display, storage and Vulkan driver are read before its figures are set. The Steam Deck
OLED is measured at 90 Hz at Stage 6. Video memory counts what an integrated GPU borrows from main
memory (GTT). Power at rest and while scrolling gets budgets on the laptop (Stage 5) and the Deck
(Stage 6), their figures set at the first measure. Intel's and NVIDIA's GPUs stay unmeasured until
one is at hand.

**Ten times the size.** A fake profile of 100,000 games, for a large emulated library, gets budgets
for lists made again, start and memory, set at Stage 5 by measure.

**Saves, proven.** A bench in CI interrupts every sync and every save written at each step: the
process killed, power cut through a file-system fault layer, a full disk, the network cut, two
devices at once. It checks that no version is lost and no conflict resolved by itself. On GOG's and
Epic's clouds, which take no conditional upload, the two-device window is measured and written as
debt; the person's own storage (Stage 8) allows none. Every mutant of cloud sync, installs and
uninstalls is killed or explained (`cargo mutants`), weekly and at each audit.

**Crashes, counted on the machine.** A local journal counts hours of use and crashes. Every crash
leaves a report without secrets: a panic, an abort, a signal, the GPU lost; a kill for lack of
memory is found at the next start. Secrets are kept out of core dumps. 99.9 % of hours without a
crash is published with its statistical bound, on the owner's and testers' use; what gates a
release is a test in CI that kills Slatty at random and finds nothing lost. Nothing is sent, ever.

**Features added.**

- Collections of the person's own and saved filters that update themselves: Stage 5, after the port
  reaches parity.
- An open, documented export of the library, and adopting existing installs from Lutris and
  Minigalaxy, beside Heroic's import, which exists: Stage 4.
- Metadata for emulated games, fetched once turned on, from a source chosen by an ADR, with the
  person's own key where the source asks one: Stage 4. It becomes an optional piece, beside those
  [ADR-0034](0034-what-slatty-is-to-become.md) lists.
- A poster mode in the couch mode: one still picture of the game in focus when nothing is touched,
  which never changes by itself, so that rest stays at 0 wake-ups: Stage 6.
- Controller mappings per game, given to SDL inside the game's sandbox; no virtual input device.
  Steam Input keeps the Deck's games in Steam: Stage 6.
- A local mark, "runs well here", from the person's own sessions, beside ProtonDB: Stage 9.
- Snapshots of a game's Wine prefix before a change of Proton or a mod, rolled back in one gesture,
  the current saves kept across the rollback, by reflink where the file system allows: Stage 10.
- Out of scope: recording, an overlay drawn in a game, streaming, parental controls, presence in
  chat services, deals, plugins. itch.io stays an open decision after Stage 10.

**The look, judged.** At Stage 5's exit, captures and recordings of Slatty are judged blind against
the most refined current interfaces: judges who never saw Slatty, the same fake content in every
capture, a forced choice between pairs, a threshold written before the test. uwui's test of
signature checks that Slatty is not recognised as an application of uwui by its look. HDR comes as
soon as uwui offers it, and is said not supported until then. The look Slatty's themes start from is
decided at Stage 5's start.

**Agents.** A person's own agents get read-only actions first: the library, the saves' state and
play time, read from the machine alone; installing, syncing, launching and achievements are left
out, or confirmed in Slatty's window each time. Read-only never reaches a network. No secret is
given; the actions are off until turned on; they are offered on a local socket only the person can
open. Driving the window is built into test builds only, so that no agent confirms its own action.
The details are decided at Stage 5's start. From Stage 5, at each stage's end, tester agents drive
Slatty end to end through uwui's MCP server, on the fake profile and fake GOG and Epic servers, and
report what blocks, crashes or misses a budget.

**Compiling.** Slatty takes uwui's budgets of compiling for an application once they are set; a
change of the interface is already seen within 2 s. Slatty's present build times are measured
during the pause, as a floor, once the owner agrees.

**The port's exit.** 0.3 needs each line of uwui's screen of reference beaten, Slatty's own budgets,
the lowest tier's, and the blind tests. uwui's lead of three times is measured; a figure missed is
debt in uwui and in Slatty, and does not hold 0.3 back.

**No line for one platform in the interface.** `slatty-gui` and `slatty-app` hold no code written
for Linux or Windows. The core gives facts; the pure models turn them into wording; what differs
lives in the core's `platform` module. From Stage 5, a check refuses `cfg` on a platform in those
crates; the one line of Iced's interface that has one (its window's application id on Unix) goes
with Iced. A piece that uwui lacks is reported to it as a need, never worked around.

**macOS** becomes an open decision, after Stage 11: GOG's native Mac games, without Proton.

## Alternatives considered

- **Every launcher measured on everything**, as SlimeEngine measures games: proof of each claim,
  at the cost of measures that would mostly confirm what is known; the interface is already
  measured against native programs by uwui. **No rival at all**, as the owner first thought, every
  launcher being built on Electron: Steam and Galaxy are built on Chromium, but Lutris, Playnite,
  ES-DE and Pegasus are native, and nothing would judge the moments of play.
- **Three times on every family**: one rule, but out of reach where the network or the disk sets
  the floor. **Beating each line**: always reached, a lead of 1 % passing.
- **Isolation counted in the lead**: Slatty penalised for a guarantee the rivals do not give, a
  debt no work could settle. **Isolation off by default**: faster, against ADR-0009.
- **No video memory at all during a game**: the window rebuilt at each return, beyond 100 ms.
  **The window kept whole**: an instant return, but about 450 MB held from the game on a 4 GB
  machine.
- **Hashing saves when a page opens**: work the click does again. **Asking the cloud at that
  moment**: GOG told which pages the person opens, and an answer stale by the click.
- **Rules of 99.9 % of frames on time**, SlimeEngine's: one hitch in a thousand allowed, looser
  than Slatty's scrolling already is.
- **The lowest tier checked at audits only**: a release that stutters on a modest machine.
- **Crash reports sent with the person's consent**: more hours counted, a server to run, against
  "no telemetry, ever".
- **A virtual controller through `uinput`**: mice and macros, but a keyboard a game could type
  with. **No remapping**: left to Steam Input, which serves only games in Steam.
- **A poster that turns**: livelier, but a wake-up at rest.
- **Agents with every action**: the most useful, a mistaken agent able to overwrite saves or touch
  a public profile. **No action**: uwui's feature not shown by its showcase.
- **0.3 held until the lead of three times**: a release that waits on uwui's work.
- **Code per platform allowed in the interface**: simpler at times, and uwui's promise never
  tested by a real application.
- **macOS left out for good**, as ADR-0030 had it: closed before GOG's Mac games were weighed.

## Consequences

- [ROADMAP.md](../ROADMAP.md#quality-bar) holds the bar, the rivals and the features in their
  stages; [debt.md](../debt.md) what falls short today; [uwui-needs.md](../uwui-needs.md) what
  Slatty asks of uwui for them.
- Tools come before the figures they judge: launches and installs timed from outside, a
  file-system fault layer, a profile of 100,000 games, power read on the laptop and the Deck, the
  light sensor's bench (SlimeEngine's Stage 6e).
- The figures of Steam, Heroic and the bare tool are not measured yet: each ratio is set when they
  are.
- Masking `/dev/uinput` in the sandbox, keeping secrets out of core dumps, and copying the runtime
  once are debt from now.
- The lowest tier's display, storage and driver are not yet known.
- [AGENTS.md](../../AGENTS.md) holds the rule on code per platform.
