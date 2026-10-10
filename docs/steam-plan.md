# Steam support: plan

Plan of 2026-10-10, built on [steam-feasibility.md](steam-feasibility.md), which holds the evidence
(marked **[code]**, **[src]** or **[unverified]**). Nothing here is implemented.

**Strategy B** is assumed throughout. SlattyLauncher:

- reads the local Steam installation, without ever writing it;
- hands installs, uninstalls and launches to the Steam client through `steam://` URLs and
  `-applaunch`;
- may read the owned list and achievements through the user's own Web API key, once turned on.

Several choices below are the owner's to make; they are listed at the end. No step runs anything
against a Steam account without the owner: real checks are run by them and recorded in
[compatibility.md](compatibility.md).

## 1. Multi-store design

### 1.1 Store and game id

- **A `Store` enum** (`Gog`, `Steam`) in a new `core/src/store.rs`.
- **A `GameId` type** holding the store and the store's own id. Its text form is the key used
  everywhere today (database, file names, settings, interface maps):

  | Store | Text form | Example |
  |---|---|---|
  | GOG | the bare product id, as today | `1456487183` |
  | Steam | `steam-` and the app id | `steam-440` |

- **Why a prefix rather than a store column:**
  - **No migration of existing data.** Every GOG row, folder, lock, prefix, record, backup and
    cache keeps its name, so nothing can be lost in a migration.
  - **The code already works this way.** It names the platform of a build in its id (`linux:`,
    `installer::linux::BUILD_PREFIX` [code]).
  - **`-`, not `:`.** It is valid in file names on every system, including the Windows host on the
    roadmap.
- **One place parses ids.** `GameId::parse` accepts only digits, or `steam-` and digits. Every path
  built from an id goes through it, which also closes the unchecked joins found in `lock.rs`,
  `installer/record.rs` and `installer.rs` [code].
- **The alternative is a `store` column.** SQLite cannot change a primary key, so `installs`,
  `install_jobs` and `game_custom` would be rebuilt by an appended migration (create, copy, drop,
  rename), and file names would change too. It is cleaner in the long run but riskier. This is
  decision D4.

### 1.2 Capabilities rather than one big trait

- **Steam under strategy B goes through none of the GOG pipeline** (installer, maintenance, setup,
  patches, cloud, Comet).
- **A single `Store` trait would force fake implementations**, and the existing traits return
  `impl Future`, so they cannot be `dyn` objects [code] `cloud/transport.rs`.
- **Hence:**
  - **Dispatch by `match` on `Store`** at the few entry points: library, launch, install,
    uninstall, verify, achievements, cloud status.
  - **A `Capabilities` value per store**, read by both front ends to show or hide what applies:
    install panel, pause, versions, languages and DLC, cloud sync by SlattyLauncher, manual
    achievements, play time reporting, Proton choice.
  - **The GOG modules stay as they are.** Steam gets its own module, `core/src/steam/`:
    - `local.rs`: Steam roots, library folders, `appmanifest_<appid>.acf`, signed-in users;
    - `client.rs`: `steam://` URLs, `-applaunch`, watching the `reaper` process;
    - `webapi.rs`: optional, with the user's key.
- **No new trait until a second implementation of the same capability exists.** This follows
  AGENTS.md: no abstraction for a single use.

### 1.3 What Steam games never touch

**Safety property:** a Steam game never gets a row in `installs` or `install_jobs`, nor a
`manifests/<id>.json`, nor a prefix under `prefixes/`. Steam owns its files. So
`maintenance::uninstall`, repair and updates cannot delete or rewrite a Steam game's files.

- **Locks.** `lock::game` and `lock::session` still apply to Steam ids, so two SlattyLauncher
  operations on one Steam game never overlap.
- **Sessions.** They are recorded in `sessions`, with the Steam user id and `reported = 2` (kept
  private). They are never sent anywhere.

### 1.4 Accounts

- **One account per store, side by side.** GOG keeps `active_user` and its keyring entry
  `gog:<user_id>` unchanged.
- **Steam under B has no sign-in.** Its user is the one signed in to the local client, read from
  Steam's `config/loginusers.vdf` [src: Lutris reads it]. It is stored in a new setting,
  `steam_user`.
- **A Web API key,** when turned on, goes in the keyring as `key:steam-webapi`, like the
  SteamGridDB key (`credentials::key_entry`) [code]. It is typed in the application, never
  elsewhere.
- **The interface no longer hides everything without a GOG account** (`ui/mod.rs` [code]):
  - the library shows when either store has something;
  - signing out of GOG removes GOG's games only;
  - `account_task` keeps one epoch per store.

### 1.5 Cache layout

- **GOG:** `cache/<gog user id>/`, unchanged.
- **Steam:** `cache/steam-<steamid64>/`, holding `library.json`, `covers/` and `images/` in the
  same formats. Covers keep their names (`steam-440.jpg`), so `library::cached_file` and the
  interface's image handling work as they are [code].

## 2. Milestone zero: the multi-store refactor, GOG unchanged

**Scope:**

1. **`Store`, `GameId` and `Capabilities`** in the core. `GameId::parse` is used wherever an id
   enters (CLI arguments, library, database reads) and wherever a path is built from an id.
2. **The store-blind spots found by the audit [code]:**
   - `playtime::unreported` and `report_pending` consider GOG ids only;
   - `runner::umu_env` and `umu::lookup` take the store instead of a hard-coded `gog`;
   - `custom::save` accepts the `steam-<digits>` form.
3. **`LibraryGame` gets its store from its id.** Nothing else in the interface changes yet.
4. **No database migration.** A test opens a copy of a version-7 database (the current
   `MIGRATIONS`) and checks that everything reads back.

| | |
|---|---|
| **Modules** | core: `store` (new), `lock`, `installer/record`, `installer`, `custom`, `playtime`, `runner`, `umu`, `library`; cli: argument parsing; gui: none beyond types |
| **Dependencies** | None |
| **Tests** | All existing tests pass unchanged. New: `GameId` round trip and refusals (`/`, `..`, empty, letters); a session of a `steam-` id is never pending for GOG; a GOG id still sent with `STORE=gog`, a Steam id with `steam`; `custom::save` accepts `steam-440`; version-7 database read back |
| **Real check** | None needed (GOG behaviour unchanged). The owner may run `slatty library list` and open the interface as usual |
| **Done when** | `cargo test`, clippy and fmt pass; the GOG flows are unchanged; architecture.md describes `store` |
| **Effort** | S |
| **Risks** | Missing a place where an id becomes a path. Mitigation: grep every `format!` and `join` that uses an id; the audit already lists them (feasibility §2.1) |

## 3. Prototype: remove the biggest unknowns first

- **Where it runs.** A throwaway branch, never merged, with hidden CLI commands. It only reads
  Steam's files and calls the Steam client.
- **Who runs it.** The owner, on their own machine, after reading what each command does. The
  results go to compatibility.md, whether they pass or fail.

| # | Unknown | Probe | Answers |
|---|---|---|---|
| P1 | Following a game launched by Steam | `steam -applaunch <appid>` on an installed game; watch for `reaper SteamLaunch AppId=<appid>` to appear and disappear [src Lutris] | Delay before the process appears; whether it covers launchers that exit early; behaviour when Steam is closed (does it start and wait for sign-in?) and with the Flatpak (`flatpak run com.valvesoftware.Steam`) [unverified] |
| P2 | Reading the local installation | Steam roots (native, `~/.steam/steam`, Flatpak `data/Steam` or `.local/share/Steam`: feasibility §2.4), `libraryfolders.vdf`, `appmanifest_*.acf`, `loginusers.vdf`; `StateFlags` before, during and after a `steam://install` | The Flatpak path; whether install progress can be read (`BytesToDownload` and its counterpart, [unverified]); multiple signed-in users |
| P3 | Covers without new network traffic | Look for the library images the client keeps on disk (`appcache/librarycache`, [unverified]) | If they exist, covers need no request; otherwise the store CDN (a new host, decision D5) |
| P4 | Web API with the user's own key (only if D2 says so) | `GetOwnedGames` and `GetPlayerAchievements` for the key owner's own SteamID, profile private and public | Whether a private profile is visible to its owner's key [unverified]; whether family-shared games appear |
| P5 | Steamworks game started outside `-applaunch` while Steam runs (only for information) | One game, through umu, with Steam signed in | Whether the `lsteamclient` bridge is enough [unverified]. Not planned as a feature; it tells whether a "use my Proton build" option could ever exist |

**Done when:** P1 to P3 have recorded answers. If P1 fails (no reliable end of session), the plan
stops at a library that shows and starts Steam games without following them, and the owner
decides whether that is worth having.

## 4. Steam milestones, highest risk first

### S1. Sessions through the Steam client

| | |
|---|---|
| **Scope** | Play for a Steam game: the session supervisor (`session.rs` [code]) gets a watch mode. It calls `steam -applaunch <appid>`, waits for the game's `reaper` process, holds `session-<id>.lock` until it is gone, and records the session (private, never reported). `PlayEvent`: `Started`, `Ended`; no cloud, Comet or achievement steps. Stop: only if P1 shows a safe way; otherwise the game is quit from inside |
| **Modules** | core: `steam/client.rs` (new), `session`, `play` (one `match` on the store); cli: `launch`; gui: `play`, `ui/panels` (session drawer wording) |
| **Dependencies** | None (`/proc` is read with the standard library) |
| **Tests** | Watch mode against a fake `reaper` (a script named so, started by the test); lock held while it runs; session recorded with `reported = 2`; Steam missing gives a clear error |
| **Real check** | Two installed Steam games, one with an early-exiting launcher. Note the start and end times against reality |
| **Done when** | Play time and "busy" match reality for both games; nothing else in Steam is touched |
| **Effort** | M |
| **Risks** | The `reaper` name or arguments change (Steam updates without notice); several Steam users on one machine; Flatpak sandboxing hides processes [unverified] |

### S2. Steam games in the library

| | |
|---|---|
| **Scope** | Installed Steam games read from Steam's files at start and on Refresh; covers (P3's answer); a Store filter; the game page with Play, Uninstall with Steam, Verify with Steam (if `steam://validate` is confirmed), Open in Steam; installed size and build from `appmanifest`. Edit game, favorites, shelves and hiding work as for GOG. GOG sign-out keeps Steam games |
| **Modules** | core: `steam/local.rs` (new), `library` (merge), `settings` (`steam_user`, Steam roots shared with `steam_roots`); gui: `boot`, `library`, `ui/library`, `ui/game`, `login` (gate), `ui/settings` (Steam section: detected client, user) |
| **Dependencies** | `keyvalues-parser` (MIT OR Apache-2.0, 0.2.4 of 2026-05-17) or `steamlocate` (MIT, 2.1.1 of 2026-08-13), or extend the existing parser in `settings::steam_libraries`: decision at implementation, after `cargo audit` |
| **Tests** | Fake Steam roots with `libraryfolders.vdf` and `.acf` files (installed, updating, uninstalled); malformed files refused without panic (a fuzz loop, as for `installer::zip`); a library of 10,000 games mixing both stores (`library_at_10000_games`); no write anywhere under the fake Steam root (checked by comparing the tree before and after) |
| **Real check** | The owner's Steam libraries (native, and Flatpak if used): every installed game listed, with size and build matching Steam's |
| **Done when** | The library shows both stores, stays within the 16 ms frame at 10,000 games, and Steam's folders are unchanged |
| **Effort** | M |
| **Risks** | Steam's text formats change; library folders on removable or network drives (`/mnt/share`) that are absent |

### S3. Installs and uninstalls through Steam

| | |
|---|---|
| **Scope** | Install opens `steam://install/<appid>` and Uninstall `steam://uninstall/<appid>` [src Lutris]. The game's state follows from `appmanifest` changes (polled, or a file watch). The Downloads tab lists Steam installs read-only, with progress if P2 found it. Pause, versions, languages and DLC stay in Steam's interface: the drawer says so instead of offering empty pickers |
| **Modules** | core: `steam/client.rs`, `steam/local.rs`; gui: `install`, `downloads`, `ui/install`, `ui/downloads`, `ui/manage`, `ui/game_settings` (capabilities) |
| **Dependencies** | None new |
| **Tests** | State changes read from changing `.acf` files; capabilities hide GOG-only pickers for Steam ids; no `install_jobs` row is ever created for a Steam id |
| **Real check** | Install and uninstall a small free game through SlattyLauncher; Steam does the work; SlattyLauncher's state follows |
| **Done when** | Install and uninstall work from both front ends, with no Steam file written by SlattyLauncher |
| **Effort** | S to M |
| **Risks** | Steam asks questions in its own windows (library folder, licence agreements): SlattyLauncher only waits. Progress may be unavailable |

### S4. Owned games and achievements through the user's Web API key (if D2 = key)

| | |
|---|---|
| **Scope** | Settings → Advanced: a Steam Web API switch, off by default, and the key typed in the application, kept in the keyring. When on: owned games not installed (`GetOwnedGames` with `include_appinfo`), Steam's play time (read only), the Achievements tab and game page for Steam (`GetSchemaForGame`, `GetPlayerAchievements`, rarity through `GetGlobalAchievementPercentagesForApp`). No manual change, ever. Cached per account for offline use |
| **Modules** | core: `steam/webapi.rs` (new), `overview`, `achievements` (dispatch); gui: `settings`, `achievements`, `ui/achievements`, `ui/game`; SECURITY.md ("What leaves your computer": `api.steampowered.com`) |
| **Dependencies** | None new (`reqwest`, `serde`) |
| **Tests** | Local HTTP server with recorded-shape answers (as `steamgriddb.rs` [code]); the key never in a URL that reaches a log or an error (`Error::network` strips URLs [code]); switch off means no request at all (the umu test's closed-port proxy [code]); the Web API call budget (100,000 a day) stays far away at 10,000 games |
| **Real check** | The owner's own key: owned list count against Steam's, achievements of two games against Steam's profile |
| **Done when** | Both front ends show owned and installed Steam games, and achievements, with the switch on; nothing leaves with it off |
| **Effort** | M |
| **Risks** | A private profile hidden from its owner's key (P4); the key's terms (domain to register, confidentiality) explained in the user guide |

### S5. Cloud, play time and wording

| | |
|---|---|
| **Scope** | The cloud card says Steam syncs these saves (no Check or Sync buttons for Steam). Play time shows "played here" from local sessions, and Steam's total when S4 is on. The Privacy and Advanced settings say which switches concern which store. The interface text that says "GOG" for any game becomes store-aware |
| **Modules** | gui: `ui/game`, `ui/settings`, `ui/format`, `cloud`; cli: help texts |
| **Tests** | Snapshot and text assertions per store; existing GOG wording tests kept |
| **Real check** | None beyond S1 to S4 |
| **Done when** | No screen promises a GOG feature for a Steam game |
| **Effort** | S |
| **Risks** | Low |

### S6. Documentation and parity

README ("What works" with a Steam column), user-guide.md (Steam section: what Steam does, what
SlattyLauncher does), architecture.md (`store`, `steam`, capabilities), SECURITY.md, roadmap.md,
testing.md (manual checks against Steam), compatibility.md. Effort S. Done when every page matches
the code.

### Not planned

These are listed in feasibility §5–6 with their reasons:

- native depot downloads (strategy A);
- the client protocol (QR sign-in, licences, cloud), unless D2 chooses it;
- manual Steam achievements;
- play time reporting to Steam;
- writing Steam's files (adopting games, choosing their Proton build);
- any emulator or DRM workaround.

## 5. Parity at the end of S1–S6

| GOG feature (README) | Steam after this plan |
|---|---|
| Sign-in in the browser, tokens in the keyring | **Partial:** no sign-in; the Steam client's user is used. Optional Web API key in the keyring |
| Library with covers, offline cache | **Partial:** installed games always; owned games only with the Web API key (S4); covers per P3 |
| Install Windows builds, pause and resume, integrity checks | **Partial:** through Steam; pause and progress in Steam's interface (progress in SlattyLauncher if P2 allows) |
| Native Linux builds | **Partial:** through Steam, as Steam decides per game |
| Launch, follow the session to the last process | **Partial:** through `-applaunch`, ended by the `reaper` process; requires the Steam client |
| Launch options | **Partial:** Steam's own choice |
| Cloud saves: three-way sync, conflicts, backups | **Partial:** Steam's sync and conflict dialog; no SlattyLauncher backups |
| Achievements: list | **Identical** with the Web API key (S4); none without it |
| Achievements: in-game unlocks | **Identical** (Steam records them; shown with the key) |
| Achievements: manual unlock or clear | **Impossible** (no public API; SSA §4.B, §4.C) |
| Edit a game | **Identical** |
| Verify, repair, uninstall | **Partial:** uninstall through Steam; verify if `steam://validate` is confirmed; repair is Steam's verify |
| Updates | **Partial:** automatic, by Steam; SlattyLauncher shows the state |
| DLC and language | **Partial:** in Steam's interface only |
| Post-install setup, redistributables | **Identical in effect** (done by Steam), not by SlattyLauncher |
| Play time | **Partial:** read (S4) and counted locally; never reported |
| Anything without the Steam client installed | **Impossible** under strategy B |

## 6. Decisions for the owner

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Strategy | B (client-driven), A (native), C (external downloader) | B |
| D2 | Owned games that are not installed | None; the user's Web API key (documented, needs a key and a domain); QR sign-in and licences over the client protocol (no key, undocumented, SSA §2.G) | Web API key, or none at first |
| D3 | Steam client required for Steam games | Accept (B), or refuse Steam support | Accept, and say it in the README |
| D4 | Game id scheme | `steam-<appid>` prefix, no migration; or a `store` column with table rebuilds | Prefix |
| D5 | Covers for Steam games | Steam's local image cache (if P3 finds it); the store CDN (a new host, a switch, a SECURITY.md entry) | Local cache first |
| D6 | Stopping a Steam game from SlattyLauncher | Only if P1 shows a safe way; otherwise no Stop button | Decide after P1 |
| D7 | README presentation | One table with GOG and Steam columns, "by Steam" where Steam does the work | One table |
