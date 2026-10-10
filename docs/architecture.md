# Architecture

## Crates

```
crates/core   slatty-core   all behaviour: GOG services, installs, sessions, cloud, Comet
crates/cli    slatty        command line, diagnostics, real-service integration tests
crates/gui    slatty-gui    Iced interface
```

The core has no user-interface dependency and is tested on its own. Both front ends call the same
core functions. Long operations report progress through callbacks or typed events (`PlayEvent`,
`InstallEvent`) and honour a `CancellationToken`. Either binary can act as the session supervisor
(see [Game sessions](#game-sessions)).

## Core modules

| Module | Responsibility |
|---|---|
| `auth` | Sign-in URL, code exchange, token refresh, game-scoped tokens |
| `account` | Active account, cross-process locked refresh, keyring storage via `credentials` |
| `credentials` | System keyring (Secret Service through the `keyring` crate): GOG tokens, and API keys such as SteamGridDB's |
| `http` | Shared HTTP client; reads that fail on the way (dropped connection, timeout, rate limit, server error) are sent again twice; errors are stripped of URLs so query-string secrets never reach logs |
| `db` | SQLite state database with versioned migrations |
| `library` | Galaxy library and gamesdb metadata, per-account cache, covers, key art and images, whether GOG offers anything to install |
| `gameinfo` | `goggame-<id>.info` parsing, case-insensitive Windows path resolution |
| `install` | Installed-game records, Proton build of each game |
| `galaxy` | Content system: builds, build metadata, depot manifests, secure links, chunks from the fastest CDN endpoint |
| `installer` | Install plans, staged verified downloads, resumable jobs, install records; `installer::linux` and `installer::zip` read Linux builds file by file from GOG's offline installers |
| `maintenance` | Verify, repair, uninstall, updates and content changes |
| `patches` | GOG's binary patches between builds: lookup, delta download, xdelta3 application |
| `runner` | Launch commands for umu/Proton, Wine and native games; prefix creation; isolation (see [Game isolation](#game-isolation)) |
| `umu` | Game id in umu's database, looked up once per game, so umu applies its fixes for it |
| `protons` | Proton builds downloaded from GitHub releases (GE-Proton, Proton-CachyOS, UMU-Proton), once turned on: the archive for this computer, resumed downloads, its published SHA-512 sum checked before it is opened, then unpacked without anything leaving its folder (paths that climb out, links out, even through another link, and special files are left out). Each project has a `<project>-latest` link to its newest downloaded build, replaced in one step (`rename`) once a newer build is unpacked, never by an older version (the numbers in the names, in order, are compared); games and the default can be set to it (`link_newest`, at start, gives builds downloaded before the links one to the build unpacked last). `update` (at most once a day at start once turned on) downloads the newest build of each project followed that way, keeps the build the link led to before and deletes older ones no game uses. A build a game or the default uses, or that a followed link leads to, is not deleted |
| `session` | Session supervisor (subreaper), session records, play time |
| `play` | Full play flow: prefix, cloud, Comet, session, upload, achievement diff |
| `cloud` | Save locations, local scan, three-way plan, transport, sync executor, diagnostics |
| `comet` | Supervised Comet process |
| `galaxy_service` | Comet's dummy `GalaxyCommunication` service, registered in game prefixes |
| `achievements` | Achievement list, manual unlock and clear |
| `overview` | Per-game achievement counts and cloud save support, cached per account |
| `playtime` | Play time read from GOG, finished sessions reported to GOG |
| `settings` | Default installation path, default Proton and platform, favorites, the download queue, privacy switches (isolation of new games among them), manual achievement changes, launch options |
| `custom` | Titles, sorting titles, covers and backgrounds the user chose, and the games they hid, apart from GOG's data; chosen or downloaded images are copied into the data folder |
| `steamgriddb` | SteamGridDB, once turned on: games searched by name, their grids (covers) and heroes (backgrounds), images downloaded from its servers only; the user's API key in a header |
| `paths`, `fsutil`, `lock`, `secret`, `error`, `doctor` | Shared utilities |

## Interface modules

The interface follows Iced's state, message, update and view split. `main.rs` holds the
application state, the `Message` enum and navigation; `update` only dispatches. `boot.rs` reads
everything the first page needs, off the interface thread. Each
feature keeps its state, message handling and background tasks in its own module, and the views
live under `ui/`.

Work started for the signed-in account (`App::account_task`) carries the sign-in it started in,
and its result is dropped once that sign-in has ended: a late answer never fills the page of the
next account, nor that of the same account signed in again. Such work, and a game session's cloud,
achievement and play time steps, load credentials with `Account::load_for`, which refuses when
another account is now signed in.

| Module | Responsibility |
|---|---|
| `login` | Browser sign-in, sign-out, avatar |
| `library` | Library sync, covers and images (cached as files named after their format, `<id>.jpg` or `<hash of the address>.jpg`, and shown from the file: the interface holds no image in memory; key art wider than 2560 pixels is scaled down once, in the cache), favorites, shelf, sort (numbers by value) and filters, per-game overview and play time. Only the rows of the cover grid in view are built (`GridWindow`, a row of margin each side), and the Achievements tab does the same: at 10,000 games they build in about 2 and 2.5 ms, the order key of each title made once (`SortKeys`). Going back from a game page scrolls the grid to where it was left (`App::library_scroll`, then `scroll_to` on `library::GRID`); another page shown again starts at its top. Each row of the whole grid is one child of its column (`ui::widgets::grid_rows`), a space of the same height when out of view, so a row still in view after a scroll keeps what Iced laid out for it: titles in a script the font lacks (Chinese, Japanese) are slow to shape again. `library_at_10000_games` (ignored test) measures both |
| `play` | Launching a game and following its session |
| `cloud` | Cloud save check, sync and conflict choices |
| `achievements` | Loading achievements, confirmed manual changes |
| `install` | Install plan, download with progress, pause, discard |
| `downloads` | The download queue: installs waiting as `queued` jobs, started one after the other, reordered by dragging (`ui/downloads.rs` for the tab) |
| `maintenance` | Verify, repair, updates, uninstall, language and DLC changes |
| `updates` | Automatic updates: games from `maintenance::auto_update_candidates` (installed by slatty, not held on an older build chosen) checked at start and every six hours (one request each for a Windows build: GOG's list of builds), then applied one at a time through Manage's own update, started after any message once no game runs, no install downloads and no other work on a game is busy |
| `settings` | The Settings page (its side list follows the scroll), defaults for installs, privacy switches, manual achievement changes (Advanced, off until turned on), the Proton build, launch option and isolation of each installed game |
| `runners` | Settings → Runners: the Proton downloads and Keep up to date switches, the newest build of each project on GitHub, a download or an update with its progress and Stop (an update due starts with the application), the downloaded builds with Delete; the Proton menus are listed again once a build is added or deleted |
| `edit` | The menu a right click on a cover or on the key art of a game page opens, and the edit form, a dialog over the library and a drawer on the game page: draft, file picker, SteamGridDB search (name, game, picture), saving. `ui/pointer.rs` reports where a right click happened, without a message per mouse move |
| `work` | Shared helpers for background work: GOG tokens, throttled progress streams |
| `ui` | Window shell (top bar, notices, quit dialog); `widgets` for the building blocks every page uses (logo, tabs, avatar, cards); `library`, `achievements`, `settings`, `game` and `panels` pages, `install` for the Install drawer and dialog, `game_settings` for the Game settings drawer and dialog (library dialogs share `panels::library_dialog`), `manage` and `cloud` for their drawers (drawers share `panels::drawer`); `format` for text shown to the user |
| `theme`, `presets`, `icons` | Design tokens (`Tokens`: every colour, the corner radii of the redesigned pages, and the page transition: a short fade with a slight rise, played when the page changes and set to zero to turn it off) and the widget styles built from them; Lucide icons. Views never name a colour; the tokens come from a built-in theme (`presets`), with `~/.config/slatty/theme.toml` on top when it exists, read at start, on Reload and when the theme changes (see [theming](theming.md)) |

Interface tests (`tests/`, one module per area, helpers in `tests/mod.rs`) drive the real views with
Iced's simulator and fictitious data. Dialogs and menus are layers over the page, each over an empty layer, and the
notice banner keeps its place: the page stays the same widget whatever opens over it, so Iced keeps
its state (the library's scroll position).

## Main flows

### Sign-in

GOG accepts only its Galaxy client's redirect URI (`https://embed.gog.com/on_login_success`).

1. The system browser opens GOG's sign-in page.
2. The user pastes the final address; SlattyLauncher extracts the code from it.
3. The code is exchanged for tokens, which go to the keyring.

Refreshes take a file lock, so the CLI and the interface never refresh concurrently. Tests showed the
refresh token is not rotated on refresh.
Download links expire; a download that outlasts the access token (about an hour) renews it the
same way before asking for new links.

### One operation per game

Installing, discarding an unfinished install, verifying or repairing, updating or changing content,
uninstalling, syncing cloud saves (checking them does not) and playing each hold
`~/.local/state/slatty/locks/game-<id>.lock` while they run. A second one on the same game, from
the same process or another one (CLI and interface), is refused instead of touching files in use.
A game started from SlattyLauncher keeps it busy until its last process ends, even once the launcher
was closed or crashed: the session supervisor holds `session-<id>.lock` for as long as it runs (as
do the prefix creation and setup steps it runs), and every operation on the game is refused while it
is held.

### Install

1. `plan_for` picks the public Windows build of generation 2 and reads its metadata. It keeps the
   depots of the base game and of the chosen owned DLC (all owned by default) for one language. DLC
   ownership comes from `embed.gog.com/user/data/games`.
2. `collect_files` reads the depot manifests. It rejects unsafe paths, merges paths that differ only
   by case, and skips "support" files and links.
3. `Download::run` checks disk space, then fills `.<Game>.slatty-partial`:
   - chunks come from the fastest of the CDN endpoints GOG lists: each is measured on a first
     request, one that fails is avoided for the retries of that chunk, and the runner-up is
     measured again now and then;
   - a chunk still waiting after three times the usual request time on its endpoint is asked again
     from another endpoint, and the first answer wins; the slow endpoint is set aside while it
     stays slow. Only one such copy runs at a time, so a slow connection is not loaded further;
   - two chunks per file are fetched at once and written at their offset as they arrive, so a late
     chunk does not hold back the following ones;
   - each file is verified first and downloaded only if missing or wrong;
   - when a file is replaced (update, repair, resumed install), the file already there is hashed at
     the new chunk offsets, and chunks whose MD5 matches are copied from it and checked again
     instead of downloaded;
   - each chunk is checked against its compressed and decompressed MD5;
   - files are written to a temporary name, then renamed; the filesystem is synced once when every
     file is in place, rather than file by file.
4. The partial folder is renamed to the game folder.
5. An install record (build, language, file list) is saved, and the game is registered with a
   prefix under `~/.local/share/slatty/prefixes/<id>`.

A Linux build takes another path (`installer::linux`), since GOG's content system has no Linux
builds (`Unsupported OS`):

1. `plan_for` with `Platform::Linux` reads the game's and its DLC's Linux installers from
   `api.gog.com/products/<id>?expand=downloads,expanded_dlcs`, and picks one per product for the
   language. Its build id is `linux:<version>`: that is how a job tells its platform, with no
   column of its own.
2. Each installer is a MojoSetup shell script followed by a zip. Its download link comes from the
   API's `downlink`. The CDN serves byte ranges but not suffix ranges, so the exact size comes from
   the `Content-Range` of the first byte (the API's size is rounded). `installer::zip` finds the
   zip's directory in the last 64 KiB, with ZIP64 when present, and shifts every offset by the
   length of the script before it. The plan reads these directories, for the size on disk.
3. Only entries under `data/noarch/` are kept; a DLC's file replaces the game's at the same path.
   `LinuxDownload` fetches each file with its own ranged request (eight at once), inflates it off
   the async threads in 1 MiB batches, checks size and CRC-32, writes it under a temporary name,
   then renames it. A dropped connection or an expired link (refused with 401, 403 or 410) is
   tried again, without counting the progress twice. Links pointing out of the game folder are
   never made.
4. The staged folder, its publication, the record and the job are those of a Windows build. The
   game is registered with `Runner::Native` and starts through `start.sh`: on NixOS inside `steam-run`
   when present, else inside umu with `UMU_NO_PROTON=1` and `RUNTIMEPATH=steamrt3` (sniper); on
   other systems through the interpreter its first line names, looked up in `PATH` when
   that path is missing. Post-install setup,
   cloud sync and Comet are skipped. Verify, repair, updates and DLC or language changes compare
   the files with the current installers the same way.

An interrupted install keeps its job in the database, and resumes with the same build, language and
folder. An install queued in the interface waits as a job in the `queued` state, with the
choices made in its panel; the order of the queue is a setting. A queued job is never taken for a
download already published, even when its folder exists. Its state tells a pause on request (`paused`, resumed when asked) from a cut-off
(`downloading`, resumed by the interface at start-up). Updates do the same with `updating-paused`
and `updating`; both keep the game from starting. A cut-off after the partial folder was renamed
but before the game was registered is resumed by checking the game folder in place; a job left
behind by a game registered just before a cut-off is dropped at start-up.

### Post-install setup

`setup::run` reproduces what Galaxy does after an install. It runs once per installed build, from
the play flow after the prefix exists, or on demand with `slatty setup`:

1. Missing support files and the shared dependencies are downloaded:
   - support files go to the game's support folder;
   - the shared dependencies (and the script interpreter when the build uses it) go to
     `~/.local/share/slatty/redist`.
2. For the base game and each installed DLC, it runs GOG's script interpreter when the build
   metadata asks for it, otherwise the product's temporary setup program, with Galaxy's arguments.
3. Shared redistributables are installed silently.

Commands are built by a pure function (`setup::commands`). They are run under the session
supervisor, so each one has really finished before the next starts. The build is then recorded as
set up. Updates, language and DLC changes clear that mark.

### Update, language and DLC changes

`maintenance::reconfigure` handles three changes the same way:

- a newer build (`Change::Update`);
- another language;
- another set of DLC, the last two staying on the installed build.

It records an "updating" job with the target build, language and DLC. Then:

1. The new file list is checked in place with the same fill routine as installs and repairs: only
   files that are missing or differ are downloaded, each written to a temporary name and renamed.
2. Files listed in the old install record but absent from the new build are deleted. Their parent
   folders are removed only if they end up empty.
3. The install record is rewritten for the new build.

While the job exists, launching the game and verifying it are refused, and running the update again
resumes it with the pinned build.

### Game isolation

An isolated game (`Install::isolated`; on by default for Windows builds run through Proton) runs
in the container umu already uses, pressure-vessel from the Steam Linux Runtime, told through its
environment (`runner::isolation_env`) to share less:

- `PRESSURE_VESSEL_HOME`: a home folder of the game's own, `~/.local/share/slatty/homes/<id>`,
  mounted where the user's is;
- `PRESSURE_VESSEL_FILESYSTEMS_RW`: the game folder; `PRESSURE_VESSEL_FILESYSTEMS_RO`: umu's data,
  the redistributables and the game's support files. The prefix is shared by umu itself;
- `TMPDIR`, `TMP`, `TEMP`, `TEMPDIR` set to `/tmp`, which is private in the container: it shares
  the folders they name;
- `DBUS_SESSION_BUS_ADDRESS` and `DBUS_SYSTEM_BUS_ADDRESS` set to `disabled:`, D-Bus's address
  for no bus: the container then shares neither bus (an address to a missing socket stops it from
  starting). Proton and umu do not use them; Wine uses the system bus only in `mountmgr` (UDisks,
  NetworkManager) and `winebth` (BlueZ), and showed the same network adapters and drives without
  it;
- `HOME` set to the game's own home as well: pressure-vessel shares the Steam installation
  (`~/.steam` and where its links lead, `~/.local/share/Steam` with the client's settings and
  tokens) read-only with every game, from the home folder `HOME` names (`wrap-home.c`,
  `expose_steam`). umu keeps its runtimes and cache where they are through `XDG_DATA_HOME` and
  `XDG_CACHE_HOME`, which the container replaces with the game's own;
- `PRESSURE_VESSEL_SHARE_HOME=0`, `PROTON_LOG_DIR` (the game's home), and an empty
  `STEAM_COMPAT_INSTALL_PATH`, `STEAM_COMPAT_LIBRARY_PATHS`, `STEAM_COMPAT_CLIENT_INSTALL_PATH`,
  `STEAM_COMPAT_MOUNT_PATHS`, `STEAM_COMPAT_TOOL_PATH`, `STEAM_COMPAT_APP_LIBRARY_PATH(S)`,
  `STEAM_EXTRA_COMPAT_TOOLS_PATHS` and `STEAM_RUNTIME_SCOUT`: the launch inherits the user's
  environment, and pressure-vessel turns each of these into a writable mount
  (`wrap-setup.c`, `known_required_env`), the first into the shared home folder, whatever the
  other settings say. umu takes `STEAM_COMPAT_INSTALL_PATH` from the environment too;
- for a Linux game, `WINEPREFIX` set to the game's home: without one umu takes `~/Games/umu/<id>`
  (or the user's `$WINEPREFIX`), and the container shares it as `STEAM_COMPAT_DATA_PATH`.

umu shares, writable, the whole filesystem a game is on (its "game drive": the first mount point
above `STEAM_COMPAT_INSTALL_PATH`, which it otherwise takes from the program's folder). With `/home`
or a library disk mounted on its own, that is all of it. An isolated game is therefore started
without `STEAM_COMPAT_INSTALL_PATH`, and never by a path umu can open: a Windows program by its
`Z:\…` path, which Proton resolves in the container, a Linux script through its interpreter
(`bash start.sh`). Proton's fixes then find the game from the working directory.

umu looks for that first argument on this computer too, relative to the working directory
(`Path(cmd).absolute()`): a file named `sh`, `sc`, `wineboot` or `Z:\…\game.exe` in the game's
folder, which the game writes, would make umu take the folder for the game's at the next launch.
And pressure-vessel shares the working directory as it is on disk: a link the game made in its
folder, named as the working directory in its `goggame-*.info`, would share where it leads.
`runner::check_shared` refuses both, for every command run isolated (the game, `wineboot`, the
setup programs, the Galaxy service), just before it starts; the play flow builds the game's
command again after the setup steps. pressure-vessel splits its path lists on every `:`, escaped
or not: a game folder with `:` in its path is refused rather than started without its folder.

The launcher itself works in the prefix and the game folder outside the container, where a link
the game planted could lead it out. When the game is isolated, cloud sync refuses a save folder
that leads out of the prefix (or of the game folder), the Galaxy service registration refuses to
copy through such a link, and an uninstall refuses to back up or delete a prefix whose `users`
folder leads out of it. Games that are not isolated keep the old behaviour: Wine alone links the
user folders to the real ones.

A Linux game runs isolated in umu without Proton (the sniper runtime), on NixOS too, rather than in
`steam-run`, which shares everything. Wine alone cannot isolate: such a game is refused while marked
isolated. The ignored tests in `cli/tests/isolation.rs` run a marker check through the real
container, and the same launch not isolated as a control.

### Game sessions

A launch spawns the running binary again (through `/proc/self/exe`, which still works after the
file was replaced by an update or a rebuild) with a hidden argument. That copy becomes the
supervisor for the session:

- It marks itself `PR_SET_CHILD_SUBREAPER`, starts the game in its own process group and reports
  events as JSON lines.
- Processes that detach (launchers that exit early, Wine services) are re-parented to the
  supervisor. The session therefore ends only when no descendant is left.

umu's `waitforexitandrun` already waits for Windows processes; the supervisor is the safety net for
everything else. A session without a recorded end (launcher crash) is reported at the next launch,
and post-game sync is skipped when the end is uncertain.

### Cloud sync

See [cloud-saves.md](cloud-saves.md). The decision logic (`cloud::plan`) is a pure function. The
executor (`cloud::sync`) works against a `CloudTransport` trait, with two implementations:

- `GogCloud` for real use;
- an in-memory transport with fault injection, used by the tests.

### Comet

Comet listens on the fixed port 127.0.0.1:9977, so only one instance can run.

- **Tokens.** SlattyLauncher passes them through Comet's Lutris importer: a 0600 file in a private
  directory under `$XDG_RUNTIME_DIR`, removed as soon as Comet listens. They never appear in process
  arguments.
- **Shutdown.** Comet is stopped with SIGINT, so pending requests can finish.
- **Achievement report.** Achievements are read before and after the session, so the report reflects
  what GOG actually recorded.
- **Galaxy service.** Some Galaxy SDK versions only reach Comet when a `GalaxyCommunication` Windows
  service exists, as GOG Galaxy installs one. Before the first session with Comet, the play flow
  registers Comet's dummy service in the prefix (`sc create`, plus the `GalaxyClient\paths`
  registry value), then copies the executable to
  `C:\ProgramData\GOG.com\Galaxy\redists\`. The copy comes last, so its presence means the
  registration completed. Wine stops the service with the last game process, so session tracking
  is unaffected.

## Storage

| Kind | Location | Rebuildable |
|---|---|---|
| Secrets | System keyring | no (sign in again) |
| Essential state | `~/.local/share/slatty/state.db` (accounts, installs, sessions, sync history, install jobs, settings) | no |
| Install records | `~/.local/share/slatty/manifests/` | from GOG, for the same build |
| Prefixes and backups | `~/.local/share/slatty/{prefixes,backups}/` | no |
| Home folders of isolated games | `~/.local/share/slatty/homes/` | no (Linux games' saves) |
| Downloaded Proton builds | `~/.local/share/slatty/protons/` | from GitHub, while the release is listed |
| Cache | `~/.cache/slatty/<user id>/` | yes |
| Logs | `~/.local/state/slatty/logs/` | yes |

## Design decisions

| Decision | Why |
|---|---|
| Rust everywhere in the application; Proton, umu and Comet stay external | Reuse proven tools instead of rewriting them; keep their boundaries explicit |
| Iced rather than GPUI (reviewed 2026-10-09) | Maintenance comes first. Iced has versioned releases and documentation. GPUI has had no maintained release since 0.2.2 (October 2025); its ecosystem pins weekly third-party snapshots (`gpui-pre`) with frequent breaking changes. Rich pages remain possible with Iced's `markdown`, `table` and `sensor` widgets. |
| No web view | Native application; sign-in happens in the user's own browser |
| Address pasted back after sign-in | GOG accepts only the Galaxy redirect URI; no local redirect is known to work |
| Isolation through umu's own container rather than another sandbox | pressure-vessel already runs every Proton game; its documented options hide the home folder with no second container to keep working with GPUs, sound and gamepads. It still shares the display, sound, devices and the network: it keeps games out of files and D-Bus, not out of the system |
| Proton builds downloaded by SlattyLauncher, not by umu | umu can fetch GE-Proton or UMU-Proton itself, but only at a game's launch, which would then wait for half a gigabyte, and its "latest" builds are replaced in place under one name, changing a game's Proton behind its back, under a game already running. SlattyLauncher keeps each build in a folder of its own; following the newest goes through a link, which umu resolves when a game starts, so a running game keeps its build, and the build before stays to go back to |
| Comet as a supervised companion process | Its library API is not meant for embedding (global state, fixed port, panics on errors) |
| Subreaper supervisor process for sessions | Launchers that exit early and Wine processes must not end the session too soon |
| Content hashes, not dates, for cloud sync | Dates are unreliable across machines and Wine; hashes plus per-file history detect real changes |
| SQLite through rusqlite behind a mutex | One writer per process, simple synchronous queries, futures stay `Send` |
| Interface in English first | Translations come later |
| GPL-3.0-or-later | Allows building on heroic-gogdl's GPL code with attribution |

## Security principles

- No password field: sign-in happens on GOG's site.
- Tokens live only in the keyring. They are never written to plain files, except Comet's short-lived
  0600 handoff file. They never appear in process arguments or logs. The `Secret` type hides its
  value from `Debug`.
- HTTP errors never include URLs, since some GOG endpoints take tokens in the query string.
- SlattyLauncher's folders are 0700 and its database 0600, set again at every start.
- Game ids are checked (letters and digits only) before they name a file: locks, installs, install
  jobs and records, covers, customisations.
- Nothing is written through a symbolic link that leads out of the game folder; links from Linux
  installers are resolved on disk once all are made, and those leading out are removed.
- Besides GOG, only umu's game database is contacted, and SteamGridDB once turned on; umu's
  database and play time reporting can be turned off (Settings → Privacy), SteamGridDB is off
  until turned on (Settings → Advanced).
- Writes to installed games, saves and prefixes are staged and renamed. Every destructive action
  either refuses when the situation looks wrong, or keeps a copy.
