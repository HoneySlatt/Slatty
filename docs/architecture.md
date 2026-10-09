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
| `credentials` | System keyring (Secret Service through the `keyring` crate) |
| `http` | Shared HTTP client; reads that fail on the way (dropped connection, timeout, rate limit, server error) are sent again twice; errors are stripped of URLs so query-string secrets never reach logs |
| `db` | SQLite state database with versioned migrations |
| `library` | Galaxy library and gamesdb metadata, per-account cache, covers, key art and images |
| `gameinfo` | `goggame-<id>.info` parsing, case-insensitive Windows path resolution |
| `install` | Installed-game records, Proton build of each game |
| `galaxy` | Content system: builds, build metadata, depot manifests, secure links, chunks from the fastest CDN endpoint |
| `installer` | Install plans, staged verified downloads, resumable jobs, install records |
| `maintenance` | Verify, repair, uninstall, updates and content changes |
| `patches` | GOG's binary patches between builds: lookup, delta download, xdelta3 application |
| `runner` | Launch commands for umu/Proton, Wine and native games; prefix creation |
| `umu` | Game id in umu's database, looked up once per game, so umu applies its fixes for it |
| `session` | Session supervisor (subreaper), session records, play time |
| `play` | Full play flow: prefix, cloud, Comet, session, upload, achievement diff |
| `cloud` | Save locations, local scan, three-way plan, transport, sync executor, diagnostics |
| `comet` | Supervised Comet process |
| `galaxy_service` | Comet's dummy `GalaxyCommunication` service, registered in game prefixes |
| `achievements` | Achievement list, manual unlock and clear |
| `overview` | Per-game achievement counts and cloud save support, cached per account |
| `playtime` | Play time read from GOG, finished sessions reported to GOG |
| `settings` | Default installation path, default Proton, favorites |
| `custom` | Titles, sorting titles, covers and backgrounds the user chose, apart from GOG's data; chosen images are copied into the data folder |
| `paths`, `fsutil`, `lock`, `secret`, `error`, `doctor` | Shared utilities |

## Interface modules

The interface follows Iced's state, message, update and view split. `main.rs` holds the
application state, the `Message` enum, navigation and start-up; `update` only dispatches. Each
feature keeps its state, message handling and background tasks in its own module, and the views
live under `ui/`.

| Module | Responsibility |
|---|---|
| `login` | Browser sign-in, sign-out, avatar |
| `library` | Library sync, covers and images, favorites, shelf, sort and filters, per-game overview and play time |
| `play` | Launching a game and following its session |
| `cloud` | Cloud save check, sync and conflict choices |
| `achievements` | Loading achievements, confirmed manual changes |
| `install` | Install plan, download with progress, pause, discard |
| `maintenance` | Verify, repair, updates, uninstall, language and DLC changes |
| `settings` | Default installation path, default Proton, and the Proton build of each installed game |
| `edit` | The menu a right click on a cover opens, and the edit dialog: draft, file picker, saving. `ui/pointer.rs` reports where a right click happened, without a message per mouse move |
| `work` | Shared helpers for background work: GOG tokens, throttled progress streams |
| `ui` | Window shell (top bar, notices, quit dialog); `widgets` for the building blocks every page uses (logo, tabs, avatar, cards); `library`, `achievements`, `settings`, `game` and `panels` pages, `install` for the Install drawer (drawers share `panels::drawer`); `format` for text shown to the user |
| `theme`, `icons` | Design tokens (`Tokens`: every colour, the corner radii of the redesigned pages, and the page transition: a short fade with a slight rise, played when the page changes and set to zero to turn it off) and the widget styles built from them; Lucide icons. Views never name a colour, so a custom theme is another `Tokens` |

Interface tests (`tests.rs`) drive the real views with Iced's simulator and fictitious data.

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
the same process or another one (CLI and interface), is refused instead of touching files in use. A
session left without an end (crash) still blocks changes until the next launch records it.

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

An interrupted install keeps its job in the database, and resumes with the same build, language and
folder.

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
| Cache | `~/.cache/slatty/<user id>/` | yes |
| Logs | `~/.local/state/slatty/logs/` | yes |

## Design decisions

| Decision | Why |
|---|---|
| Rust everywhere in the application; Proton, umu and Comet stay external | Reuse proven tools instead of rewriting them; keep their boundaries explicit |
| Iced rather than GPUI (reviewed 2026-10-09) | Maintenance comes first. Iced has versioned releases and documentation. GPUI has had no maintained release since 0.2.2 (October 2025); its ecosystem pins weekly third-party snapshots (`gpui-pre`) with frequent breaking changes. Rich pages remain possible with Iced's `markdown`, `table` and `sensor` widgets. |
| No web view | Native application; sign-in happens in the user's own browser |
| Address pasted back after sign-in | GOG accepts only the Galaxy redirect URI; no local redirect is known to work |
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
- Writes to installed games, saves and prefixes are staged and renamed. Every destructive action
  either refuses when the situation looks wrong, or keeps a copy.
