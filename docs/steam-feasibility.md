# Steam support: feasibility

Study of 2026-10-10. Nothing here is implemented. It answers one question: can SlattyLauncher
offer for Steam the features its README lists for GOG ("What works"), and at what cost?

Every statement carries its evidence:

- **[code]** read in this repository (file and function named);
- **[src]** read in a primary external source (URL given);
- **[unverified]** an assumption or an inference, not checked.

No request was made to Steam with an account, nothing was downloaded or run. Two sources could not
be read directly: developer.valvesoftware.com (a bot check) and PCGamingWiki's HTML (read through its
public API instead). Facts that depend on them are marked.

Steam emulators (Goldberg, gbe_fork, SmartSteamEmu and the like) and anything that removes or
bypasses DRM are excluded from this study and from the plan. They are named only to exclude them.

## 1. Summary

1. **Most Steam games need the Steam client to run.** A game that calls the Steamworks API fails to
   initialise when the client is not running, and the Steam DRM wrapper starts Steam itself. [src]
   https://partner.steamgames.com/doc/sdk/api, https://partner.steamgames.com/doc/features/drm
2. **No open-source replacement for the client exists in usable form.** Nothing plays the role
   Comet plays for GOG: a non-emulator service that answers an unmodified game and reports to the
   real account. [src] see §4.6.
3. **Sign-in without a password field is possible** (QR code scanned with the Steam mobile app), but
   only over Steam's reverse-engineered client protocol. [src] see §4.1.
4. **Downloading Steam content without the client is technically proven** (DepotDownloader,
   steamroom), but it is useless for games that then need the client to start. [src] see §4.3.
5. **Several features have no lawful equivalent:** manual achievement changes and play time
   reporting. The Steam Subscriber Agreement names faked play time as prohibited automation.
   [src] §4.C of https://store.steampowered.com/subscriber_agreement/english/ (updated 2026-09-10).

Full parity with GOG is therefore not reachable. The realistic target is a Steam section driven by
the installed Steam client, with SlattyLauncher as a unified front end (strategy B, §6).

## 2. Coupling map of the current code

### 2.1 Game identifiers

- Game ids are untyped `String`s everywhere, with no store attached: `LibraryGame.id`,
  `Install.game_id`, `InstallJob.game_id`, `PlayRequest.game_id`, `SyncTarget.game_id`. [code]
- They appear bare in:
  - **file names:**
    - `locks/game-<id>.lock`, `session-<id>.lock` (`lock::game`, `lock::session`);
    - `prefixes/<id>` (`installer::install`);
    - `manifests/<id>.json` (`InstallRecord`);
    - `support/<id>`, `custom/<id>`, `backups/prefixes/<id>`;
    - `logs/game-<id>.log`;
    - `cache/<user>/covers/<id>.<ext>` (`library::cover`). [code]
  - **the database:** `installs`, `install_jobs` and `game_custom` are keyed by `game_id TEXT PRIMARY
    KEY`; `sessions`, `sync_roots` and `sync_baseline` carry `game_id`. No table has a store column.
    [code] `db::MIGRATIONS`
  - **settings:** `launch_task:<id>`, and the `favorites` and `download_queue` id lists.
    [code] `settings.rs`
- **Collisions.** GOG product ids are 10-digit numbers (1207658924, 1456487183 in compatibility.md).
  Steam app ids are far smaller [unverified: no maximum is published]. An actual collision is
  therefore unlikely. The danger is in mixing stores without a namespace:
  - `installs` and `install_jobs` upsert on `game_id`, so one store's row silently replaces the
    other's;
  - `maintenance::uninstall` deletes the files listed in `manifests/<id>.json`;
  - `prefixes/<id>` holds saves. [code]
- **Bugs-in-waiting found by the audit:**
  - **`playtime::report_pending`** sends every unreported finished session of the signed-in GOG
    user to `gameplay.gog.com/games/{game_id}/…`, whatever the game's store. A Steam session
    recorded under the GOG user id would be reported to GOG. [code]
  - **`custom::save`** refuses an id that is not ASCII alphanumeric, so any namespaced id
    (`steam-440`, `steam:440`) is refused. [code]
  - **`runner::umu_env`** always sets `STORE=gog`, and `umu::lookup` always queries umu's database
    with `store=gog`. [code]
  - **Paths built from ids are not validated** in `lock.rs`, `installer/record.rs` and
    `installer.rs`. Harmless with GOG's numeric ids; an id with `/` would escape the folder
    [unverified: no such id exists today].

### 2.2 Accounts

- **One active account.** It is the single setting `active_user` (`account::ACTIVE_USER`).
  `Account::load_for` refuses when the active user changed. `Account` holds GOG `auth::Tokens`.
  [code]
- **Keyring.** Service `slatty-launcher`, user `gog:<user_id>` for GOG tokens and `key:<name>` for
  API keys (`credentials::entry`, `key_entry`). The store prefix already exists here. [code]
- **Cache.** Per user id under `cache/<user_id>/` (`Dirs::account_cache`). A SteamID64 cannot be
  mistaken for a GOG user id [unverified: based on observed ranges], but the folder name says
  nothing about the store. [code]
- **Interface.** `App.account: Option<AccountInfo>` gates the whole interface (`ui/mod.rs`).
  `App::account_task` tags work with an epoch (`Message::AccountResult(u64, …)`). Installs, play
  and maintenance take the currently active account (`work::tokens`); library, cloud and
  achievements take the account that started them (`work::tokens_for`). [code]

### 2.3 Abstractions

| Abstraction | Store-agnostic? | Notes |
|---|---|---|
| `cloud::CloudTransport` (list, download, upload, delete) | Interface yes | `GameCloud.transport` is the concrete `GogCloud`. `RemoteEntry.hash` is assumed to be GOG's MD5, and remote names start with `<location>/`. The trait returns `impl Future`, so it cannot be a `dyn` object. [code] `cloud/transport.rs`, `cloud/mod.rs` |
| `cloud::plan`, `scan`, `sync` | Yes | The three-way logic is generic over `T: CloudTransport`; only `IGNORED_REMOTE_HASH` is GOG's. [code] |
| `galaxy::ContentSource` | No | Expressed in GOG `Depot`, `DepotItem` and MD5 chunks. [code] |
| `installer::linux::Source` | Yes (ranged blobs) | `Offer`, `Installer` and `data/noarch/` are GOG's. [code] |
| `installer::zip`, `fsutil`, `lock`, `session`, `secret`, `http` | Yes | [code] |
| `Runner` (`Native`, `Umu`, `Wine`) | Yes | Apart from `STORE=gog`. [code] `runner.rs` |
| `install::Platform` (`Windows`, `Linux`) | Yes | [code] |
| `PlayEvent` | Mostly | `CometReady`/`CometUnavailable`, and GOG wording in the front ends. [code] `play.rs` |
| `InstallEvent` | Mostly | `support_files` and `dependencies` are GOG notions. [code] |
| Job states (`queued`, `downloading`, `paused`, `failed`, `updating`, `updating-paused`) | Yes | Free text in `install_jobs.state`. [code] `installer/job.rs`, `maintenance.rs` |
| Build id | No | GOG's raw build id, or `linux:<version>` (`installer::linux::BUILD_PREFIX`). The platform of a resumed job is read from that prefix (`installer::install`, `gui/src/install.rs`). [code] |
| `play::play` | No | It hard-wires GOG cloud (`cloud::sync_held`), Comet, the Galaxy service, play time reporting and achievement diffs. [code] |
| Missing | — | No `Store` type, no library-source abstraction, no launcher abstraction, no achievement source abstraction. |

### 2.4 Front ends

- **CLI:** every command takes a bare `<game_id>`, and almost every one starts with `Account::load`
  (the GOG account). Help texts say "GOG" 17 times. [code] `crates/cli/src`
- **GUI:**
  - Every per-game map is `HashMap<String, _>` keyed by the bare id (`installs`, `records`,
    `overview`, `achievements`, `cloud`, `maintenance`, `install_views`, `customs`, `favorites`).
    The queue is a `Vec<String>`. `play: Option<PlayState>` allows one game at a time. [code]
    `gui/src/main.rs` (`App`)
  - User-facing text names GOG 79 times. [code]
  - **Sign-in:** a pasted-address flow (`login.rs`, `ui/settings.rs`); a test asserts that no
    "Password" field exists (`tests/library.rs`). [code]
  - **GOG-only pages and drawers:**
    - the Install drawer: build, language, DLC and redistributable pickers built from
      `galaxy::Build`;
    - Game settings: version, language, DLC;
    - Manage;
    - the cloud card;
    - manual achievements, with a warning about GOG's terms;
    - the Privacy switches (umu, Comet, play time on GOG). [code]
- **Already Steam-aware:** `settings::steam_roots` and `steam_libraries` read `libraryfolders.vdf`
  to find Proton builds. Its Flatpak path is `.var/app/com.valvesoftware.Steam/data/Steam`, while the
  `steamlocate` crate uses `.var/app/com.valvesoftware.Steam/.local/share/Steam`.
  [code] `settings.rs`; [src] https://github.com/WilliamVenner/steamlocate-rs (`src/locate/linux.rs`).
  Which one is right is [unverified].

## 3. What Steam offers, and how it can be reached

| Means | What it gives | Status for a third party |
|---|---|---|
| **Steam client, installed and running** | Everything | Documented hand-off: `steam -applaunch <appid>` and `steam://` URLs (`install`, `uninstall`, `rungameid`, `run`). The URLs return at once; Lutris detects the end of a game from the `reaper SteamLaunch AppId=<appid>` process. [src] https://github.com/lutris/lutris/blob/master/lutris/runners/steam.py. Valve's own page on the URLs (https://developer.valvesoftware.com/wiki/Steam_browser_protocol) could not be read: [unverified] for the full list, e.g. `steam://validate/`. |
| **Steam's local files** (read only) | Installed games, build, size, state, library folders, Proton prefixes | `libraryfolders.vdf`, `appmanifest_<appid>.acf` (`appid`, `installdir`, `StateFlags`, `buildid`, `SizeOnDisk`, `BytesToDownload`…), `compatdata/<appid>/pfx`. [src] steamlocate `src/app.rs`, Lutris `appmanifest.py`. Binary `appinfo.vdf` is version 40 or 41. [src] https://github.com/SteamDatabase/SteamAppInfo |
| **Steam Web API** with the user's own key | Owned games and play time (`IPlayerService/GetOwnedGames`), achievement schema and progress (`ISteamUserStats`) | Documented by Valve. A key belongs to one account, is registered with a domain name, and must stay confidential; 100,000 calls a day. [src] https://partner.steamgames.com/doc/webapi/IPlayerService, https://partner.steamgames.com/doc/webapi/ISteamUserStats, https://steamcommunity.com/dev/apiterms. Whether a user's own key sees their own private library is [unverified]. |
| **Public store services** | Artwork paths, including hashed ones | `IStoreBrowseService/GetItems` with `include_assets` (observed anonymously for apps 440 and 1086940); store CDN `shared.fastly.steamstatic.com/store_item_assets/steam/apps/<appid>/…`. [src] https://github.com/SteamDatabase/Protobufs/blob/master/steam/steammessages_storebrowse.steamclient.proto, https://store.steampowered.com/app/440/ |
| **Steam's client protocol** (CM servers) | Sign-in, licences, PICS app info, depot keys, manifests, cloud files, stats | Not documented by Valve; known from reverse engineering. [src] https://github.com/SteamDatabase/Protobufs, https://github.com/SteamRE/SteamKit |
| **SteamCMD** | Downloads (`app_update <appid> validate`, other platforms with `+@sSteamCmdForcePlatformType`) | Official, proprietary; packaged in nixpkgs as `unfreeRedistributable`. [src] https://github.com/NixOS/nixpkgs/blob/master/pkgs/by-name/st/steamcmd/package.nix. Sign-in prompts for the password; QR support was not found. Valve's page: [unverified] (bot check). |

## 4. Points to settle

### 4.1 Sign-in

- **Current flow:** `Authentication` service, with `BeginAuthSessionViaQR`,
  `BeginAuthSessionViaCredentials` (password encrypted with the key from `GetPasswordRSAPublicKey`),
  `PollAuthSessionStatus`, `UpdateAuthSessionWithSteamGuardCode`, `GenerateAccessTokenForApp`.
  [src] https://github.com/SteamDatabase/Protobufs/blob/master/steam/steammessages_auth.steamclient.proto
- **QR sign-in asks for no password:**
  - The launcher shows a QR code, which the Steam mobile app scans and confirms. SteamKit's sample
    signs in with nothing but that. [src]
    https://github.com/SteamRE/SteamKit/blob/master/Samples/001_AuthenticationWithQrCode/Program.cs,
    https://github.com/DoctorMcKay/node-steam-session/blob/master/README.md
  - It fits "no password field" and "no web view".
  - A user without the mobile app cannot use it. The credential flow would need a password field,
    which [code] `docs/architecture.md` rules out.
- **Token:**
  - The result is a refresh token (a JWT, valid "~200 days" according to node-steam-user, so read
    its `exp`).
  - Only a token issued for the `SteamClient` platform can sign in to the CM servers. The reference
    libraries obtain such tokens over a CM WebSocket, not plain HTTPS.
  - Renewing it invalidates the old one.
  - [src] node-steam-session README; https://github.com/DoctorMcKay/node-steam-user/blob/master/components/09-logon.js
  - It would go in the keyring as `steam:<steamid64>`, beside `gog:<user_id>`.
- **Terms.** The Web API terms forbid intercepting or storing the user's Steam password at sign-in
  [src] https://steamcommunity.com/dev/apiterms. The QR flow never sees it.
- **Steam Guard.** A QR confirmation in the mobile app is itself the second factor. The machine
  token (`new_guard_data`) only matters for the credential flow. [src] node-steam-session README.
- **Not needed at all with strategy B.** The Steam client is signed in already; SlattyLauncher
  needs no Steam sign-in unless it wants the owned list (§4.2).

### 4.2 Library

| Source | Gives | Limits |
|---|---|---|
| Local files (strategy B) | Installed games only | Owned games that are not installed are missing. A readable cache of the licences was not found [unverified]. |
| Web API key of the user | Owned games, play time, `capsule_filename`, `sort_as` | Needs a key registered by the user; the key would live in the keyring, as the SteamGridDB key does today (`credentials::key_entry`) [code]. Whether family-shared games are included is [unverified] (they have their own RPC, `IFamilyGroupsService.GetSharedLibraryApps` [src]). |
| CM: `CMsgClientLicenseList` + PICS | Owned packages, apps, launch configuration, depots, `ufs` (cloud), `library_assets_full` | Reverse-engineered protocol; needs the QR sign-in. [src] `steammessages_clientserver.proto`, `steammessages_clientserver_appinfo.proto` |

- **Covers:**
  - `library_600x900.jpg` and the hashed paths of `library_assets_full`, under the store CDN.
    [src] https://github.com/SteamTracking/SteamTracking/blob/master/ClientExtracted/steamui/sp.js
  - That every hashed path resolves under `store_item_assets/steam/apps/<appid>/` is [unverified].
  - The existing image cache (`library::cached`, `cached_file`, files named after their format)
    can be reused unchanged. [code]
- **Offline cache.** Nothing found forbids keeping the owned list on disk for the user's own use.
  The Web API terms ask to tell users what is stored, and to delete it when access ends.
  [src] apiterms §2, §11.

### 4.3 Installation (SteamPipe)

How it works [src] (SteamKit `Steam/CDN/Client.cs`, `DepotChunk.cs`, `Types/DepotManifest.cs`;
DepotDownloader `ContentDownloader.cs`, `Steam3Session.cs`):

1. **Depot key:** `CMsgClientGetDepotDecryptionKey`.
2. **Manifest request code:** `ContentServerDirectory.GetManifestRequestCode`.
3. **Servers:** `GetServersForSteamPipe`.
4. **Manifest:** `depot/<id>/manifest/<gid>/5/<code>`. It is a zip holding protobuf sections, with
   file names encrypted in AES-256.
5. **Chunks:** `depot/<id>/chunk/<sha1>`. Each is AES-256 encrypted, then compressed with VZip
   (LZMA), VSZa (zstd, since 2025-05) or zip, and checked with a zero-seed Adler-32. Each file has a
   SHA-1.
6. **CDN token:** `GetCDNAuthToken`, asked only after a 403.

| Feature | Natively | Through the client (B) |
|---|---|---|
| Download, verified | Feasible: a new `ContentSource`-like source; staging, `safe_relative`, `refuse_outside` and publication are reusable [code] | `steam://install/<appid>` [src Lutris] |
| Pause and resume | Feasible (chunk-level resume, as DepotDownloader) | In Steam's interface only [unverified: no URL found] |
| Verify, repair | Feasible: per-chunk Adler-32 and per-file SHA-1 | `steam://validate/<appid>` [unverified] |
| Updates | Feasible: chunk reuse by SHA-1 [src]. Steam's binary patches (`GetDepotPatchInfo`, `ContentDeltaChunks`) exist in the protocol but no open implementation uses them [src], so: chunks only | Automatic, by Steam |
| Branches (betas) | Feasible (`depots/<id>/manifests/<branch>`; passworded branches through `CheckAppBetaPassword`) [src] | Steam's interface |
| Languages, OS | Feasible (`config.oslist`, `osarch`, `language`) [src] | Steam's interface |
| DLC | Feasible per depot: a depot is installed when an owned package lists it [src]. How a depot maps to its DLC (`dlcappid`) is [unverified] | Steam's interface |
| Post-install (`installscript.vdf`) | Partial. It is signed and verified by Steam; it has Registry, Run Process and Firewall sections; Valve says there is no install script on Linux [src] https://partner.steamgames.com/doc/sdk/installscripts. DepotDownloader does nothing here [src]. Redistributables (app 228980) [unverified] | By Steam |
| Linux builds | Feasible (download); running them through the Steam Linux Runtime works outside Steam [src] https://gitlab.steamos.cloud/steamrt/steam-runtime-tools/-/blob/main/docs/slr-for-game-developers.md | By Steam |

What is reusable from today's code [code]:

- **As is:** staged publication, `installer::safe_relative`, `refuse_outside`, `free_space`, job
  states, `InstallRecord` and record-based uninstall, the `lock` rules, chunk reuse at new offsets
  (`installer/download.rs`) and the endpoint racing (`galaxy::speeds`).
- **Not reusable:** `galaxy` (GOG formats), `patches` (GOG xdelta3), `setup` (GOG script
  interpreter), `installer::linux` (MojoSetup installers).

### 4.4 Running games without the Steam client

- **Steamworks API.** `SteamAPI_Init` returns false when "The Steam client isn't running"; a
  running client provides the Steamworks interfaces. `steam_appid.txt` only stops the relaunch
  through Steam; it is for development and must not ship. [src] https://partner.steamgames.com/doc/sdk/api
- **DRM wrapper.** The Steam DRM wrapper starts Steam before the game. [src] https://partner.steamgames.com/doc/features/drm
- **Proton.** Inside Proton, `lsteamclient` loads the host's `~/.steam/sdk64/steamclient.so`, the
  Linux Steam client library. [src] https://github.com/ValveSoftware/Proton/blob/proton_10.0/lsteamclient/unixlib.cpp
  Whether a Steamworks game started through umu works while Steam runs and is signed in is
  [unverified]. It needs a test, and `lsteamclient` is under the Steamworks SDK licence, not an
  open-source one. [src] `lsteamclient/LICENSE`
- **DRM-free share.** No official figure. PCGamingWiki's list of DRM-free Steam games has about
  1,200 rows and says it is far from complete. [src]
  https://www.pcgamingwiki.com/wiki/The_big_list_of_DRM-free_games_on_Steam (revision 2026-09-14,
  read through its API). The share of a given user's library that runs without the client is
  [unverified]; it can be measured only game by game.
- **Anti-cheat.** Easy Anti-Cheat and BattlEye work under Proton once the developer enables them.
  [src] https://partner.steamgames.com/doc/steamdeck/proton. That such games also need the client is
  [unverified], but likely since they usually use Steamworks.

**Conclusion.** Without the client, only DRM-free games run, and they report neither achievements
nor play time to Steam.

### 4.5 Cloud saves

- **Two systems.** The Cloud API (`ISteamRemoteStorage`), and Auto-Cloud, configured as roots
  (`WinMyDocuments`, `WinAppDataLocal`, `LinuxXdgDataHome`, …), patterns and root overrides. Steam
  syncs at launch and exit. [src] https://partner.steamgames.com/doc/features/cloud
- **Web API.** `ICloudService` needs a user OAuth token whose client id is requested from Valve
  per app, i.e. by a publisher. [src] https://partner.steamgames.com/doc/webapi/ICloudService,
  https://partner.steamgames.com/doc/webapi_overview/oauth
- **Client protocol.** `Cloud.EnumerateUserFiles`, `ClientFileDownload`, `ClientBeginFileUpload`,
  `ClientCommitFileUpload`, `ClientDeleteFile`, change numbers and per-file SHA-1.
  [src] https://github.com/SteamDatabase/Protobufs/blob/master/steam/steammessages_cloud.steamclient.proto
- **Precedent.** Aurelia (Rust, GPL-3.0, on steam-vent) syncs this way, and warns that accounts
  risk suspension. [src] https://github.com/Drackrath/Aurelia (README, last push 2026-10-01)
- **Fit with the existing code:**
  - A `SteamCloud` transport could implement `CloudTransport`, since the planner is generic [code].
  - The remote hash would be SHA-1 instead of MD5, and save roots come from appinfo `ufs`, not
    GOG's remote config.
  - The executor would need changes: `remote_map`'s location prefix, and `GameCloud.transport` is
    the concrete `GogCloud` [code].
- **Through the client (B).** Steam syncs by itself. SlattyLauncher's three-way sync, backups and
  conflict choices do not apply, and Steam's own conflict dialog takes over.

### 4.6 Achievements

| Feature | Steam |
|---|---|
| List with progress and rarity | Web API with a key: `GetPlayerAchievements`, `GetSchemaForGame`; `GetGlobalAchievementPercentagesForApp` needs no key [src] https://partner.steamgames.com/doc/webapi/ISteamUserStats. Or the client protocol (`CMsgClientGetUserStats`) [src] |
| Unlocks made in game | Only through the running client: `SetAchievement` then `StoreStats` go through it [src] https://partner.steamgames.com/doc/api/ISteamUserStats. With the client (B) it works with no extra component |
| Comet equivalent | None usable. Argon (last push 2022) and argonx (2020) were incomplete open `steamclient` replacements; OpenSteamClient wraps Valve's closed library and is deprecated [src] https://github.com/emily33901/Argon, https://github.com/OpenSteamClient/OpenSteamClient. Consequence: without the client, in-game unlocks are lost |
| Manual unlock or clear | No public API: `SetUserStatsForGame` needs the publisher's key, from a server [src]. The remaining ways are the client protocol (`CMsgClientStoreUserStats2`) or driving the client like Steam Achievement Manager, which needs the client running [src] https://github.com/gibbed/SteamAchievementManager. Both fall under SSA §4.B (unauthorized third-party software controlling Steam's processes) and §4.C (earning progress without genuine user input) [src]. **Excluded** |

### 4.7 Play time, launch options, post-install, Linux builds

- **Play time:**
  - Steam counts it when the game runs through the client [unverified for games run without it,
    but no API exists to report it].
  - `IPlayerService` only reads (`GetOwnedGames`, `GetRecentlyPlayedGames`, `GetSingleGamePlaytime`).
    [src] https://partner.steamgames.com/doc/webapi/IPlayerService
  - SSA §4.C lists faked play time as prohibited automation. Reporting it the way
    `playtime::report` does for GOG is **excluded**.
  - Local sessions can still be counted by SlattyLauncher (`session::playtime` [code]) and shown as
    "played here".
- **Launch options.** PICS `config.launch` lists them [src] (a PICS dump, not Valve docs). Through
  the client, Steam shows its own launch choice; arguments can be passed with
  `steam://run/<appid>//<args>/` [src Lutris].
- **Post-install.** See §4.3. With B, Steam runs it.
- **Native Linux builds.** Steam runs them in SLR (scout by default, sniper or 4.0 when chosen).
  Outside Steam, umu already ships sniper. [src] https://github.com/ValveSoftware/steam-runtime,
  umu README. The code already starts GOG's Linux builds in sniper through umu
  (`runner::native_command`). [code]

### 4.8 Living with an installed Steam client

- **Detection.** `steam_roots` and `steam_libraries` already exist [code]. `appmanifest_<appid>.acf`
  gives the installed games, `StateFlags` (4 = fully installed, 64 = running), `buildid` and size
  [src] Lutris `appmanifest.py`.
- **Crates.** `steamlocate` 2.1.1 (2026-08-13, MIT) and `keyvalues-parser` 0.2.4 (2026-05-17,
  MIT OR Apache-2.0) can read these files; checked on crates.io. The current hand-written parser in
  `settings::steam_libraries` may also do.
- **Import.** With B, every game installed by Steam appears without any import.
- **Writing into Steam's folders.** Making Steam adopt a game downloaded by SlattyLauncher (by
  writing `appmanifest` files) is what Aurelia does ("modifies Steam's files directly" [src]). It
  breaks the rule that SlattyLauncher never writes another program's data. **Excluded.**

## 5. Feature by feature

Classes:

- **N:** native in Rust;
- **X:** with an external tool;
- **C:** only with the Steam client installed (and running when the feature is used);
- **✗:** not feasible;
- **L:** legally problematic (the SSA clause is named).

| GOG feature (README) | Steam equivalent | Class |
|---|---|---|
| Sign-in in the system browser, tokens in the keyring | QR code with the mobile app over the client protocol, refresh token in the keyring. Without the mobile app, a password field would be needed (refused by design). With B, no sign-in at all | N (QR; protocol not documented by Valve, see SSA §2.G below), or C |
| Library with covers, offline cache | Installed games from local files (C); owned games through a Web API key (N, documented) or the client protocol (N, undocumented); covers from the store CDN | N / C |
| Install Windows builds, pause and resume, integrity checks | Natively: proven technically, L effort. Through the client: `steam://install`, pause in Steam's own interface | N (useless for games that need the client), or C |
| Native Linux builds | Same as Windows builds | N / C |
| Launch through umu + Proton, follow the session | DRM-free games: N. Others: only through the client (`-applaunch`, end read from the `reaper` process) | N for DRM-free games, C for the others |
| Launch options (choose a tool, change later) | Through the client: Steam's own choice; arguments through `steam://run` | C, partial |
| Cloud saves, three-way sync, conflicts, backups | Through the client: Steam's sync, not SlattyLauncher's. Natively: client protocol (Aurelia precedent) | C (Steam's rules), or N + L |
| Achievements: list | Web API with a key, or client protocol | N |
| Achievements: in-game unlocks | Only through the client | C |
| Achievements: manual unlock or clear | No public API; the other ways fall under SSA §4.B/§4.C | ✗ / L |
| Edit a game (title, cover, background, hide) | Local data only | N (identical) |
| Verify, repair, uninstall | Natively: feasible. Through the client: `steam://uninstall` [src], `steam://validate` [unverified] | N / C |
| Updates: detect and apply | Natively: chunk reuse, no binary patches. Through the client: automatic | N partial / C |
| DLC and language at install and later | Natively: depot filters. Through the client: Steam's interface only | N / C (Steam's interface) |
| Post-install setup, redistributables | Natively: partial (signed `installscript`, registry and processes under Wine). Through the client: done by Steam | N partial / C |
| Play time (read, report) | Read: Web API `GetOwnedGames`. Report: none, and SSA §4.C names faked play time | Read N; report ✗ / L |

### Steam Subscriber Agreement: the clauses involved

Source: https://store.steampowered.com/subscriber_agreement/english/, updated 2026-09-10. Facts
only, no legal conclusion.

- **§1.C:** the account and its password must not be shared. A QR flow and a client-driven flow do
  not share them.
- **§2.A:** content is licensed. To use it, the user "may be required to be running the Steam
  client" and to stay online.
- **§2.G:**
  - no reverse engineering of the Content and Services, or of software accessed via Steam, without
    written consent;
  - no emulating or redirecting of the communication protocols used by Valve in any network
    feature, through protocol emulation among other means.
  - Every native strategy (A, and the CM parts of C) speaks a protocol known from reverse
    engineering by others (SteamRE, SteamDatabase).
- **§4.B:**
  - no tampering with the execution of Steam unless authorized;
  - no unauthorized third-party software to interact with or control Steam's processes or user
    interface.
  - This concerns tools that drive the client beyond its documented hand-offs (Steam Achievement
    Manager, writing Steam's files). Whether `steam://` URLs and `-applaunch` count as authorized is
    not stated; Valve documents them on its developer wiki [unverified: page not readable].
- **§4.C (Automation):** no scripts or bots interacting with Steam, including "faking gameplay
  statistics (e.g., inflated wins or losses, XP, playtime)".
- **Web API terms:**
  - no storing of the user's password;
  - keep the key confidential;
  - 100,000 calls a day;
  - tell users what is stored.
  - https://steamcommunity.com/dev/apiterms
- **Precedent.** Aurelia, a third-party Steam client, warns its users of possible account action.
  [src] its README

## 6. Strategies

### A. All native: client protocol and downloads reimplemented

| | |
|---|---|
| **What it gives** | QR sign-in, licences and PICS, depot downloads with verify, repair, updates, branches, languages and DLC; cloud through the client protocol; achievement list |
| **What it cannot give** | Starting a game that uses Steamworks or the DRM wrapper, unless the Steam client is running anyway. In-game achievements, play time recorded by Steam. Steam's binary patches |
| **Cost** | XL. A CM client (WebSocket, protobuf, sessions), auth, PICS, depot keys, manifests (protobuf, AES), chunks (AES, LZMA, zstd, Adler-32), installscript under Wine, cloud with SHA-1 and change numbers. Possible base: steam-vent (MIT, 0.5.0 of 2026-04-03, no QR, no CDN) and steamroom (MIT/Apache-2.0, 0.3.0 of 2026-07-15, single author, written by LLM sessions from a GPL-2.0 project, by its own README: provenance risk), or porting SteamKit2 (LGPL-2.1, compatible with GPL-3.0) |
| **Risks** | SSA §2.G; account action (Aurelia's warning); protocol changes without notice; a big dependency surface reading untrusted data; DepotDownloader's code is GPL-2.0 (not "or later" in its headers), so it cannot be copied into GPL-3.0 code |
| **Parity** | Low where it matters: games install but most do not start without the client |

### B. Hybrid: read the local Steam installation, delegate to the Steam client

| | |
|---|---|
| **What it gives** | Installed Steam games in the library, with their state, size and build; install, uninstall and launch through `steam://` and `-applaunch`; session followed through the `reaper` process; cloud saves, in-game achievements, play time, updates, DLC and post-install handled by Steam. Plus, as options, the owned list and achievement progress through the user's own Web API key (documented, kept in the keyring, off until turned on, like SteamGridDB). Edit game, favorites, shelves and the 10,000-game grid unchanged |
| **What it cannot give** | Pause or resume, DLC, language and branch choices from SlattyLauncher's own panels (Steam's interface does them); SlattyLauncher's three-way cloud sync and conflict choices; a Proton build chosen per game from SlattyLauncher (it is in Steam's `config.vdf`, which SlattyLauncher must not write); manual achievements; play time reporting; anything without the client installed |
| **Cost** | M. VDF readers (crate or the existing parser), a `steam://` launcher, a `reaper` watcher, a Web API client (two or three documented methods), the multi-store refactor |
| **Risks** | Low on terms: documented hand-offs and read-only file access. Steam's local formats can change (`appinfo.vdf` went from 40 to 41 in 2024 [src]). Exit detection depends on a process name [src Lutris], which can change. The Flatpak path must be confirmed |
| **Parity** | Medium: most rows exist, but Steam performs them, with its own rules and windows |

### C. Supervised external tool for part of the work

| | |
|---|---|
| **Candidates** | SteamCMD (official, proprietary, nixpkgs `unfreeRedistributable`) for downloads; DepotDownloader (GPL-2.0, .NET, supports `-qr`) for downloads |
| **What it gives** | Installs without the client, with a clear process boundary, as with Comet and umu |
| **What it cannot give** | The same as A: games using Steamworks still need the client to start. And sign-in: SteamCMD prompts for the password (QR not found), DepotDownloader keeps its tokens in its own files (`-remember-password`), not in the keyring. Both break a security principle of SlattyLauncher unless the user signs in to the tool themselves, in a terminal, outside SlattyLauncher (as steam-tui asks [src] https://github.com/dmadisetti/steam-tui) |
| **Cost** | M to L |
| **Risks** | Same as A on the terms side for DepotDownloader; a proprietary binary with SteamCMD; a .NET runtime for DepotDownloader |
| **Parity** | Low, for the same reason as A |

### Recommendation: B

- **The deciding fact.** A Steam game that needs the client to start makes any native download
  pointless, and that covers most of the catalogue (§4.4). Only B gives working games, achievements
  and play time.
- **It follows the existing design decisions:**
  - "Proton, umu and Comet stay external, with explicit boundaries": the Steam client becomes one
    more external tool. Its boundary is documented URLs and command-line options, plus files read
    without ever being written.
  - "No password field", "no web view", "tokens only in the keyring": no Steam sign-in at all; at
    most a Web API key in the keyring, as for SteamGridDB.
  - "Safety comes before features" (AGENTS.md): B never writes Steam's files or its saves, and does
    not speak an undocumented protocol with the user's account.
- **Cost.** It is the only strategy whose cost (M) matches its value.
- **What B gives up has to be said in the README.** Steam games follow Steam's rules for cloud
  saves, updates and DLC, and manual achievements do not exist for them.

The native parts of A are not refused for technical reasons. If they are wanted later, they should
start only with the QR sign-in and the owned list (read only). Those risk the least and remove the
need for a Web API key. That choice is the owner's (see steam-plan.md, decisions).
