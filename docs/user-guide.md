# User guide

SlattyLauncher has two front ends that share the same core:

- `slatty`, a command-line tool, which also exposes diagnostics;
- `slatty-gui`, the graphical interface.

Anything done in one is visible in the other.

## Contents

- [Signing in](#signing-in)
- [Library](#library)
- [The interface](#the-interface)
- [Installing games](#installing-games)
- [Playing](#playing)
- [Cloud saves](#cloud-saves)
- [Achievements](#achievements)
- [Maintenance](#maintenance)
- [Games installed elsewhere](#games-installed-elsewhere)
- [Settings](#settings)
- [Where data is stored](#where-data-is-stored)
- [Troubleshooting](#troubleshooting)

## Signing in

```sh
slatty auth login
```

Your browser opens GOG's sign-in page. SlattyLauncher never sees your password. After signing in,
the browser lands on an almost blank page on `embed.gog.com`. Copy that page's full address and
paste it into the terminal (or into the interface, which has a Paste button).

The address contains a one-time code. SlattyLauncher exchanges it for tokens and stores them in
your system keyring, never in a plain file.

| Command | Effect |
|---|---|
| `slatty auth status` | Shows the account and when the current access token expires |
| `slatty auth refresh` | Renews the session now (it is renewed automatically when needed) |
| `slatty auth logout` | Removes the tokens from the keyring; cached data and saves stay |

## Library

```sh
slatty library sync           # download the library and covers
slatty library list           # list from the cache, works offline
slatty library list witcher   # filter by title
```

## The interface

`slatty-gui` has three tabs in its top bar: **Library**, **Achievements** and **Settings** (the gear).
Your GOG avatar, top right, also opens Settings; it comes from your public GOG profile, and its green
dot says you are signed in.

**Library** shows your games as covers. Above the grid:

- the shelf menu shows **All** games, the **Installed** ones or your **Favorites**, and
  **Hidden games** once you have hidden one;
- the sort menu orders by name, most recently played or most played, and is kept for the next
  start;
- the filter button shows games for Windows or Linux, with achievements, or with cloud saves.

The size of the covers is set in Settings → Appearance.

Hovering a cover shows its title, a settings button and a play (or install) button. Clicking it
opens the game page. The install button opens the install choices in a dialog over the library,
which stays where it was, and so does the settings button of an installed game; the game page has
the same choices in its drawers.

Right-click a cover and choose **Edit game** to change its **Title**, its **Sorting title** (used
when the library is sorted by name, for example "Witcher 3" for "The Witcher 3"), its **Cover**
and its **Background** (the key art of the game page). Click a picture to choose an image file (PNG, JPEG,
WebP, GIF or BMP); it is copied into SlattyLauncher's data, so the original can be moved or
deleted. **Hide game** takes the game out of All, Installed, Favorites and search; it is then
listed only under Hidden games, where the same switch brings it back. Nothing changes until
**Save**; **Reset to default** goes back to GOG's title and images, and shows the game again.
On a game page, right-click the key art (not the title or the buttons over it) for the same menu;
the form then opens in a drawer beside the page.
These changes are kept apart from GOG's data, so refreshing the library keeps them.

**The game page** shows the key art across the window with the title, Play (Install when the game
is not installed) and the favorite button over it; below, three cards give play time and last
session, cloud save status, and achievement progress with the latest unlocks. Play time is the
total GOG records, so it includes GOG Galaxy and other launchers that report sessions. "Last played"
only knows sessions started by SlattyLauncher: GOG does not expose that date. The tools open in
panels:

| Where | Panel |
|---|---|
| Sliders button (top right) | Game settings, in a drawer beside the page: folder, launch option (for a game that has several), Proton build (used from the next launch; the platform for a Linux build), game version (switch to an older or newer build), language and DLC |
| ⋮ button (top right) | Manage, in a drawer beside the page: verify, repair, check for update, uninstall. Verify, repair and updates show their progress and can be paused |
| Cloud saves card | A drawer beside the page: status, save folder, what a sync would do (upload, download, compare, unchanged, deleted on one side), Check, Sync now, conflict choices |
| Achievements card | Full list from the most common to the rarest (with unlock or clear, once turned on in Settings → Advanced), in a drawer beside the page (over it in a narrow window) |
| Install button | A drawer beside the page: version, download and disk size, free space, platform (Windows or Linux), folder, language, DLC, Proton and game version (Windows builds; the newest unless another is chosen), start, discard |
| Details, under Play | Session, in a drawer beside the page: each step of the launch and of the session (cloud check, Comet, start, end, play time), and Stop game while it runs |

Some products GOG lists in your library have nothing to install, such as a pack whose games are
in your library on their own. Their page shows **Not installable** instead of Install, with the
reason, once GOG has said so (one request to GOG's public product data, when the page opens).

Once started, a download shows on the game page itself, with its progress, its percentage and its
speed over the last seconds. The big button pauses it (then resumes it), and **Cancel** deletes it
after asking. The rest of the launcher stays usable meanwhile; the banner at the top of the Library
tab leads back to the game.

**Downloads tab.** The download button of the top bar (next to Settings) lists:

- the install downloading, with its folder, progress, size downloaded, speed and time left, and
  Pause and Cancel; downloads paused or cut off are listed too, with Resume and Discard;
- the **queue**: an install started while another one downloads waits there instead of being
  refused. It starts by itself once the ones before it are done, and also after a failed or
  cancelled download. Drag a row by its handle to change the order, or remove it with ×. Pausing
  the running download holds the queue. The queue and its order are kept when SlattyLauncher
  closes; it goes on at the next start;
- the installs **completed** since SlattyLauncher started, with Play.

Escape closes the panel, then the game page.

A download or an update cut off by a closed window, a crash or a power cut resumes by itself when
SlattyLauncher starts again (one download at a time). Those paused on request are listed at the top
of the Library tab instead: **Resume** or **Discard** a download, **Finish update** for an update,
which the game needs before it can start again, paused or not.

Nothing an interruption leaves behind is trusted: every file already on disk is checked again
against GOG's checksums before it is kept, and the game folder appears only once every file is
verified.

Closing the window while something runs (a download, an update or repair, a cloud sync, a game)
asks first and says what would be interrupted.

**Achievements** shows every game with achievements as a card (cover, unlocked count, share and
progress bar), the most completed first. Clicking a game opens its own
achievements page in the same tab, where you can also unlock or clear them once **Manual achievements** is on in Settings → Advanced. SlattyLauncher reads which games
have achievements and cloud saves from GOG in the background and keeps the answer in
`~/.cache/slatty/<user id>/overview.json`; **Refresh** reads it again.

## Installing games

SlattyLauncher installs the **Windows** build of a game from GOG's Galaxy content system and runs it
through Proton, or its **native Linux** build when GOG offers one.

**Windows or Linux.** For a game GOG offers on both, the Install panel has a **Platform** menu. It
starts on the default platform from Settings, Windows until you change it. On the command line, use
`--platform linux` or `--platform windows`. A Linux build:

- comes from GOG's offline Linux installer. Only the game's own files are read out of it, one by
  one, so nothing else is downloaded and no copy of the installer is kept. Each file is checked
  against the installer's CRC-32 before it is kept;
- runs natively through its `start.sh`, without Proton or a Wine prefix. On NixOS, GOG's scripts
  ask for `/bin/bash` and the games load libraries (OpenGL, sound) that NixOS does not keep in the
  usual places, so the game runs in a usual Linux layout: through `steam-run` when installed
  (Steam provides it, and the `steam-run-free` package without Steam), else through umu without
  Proton, in the Steam Linux Runtime 3.0 that umu downloads for Windows games anyway. No Steam is
  needed. Elsewhere, an interpreter missing at the path the script names is looked up in `PATH`;
- has no cloud saves: GOG lists save folders for Windows and macOS builds only;
- reports no achievements: GOG's Linux builds do not include the Galaxy SDK that Comet talks to;
- comes in the single version GOG offers, so there is no **Game version** choice. Its DLC are
  their own Linux installers, installed into the game folder.

An interrupted install resumes on the platform it started with.

```sh
slatty install <game-id> --info
```

`--info` shows the version, the download and disk size, the languages offered and the
redistributables the game declares. It downloads nothing (for a Linux build, it reads the list of
files from the installer).

```sh
slatty install <game-id> --proton ~/.local/share/Steam/compatibilitytools.d/<Proton build> --dir ~/Games/GOG
```

`--proton` and `--dir` are remembered, so later installs need only the game id. The default folder
is `~/Games/GOG`. Use `--language fr-FR` (or any language listed by `--info`) for another language.

In the interface, the Install panel shows the free space on the drive that would hold the game
(in red when it is short) and starts from the default installation path and the default
Proton set in Settings. **Install in** changes the folder for this game only: type a path, or use
**Browse** to pick a folder with your desktop's file chooser (through the XDG desktop portal). The
game gets its own subfolder there, shown below the field. An interrupted install keeps the folder it
started in. **Proton** picks the build this game runs with; the game settings panel can change it
later, from the next launch on, keeping the game's prefix.

**DLC.** Every owned DLC is installed by default, as Galaxy does. Use `--no-dlc` for the base game
only, or `--dlc <id>…` to pick. `--info` lists the DLC of the build, with their size and whether you
own them.

How an install behaves:

- **Staged download.** Files go to a hidden `.<Game>.slatty-partial` folder next to the
  destination. The game folder appears only once every file has been verified.
- **Integrity checks.** Every chunk is checked twice against GOG's checksums, before and after
  decompression. A Linux build's files are checked against the installer's CRC-32.
- **Pause and resume.** Ctrl+C (or Pause in the interface) stops the download. Running the same
  command again resumes it: files already on disk are re-checked and kept when correct.
- **Safety refusals.** Missing disk space, an existing destination folder, or a file path that
  would escape the game folder are refused before anything is downloaded.
- `slatty installs` lists installed games and interrupted installs.
- `slatty install <game-id> --cancel` abandons an interrupted install. It deletes only its hidden
  partial folder. In the interface, use **Discard download** in the Install panel, or **Cancel**
  while it downloads: it asks first, then stops the download and deletes what was downloaded.

Dependencies that ship files into the game folder are installed with the game. GOG's installer
scripts ("support" files) are kept in `~/.local/share/slatty/support/<id>/`; they are used by the
setup that runs at first launch (see below).

## Playing

```sh
slatty launch <game-id>
```

**Launch options.** Some games can be started several ways: the game, and tools such as a
configuration program (GOG lists them in the game's `goggame-<id>.info`). The first Play of such a
game asks which one to start, and the choice is kept; **Launch** in Game settings changes it. The
command line starts the one chosen, or the game itself.

A launch goes through these steps:

1. On the first launch, SlattyLauncher looks the game up in umu's public database, as Heroic does,
   and keeps the answer. umu then applies the fixes its community wrote for that game (protonfixes);
   a game the database does not list runs without them. Only the GOG product id is sent. If the
   database cannot be reached, the game starts without fixes and the lookup is tried again next
   time.
   On the first launch of a fresh install, the Wine prefix is created (`wineboot`), so that save
   folders exist.
   The post-install setup that GOG Galaxy performs runs once per installed build: GOG's script
   interpreter or each product's setup program (registry entries and similar), then the shared
   redistributables the game declares (Visual C++, DirectX…), installed silently. Offline or on
   failure the game still starts and the setup is retried at the next launch. `slatty setup <id>
   --dry-run` shows what it would run; `--force` runs it again.
2. Cloud saves are synchronised. If both sides changed, the launch stops and asks you to choose
   (see [cloud saves](#cloud-saves)). Offline, the game starts with your local saves. On the
   first launch this is where your saves from GOG's cloud arrive, right after the prefix is
   created and before the game starts.
3. Comet starts, so the game can report achievements: only for a game that ships GOG's Galaxy SDK
   (`Galaxy64.dll` and the like), and unless **Achievements in game** is off in Settings → Privacy.
   Before the first session, Comet's dummy
   `GalaxyCommunication` service is registered in the prefix; some games need it to reach Comet.
4. The game runs. SlattyLauncher waits until **every** game process has exited, not only the
   launcher.
5. Cloud saves are uploaded, Comet stops, and newly recorded achievements are listed.
6. The session is added to your play time on GOG, as GOG Galaxy does. Sessions shorter than a
   minute are not sent (GOG ignores them). Offline, it is sent after the next session.

Ctrl+C asks the game to quit; a second Ctrl+C forces it. Options:

- `--no-cloud` skips step 2 and step 5's upload;
- `--no-comet` runs without achievements.

Game output goes to `~/.local/state/slatty/logs/game-<id>.log`.

## Cloud saves

```sh
slatty cloud status <game-id>     # what a sync would do, changes nothing
slatty cloud sync <game-id>
slatty cloud diff <game-id>       # compare local and cloud copies file by file
```

Sync runs automatically around each game session. When the same file changed on both sides,
nothing is overwritten. Resolve it explicitly:

```sh
slatty cloud sync <game-id> --prefer local    # keep yours, the cloud copy is backed up
slatty cloud sync <game-id> --prefer remote   # keep the cloud one, yours is backed up
```

Deleting a file in the cloud, or locally, needs `--allow-deletions`. It is refused whenever a folder
looks empty or moved. Every replaced file is backed up first; the backup folder is printed.

A save that changes during the sync, because the game or anything else writes it, is left as it
is and reported; sync again once it has stopped changing. [docs/cloud-saves.md](cloud-saves.md)
explains the rules in detail.

If you switch GOG accounts while a game runs, its saves and achievements are not checked with the
new account when the session ends: sign back into the account that started it, then sync.

## Achievements

```sh
slatty achievements <game-id>
```

This lists achievements as GOG records them, with how common each one is.

When a game is launched by SlattyLauncher, unlocks made in the game are sent to GOG by Comet.
Support depends on the game and its Galaxy SDK version; see
[docs/compatibility.md](compatibility.md).

Achievements can also be changed manually, for any game you own, installed or not. In the
interface, turn on **Manual achievements** in Settings → Advanced (off until then): Unlock, Clear and
Unlock all then appear beside a game's achievements. On the command line:

```sh
slatty achievements <game-id> --unlock NEGLECT "Steel Soul"
slatty achievements <game-id> --unlock-all
slatty achievements <game-id> --clear NEGLECT
```

Achievements are matched by key, id or exact name. You are asked to confirm before anything is
written. Manual changes appear on your public GOG profile, dated today, and are probably against
GOG's terms of use. A game that keeps its own record of its achievements can unlock a cleared
one again the next time it runs.

## Updates

```sh
slatty update                  # check every game installed by SlattyLauncher
slatty update <game-id> --check
slatty update <game-id>        # apply
```

An update compares each file of the new build with the one on disk:

- unchanged files are kept;
- changed and new files are downloaded and replaced one by one, atomically;
- files the new build no longer contains are removed, but only those SlattyLauncher installed.
  Anything else in the game folder is left alone.

If an update is interrupted, the game cannot be launched until `slatty update <game-id>` completes
it. The command resumes with the same build and only downloads what is still missing.

When GOG publishes a binary patch from the installed build to the new one, changed files are rebuilt
from the installed version and a small delta, often a few hundred bytes for a file of several
megabytes. Each rebuilt file must match GOG's checksum; otherwise it is downloaded normally. GOG
seems to publish patches only between consecutive builds, so skipping several versions downloads
the changed files instead.

Otherwise, when a file changed only in places, the parts that did not change are copied from the
installed file instead of downloaded. GOG cuts files into chunks (10 MiB on the games checked so far), so a
small change still costs at least one chunk. Repairs work the same way: `slatty verify --repair`
downloads only the damaged chunks of a damaged file.

The interface offers **Check for update** and **Update now** in the game's Manage panel (⋮).

## Language and DLC after installing

```sh
slatty content <game-id>                      # current language, offered languages, DLC
slatty content <game-id> --language fr-FR
slatty content <game-id> --add-dlc <id>
slatty content <game-id> --remove-dlc <id>
```

These changes work like updates:

- only the files that differ are downloaded;
- only files that SlattyLauncher installed and that are no longer needed are removed.

They stay on the installed build. If GOG no longer offers that build, update the game first. In the
interface, use the game settings panel (sliders button): it names the installed language and, when
GOG offers others, lets you switch.

Many games (Hollow Knight, Undertale) come as a single download that holds every language. GOG
then lists only one, and the language is chosen in the game's own options.

## Maintenance

```sh
slatty verify <game-id>              # check every file against the installed build
slatty verify <game-id> --repair     # download damaged or missing files again
slatty uninstall <game-id>
slatty uninstall <game-id> --delete-prefix
```

`uninstall` works only for games installed by SlattyLauncher:

- It deletes only the files it installed. Anything else in the game folder is kept and listed:
  saves stored there, mods, configuration files.
- The Wine prefix, where most saves live, is kept unless you pass `--delete-prefix`. Even then, its
  `users` folder is copied to `~/.local/share/slatty/backups/prefixes/` first.

The interface offers the same actions in the game's Manage panel (⋮).

## Games installed elsewhere

```sh
slatty import <folder> --runner umu --proton <Proton dir> --prefix <prefix>
slatty import --from-heroic <game-id>
slatty forget <game-id>
```

- `slatty import` registers a game installed by another tool, so it can be launched and synced.
- `--from-heroic` reads Heroic's records without changing them.
- `slatty forget` drops any game from SlattyLauncher's records without touching its files.

Imported games cannot be verified or uninstalled, because SlattyLauncher does not know which files
belong to them.

## Settings

The interface's **Settings** tab lists its parts on the left (Account, Library, Installs,
Appearance, Privacy, Advanced, About); choosing one brings it to the top, and the list follows the page as it
scrolls. **Account** shows who is signed in, with
Log out. **Library** shows how many games you own and when the list was refreshed, with Refresh
library, and the size of the covers. **Installs** holds:

- **Default installation path:** where new games are installed unless the Install panel says
  otherwise. Type it, or pick a folder with **Browse**; it is saved as soon as it is an absolute
  path.
- **Default platform:** the build installed for a game GOG offers on both Windows and Linux,
  Windows until changed. Each install can choose the other one.
- **Default Proton:** the build new installs start with. The menus list custom builds from
  `~/.local/share/Steam/compatibilitytools.d`, Valve's builds (Proton Experimental, stable,
  Hotfix) that Steam downloaded in any of its libraries, and those umu downloaded. Valve's builds
  are kept up to date by Steam; a game set to one that Steam removes needs another one chosen in its
  settings. Each game can use another build.

`slatty install --dir` and `--proton` set the same values.

**Privacy** has three switches, all on until turned off: **Proton fixes** (a game's GOG id goes to
umu's public database at its first launch, to pick its community fixes; off, the game runs without
them), **Achievements in game** (Comet runs while a game that uses GOG's Galaxy runs; off,
unlocks made in games are not reported; unlocking by hand does not need it) and **Play time on GOG** (each session goes to GOG so it counts on your profile, as Galaxy
does; sessions played while it is off are never sent). SlattyLauncher has no telemetry.

**Advanced** holds **Manual achievements**, off until turned on: it adds Unlock, Clear and Unlock
all beside a game's achievements (see [Achievements](#achievements)).

**Appearance** picks a built-in theme (Carbonfox, Everforest, Pastel Glow, Gruvbox Dark
or Light) and the interface font, applied at once and kept, and shows the theme file, `~/.config/slatty/theme.toml`: create it, edit it, and reload it to change colours,
corners and the page transition. See [theming](theming.md).

## Where data is stored

| Path | Content |
|---|---|
| System keyring, entry `slatty-launcher` / `gog:<user id>` | Session tokens |
| `~/.local/share/slatty/state.db` | Accounts, installed games, sessions, cloud sync history, settings, the titles you gave games and the games you hid |
| `~/.local/share/slatty/prefixes/<id>/` | Wine prefixes of installed games (most saves live here) |
| `~/.local/share/slatty/manifests/<id>.json` | Files installed for each game |
| `~/.local/share/slatty/custom/<id>/` | Covers and backgrounds you chose for a game |
| `~/.local/share/slatty/backups/` | Copies made before any save is replaced or a prefix deleted |
| `~/.local/share/slatty/diagnostics/` | Cloud copies downloaded by `slatty cloud diff` |
| `~/.local/share/slatty/comet/`, `~/.config/slatty/comet/` | Comet's data and configuration |
| `~/.cache/slatty/<user id>/` | Library and covers; safe to delete |
| `~/.local/state/slatty/logs/` | Game, prefix creation, setup, Galaxy service and Comet logs |

Comet's log may contain game client identifiers; review it before sharing.

## Troubleshooting

| Symptom | Check |
|---|---|
| `slatty doctor` reports the Comet port as busy | Another Comet is running, usually Heroic's. Close the game in Heroic. |
| "secret storage unavailable" | A Secret Service keyring must be running and unlocked. |
| A game does not start | `~/.local/state/slatty/logs/game-<id>.log`. `slatty launch-spec <id>` shows the exact command. |
| The first launch fails while creating the prefix | `~/.local/state/slatty/logs/prefix-<id>.log` |
| Achievements are not reported | `~/.local/state/slatty/logs/comet.log`. `slatty doctor` must find `GalaxyCommunication.exe`; the launch output says whether the Galaxy service was registered. |
| A cloud conflict blocks the launch | `slatty cloud diff <id>`, then `slatty cloud sync <id> --prefer local` or `--prefer remote`. |
| The session was interrupted (crash, power loss) | The next launch reports it. Check `slatty cloud status <id>` before playing. |
| Something went wrong without an explanation | Start `slatty` or `slatty-gui` with `SLATTY_LOG=warn` to print warnings on the terminal, such as a binary patch that could not be used. |
| "another operation on this game is running" | An install, update, repair, uninstall, cloud sync or game session of that game is still running, maybe in another SlattyLauncher window or terminal. Wait for it to finish. |
