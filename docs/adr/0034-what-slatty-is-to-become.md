# 0034. What Slatty is to become

- Status: Accepted; amends [0009](0009-isolation-through-umus-container.md) (a game's network cut;
  Linux games isolated too), [0013](0013-steam-read-from-its-files-played-through-its-client.md)
  (Slatty's own games written into Steam's shortcuts, guarded),
  [0018](0018-a-couch-mode-of-its-own.md) (gamepads, focus and the on-screen keyboard from uwui; the
  Guide button; sounds), [0019](0019-the-interfaces-state-free-of-any-framework.md) (the interface's
  state in uwui's signals; pure models free of any framework),
  [0021](0021-interface-in-english-first.md) (French with the port), [0022](0022-windows-hosts.md)
  (Windows gains what needs no Linux), [0023](0023-a-quality-bar-close-to-what-is-measured.md) (the
  wake-ups at rest fixed in Stage 5),
  [0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md) (the lowest tier is the owner's
  laptop), [0025](0025-paused-after-m3-3-until-slime-ui.md) (Slatty goes on; the order after Stage
  5) and [0030](0030-ported-second-onto-uwui.md) (ported whole in uwui's first stage; Stage 5's open
  decisions taken; the market's bar in its exit)
- Date: 2026-11-07

## Context

Slatty's decisions so far answered the needs of the day: a store, a format, a framework. What Slatty
must become in the end, for whom, and why a person would open it, had never been asked whole.

On 2026-11-05, GOG announced a new GOG Galaxy for every system, Linux included, "coming in 2027"
([GamingOnLinux](https://www.gamingonlinux.com/2026/11/a-new-gog-galaxy-is-coming-in-2027-with-full-linux-support/)).
Its features on Linux are not public: Proton, cloud saves, achievements and the Steam Deck are not
mentioned. Its beta was not yet open to the owner. The owner first thought Slatty was no longer
needed, and SlimeEngine's
[ADR-0203](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0203-pilots-replaced-pluto-and-a-daw.md)
(2026-11-06) dropped Slatty's port onto uwui, keeping its library as a screen of reference alone.
On 2026-11-07 the owner changed their mind: Slatty goes on, ported whole onto uwui in uwui's first
stage, with Opium and Pluto's client (SlimeEngine's
[ADR-0206](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0206-slatty-ported-again-the-daw-later.md)).
uwui had chosen signals for applications' state that same day, from three other programs' lists;
ADR-0206 checks that choice against Slatty's list once it is written.

The owner answered on 2026-11-07, theme by theme, in a design session that built nothing. The
questions came from the ADRs' open points, the [known debt](../debt.md), the inventory of
what Slatty asks of a framework (2026-11-04), uwui's requirements (SlimeEngine's
[ADR-0190](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0190-uwuis-requirements.md)),
and a survey of the field. A fresh review then attacked the recommendations; what it found changed
the order of the port's work, the packages, and the guards of saves, mods and Steam's shortcuts.
What the survey found, in short:

- **GOG Galaxy 2** (Windows, macOS) shows one library across stores, but only Epic and Xbox are
  official integrations; the rest are community plugins, some fragile. Play time counts only games
  started from it.
- **Steam** and its Big Picture are the reference for a gamepad and a television. A game outside
  Steam is only a shortcut there: no cloud, no achievements.
- **Heroic** (Electron) covers Epic, GOG and Amazon, with Comet for GOG's achievements since 2.21
  and a basic console mode. Its GOG saves are decided by file dates
  ([heroic-gogdl's `saves.py`](https://github.com/Heroic-Games-Launcher/heroic-gogdl/blob/main/gogdl/saves.py)).
- **Lutris** has many runners and install scripts, but no gamepad navigation and no cloud saves.
  **Bottles** manages Wine prefixes, with snapshots. **Faugus** is a simple launcher on umu, games
  added one by one.
- **Playnite** (Windows) has the richest extensions and a mode for controllers; it has no Linux
  build yet. **Ludusavi** backs up saves of every store, synced only through rclone.
- No launcher isolates each game, decides saves by their content, or measures its budgets. No
  launcher manages mods: on Linux they need a program apart (Limo), or Windows' (Mod Organizer 2)
  under Proton.

Facts checked for the decisions below:

- A Flatpak application cannot start bubblewrap itself: Flatpak blocks new user namespaces, and
  offers only its portal's smaller sandbox
  ([flatpak#4187](https://github.com/flatpak/flatpak/issues/4187)).
- bubblewrap mounts overlays since 0.11.0, and Linux allows them in a user namespace since 5.11
  ([bubblewrap's NEWS](https://github.com/containers/bubblewrap/blob/main/NEWS.md)).
- Steam rewrites its shortcuts file (`userdata/<id>/config/shortcuts.vdf`, binary) while it runs;
  the tools that edit it ask for Steam to be closed
  ([Heroic#3653](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher/issues/3653)).
- Nexus Mods asks a public application to be registered, and free accounts download through its
  site's `nxm://` links ([its policy](https://help.nexusmods.com/article/114-api-acceptable-use-policy)).
- Ludusavi's manifest is MIT, compiled from PCGamingWiki, about 18 MB of YAML, updated several
  times a day ([its repository](https://github.com/mtkennerly/ludusavi-manifest)).
- Making a symbolic link on Windows needs a right users lack ([debt.md](../debt.md)).

## Decision

The owner's decisions of 2026-11-07.

**Why Slatty.** It is first a showcase of uwui: the couch mode, motion and materials, and speed at
10,000 games. It is also what the owner opens each week beside Steam, for four reasons: all their
games in one place, the couch mode and the Steam Deck, saves that follow between the computer and
the Deck, and Windows games isolated. In the survey above, no other launcher does the last three.
Safety still comes before features, and before the showcase.

**Who it is for.** The general public: made first for its owner, built so that anyone can install
and understand it. One person per installation, one account per store.

**Stores.** GOG, Steam and Epic, with games added by hand and emulated games. Another store
(itch.io, Amazon) is an open decision. **Slatty keeps its own GOG engine** (installs, Proton,
cloud, achievements through Comet) and never depends on GOG Galaxy.

**The couch mode and the Steam Deck.** Two interfaces stay, the desktop's and the couch mode's, each
with its own pages over one state (ADR-0018). Their gamepads, focus and on-screen keyboard are
uwui's, not Slatty's own code. Discreet sounds follow the focus in the couch mode, and can be turned
off. During a game on a computer, the Guide button brings Slatty to the front, the game running
behind it; Slatty draws nothing inside a game. On the Steam Deck, **Slatty can write each of its
installed games into Steam's library**, as a shortcut that starts the game through Slatty, which
follows the session to its end. Guards: a setting, off until turned on; Steam's shortcuts file
written only while Steam is closed, and copied first; only the entries Slatty added are changed or
removed. In Game Mode, where Steam always runs, Slatty keeps the changes and writes them once Steam
is closed, and says that a game shows in Steam after that. A game can be installed on a disk chosen
for it and moved between disks, the copy verified before the old one goes. A disk that is absent,
such as the Deck's SD card, shows its games as absent, never as uninstalled.

**Saves.** GOG's and Epic's clouds stay as they are
([ADR-0012](0012-cloud-saves-decided-by-content-hashes.md)). Games without a store's cloud (added by
hand, emulated, GOG games without cloud saves) sync through a storage of the person's: a folder
(synced by Syncthing, or on a NAS) or a WebDAV server, with the same rules of content hashes and
per-file history. Another program may write into that folder at the same time, so the storage gets a
format of its own, its files never written over, decided by an ADR with tests of interrupted,
concurrent and offline syncs before it is built. Later, devices exchange saves directly, through the
same model of pairing and trust as Opium Connect. Where a game keeps its saves comes from Ludusavi's
manifest, off until turned on, or from a folder chosen by hand. Each game's save history is shown,
and one gesture brings an older version back; the oldest go first past a limit of disk space. A save
never moves between two stores' versions of a game.

**Proton, Wine and isolation.** A game's network can be cut by a switch of its own; the cut keeps
the game's link to Comet, so that achievements still count. GOG's native Linux games are isolated
too. Per game: the Proton build, MangoHud, gamescope (resolution, FSR), GameMode and a few launch
options, each explained, with defaults that work. ProtonDB's rating shows on a game's page once
turned on.

**Mods.** A mod manager of Slatty's own, with Nexus Mods and Thunderstore as sources. Mods never
change the game's installed files: on Linux the sandbox lays them over the game folder, and over
its prefix where a mod lives there, at launch. On Windows, hard links or copies stand in, with a
copy of what they cover, never symbolic links. Whether the overlay works inside umu's container is
answered by a prototype first.

**Game data.** Play time per game, the history of sessions and simple statistics, followed between
devices through the same storage as saves; GOG still receives its games' play time. The
achievements of GOG, Steam and, if they can be read safely, Epic, on one page; RetroAchievements for
emulated games, with the person's account. A game's page shows the screenshots and videos its store
gives, and HowLongToBeat's times once turned on. Emulated games are launched well and checked
emulator by emulator; Slatty does not install emulators. Updates of GOG's and Epic's games are asked
for by default, automatic by a setting per game or for all, never during a game. GOG's offline
installers can be kept, on demand, in a folder the person chooses, and installed from.

**The interface.** A game's page shows its key art with light motion; its trailer plays when asked.
Slatty draws its own title bar, the system's offered as a setting. The window is glass over the
desktop where the compositor allows it, opaque otherwise, and on battery, on the lowest tier and
under reduced transparency. Closed during work (downloads, saves), Slatty stays in the system's
tray, tells when the work ends, then quits by itself; a setting quits at once. Slatty's six themes become
themes in uwui's format, and a person's `theme.toml` is converted at the first start on uwui, the
old file kept.

**Accessibility and language.** WCAG 2.2 at level AA, on the desktop and in the couch mode: screen
readers (Orca, Narrator), everything by the keyboard and by a gamepad, high contrast, reduced motion
and transparency. French ships with the port (0.3), the views written to be translated from the
start, through uwui's translation.

**Machines.** The lowest tier is the owner's laptop, the same as uwui's and Opium's: an AMD Athlon
Silver 3050U (two cores), its Radeon Vega, 4 GB, 1080p at 60 Hz. It replaces ADR-0024's 2016
laptop, which nobody here has, and can be measured. It has two threads where the old one had four,
and about 3.25 GB left to programs; it runs Linux, so Windows on the lowest tier stays unmeasured.
The budget of a cold start on a hard disk stays, measured once a hard disk is at hand.

**The port's exit.** Slatty on uwui runs at least as well as on Iced (ADR-0030), within its
[quality bar](../ROADMAP.md#quality-bar), and better than Steam, gThumb and Nautilus on each
criterion of uwui's screen of reference "Slatty's library"
([uwui's quality bar](https://github.com/HoneySlatt/uwui/blob/main/docs/ROADMAP.md#the-bar-of-each-screen)).

**The interface's state, as if Slatty started today.** `slatty-core` holds all behaviour, as now.
Beside it, a layer of **pure models**, free of any framework: the library's lists, sorting and
filters, the wording shown, the couch mode's and the downloads' rules, each with its tests. The
interface's own state (the page open, focus, selection, motion, the work each part started) is
written in uwui's signals, in `slatty-gui`. Slatty's own runtime of messages, `update`, tasks and
subscriptions (`task.rs`, `listen.rs`, `Effect`) goes, as uwui gives these. Neither `slatty-core`
nor the pure models depend on uwui (ADR-0030). Where each part of today's `slatty-app` goes is
decided at the start of Stage 5. **The pure models come first, under Iced**: Stage 5's first
milestone moves the rules of `update` into them with their tests, Iced's figures measured
unchanged; then the views on uwui are written in signals over them. Each step is checked alone,
so that a regression has one cause.

**Native or optional.** Native means always built, perhaps off by default. Optional means a piece
that a build can leave out whole, a Cargo feature, built by default. The optional pieces are Epic,
Nexus Mods, Thunderstore, HowLongToBeat, ProtonDB, RetroAchievements, SteamGridDB, Ludusavi's
manifest, Comet, and the exchange between devices: each reaches a third party that may break, close
or raise a legal question. This list is the whole of the optional pieces. Everything else is native:
GOG, Steam read from its files, games added by hand and emulated, the clouds and the person's
storage, the save history, isolation and the network cut, the couch mode and its sounds, play time
and statistics, the mod manager's core (local archives laid over a game), the shortcuts in Steam,
disks, offline installers, game updates, translation and accessibility. Every third party is off
until turned on ([ADR-0005](0005-every-third-party-off-until-turned-on.md)), and so are the
shortcuts in Steam, the network cut and automatic updates.

**Windows.** ADR-0022's scope stays, and Windows gains what the decisions above add without Linux:
the person's storage for saves, mods, play time and achievements.

**Order.** Slatty stays paused until its port opens in uwui's first stage (ADR-0030). Then:

1. Stage 5, the port (0.3), with French and accessibility.
2. Stage 6, the Steam Deck, with its packages (0.4): an AppImage, which runs on the Deck as it is,
   and the Nix package, both with the whole isolation. Flathub only if a prototype shows that
   Flatpak's sandbox gives the same guarantees.
3. Stage 8, saves, play time and achievements everywhere (0.5).
4. Stage 3's rest, Windows (0.6).
5. Stage 4, the smaller work outside the interface (0.7), its fuzzing before mods bring archives
   from outside.
6. Stage 10, mods (0.8).
7. The rest: Stage 9, playing further on Linux (0.9); Stage 11, devices that exchange saves.

Stage 7, translations, is folded into Stage 5. The stages are in
[ROADMAP.md](../ROADMAP.md#4-stages).

**When Galaxy's Linux beta opens,** the owner tries it, and a short session compares it with Slatty
and adjusts the order if needed, without reopening this interview.

**Left open**, each decided when it becomes necessary:

| Decision | Decided at |
|---|---|
| What GOG Galaxy on Linux changes in the order of stages | When its beta opens |
| Where each part of `slatty-app` goes: pure models, signals, or the core | Stage 5's start |
| Whether Flatpak's sandbox can give Slatty's guarantees, for a Flathub package | Stage 6, by a prototype |
| How the AppImage updates itself | Stage 6 |
| The Steam Deck's budgets | Stage 6, on the owner's Deck |
| The couch mode's sounds and their licence | Stage 6 |
| A session of Slatty's own on a living room's computer (gamescope at login) | After Stage 6 |
| The format of the person's storage for saves and play time, and its retention | Stage 8, by an ADR |
| Whether Ludusavi's manifest, compiled from PCGamingWiki, may be used, and how it is stored | Stage 8 |
| Whether Epic's achievements can be read safely | Stage 8 |
| How Windows updates Slatty: uwui's crate or Slatty's | Stage 3, M3.7 |
| How the network cut keeps the game's link to Comet | Stage 9 |
| How native Linux games are isolated: the Steam Linux Runtime's container, or bubblewrap of Slatty's own | Stage 9, by an ADR |
| Whether mods can be laid over a game inside umu's container; hard links or copies on Windows | Stage 10, by a prototype |
| The pairing and trust between devices, shared with Opium Connect | Stage 11, by an ADR |
| Another store (itch.io, Amazon) | After Stage 10 |
| Games made with SlimeEngine recognised by Slatty | When SlimeEngine ships a game |
| What 1.0 holds | Later |

## Alternatives considered

- **Slatty stopped**, as SlimeEngine's ADR-0203 had it: no more work, but a launcher that does what
  no other does left at 0.2, and no program of uwui's first stage with a gamepad.
- **Slatty over the official clients**, as Playnite does: Galaxy installs and runs GOG's games,
  Slatty shows them. Less to maintain, but no isolation, no control of saves, and Galaxy required.
  Rejected by the owner.
- **Speed at the desk, or safety alone, as the first aim**: the first is what Galaxy 2 aims at too;
  the second is a quality people rarely choose a program for. The owner chose the showcase, and the
  four reasons of weekly use beside it.
- **Only Slatty's entry in Steam on the Deck**: ADR-0013 untouched, but one gamepad layout for every
  game. The owner chose each game in Steam, with guards. **Talking to the running Steam client**,
  as Decky Loader does, to add a game at once: intrusive, and Decky broke three times in 2026 with
  Steam's betas. Rejected.
- **Flathub, with Flatpak's smaller sandbox** (no tools read-only, no mods laid over a game): the
  store, but two levels of safety by package. Rejected unless a prototype shows the same guarantees.
- **One interface that adapts** to the couch, or one mixed: less code, but compromises on both
  sides. Rejected.
- **Saves through a store's cloud alone**, or **device to device alone**: no work, or both devices
  awake at once. The person's storage comes first, devices later.
- **Save locations chosen by hand alone**: no third party, but tedious and error-prone.
- **Moving a save between two stores' versions**: formats and folders differ; a copy can break a
  game. Rejected.
- **Mods out of scope**, as the roadmap said, or **files of mods copied into the game everywhere**,
  as Vortex does: no work, or, on Linux where the sandbox can do better, a game folder that changes
  and fights updates. Rejected.
- **An overlay drawn inside games**: the most comfortable, but a Vulkan layer that is fragile from
  game to game. Rejected.
- **A trailer playing behind a game's page**: lively, but the GPU, the network and the Deck's
  battery spent while nothing is asked. Rejected.
- **Accessibility on the desktop alone**, **translations at Stage 7**, **Slatty's theme format
  kept**: less work now, at a higher cost later or two formats to keep. Rejected.
- **The 2016 laptop kept as the lowest tier**: nothing changes, but nobody has one, so it is never
  measured. Rejected.
- **The state kept as messages and `update`**, with an adapter that feeds uwui's signals, marking
  what changed: little rewritten and less risk of regression, but two models of state in one
  program, and a pilot that tests less of how uwui writes an application. Chosen first, then
  replaced by the owner once asked what Slatty would be if it started today. **The state written in
  signals in `slatty-app`**: against uwui's own rule that a project's logic never depends on it.
  **uwui drawing today's state first**, then signals page by page: the most careful, but an adapter
  written to be thrown away. **The state and the views changed at once**: shortest, but a
  regression with no clear cause. Both rejected.
- **Windows before the Deck**, or **ADR-0025's order kept**: the public of Windows sooner, where
  Playnite and Galaxy are already strong, and nothing that makes the owner open Slatty for a long
  time. Rejected.
- **Every piece native**: one build to test, but a service that breaks or closes needs a change of
  code to leave out.

## Consequences

- [ROADMAP.md](../ROADMAP.md) gains its direction, its new order, Stages 8 to 11, the lowest tier
  and the port's exit, and its open decisions; mods leave its list of what is out of scope.
- [uwui-needs.md](../uwui-needs.md) lists what these needs ask of uwui, the general apart from
  Slatty's own, and says how signals meet each that weighs on a state model, for ADR-0206's check.
  The owner carries it to SlimeEngine and uwui; this repository does not touch theirs.
- Several needs of 0.3 (translation, accessibility at level AA, the tray) are in uwui's plan for
  after its first stage. Slatty's port asks for them sooner, as Opium's first release does; they are
  built in uwui, never worked around in Slatty
  ([ADR-0025](0025-paused-after-m3-3-until-slime-ui.md)).
- Stage 5's first milestone changes `slatty-app` while Iced still draws it: state work, not
  interface work, measured by `slatty-measure`.
- Each new third party (Ludusavi's manifest, Nexus Mods, Thunderstore, ProtonDB, HowLongToBeat,
  RetroAchievements, a WebDAV server) needs its switch and its entry in
  [SECURITY.md](../../SECURITY.md#what-leaves-your-computer) when it is built.
- Writing Steam's shortcuts is the one place where Slatty writes a file of Steam's; Steam's games'
  own files stay out of reach (`paths::check_owned_id`).
- The port grows: the state is written again, not only the views. The rules that `update` holds,
  found bug by bug, move to the pure models with their tests before the views change.
- Each optional piece becomes a Cargo feature. CI builds every piece on and every piece off, not
  each combination.
- The work is large for one developer with agents: about twenty-five features beside the port.
  The order above puts first what makes the owner open Slatty.
- Stage 5's open decisions of ADR-0030 are taken: uwui's theme format, and uwui's gamepads and
  focus.
- The wake-ups at rest, which ADR-0023 left to Stage 4, are found and stopped in Stage 5, whose exit
  is within the quality bar.
