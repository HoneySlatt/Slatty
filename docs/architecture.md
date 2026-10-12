# Architecture

This page is for people reading or changing the code. [AGENTS.md](../AGENTS.md) lists the rules
the code depends on; [SECURITY.md](../SECURITY.md) the protections, from the user's side.

## Overview

```
 slatty-gui (Iced) ─┐                  ┌─ GOG: sign-in, library, content system and CDN,
 slatty (CLI) ──────┼──> slatty-core ──┤        cloud storage, achievements, play time
                    │                  ├─ the Steam client's files (read only), steam:// addresses
 session supervisor ┘                  ├─ umu-launcher → pressure-vessel (in bubblewrap) → Proton
 (/proc/self/exe)                      ├─ steam-run, for native games on NixOS
                                       ├─ Comet and its Galaxy service, beside a game
                                       ├─ GitHub (Proton builds), SteamGridDB, umu's database
                                       └─ disk: keyring, state.db, data, cache, state, config
```

```
crates/core   slatty-core   every behaviour; no interface code
crates/cli    slatty        command line, diagnostics; tests that spawn real processes
crates/gui    slatty-gui    Iced interface
```

The core has no interface dependency and is tested on its own. Both front ends call the same core
functions. Long operations take a `CancellationToken` and report progress through a callback or
typed events (`PlayEvent`, `InstallEvent`, `Progress`); the core returns typed errors
(`error::Error`), and the words the user reads are chosen by the front ends. Either binary can
become the session supervisor (see [Game sessions](#game-sessions)).

## Core modules

**Accounts and network**

| Module | Responsibility |
|---|---|
| `auth` | Sign-in address, code exchange, token refresh, game-scoped tokens |
| `account` | The active account; refresh under a lock shared by every process; avatar |
| `credentials` | The system keyring (Secret Service): GOG tokens, SteamGridDB and Steam Web API keys |
| `secret` | `Secret`, whose `Debug` hides its value |
| `http` | The shared client; reads retried on transient failures; errors stripped of their address; `gog_only` for addresses a GOG answer names |

**Library**

| Module | Responsibility |
|---|---|
| `library` | GOG's library and gamesdb metadata, cached per account; covers, key art and images; whether a product can be installed |
| `overview` | Per game: achievement counts, cloud save support and play time, cached per account |
| `custom` | Titles, sorting titles, covers, backgrounds and hidden games the user chose, apart from GOG's data |
| `steamgriddb` | SteamGridDB search, grids and heroes, once turned on |
| `store` | Which store a game id belongs to: GOG's product id, `steam-<app id>`, `local-<number>` |
| `steam` | Steam's games, read from the client's files: `local`, `appinfo`, `vdf`, `stats`, `running`, `library`, `client`, `web` (see [Steam games](#steam-games)) |
| `added` | Games added by hand (see [Games added by hand](#games-added-by-hand)) |

**Installing and looking after games**

| Module | Responsibility |
|---|---|
| `galaxy` | GOG's content system: builds, build metadata, depot manifests, secure links, chunks from the fastest CDN endpoint (`galaxy::speeds`) |
| `installer` | Install plans (`plan`), staged and verified downloads (`download`), resumable jobs (`job`), install records (`record`); `installer::linux` and `installer::zip` read Linux builds from GOG's offline installers |
| `offline` | GOG's offline installers downloaded whole to keep: the offer by system and language, resumable jobs, downloads checked against GOG's MD5 sums |
| `patches` | GOG's binary patches between builds: lookup, delta download, xdelta3 |
| `install` | Installed games: their record in the database, Proton build, isolation; imports from a folder or from Heroic |
| `maintenance` | Verify, repair, uninstall, updates and content changes (`reconfigure`) |
| `setup` | GOG's post-install setup: install scripts, setup programs, redistributables |
| `gameinfo` | A game's `goggame-<id>.info`; Windows paths resolved case-insensitively |
| `units` | Sizes and speeds as shown (decimal units, megabits a second) |

**Running games**

| Module | Responsibility |
|---|---|
| `runner` | Launch commands for umu and Proton, Wine and native games; prefix creation; isolation |
| `umu` | A game's id in umu's database, so umu applies its fixes |
| `session` | The session supervisor, session records, play time |
| `play` | The whole play flow (see [Play](#play)) |
| `playtime` | Play time read from GOG, sessions reported to GOG |
| `logs` | Logs of the programs the launcher starts, the last run and the two before |
| `protons` | Proton builds downloaded from GitHub (see [Proton builds](#proton-builds)) |

**Saves and achievements**

| Module | Responsibility |
|---|---|
| `cloud` | Save locations (`locations`), local scan (`scan`), three-way plan (`plan`), transport (`transport`), the sync itself (`sync`), diagnostics (`inspect`) |
| `achievements` | Achievement lists, manual unlock and clear |
| `comet` | Comet, supervised beside a game; `comet::Unlocks` reads what it records |
| `galaxy_service` | Comet's dummy `GalaxyCommunication` service, registered in a game's prefix |

**Foundations**

| Module | Responsibility |
|---|---|
| `db` | SQLite state database; versioned migrations, only ever appended |
| `settings` | Every setting, a key/value row with a getter and a setter |
| `paths` | Data folders (`Dirs`), game id checks |
| `fsutil` | Staged writes, file reads of bounded size, work at the lowest priority (`in_background`) |
| `lock` | One operation per game, one interface at a time |
| `error` | Typed errors |
| `doctor` | Checks of the system (`slatty doctor`) and the sandbox check before an isolated launch |

## Command line

`crates/cli/src/main.rs` declares the commands with clap and dispatches them; each group has its
module: `auth`, `library`, `install`, `games` (import, forget, isolation, launch-spec, installs),
`added` (add, remove), `maintenance` (uninstall, verify, update, content, setup), `launch`,
`cloud`, `achievements`, `proton`, `steam`. Its tests (`crates/cli/tests`) start real processes:
the supervisor with detached children, fake games, a fake Steam installation; those that need real
Proton, umu or Comet are ignored by default ([testing](testing.md)).

## Interface

The interface follows Iced's state, message, update and view split:

- `main.rs` holds `App` (the state), the `Message` enum and navigation; `update` only dispatches;
- each feature keeps its state, its messages and its background tasks in a module of its own;
- views live under `ui/`, one module per page or panel.

| Module | Responsibility |
|---|---|
| `boot` | What the first page needs, read on a thread begun as the program starts (see [Start-up](#start-up)) |
| `graphics` | The graphics backends the window starts with |
| `login` | Sign-in, sign-out, avatar; work tied to the account signed in (`account_task`) |
| `library` | Library sync, covers and images, shelves, sort and filters, kept lists and sort keys |
| `scaled` | Covers and key art drawn from copies made at the size they are shown |
| `install`, `downloads` | The Install drawer, the download, the queue |
| `maintenance`, `updates` | Verify, repair, uninstall, content changes; automatic updates |
| `play`, `cloud`, `achievements` | A game session, cloud saves, achievements |
| `steam` | Steam's games beside GOG's |
| `add`, `edit` | Adding a game by hand; editing a game's title and images |
| `runners`, `settings` | Settings → Runners; the rest of Settings |
| `notify` | Cards in the window, desktop notifications |
| `work` | Shared helpers for background work: GOG tokens, throttled progress |
| `theme`, `presets`, `icons` | Design tokens and widget styles, built-in themes, Lucide icons |
| `ui/*` | The views: top bar and layers (`mod`), building blocks (`widgets`), pages (`library`, `game`, `achievements`, `settings`, `downloads`), drawers and dialogs (`install`, `game_settings`, `manage`, `cloud`, `edit`, `add`, `panels`), text shown to the user (`format`), `even_grid`, `pointer` |

Views never name a colour: every colour, radius and motion comes from `theme::tokens()`, a built-in
theme (`presets`) with `~/.config/slatty/theme.toml` over it ([theming](theming.md)).

Dialogs and menus are layers over the page, each over an empty layer, and the notice banner keeps
its place: the page stays the same widget whatever opens over it, so Iced keeps its state (the
library's scroll position).

### Start-up

The window and the graphics driver take about 100 ms to set up. `boot::start` reads what the first
page needs meanwhile, on a thread begun as the program starts: the database, the account, the
library cache, settings. It reads nothing from game folders or Steam libraries, which can sit on a
network drive taking seconds to wake up, nor from the keyring, whose prompt would hold it: how
each game starts, the Proton builds found there, and whether keys are saved are read once the
library shows. The system's fonts are read on another thread begun at the same time.

`main` reads the theme, the font and the graphics backend before the first frame, so the window
never shows another look first. One interface runs at a time: `boot` takes
`locks/interface.lock`, and a second window says SlattyLauncher is already open. The command line
still runs beside it.

`graphics` chooses the backends: Iced has wgpu start every backend and pick the best adapter. Where
that is a Vulkan GPU anyway (not drawn by the processor), the next starts set `WGPU_BACKEND=vulkan`
before any thread runs (`settings::vulkan_only`), which shows the library sooner and takes less
memory. Five seconds after each start, `boot`'s thread checks that the backend still finds an
adapter; one that stopped working costs one start drawn in software, then every backend is started
again. A `WGPU_BACKEND` the user set is left alone.

### Work for an account

Work started for the signed-in account (`App::account_task`) carries the sign-in it began in, and
its result is dropped once that sign-in has ended: a late answer never fills the page of the next
account, nor that of the same account signed in again. For an account forgotten since
(`App::forgotten`), what that work wrote into the account's cache as it ended is deleted again
(`App::forget_again`). Such work, and a game session's cloud, achievement and play time steps,
load credentials with `Account::load_for`, which refuses when another account is now signed in.

### Staying fluid at 10,000 games

- **Only the rows in view are built.** The cover grid and the Achievements grid build the rows in
  view and one row of margin (`library::GridWindow`), with one child per row of the whole grid
  (`ui::widgets::grid_rows`), a space of the same height when out of view: a row still in view
  after a scroll is the same child, and Iced keeps what it laid out for it (titles in scripts the
  font lacks are slow to shape).
- **Lists are kept.** The games each tab lists (`library::Lists`) are kept until a message that
  can change them; those that cannot (scroll steps, frames, covers, progress…) are in
  `Message::keeps_lists`. Sort keys are made once per title (`library::SortKeys`), at start off
  the interface thread.
- **Images come from files.** Covers and images are cached as files named after their format and
  shown with `image::Handle::from_path`: the interface holds no image bytes. At start the covers
  already cached are found in one pass over their folder and shown in one message.
- **Images are drawn at their size.** Iced draws an image larger than its place by sampling four of
  its pixels, without smaller copies to read from, and enlarges a smaller one by blending, softly.
  `scaled` makes each cover once at the card's width in pixels (the card's width times the
  window's scale), with Lanczos3, at the lowest priority, four at a time, those in view first, in
  `scaled/<width>/` of the cover's cache; copies of the size before stay, for a window moved
  between screens. The game page's key art is made at the window's width rounded up to 1920, 2560
  or 3840 pixels; narrower art is enlarged with Lanczos3 and sharpened with AMD's RCAS (FSR 1),
  up to a 4K screen's pixels. Key art copies take at most 2 GiB, those shown longest ago going
  first, never one shown in this session. Copies of GOG's images go into the account's cache, so
  they go with it.

`library_at_10000_games` (an ignored test) measures the views; [testing](testing.md) gives the
references measured in the application itself.

## Rules every part follows

- **Game ids.** A game id names files (locks, records, prefixes, covers), so it is checked before
  any path is built from it. `store::of` accepts GOG's product id, `steam-<app id>` and
  `local-<number>`, and refuses anything unsafe in a file name. `paths::check_owned_id` accepts
  GOG's and those added by hand, and guards whatever installs, records or deletes a game's files:
  a Steam game's files, which Steam owns, never are. `paths::check_game_id` accepts GOG's alone and
  guards every request to GOG about a game, so GOG never learns of the others.
- **One operation per game.** Installing, discarding an install, verifying or repairing, updating
  or changing content, uninstalling, syncing cloud saves (checking does not), running setup and
  playing each hold `locks/game-<id>.lock`. A second one on the same game, from the same process
  or another (command line and interface), is refused rather than touch files in use. A game
  started from SlattyLauncher keeps it busy until its last process ends, even once the launcher
  closed or crashed: the session supervisor holds `session-<id>.lock` while it runs.
- **Staged writes.** Files are written under a temporary name, then renamed
  (`fsutil::write_atomic`, `temp_sibling`). A game folder appears only once every file is
  verified. Nothing is written through a symbolic link that leads out of the game folder
  (`installer::refuse_outside`, `leads_inside`), and every path from GOG goes through
  `installer::safe_relative`.
- **Destructive actions refuse or keep a copy.** Only files SlattyLauncher installed (its install
  record) are ever deleted from a game folder; saves are copied before they are replaced.
- **Network behind traits.** `galaxy::ContentSource`, `cloud::CloudTransport` and
  `installer::linux::Source` have in-memory fakes with fault injection, so tests need no network.
- **Settings** are key/value rows, each with a getter defaulting sensibly and a setter in
  `settings.rs`. The database changes only by appending a migration to `db::MIGRATIONS`.

## Main flows

### Sign-in

GOG accepts only its Galaxy client's redirect address (`https://embed.gog.com/on_login_success`).

1. The system browser opens GOG's sign-in page.
2. The user pastes the address they land on; SlattyLauncher takes the code from it.
3. The code is exchanged for tokens, which go to the keyring.

Refreshes take a file lock, so the command line and the interface never refresh at once; GOG does
not rotate the refresh token. A download that outlasts the access token (about an hour) renews it
the same way before asking for new links.

### Library and overview

`library` reads the account's products from `galaxy-library.gog.com`, then each game's metadata
and images from gamesdb, and caches them per account; a game gamesdb fails to describe during a
sync keeps what an earlier sync got. In the background, `overview` asks, for the games not read
yet, whether they have achievements and cloud saves (through a token for the game, as Galaxy does)
and their play time, and keeps the answers; the Achievements tab's **Refresh** reads every game
again.

### Install

1. `plan_for` picks the build: the one chosen or kept by an interrupted install, else the newest
   public Windows build of generation 2. It reads its metadata and keeps the depots of the base
   game and of the chosen owned DLC (all by default) for one language; ownership comes from
   `embed.gog.com/user/data/games`.
2. `collect_files` reads the depot manifests. It refuses unsafe paths, merges paths that differ
   only by case, and leaves "support" files and links aside.
3. `Download::run` checks the disk space, then fills `.<Game>.slatty-partial`:
   - chunks come from the fastest of the CDN endpoints GOG lists, each measured on a first request;
     a chunk still waiting after three times the usual time is asked again from another endpoint,
     one such copy at a time, and the first answer wins;
   - two chunks per file are fetched at once and written at their offset as they arrive;
   - each chunk is checked against its compressed and decompressed MD5;
   - a file already there (a repair, an update, a resumed install) is hashed at the new chunk
     offsets, and chunks that match are copied from it instead of downloaded;
   - files are checked four at a time, a file of 64 MiB or more in parts on up to four threads,
     each on a thread at the lowest priority (`fsutil::in_background`);
   - each file is written under a temporary name and renamed. Files of 1 MiB or more are synced to
     disk before taking their name (`SYNCED_FROM`), so a resumed install keeps one of the right size
     without reading it again; smaller ones, quick to read, are checked again. The file system is
     synced once at the end for the rest;
   - a file that fails does not stop the others, except on a full disk.
4. The partial folder is renamed to the game folder.
5. The install record (build, language, files) is saved, and the game registered with its prefix,
   `prefixes/<id>`.

A **Linux build** takes another path (`installer::linux`), since GOG's content system has none:

1. `plan_linux` reads the game's and its DLC's Linux installers from
   `api.gog.com/products/<id>?expand=downloads,expanded_dlcs` and picks one per product for the
   language. An installer in several parts is left out. Its build id is `linux:<version>`, which is
   how a job tells its platform.
2. Each installer is a MojoSetup script followed by a zip. GOG's CDN serves byte ranges but not
   suffix ranges, so the exact size comes from the `Content-Range` of the first byte.
   `installer::zip` finds the zip's directory in the last 64 KiB (ZIP64 too) and shifts every offset
   by the script's length.
3. Only entries under `data/noarch/` are kept; a DLC's file replaces the game's at the same path.
   `LinuxDownload` fetches each file with a ranged request (eight at once), inflates it off the
   async threads, checks its size and CRC-32, and writes it as above. A dropped connection or an
   expired link (401, 403, 410) is tried again. Links out of the game folder are never made.
4. The staged folder, its publication, the record and the job are a Windows build's. The game runs
   through `start.sh` with `Runner::Native` (see [Play](#play)).

### Interrupted and queued installs

An interrupted install keeps its job in the database and resumes with the same build, language
and folder. Its state tells a pause asked for (`paused`, resumed when asked) from a cut-off
(`downloading`, resumed by the interface at start-up). An install queued in the interface waits as
a `queued` job with the choices of its panel; the queue's order is a setting.

The partial folder gets a mark (`.slatty-staged`, holding the game's id and the build) as its
download starts, which the rename carries into the game folder and the registration deletes:

- a folder already where the game goes without it (another launcher's copy, a modded one), or with
  another game's id, is never adopted, only refused;
- a download whose mark names another build than the one planned (GOG replaced a Linux installer)
  starts again from nothing: not all of it is the new installer's, and none of it would be
  recorded;
- a cut-off after the rename but before the registration is finished by checking the game folder
  in place.

A job whose folder is gone (a network share not mounted, a drive unplugged) stops as failed rather
than start again on the system disk. A download stopped because GOG could not be reached
(`Error::is_temporary`: network, 429, 5xx) stays `downloading` and holds the queue, asked again
every minute; any other error leaves it `failed`.

### Post-install setup

`setup::run` does what Galaxy does after an install, once per installed build, from the play flow
once the prefix exists, or with `slatty setup`:

1. Missing support files go to the game's support folder; shared dependencies, and GOG's script
   interpreter when the build uses it, go to `redist/`.
2. For the base game and each DLC, GOG's script interpreter runs when the build asks for it,
   otherwise the product's setup program, with Galaxy's arguments.
3. Shared redistributables are installed silently.

Commands are built by a pure function (`setup::commands`) and run under the session supervisor,
each finished before the next. A step fails on any exit code but 0 and the Windows Installer codes
that mean done (1638, 1641, 3010, and their low bytes through Wine). A run with a failed step leaves
the build not set up, so setup runs again at the next launch, up to `setup::SETUP_RUNS` (3) runs.
Programs run in a game's prefix get `PROTONFIXES_DISABLE=1`: a game's fixes can add arguments to
whatever runs with its id, which setup programs refuse.

### Updates, language, DLC and build changes

`maintenance::reconfigure` handles four changes the same way: a newer build (`Change::Update`),
another build GOG offers, older or newer (`Change::Build`), another language, and another set of
DLC.

It records an `updating` job with the target just before the first file changes, so a change
refused or paused before (not enough space, GOG not answering) leaves the game playable. Then the
new file list is filled in place with the install's routine, GOG's binary patches applied where
GOG offers them, files the new build no longer has deleted if SlattyLauncher installed them, and
the install record rewritten. While the job exists, launching and verifying the game are refused,
and running the change again resumes it with the pinned build.

Automatic updates (`gui/src/updates.rs`) take the games SlattyLauncher installed that are not held
on an older build (`maintenance::auto_update_candidates`), check them at start, at sign-in and every
six hours, and apply them one at a time through Manage's own update, once no game runs, no install
downloads and no cloud sync or other work on a game is busy. Whether a game is held back is decided
when its build is chosen: never when a queued or interrupted install resumes, by which time GOG may
list a newer build.

### Verify, repair, uninstall

Verify hashes every installed file against the installed build's manifest; repair downloads only
the damaged chunks of damaged files. Uninstall deletes the files in the install record and nothing
else, then folders left empty; the prefix stays unless asked, and then its `users` folder, Wine's
registry files and `C:\ProgramData` are copied to `backups/prefixes/` first. Verify, repair, updates
and content changes refuse a game whose folder is missing or empty (its drive not mounted).

### Play

`play::play`, for a game installed or added:

1. The game's id in umu's database (`umu::resolve`), from its Proton build's own copy, or umu's
   server when **Proton fixes online** is on.
2. An unfinished update is refused.
3. For an isolated game, umu's runtimes are refreshed (at most once a day) and the one the launch
   needs installed, outside the sandbox.
4. The launch command is built and checked first, so a game that cannot start says so first.
5. At the first launch, the Wine prefix is created (`wineboot`), then the post-install setup runs.
6. With Comet, its Galaxy service is registered in the prefix if it is not yet.
7. Cloud saves are synced; a conflict or an error stops the launch, the cloud out of reach does not.
8. Comet starts, for a Windows game that ships the Galaxy SDK and once **Achievements in game** is
   on (`comet::uses_galaxy`, `settings::game_achievements`).
9. The game runs under the session supervisor; Comet's unlocks are read as they come.
10. After the game: Comet stops, cloud saves are uploaded (not when the session's end is
    uncertain), play time is reported, and the achievements GOG recorded are compared with those
    before.

GOG is never asked anything about a game added by hand, and Linux builds skip setup, cloud saves and
Comet.

A **native Linux game** runs through `start.sh`: on NixOS inside `steam-run` when present (which
mounts an empty `/tmp` of its own, so temporary folders under `/tmp` become `/tmp`,
`runner::steam_run_temp`), else inside umu's Steam Linux Runtime 3.0 through a compatibility tool of
SlattyLauncher's own (`runner::runtime_tool`) rather than umu's `UMU_NO_PROTON`, which asks GitHub
at each launch; on other systems through the interpreter its first line names.

### Game sessions

A launch starts the running binary again (through `/proc/self/exe`, which still works after the
file was replaced by an update or a rebuild) with a hidden argument. That copy becomes the session's
supervisor: it marks itself `PR_SET_CHILD_SUBREAPER`, starts the game in its own process group and
reports events as JSON lines. Processes that detach (launchers that exit early, Wine services) are
re-parented to it, so the session ends only when no descendant is left. umu's `waitforexitandrun`
already waits for Windows processes; the supervisor is the safety net for everything else.

The session's length is measured on a monotonic clock, without the time the computer was
suspended. A session without a recorded end (a crash) is reported at the next launch, and the
upload after it is skipped.

### Game isolation

An isolated game (`Install::isolated`; Windows builds run through Proton by default) runs in the
container umu already uses, pressure-vessel from the Steam Linux Runtime. umu starts a Proton build
in it only when the build's `toolmanifest.vdf` names a runtime; `runner::container_of` refuses a
build made for the host (Proton-tkg) for an isolated game.

**What the container is told** (`runner::isolation_env`):

- `PRESSURE_VESSEL_HOME`: a home folder of the game's own, `homes/<id>`, mounted where the user's
  is; `HOME` set to it as well, since pressure-vessel shares the Steam installation it finds there;
- `PRESSURE_VESSEL_FILESYSTEMS_RW`: the game folder; `PRESSURE_VESSEL_FILESYSTEMS_RO`: umu's data,
  the redistributables and the game's support files; the prefix is shared by umu itself;
- `TMPDIR`, `TMP`, `TEMP`, `TEMPDIR` set to `/tmp`, private in the container (it shares the
  folders they name);
- `DBUS_SESSION_BUS_ADDRESS` and `DBUS_SYSTEM_BUS_ADDRESS` set to `disabled:`, so neither bus is
  shared;
- `PRESSURE_VESSEL_SHARE_HOME=0`, `PROTON_LOG_DIR` in the game's home, and every `STEAM_COMPAT_*`
  path variable, `STEAM_EXTRA_COMPAT_TOOLS_PATHS` and `STEAM_RUNTIME_SCOUT` emptied: the launch
  inherits the user's environment, and pressure-vessel turns each of these into a writable mount;
- for a Linux game, `WINEPREFIX` set to the game's home, which umu would otherwise take from
  `~/Games/umu/<id>` and share;
- `PRESSURE_VESSEL_SHARE_PID=0`: a process namespace of its own, so the game sees and signals only
  its own processes.

A Valve Proton build needs `$HOME/.steam/sdk64/steamclient.so`: the game's home gets `.steam/sdk64`
and `sdk32` folders with links to Steam's `linux64` and `linux32`, shared read-only
(`runner::steam_sdk_links`). Links the game itself put in its `.steam` are removed before each
launch (`runner::forget_steam_links`).

**The tools stay read-only.** umu shares the tools a game runs through with its container,
writable: its runtime, the Proton build, the runtime tool of a Linux game. An isolated launch
therefore runs umu inside bubblewrap (`runner::read_only_tools`): the system as it is, with those
bound read-only over themselves, by where they lead (`runner::bind`: bubblewrap cannot mount over a
link, which a `<project>-latest` build is). Each runtime's `var/`, its `.ref` lock files and a Valve
build's `dist.lock` stay writable; what a Valve build's script writes into its own folder after a
Steam update (SteamPipe's fixups) is done before the launch, outside the sandbox
(`runner::set_up_valve_build`). pressure-vessel then cannot hard-link the runtime and copies it
(reflinked on btrfs and XFS, written in full on ext4). umu cannot update a runtime it sees
read-only, so isolated launches set `UMU_RUNTIME_UPDATE=0`, and `runner::refresh_runtimes` updates
it at most once a day, outside the sandbox.

**Before each isolated launch**, `runner::sandbox` tries bubblewrap (`doctor::sandbox_problem`, a
few milliseconds): a sandbox, then one inside it. On Ubuntu 23.10 and later, without an AppArmor
profile, either can fail: the launch is refused with the reason, never run without isolation.

**What umu would share by itself.** umu shares, writable, the whole filesystem a game is on (the
first mount point above `STEAM_COMPAT_INSTALL_PATH`, else the program's folder). An isolated game
is therefore started without that variable and never by a path umu can open: a Windows program by
its `Z:\…` path, a Linux script through its interpreter (`sh`, `bash` or `dash` only). umu also
looks for its first argument relative to the working directory and the home folder: a file named
`sh`, `sc`, `wineboot` or `Z:\…\game.exe` the game wrote in its folder would make umu take the
folder for the game's. pressure-vessel shares the working directory as it is on disk, and splits
its path lists on every `:`. `runner::check_shared` refuses all of these for every command run
isolated, just before it starts.

**Outside the container**, the launcher itself works in the prefix and the game folder, where a
link the game planted could lead it out. For an isolated game, cloud sync refuses a save folder
that leads out of the prefix or game folder, the Galaxy service registration refuses to copy
through such a link, and an uninstall refuses to back up or delete a prefix whose `users` folder
leads out of it.

The ignored tests in `crates/cli/tests/isolation.rs` run each check through the real container,
with the same launch without the measure, or not isolated, as a control.

### Cloud sync

See [cloud-saves.md](cloud-saves.md) for the rules. The decision (`cloud::plan`) is a pure
function. The sync (`cloud::sync`) works against the `CloudTransport` trait: `GogCloud` for real
use, an in-memory transport with fault injection for the tests.

### Comet

Comet listens on the fixed port 127.0.0.1:9977, so only one runs at a time.

- **Tokens.** SlattyLauncher passes them through Comet's Lutris importer: a 0600 file in a private
  folder under `$XDG_RUNTIME_DIR`, removed as soon as Comet listens, never in process arguments.
  `$XDG_RUNTIME_DIR` must belong to the user and be closed to others (`comet::private_dir`). Comet
  is taken as ready only once a listener of the user's own holds the port (`/proc/net/tcp`).
- **Exposure.** Comet hands the account's access token to anything that connects and asks, which
  is why it runs only once allowed. `COMET_LOG` keeps out of its log the messages that would hold
  the refresh token.
- **Shutdown.** Comet stops with SIGINT when the game ends, when the launcher drops it, and when the
  launcher dies (`PR_SET_PDEATHSIG`). It runs in a process group of its own, so a Ctrl+C in the
  terminal reaches the launcher only.
- **Unlocks as they come.** Comet records an unlock in its SQLite database at once and sends it to
  GOG later. `comet::Unlocks` opens that database read-only, never waiting on Comet's locks, and
  looks each second; `PRAGMA data_version` tells whether Comet wrote since. What was unlocked on
  GOG at the start, or dated more than a minute before it, is not new.
- **Galaxy service.** Some Galaxy SDK versions reach Comet only when a `GalaxyCommunication`
  Windows service exists. Before the first session with Comet, the play flow registers Comet's
  dummy service in the prefix (`sc create`, the `GalaxyClient\paths` registry value), then copies
  the executable to `C:\ProgramData\GOG.com\Galaxy\redists\`; the copy comes last, so its presence
  means the registration completed.

### Steam games

`core/src/steam` reads the local Steam client and never writes to it:

- `local` finds Steam (native or Flatpak; with both, the one signed in to last), its library
  folders, the account last signed in (`config/loginusers.vdf`), its games
  (`userdata/<account>/config/librarycache`), those installed (`appmanifest_*.acf`), and each
  game's play time and last session from `localconfig.vdf`, of which nothing else is kept: it also
  holds the client's download tokens;
- `appinfo` decodes `appcache/appinfo.vdf` (binary KeyValues, versions 40 and 41), only the apps
  asked for, and of them only the name, type, systems and images; `vdf` reads Steam's text
  KeyValues, bounded in size, depth and entries;
- `stats` reads a game's achievements from the client's stats files;
- `running` finds the games the client runs, from the user's own processes named `reaper` started
  from the client's folders, as `reaper SteamLaunch AppId=<id>`; no other process's arguments are
  read;
- `library` turns them into library games, and `twins` pairs a GOG game with the Steam game it also
  is: by the Steam app ids gamesdb gives for it (`game.releases`), or, for a game gamesdb did not
  describe, by a title written alike;
- `client` builds the `steam://` addresses that ask the client to play, install, verify or
  uninstall;
- `web` asks Steam's Web API, once turned on, with the user's key from the keyring.

Every file is opened without waiting and read only when it is a regular file of bounded size
(`steam::open`); cached images only when they are not links.

In the interface (`gui/src/steam.rs`), Steam's games are read at start off the interface thread
and merged into the library (`merge_steam`), which leaves out a Steam game owned on GOG too: its
GOG game's page offers it. What Steam installed and its play times are read again when the window
comes back to the front. After Play, and while a game runs, the client's games are looked at every
two seconds. What the client downloads comes from each game's manifest and from what Steam wrote so
far in `steamapps/downloading/<app id>`, every five seconds while something moves, every thirty
while the window is in front, not at all while it is behind.

### Games added by hand

`core/src/added.rs`: an install with the id `local-<number>`, from a count in the settings never
given twice, and what it starts (`added/<id>.json`: program, arguments, working directory, relative
to its folder, checked to be still in it at each launch). There is no install record, which would
have GOG's setup, updates and checks run.

- `platform_of` tells a Windows program (`MZ`) from a Linux one (ELF, `#!`), whatever its name.
- `check_folder` refuses a folder an isolated game could reach more through: one holding the home
  folder or the whole disk, a folder of programs, another game's folder, one inside or holding it,
  one holding the default installation path (paths compared as resolved on disk).
- `run_installer` runs a Windows installer under the session supervisor in a prefix made for the
  game; `NewGame::installed_by` then adds the program it installed with that id and prefix, or
  `discard_installer` throws it away, saves backed up.
- `unpack` unpacks a Linux installer or archive without running it (a MojoSetup installer's
  `data/noarch` through `installer::linux`, a zip whole, a tarball through `protons::unpack_tar`)
  and lists the files with their size and modification time; `remove` deletes those alone, and
  only while they are as they were unpacked, never the game's other files.

`runner::launch_spec` starts the program as a GOG build's (Windows) or script (Linux; an ELF
program isolated through `sh -c`, an AppImage told to unpack itself).

### Proton builds

`core/src/protons.rs`, once **Proton downloads** is on:

- the archive for this computer is downloaded from the project's GitHub release, resumed when cut
  off, checked against the release's SHA-512 sum before it is opened, then unpacked in
  `protons/.staging` (a `.tar.xz` made in blocks on up to four threads), nothing leaving its folder
  (paths that climb out, links out, special files are left out), and moved into place;
- each project has a `<project>-latest` link to its newest build, replaced in one step (`rename`)
  once a newer build is unpacked, never by an older version (the numbers in the names compared);
  `link_newest`, at start, gives a project without a link, or whose build was deleted, one to its
  newest build;
- `update` (once a day at start, once turned on) downloads the newest build of each project
  followed that way, keeps the build before and deletes older ones no game uses; a project that
  fails is reported beside the others;
- one lock (`protons.lock`) covers each change of the builds or links, so the interface and the
  command line never interleave;
- a build a game, an unfinished install or the default uses, or that a followed link leads to, is
  not deleted;
- `sync_shared`, once **Share with Steam and Heroic** is on, keeps one link per downloaded build in
  native Steam's `compatibilitytools.d` (numbered builds only; a name already taken is left alone)
  and removes only links leading into the builds folder. A failure there only leaves builds out of
  Steam and Heroic.

## Storage

Paths follow the XDG variables; the defaults are shown.

| Kind | Location | Rebuilt from |
|---|---|---|
| Secrets | System keyring | nowhere (sign in again) |
| State | `~/.local/share/slatty/state.db` (accounts, installs, jobs, sessions, sync history, settings, customisations) | nowhere |
| Install records | `~/.local/share/slatty/manifests/` | GOG, for the same build |
| Prefixes, isolated games' homes, backups | `~/.local/share/slatty/{prefixes,homes,backups}/` | nowhere |
| Games added by hand | `~/.local/share/slatty/added/`, `custom/` (user's images) | nowhere |
| Setup files | `~/.local/share/slatty/{support,redist}/` | GOG |
| Proton builds, runtime tool | `~/.local/share/slatty/{protons,umu-runtimes}/` | GitHub; made again |
| Comet | `~/.local/share/slatty/comet/`, `~/.config/slatty/comet/` | GOG |
| Cache | `~/.cache/slatty/<user id>/` (GOG), `~/.cache/slatty/{steam,steamgriddb,scaled,art}/` | the network, or made again |
| Logs and locks | `~/.local/state/slatty/{logs,locks}/` | made again |
| Theme | `~/.config/slatty/theme.toml` | the user |

## Design decisions

| Decision | Why |
|---|---|
| Rust everywhere in the application; Proton, umu and Comet stay external | Proven tools reused rather than rewritten, with explicit boundaries |
| Iced rather than GPUI (reviewed 2026-10-09) | Maintenance comes first: Iced has versioned releases and documentation; GPUI had no maintained release, and its ecosystem pinned weekly third-party snapshots |
| No web view; the address pasted back after sign-in | A native application; GOG accepts only Galaxy's redirect address, and no local redirect is known to work |
| Linux builds from GOG's offline installers, read file by file | GOG's content system has no Linux builds; ranged reads keep the installer off the disk |
| Isolation through umu's own container, with bubblewrap around umu | pressure-vessel already runs every Proton game and keeps GPUs, sound and gamepads working; its options hide the home folder. Bubblewrap only makes the tools read-only. It keeps games out of files and D-Bus, not out of the system |
| Proton builds downloaded by SlattyLauncher, not by umu | umu fetches them at a game's launch, which would wait for half a gigabyte, and replaces its "latest" in place under a running game. A folder per build, followed through a link umu resolves at launch, keeps a running game on its build and the build before at hand |
| Comet as a supervised process beside the game, off until turned on | Its library is not meant for embedding (global state, fixed port); while it runs, any local program can get the access token from it |
| A subreaper supervisor process for sessions | Launchers that exit early and Wine processes must not end the session too soon |
| Content hashes, not dates, for cloud sync | Dates are unreliable across machines and Wine; hashes and per-file history show real changes |
| Steam read from its files, played through its client | Most Steam games need the client running; reading never writes into Steam or asks its servers without a switch |
| GOG never told of other stores' games | GOG learns nothing of what you own elsewhere |
| Images shown from files, made at the size drawn | The interface held 880 MB of covers at 10,000 games when it kept their bytes; Iced has no smaller copies to sample from |
| Grids that build only the rows in view | A library of 10,000 games stays within a frame |
| SQLite through rusqlite behind a mutex | One writer per process, simple synchronous queries, futures stay `Send` |
| Interface in English first | Translations come later |
| GPL-3.0-or-later | Allows building on heroic-gogdl's GPL code with attribution |

## Security principles

The protections are told from the user's side in [SECURITY.md](../SECURITY.md); in the code:

- no password field; tokens only in the keyring (Comet's short-lived 0600 handoff file aside), never
  in process arguments or logs, wrapped in `Secret`;
- network errors stripped of their address (`Error::network`), since some GOG endpoints take tokens
  in the query string; the token sent only to `https://` addresses on `gog.com` (`http::gog_only`);
- SlattyLauncher's folders 0700 and its database 0600, set again at every start;
- every request SlattyLauncher itself sends to a host other than GOG's has a switch in Settings,
  off until turned on, and a line in SECURITY.md, which also lists what the programs it starts
  contact (umu asks Valve about its runtime, Comet talks to GOG);
- no telemetry.
