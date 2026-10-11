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
- [Games from no store](#games-from-no-store)
- [Steam games](#steam-games)
- [Proton builds](#proton-builds)
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
| `slatty auth logout --forget` | Also deletes what SlattyLauncher keeps of the account (below) |

Forgetting an account deletes, for that account only: its name, its play history (play time not
yet sent to GOG included), its cloud sync history, its library cache (covers, images, avatar) and
Comet's achievement records. Games, saves, prefixes, the home folders of isolated games and save
backups are never deleted, nor are the logs and the cloud copies made by `slatty cloud diff`. If
you sign in with the account again, the first cloud sync has no history to rely on: it compares
contents, adopts identical files and reports different ones as conflicts.

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
- the filter button shows games for Windows or Linux, with achievements, or with cloud saves;
- the **+** button adds a game from no store (see [Games from no store](#games-from-no-store)).

The size of the covers is set in Settings → Appearance.

Hovering a cover shows its title, a settings button and a play (or install) button. Clicking it
opens the game page; going back (the Library link or Escape) finds the library scrolled where you
left it. Another tab shows it from the top. The install button opens the install choices in a dialog over the library,
which stays where it was, and so does the settings button of an installed game; the game page has
the same choices in its drawers.

Right-click a cover and choose **Edit game** to change its **Title**, its **Sorting title** (used
when the library is sorted by name, for example "Witcher 3" for "The Witcher 3"), its **Cover**
and its **Background** (the key art of the game page). Click a picture to choose an image file (PNG, JPEG,
WebP, GIF or BMP); it is copied into SlattyLauncher's data, so the original can be moved or
deleted. With SteamGridDB turned on (Settings → Advanced), **From SteamGridDB** under a picture searches
SteamGridDB by name, starting from the title the library shows: pick the right game among those
found, then one of its covers (grids) or backgrounds (heroes); it is downloaded and shown, and kept
on **Save** like a chosen file. **Hide game** takes the game out of All, Installed, Favorites and search; it is then
listed only under Hidden games, where the same switch brings it back. Nothing changes until
**Save**; **Reset to default** goes back to GOG's title and images, and shows the game again.
On a game page, right-click the key art (not the title or the buttons over it) for the same menu;
the form then opens in a drawer beside the page.
These changes are kept apart from GOG's data, so refreshing the library keeps them.

**The game page** shows the key art across the window with the title, Play (Install when the game
is not installed) and the favorite button over it; below, three cards give play time and last
session, cloud save status, and achievement progress with the latest unlocks. Play time is the
total GOG records, so it includes GOG Galaxy and other launchers that report sessions. "Last played"
only knows sessions started by SlattyLauncher: GOG does not expose that date. A Steam game's page
gives Steam's own play time and last session instead, and a game owned on both adds them, or
shows one store's alone once preferred (see [Steam games](#steam-games)). The tools open in
panels:

| Where | Panel |
|---|---|
| Sliders button (top right) | Game settings, in a drawer beside the page: folder, launch option (for a game that has several), Proton build (used from the next launch; the platform for a Linux build), isolation from your files, game version (switch to an older or newer build), language and DLC |
| ⋮ button (top right) | Manage, in a drawer beside the page: verify, repair, check for update, uninstall. Verify, repair and updates show their progress and can be paused |
| Cloud saves card | A drawer beside the page: status, save folder, what a sync would do (upload, download, compare, unchanged, deleted on one side), Check, Sync now, conflict choices |
| Achievements card | Full list from the most common to the rarest (with unlock or clear, once turned on in Settings → Advanced), in a drawer beside the page (over it in a narrow window) |
| Install button | A drawer beside the page: version, download and disk size, free space, platform (Windows or Linux), folder, language, DLC, Proton and game version (Windows builds; the newest unless another is chosen), start, discard |
| Details, under Play | Session, in a drawer beside the page: each step of the launch and of the session (cloud check, Comet, start, end, play time), and Stop game while it runs |

Some products GOG lists in your library have nothing to install, such as a pack whose games are
in your library on their own. Their page shows **Not installable** instead of Install, with the
reason, once GOG has said so (one request to GOG's public product data, when the page opens);
from then on their cover offers no install either, and its settings button opens the page.

Once started, a download shows on the game page itself, with its progress, its percentage and its
network speed over the last seconds. Sizes and speeds read as in Steam: sizes in decimal megabytes
and gigabytes (1 GB = 1,000,000,000 bytes), the speed in megabits a second (Mbps) of data received,
which GOG sends compressed, while the size downloaded counts the game's files as written. The big button pauses it (then resumes it), and **Cancel** deletes it
after asking. The rest of the launcher stays usable meanwhile; the banner at the top of the Library
tab leads back to the game.

**Downloads tab.** The download button of the top bar (next to Settings) lists:

- the install downloading, with its folder, progress, size written, network speed and time left,
  and Pause and Cancel; downloads paused, cut off or failed are listed too, with Resume and Discard;
- the **queue**: an install started while another one downloads (its button then reads **Add to
  queue**) waits there instead of being refused. It starts by itself once the ones before it are done, and also after a failed or
  cancelled download. Drag a row by its handle to change the order, or remove it with ×. Pausing
  the running download holds the queue. The queue and its order are kept when SlattyLauncher
  closes; it goes on at the next start;
- the installs **completed** since SlattyLauncher started, with Play;
- with Steam's games on, the installs and updates the Steam client makes (**In Steam**, see
  [Steam games](#steam-games)).

Escape closes the panel, then the game page.

A download or an update cut off by a closed window, a crash or a power cut resumes by itself when
SlattyLauncher starts again (one download at a time). When GOG cannot be reached (offline, GOG
failing or limiting requests), the download stopped and the queue wait for it, each in its place,
rather than fail one after the other: the Downloads tab says so, GOG is asked again every minute,
and **Try now** asks at once. Those paused on request, and downloads stopped
by an error (tried again only when you ask, since they would most likely fail the same way), are
listed at the top of the Library tab instead: **Resume** or **Discard** a download, **Finish update** for an update,
which the game needs before it can start again, paused or not.

Nothing an interruption leaves behind is trusted: every file already on disk is checked again
against GOG's checksums before it is kept, and the game folder appears only once every file is
verified.

Closing the window while something runs (a download, an update or repair, a cloud sync, a game,
a Proton download) asks first and says what would be interrupted.

**Achievements** shows every game with achievements as a card (cover, unlocked count, share and
progress bar), the most completed first, then by title as in the library. Clicking a game opens its own
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
  Once every file is downloaded and the game folder is in place, the install can no longer be
  discarded, only finished: install the game again, then uninstall it if you do not want it.

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

1. SlattyLauncher looks the game up in umu's database, as Heroic does, in the copy that comes with
   its Proton build (UMU-Proton, GE-Proton and Proton-CachyOS carry one; Valve's builds apply no
   fixes): nothing leaves the computer. umu then applies the fixes its community wrote for that
   game (protonfixes), to the game only, not to its setup programs; a game the database does not
   list runs without them. With **Settings → Privacy → Proton fixes online** turned on (off by
   default), a game the copy does not list is looked up in umu's public database at its first
   launch, and the answer kept: only the GOG product id is sent. If that database cannot be
   reached, the game starts without fixes and the lookup is tried again next time. A fix is
   community code run with the game: some download what they install (redistributables from
   Microsoft and others) from other sites. umu itself asks Valve (`repo.steampowered.com`) at each
   launch whether its Steam Linux Runtime is current, and downloads it on first use.
   On the first launch of a fresh install, the Wine prefix is created (`wineboot`), so that save
   folders exist.
   The post-install setup that GOG Galaxy performs runs once per installed build: GOG's script
   interpreter or each product's setup program (registry entries and similar), then the shared
   redistributables the game declares (Visual C++, DirectX…), installed silently. Offline, or
   when a step fails, the game still starts and the setup runs again at the next launch; after
   three runs with a failed step it is left as it is, so that it does not slow every launch.
   `slatty setup <id> --dry-run` shows what it would run; `--force` runs it again.
2. Cloud saves are synchronised. If both sides changed, the launch stops and asks you to choose
   (see [cloud saves](#cloud-saves)). Offline, the game starts with your local saves. On the
   first launch this is where your saves from GOG's cloud arrive, right after the prefix is
   created and before the game starts.
3. Comet starts, so the game can report achievements: only for a game that ships GOG's Galaxy SDK
   (`Galaxy64.dll` and the like), and once **Achievements in game** is on in Settings → Privacy.
   Before the first session, Comet's dummy
   `GalaxyCommunication` service is registered in the prefix; some games need it to reach Comet.
4. The game runs. SlattyLauncher waits until **every** game process has exited, not only the
   launcher. With Comet, each achievement is told as the game unlocks it.
5. Cloud saves are uploaded, Comet stops, and the achievements GOG recorded during the session are
   listed.
6. The session is added to your play time on GOG, as GOG Galaxy does. Its length leaves out the
   time the computer was suspended, and does not change if the clock is set meanwhile. Sessions
   shorter than a minute are not sent (GOG ignores them). Offline, it is sent after the next session. A session
   GOG refuses outright is not sent again, and the ones after it still are.

Ctrl+C asks the game to quit; a second Ctrl+C forces it. Pressed before the game starts (Ctrl+C,
or Stop game in the interface), it keeps the game from starting. Options:

- `--no-cloud` skips step 2 and step 5's upload;
- `--no-comet` runs without achievements.

Game output goes to `~/.local/state/slatty/logs/game-<id>.log`, one session per file: the two
sessions before are kept as `game-<id>.log.1` and `.log.2`.

### Isolation from your files

An isolated game sees its own folder, its Wine prefix and a home folder of its own,
`~/.local/share/slatty/homes/<id>/`, instead of yours. Wine's `z:` drive then shows nothing of your
files. Windows games installed with Proton start isolated, unless **Isolate new Windows games** is
off in Settings → Privacy; Linux games do not, since their saves may be in your home folder. The
session says when a game runs isolated. A Proton build made to run outside the Steam Linux Runtime
(Proton-tkg, some custom builds: their `toolmanifest.vdf` names no runtime) cannot isolate a game,
which then refuses to start with it: choose another build, or turn isolation off for that game.

**Isolation** in a game's Game settings turns it on or off from its next launch; so does the
command line:

```sh
slatty isolation <game-id>          # show
slatty isolation <game-id> --off    # access to your files, from the next launch
slatty isolation <game-id> --on
```

- An isolated launch needs bubblewrap (`bwrap`), which keeps umu's runtime, the Proton build and
  the other tools the game runs through read-only: a game cannot change what other games run.
  The runtime is then copied at each launch rather than linked: about a second more on btrfs or
  XFS, about 750 MB written on ext4.
- An isolated game sees and signals only its own processes, not your other programs.
- An isolated Linux game runs in umu's Steam Linux Runtime (`umu-run` is needed), on NixOS too,
  instead of `steam-run`. It does not find the saves it made in your home folder: it starts from
  its own. To carry on with a save, copy it to the same place under its home folder.
- An isolated game reaches no service over D-Bus. A Linux game that keeps the screen awake that
  way, or asks GameMode for performance, no longer can; a Windows game no longer sees Bluetooth
  devices through Wine (gamepads are not affected).
- A game run through Wine alone (imported with `--runner wine`) cannot be isolated.
- An isolated launch is refused, with the reason, when the container could not keep its promise:
  a game folder whose path contains `:` (the container cannot be told such a path), a file in the
  game's folder named like the program umu is given (`sh`, `sc`, `Z:\…\game.exe`), or a working
  directory that is a link out of the game folder. Rename or remove what the message names, or
  turn isolation off for that game.
- Proton's logs (`PROTON_LOG=1`) of an isolated game go to its own home folder.
- Isolation keeps a game out of your files, not a hostile program out of your system: see
  [SECURITY.md](../SECURITY.md#games-and-isolation).

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
slatty update --automatic off  # or on: automatic updates (on until turned off)
```

Games are updated without asking while **automatic updates** are on, which they are until turned
off, but only those on the newest build. A game on an older build you chose (in the Install panel
or as its game version) stays on it and is updated only when you ask; updating it, or choosing the
newest build, makes it follow updates again. A language or DLC change does not change this.
Checking a Windows game takes one request to GOG (the list of builds).

The interface checks these games at start, at sign-in and every six hours while it stays open, and
applies the updates it finds one after the other, never while a game runs, an install downloads or
cloud saves sync; they wait in the **Updates** part of the Downloads tab, where the one being
applied shows its progress and **Pause**. While a game updates, its page shows **Updating** instead
of Play. A paused update waits until you apply it again from Manage, and the games waiting after it
wait for the next check; one stopped by a closed window is finished at the next start, as any
update. A game you change while it waits (another version, an update, an uninstall) is not
updated by itself afterwards. The switch is in Settings → Installs.

An update compares each file of the new build with the one on disk:

- unchanged files are kept;
- changed and new files are downloaded and replaced one by one, atomically;
- files the new build no longer contains are removed, but only those SlattyLauncher installed.
  Anything else in the game folder is left alone.

If an update is interrupted, the game cannot be launched until `slatty update <game-id>` completes
it. The command resumes with the same build and only downloads what is still missing. An update
refused before any file changes (not enough disk space, GOG not answering) leaves the game playable
as it was. The space check counts a file that keeps its size as already there, and each replaced
file is written beside the old one before it takes its place: an update that changes large files
without changing their size can still run out of space part way. It then stops, and resumes once
space is freed.

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
  `users` folder is copied to `~/.local/share/slatty/backups/prefixes/` first, with Wine's registry
  files (where Unity games keep their progress) and `C:\ProgramData` beside it, and only the prefix
  SlattyLauncher made for that game (`prefixes/<game-id>`) is ever deleted.

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

## Games from no store

Games bought elsewhere, or made by yourself, can join the library by the program that starts
them. In the interface, the **+** button above the library opens **Add a game**: the program
(typed, or chosen with Browse), a title (the program's name until you give one), its arguments,
the game's folder when the program sits deeper in it, the Proton build for a Windows `.exe`, and
whether it runs isolated. The game's page then offers Play, Game settings and Edit game like any
other; Manage → **Remove from library** takes it out, asking first. On the command line:

```sh
slatty add <program> --title <title> [--folder <folder>] [--working-dir <folder>] [--proton <Proton dir>] [--isolated | --not-isolated] [-- <arguments>]
slatty add --installer <setup.exe> [--proton <Proton dir>] [--isolated | --not-isolated]
slatty add --installer <installer.sh | archive> --title <title> [--into <folder>]
slatty add <program in its prefix> --title <title> --installed-by <game-id>
slatty remove <game-id> [--delete-prefix]
```

- A game that comes as an installer or an archive is installed first: **Install it first, from an
  installer or archive** in Add a game, or `slatty add --installer`.
  - A Windows installer runs in a Wine prefix made for the game (finish it in its own window).
    Once it ends, the program is picked where it installed the game, in the prefix's `drive_c`.
    Isolated, the installer can write in its own folder only.
  - A Linux installer (MojoSetup `.sh`, the format of GOG's offline installers, also sold
    elsewhere) or archive (`.zip`, `.tar.gz`, `.tar.xz`) is unpacked, never run, into a folder
    named after the title in the default installation path (or the one given). Of a MojoSetup
    installer, only the game's files are taken. Paths that climb out of the folder and links
    that lead out are left out. The program is then picked in that folder.
  - `--installed-by` names the game the installer ran for. Closing the dialog before adding it,
    or `slatty remove` on that id, throws away what was installed (a prefix's saves, its `users`
    folder, registry and `C:\ProgramData`, copied to the backups first).
- A game moved elsewhere is found again with **Locate…** in its Game settings (Folder): its
  program must be at the same place in the folder chosen.
- A game unpacked by SlattyLauncher keeps the list of the files it unpacked, with their size and
  modification time: removing it deletes those files and only those, while they are as they were
  unpacked. What the game wrote in its folder since (saves, settings), a file it shipped with
  and rewrote included, stays.

- A Windows program (`.exe`) runs through umu and Proton, in a Wine prefix of its own made at its
  first launch; a Linux program, script or AppImage runs as GOG's Linux builds do (see
  [Playing](#playing)). Which one it is comes from the file itself, not its name.
- The game's folder is the program's own unless `--folder` names one holding it; an isolated game
  can write in it, so a folder through which it would reach more is refused (see SECURITY.md:
  your home folder or one holding it, a folder of programs, `~/.config`, SlattyLauncher's own
  folders…): put the game in a folder of its own. Isolation
  is as for a game installed now (Settings → Installs) unless told.
- The game gets the id `local-<number>`, listed by `slatty library list` (no GOG account needed),
  and is played with `slatty launch`, like any other.
- It has no cloud saves and no achievements, and GOG is never told of it: its play time stays on
  this computer. umu's database is not asked for its fixes.
- `slatty add` says so when the folder holds a GOG game: installed from GOG, it would have its cloud
  saves and achievements.
- `slatty remove` takes it out of the library and never deletes its files, which are yours. With
  `--delete-prefix`, the prefix SlattyLauncher made for it is deleted too, its saves (its `users`
  folder, registry and `C:\ProgramData`) copied to the backups first.

## Steam games

The games of the account signed in to the Steam client can show in the library beside GOG's, off
until turned on: in **Settings → Steam**, or on the GOG sign-in page with **Use my Steam games
without GOG**, which needs no GOG account at all.

```sh
slatty steam status    # Steam found, the account signed in, its games and those installed
slatty library list    # Steam's games too once turned on, with or without a GOG account
```

- **Read from Steam's own files, never written.** SlattyLauncher finds Steam (installed natively or
  as a Flatpak; with both, the one an account signed in to last) and the account last signed in to
  it, then reads the account's games
  (`userdata/<account>/config/librarycache`), their names and images (`appcache/appinfo.vdf` and
  the client's image cache) and the games installed in each Steam library folder
  (`appmanifest_*.acf`). No Steam password is ever asked for, and a key only for the Web API
  (below), which is optional.
- **Images from Steam**, off until turned on in Settings → Steam: the covers, backgrounds and logos
  the Steam client has not saved on this computer (games it never showed), and the icons of
  achievements, are downloaded from
  Steam's image servers (`shared.steamstatic.com`), without any account, and kept in
  `~/.cache/slatty/steam/`. Off, those games show without them. With both this and the Web API
  off, nothing at all is asked of Steam's servers.
- **Run by the Steam client.** Play, Install, **Verify files in Steam** and **Uninstall in Steam**
  hand the game to the Steam client (`steam://` addresses, opened by the desktop): it must be
  installed and signed in, and does the work in its own window. What it installed or removed shows
  once the launcher's window is in front again.
- **Steam's downloads.** An install or update the client makes shows on the game's page
  (Downloading, Paused or Queued in Steam) and in the Downloads tab under **In Steam**, with its
  progress, and a notification says when it is done. They are read from the game's manifest
  (`appmanifest_<id>.acf`) every five seconds while one downloads; once it stayed the same for two
  minutes (Steam closed, or busy with another), every thirty seconds while the launcher's window is
  in front, and not at all while it is behind (it is read again as the window comes to the
  front). Steam's own window pauses or cancels them.
- **Play time and last played** are Steam's own records for the account
  (`userdata/<account>/config/localconfig.vdf`), shown on the game page and used by the library's
  sorts with GOG's; they are never sent to GOG. They are read again when a game Steam ran ends,
  and when the launcher's window is in front again.
- **A game running.** After Play, and while the Steam client runs one of its games (even one
  started from Steam itself, seen once the launcher's window is in front again), its page shows
  **Running**: every two seconds meanwhile, SlattyLauncher looks for the client's `reaper` process
  of that game among your processes (it reads the name of each, and the arguments of the
  client's `reaper` alone). Steam cannot be asked to stop a game: close it in the game.
- **Steam Web API**, off until turned on in Settings → Steam, fills in what the Steam client has
  not saved on this computer: the account's games it has not described yet (they show without
  their systems, so the Windows and Linux filters leave them out), the play time and last session
  of games never played here, and the achievements of games it keeps none of (asked when shown;
  how common each is comes with them). It needs your own key, created on
  [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey): paste it in **Web API
  key**; it goes to the system keyring and is never shown again. The key and your Steam id go to
  `api.steampowered.com` in each request's address, as the API takes them. A profile whose game
  details are private may get nothing back.
- **Achievements** show in the Achievements tab with GOG's games, and on the game's page, as the
  Steam client keeps them for the account (`appcache/stats`): those of a game whose page was shown
  in Steam or that was played on this computer. How common each is shows for the few the client
  knows. Their icons come from Steam's image servers with **Images from Steam** on; without it,
  a trophy stands in. They are never unlocked or cleared from SlattyLauncher, even with Manual
  achievements on.
- **What Steam keeps.** Updates, cloud saves, achievements in game and launch options are the Steam
  client's; Steam's games cannot run isolated from your files.
- **GOG is the better choice where you have it.** GOG's games run isolated from your files, need no
  client running, and their saves and achievements reach GOG without one: SlattyLauncher
  recommends their GOG build for your privacy and security.
- **A game you own on both** shows once, as its GOG game: matched by the Steam release GOG's
  database gives for it, read with the library. A game the database does not describe, or a
  library refreshed before this version until the next **Refresh library**, is matched by a title
  written alike (case, spaces and punctuation aside); one the database knows no Steam release of
  is not matched, even by title: a Steam game of the same name is then another game.
  Its page offers **Play the Steam version**, or **Install the Steam version**, through Steam,
  under the GOG build's own buttons; it counts as installed when either is; its play time adds
  the sessions of both, or is one store's once preferred in Settings → Library (**Play time on
  both**: Steam + GOG, Prefer Steam or Prefer GOG; the other store's while the preferred one
  records none), and its last session is the latest of either. Its achievements and cloud
  saves are GOG's.
- Steam's games can be renamed, illustrated, hidden and marked as favorites like GOG's. A game the
  Steam client has no details of yet (some recently added games) does not show until it has, or
  until the Web API names it. If
  Steam changes the format of its list of apps (`appcache/appinfo.vdf`), no Steam game can show
  until SlattyLauncher reads the new one: Settings → Steam and `slatty steam status` say so.

## Proton builds

SlattyLauncher can download the newest build of three Proton projects from their GitHub releases:
GE-Proton, Proton-CachyOS (its x86-64-v3 build on processors that have those instructions) and
UMU-Proton. GitHub is contacted only once **Proton downloads** is turned on, in Settings → Runners
or with `slatty proton downloads on`; turning it off in Settings stops a download running.

```sh
slatty proton downloads on
slatty proton available
slatty proton install ge-proton        # or proton-cachyos, umu-proton
slatty proton list
slatty proton remove GE-Proton11-7-x86_64
slatty proton updates on               # keep <project>-latest up to date, once a day at start
slatty proton update                   # or now
slatty proton share on                 # list the downloaded builds in Steam and Heroic
```

- Each archive is checked against the SHA-512 sum its release publishes before it is opened, then
  unpacked in `~/.local/share/slatty/protons/<name>`. A download stopped half way (Ctrl+C, a lost
  connection) goes on from where it was the next time.
- Downloaded builds appear first in every Proton menu, beside those of Steam and umu, with one
  entry per project that always leads to its newest downloaded build: `GE-Proton-latest`,
  `Proton-CachyOS-latest`, `UMU-Proton-latest`. A game or the default Proton set to one of them
  runs the newest build from its next launch; one set to a numbered build keeps it.
- **Keep up to date** (Settings → Runners, or `slatty proton updates on`), off until turned on,
  checks once a day at start the projects a game or the default follows as `-latest`, downloads a
  newer build in the background and switches to it once it is checked and unpacked. A `-latest`
  link never goes back to an older version, even when a project publishes one last (CachyOS's
  `10.0-sunset` came out between two 11.0 builds). A link whose build was deleted by hand leads
  to the newest build left at the next start. A game already
  running keeps the build it started with. While it is on, the Proton menus offer a downloaded
  build through its `-latest` link only. The build before stays, for a game a new build breaks:
  turn **Keep up to date** off for a moment to choose that build by its number for the game, then
  on again; the game keeps it, and no update deletes a build a game uses. Older builds no game
  uses are deleted. A project that cannot be updated
  (GitHub refusing, a release without its sum) is reported and leaves the others to be updated;
  when GitHub cannot be reached at all, the check is made again at the next start. **Update now**
  and `slatty proton update` do the same at once.
- Proton upgrades a game's Wine prefix when its build changes, as under Steam; going back to an
  older build afterwards usually works but is less tested.
- A build is kept until you delete it, even once a newer one is out, unless an update deletes it as
  above. Deleting one is refused while a game or the default Proton uses it, or while games follow
  the `-latest` link that leads to it.
- **Share with Steam and Heroic** (Settings → Runners, or `slatty proton share on`), off until
  turned on, links each downloaded build into `~/.local/share/Steam/compatibilitytools.d`, where
  Steam and Heroic list custom builds (Steam after a restart). Only the builds themselves are
  shared, by their numbered names, not the `-latest` links; a name already there is left alone.
  The links follow the builds (a download adds one, a deletion removes one) and go when the
  setting is turned off. Steam installed as a Flatpak cannot see them. A build that cannot be
  linked is only left out of Steam and Heroic; the launcher says so and carries on.
  SlattyLauncher only knows which builds its own games use: a build that **Keep up to date**
  deletes, or that you delete, disappears from Steam and Heroic too, even if one of their games
  was set to it; choose another build for that game in Steam or Heroic.
- Valve's own builds (Proton Experimental, stable, Hotfix) need Steam's client library: an
  isolated game run with one is given `steamclient.so` from Steam's `linux64` and `linux32`
  folders, read-only, and nothing else of Steam. The other builds run without it. A game that is
  not isolated finds it in your own `~/.steam/sdk64`, which only Steam installed natively makes:
  with Steam's Flatpak alone, such a launch is refused and says to turn isolation on or to choose
  another build.

## Settings

The interface's **Settings** tab lists its parts on the left (Account, Library, Steam, Installs,
Runners, Appearance, Notifications, Privacy, Advanced, About); choosing one brings it to the top, and the list follows the page as it
scrolls. **Steam** is described in [Steam games](#steam-games). **Account** shows who is signed in, with
Log out, and Forget account, which asks first, then does what `slatty auth logout --forget` does
(see [Signing in](#signing-in)). **Library** shows how many games you own on GOG and when the list was refreshed, with Refresh
library, and the size of the covers; with Steam's games on, **Play time on both** says what a
game owned on GOG and on Steam shows: both added up (the default), or one store's.
**Installs** holds:

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
- **Update games automatically:** on until turned off; see [Updates](#updates).

`slatty install --dir` and `--proton` set the same values.

**Notifications** tells what happens in the background, as Steam does: a download started,
complete, failed or waiting for GOG; a game or Proton update applied or failed; an achievement
unlocked in a game, as it unlocks (one only GOG reports, when the game ends); cloud saves needing
attention before a game or left out of the upload after it. While
the window is in front, they show as cards in its bottom right corner for a few seconds, a click
opening the game's page; otherwise (a game in front, the window hidden) as the desktop's own
notifications, with the game's cover. A switch turns them all off, and one each kind; all are on
until turned off.

**Runners** holds **Proton downloads**, off until turned on (see [Proton builds](#proton-builds)).
Once on, **Check GitHub** shows the newest build of GE-Proton, Proton-CachyOS and UMU-Proton, each
with **Install**; a download shows how far it is and can be stopped, and goes on from there the
next time. **Keep up to date**, off until turned on, updates the builds games follow as
`<project>-latest`; **Update now** does it at once. **Downloaded** lists the builds SlattyLauncher
downloaded, with **Delete**.

**Privacy** has four switches:

- **Proton fixes online**, off until turned on: a game that the copy of umu's database in its
  Proton build does not list is looked up in umu's public database at its first launch, sending
  its GOG id; off, such a game runs without fixes.
- **Achievements in game**, off until turned on: Comet runs while a game that uses GOG's Galaxy
  runs. While it runs, it keeps a connection to GOG open for your account, and any program on
  this computer can get your GOG access token from it (see
  [SECURITY.md](../SECURITY.md#comet)). Off, unlocks made in games are not reported; unlocking by
  hand does not need it.
- **Isolate new Windows games**, on until turned off: Windows games installed from then on run
  [isolated from your files](#isolation-from-your-files); each game's Game settings changes it.
- **Play time on GOG**, on until turned off: each session goes to GOG so it counts on your
  profile, as Galaxy does; sessions played while it is off are never sent.

SlattyLauncher has no telemetry. [SECURITY.md](../SECURITY.md#what-leaves-your-computer) lists
everything that leaves your computer, and when.

**Advanced** holds **Manual achievements**, off until turned on: it adds Unlock, Clear and Unlock
all beside a game's achievements (see [Achievements](#achievements)). **SteamGridDB**, off until turned
on, offers covers and backgrounds from [SteamGridDB](https://www.steamgriddb.com) in Edit game. It
needs your own SteamGridDB API key (free, created on its site under Preferences → API): paste it in
**API key**; it goes to the system keyring and is never shown again. Searching sends the name you
type and the key to steamgriddb.com.

**Appearance** picks a built-in theme (Carbonfox, Everforest, Pastel Glow, Gruvbox Dark
or Light) and the interface font, applied at once and kept, and shows the theme file, `~/.config/slatty/theme.toml`: create it, edit it, and reload it to change colours,
corners and the page transition. See [theming](theming.md).

## Where data is stored

| Path | Content |
|---|---|
| System keyring, entry `slatty-launcher` / `gog:<user id>` | Session tokens |
| System keyring, entry `slatty-launcher` / `key:steamgriddb` | Your SteamGridDB API key, once saved |
| System keyring, entry `slatty-launcher` / `key:steam-web-api` | Your Steam Web API key, once saved |
| `~/.local/share/slatty/state.db` | Accounts, installed games, sessions, cloud sync history, settings, the titles you gave games and the games you hid |
| `~/.local/share/slatty/prefixes/<id>/` | Wine prefixes of installed games (most saves live here) |
| `~/.local/share/slatty/protons/<name>/` | Proton builds downloaded from GitHub |
| `~/.local/share/slatty/homes/<id>/` | The home folder of an isolated game: what it writes there, a Linux game's saves among them. Kept when the game is uninstalled |
| `~/.local/share/slatty/manifests/<id>.json` | Files installed for each game |
| `~/.local/share/slatty/added/<id>.json` | What a game added by hand starts: its program, arguments and working directory |
| `~/.local/share/slatty/custom/<id>/` | Covers and backgrounds you chose for a game |
| `~/.local/share/slatty/backups/` | Copies made before any save is replaced or a prefix deleted |
| `~/.local/share/slatty/diagnostics/` | Cloud copies downloaded by `slatty cloud diff` |
| `~/.local/share/slatty/comet/`, `~/.config/slatty/comet/` | Comet's data and configuration |
| `~/.cache/slatty/<user id>/` | Library and covers; safe to delete |
| `~/.cache/slatty/steam/` | Images of Steam's games downloaded from Steam's image servers, once turned on; safe to delete |
| `~/.local/state/slatty/logs/` | Game, prefix creation, setup, Galaxy service and Comet logs: the last run of each and the two before (`.1`, `.2`); a game's go when it is uninstalled |

Read a log before sharing it: a game's log holds whatever the game printed, which can include a
GOG ticket of its own, and Comet's holds game client identifiers.

## Troubleshooting

| Symptom | Check |
|---|---|
| `slatty doctor` reports the Comet port as busy | Another Comet is running, usually Heroic's. Close the game in Heroic. |
| "secret storage unavailable" | A Secret Service keyring must be running and unlocked. |
| A game does not start | `~/.local/state/slatty/logs/game-<id>.log`. `slatty launch-spec <id>` shows the exact command. |
| The first launch fails while creating the prefix | `~/.local/state/slatty/logs/prefix-<id>.log` |
| A game misbehaves since it runs isolated (missing files, settings not found) | `slatty isolation <id> --off`, and tell us which game. |
| "bubblewrap (bwrap) is needed to run a game isolated", or an isolated game does not start | Install bubblewrap (Flatpak needs it too). `slatty doctor` says whether it can start a sandbox, and one inside another, as an isolated game needs; where it cannot, the launch says why. On Ubuntu, see [below](#ubuntu-and-apparmor). `slatty isolation <id> --off` runs the game without isolation, and without its protection. |
| Achievements are not reported | `~/.local/state/slatty/logs/comet.log`. `slatty doctor` must find `GalaxyCommunication.exe`; the launch output says whether the Galaxy service was registered. |
| A cloud conflict blocks the launch | `slatty cloud diff <id>`, then `slatty cloud sync <id> --prefer local` or `--prefer remote`. |
| The session was interrupted (crash, power loss) | The next launch reports it. Check `slatty cloud status <id>` before playing. |
| The window looks wrong or slow after a graphics driver change | It starts with Vulkan alone once Vulkan was found to drive the graphics card, and goes back to every backend by itself after one start if Vulkan stops working. To choose yourself, start it with `WGPU_BACKEND=vulkan`, `gl` or `vulkan,gl`. |
| Something went wrong without an explanation | Start `slatty` or `slatty-gui` with `SLATTY_LOG=warn` to print warnings on the terminal, such as a binary patch that could not be used. |
| "another operation on this game is running" | An install, update, repair, uninstall, cloud sync or game session of that game is still running, maybe in another SlattyLauncher window or terminal, or the game itself still runs after SlattyLauncher was closed. Wait for it to finish. |

### Ubuntu and AppArmor

Since Ubuntu 23.10 (24.04 LTS included), a program may start a sandbox only if an AppArmor
profile allows it, and bubblewrap has none on Ubuntu 24.04 as installed. umu needs bubblewrap for
every Windows game, isolated or not, as Heroic and Lutris do; an isolated game also needs it to
start a sandbox inside another. Until bubblewrap has a profile, `slatty doctor` reports it and
launches say so. Choose one of these, from the most to the least protective, then check with
`slatty doctor`:

1. **Ubuntu's own profile for bubblewrap**, the one umu's Ubuntu package uses. It lets bubblewrap
   start a sandbox, but the programs inside it cannot get privileges. On Ubuntu 25.04 and later it
   is installed and active already; on 24.04:

   ```sh
   sudo apt install apparmor-profiles
   sudo install -m 644 /usr/share/apparmor/extra-profiles/bwrap-userns-restrict /etc/apparmor.d/
   sudo apparmor_parser -r /etc/apparmor.d/bwrap-userns-restrict
   ```

   Windows games then start. Isolated games may not, if the profile keeps the sandbox inside
   another from starting: `slatty doctor` tells. Turn isolation off for them, or use the next
   choice.
2. **A profile with no restriction for bubblewrap**: it and the programs it starts may create user
   namespaces, as before Ubuntu 23.10. Save this as `/etc/apparmor.d/bwrap`, then run
   `sudo apparmor_parser -r /etc/apparmor.d/bwrap`:

   ```
   abi <abi/4.0>,
   include <tunables/global>

   profile bwrap /usr/bin/bwrap flags=(unconfined) {
     userns,
     include if exists <local/bwrap>
   }
   ```
3. **Turning the restriction off for the whole system**
   (`kernel.apparmor_restrict_unprivileged_userns = 0` in a file of `/etc/sysctl.d/`). Every
   program may then create user namespaces again: not recommended.

These steps come from Ubuntu's AppArmor profiles and from what umu, Lutris and Heroic users found;
they have not been tried with SlattyLauncher on Ubuntu yet.
