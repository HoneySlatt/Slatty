# User guide

Slatty comes as two programs that share everything: `slatty-gui`, the window, and
`slatty`, the [command line](command-line.md). A game installed from one shows in the other. This
guide follows the window, and gives the command where there is one.

## Contents

- [Getting started](#getting-started)
- [The window](#the-window)
- [Installing GOG games](#installing-gog-games)
- [Playing](#playing)
- [Cloud saves](#cloud-saves)
- [Achievements](#achievements)
- [Updates, languages and DLC](#updates-languages-and-dlc)
- [Verify, repair, uninstall](#verify-repair-uninstall)
- [Offline installers](#offline-installers)
- [Steam games](#steam-games)
- [Games added by hand](#games-added-by-hand)
- [Emulated games](#emulated-games)
- [Games installed elsewhere](#games-installed-elsewhere)
- [Proton builds](#proton-builds)
- [Couch mode](#couch-mode)
- [Settings](#settings)
- [Where data is stored](#where-data-is-stored)
- [Troubleshooting](#troubleshooting)

## Getting started

### Checking your system

```sh
slatty doctor
```

It checks umu-launcher, bubblewrap (and whether it can start a sandbox inside another, as
isolated games need), Comet and its port, the Galaxy service file, the keyring and the runtime
folder. Each line says what is missing and what it is for.

### Signing in

Click **Open the GOG sign-in page** (or run `slatty auth login`). Your browser opens GOG's own
sign-in page; Slatty never sees your password. Once signed in, the browser lands on an
almost blank page on `embed.gog.com`: copy its full address, paste it into Slatty
(**Paste**, then **Sign in**), and you are in.

That address holds a one-time code, which Slatty exchanges for a session. The session is
kept in your system keyring, never in a file of its own. GOG accepts no other way for a launcher
other than Galaxy to sign in, which is why the address is pasted back.

`slatty auth status` shows the account and when its session expires; it renews itself.

### Without a GOG account

A GOG account is not needed for your Steam games or for games you add by hand. On the sign-in
page, **Use my Steam games without GOG** shows the library with them alone; Settings → Account
offers **Sign in to GOG** later.

### Signing out and forgetting an account

**Log out** (Settings → Account, or `slatty auth logout`) deletes the session from the keyring.
Your library cache, games and saves stay.

**Forget account** (Settings → Account, or `slatty auth logout --forget`) asks first, then also
deletes what Slatty keeps of that account: its name, its play history (play time not yet
sent to GOG included), its cloud sync history, its library cache (covers, images, avatar) and
Comet's record of its achievements. It never deletes games, saves, prefixes, the home folders of
isolated games, save backups, logs, or the cloud copies `slatty cloud diff` made. The play history
of games added by hand belongs to no account and stays too.

Signing in again with a forgotten account starts the cloud sync without history: identical files
are adopted, different ones reported as conflicts for you to settle.

## The window

The top bar holds four tabs: **Library**, **Achievements**, the download button for the
**Downloads** tab, and the gear for **Settings**; the gamepad beside them opens the
[couch mode](#couch-mode). The search field (**Search a game**) filters
the library by title. Your GOG avatar, top right, comes from your public GOG profile; it also opens
Settings, and its green dot says you are signed in.

### Library

Your games, as covers. Above the grid:

- **Shelves**: **All**, **Installed**, **Favorites**, and **Hidden games** once you hid one.
- **Sort**: **Name A–Z**, **Name Z–A**, **Recently played** or **Most played**. Numbers sort by
  value (Game 2 before Game 10). The choice is kept.
- **Filters**:
  - by store: **GOG**, **Steam**, **Added** (by hand). Ticking several shows the games of any of
    them; a game owned on both GOG and Steam counts for both;
  - **Windows**, **Linux**, **Has achievements**, **Has cloud saves**: ticking several shows the
    games that match all of them (Windows and Linux: games offered for both).

  With **Remember filters** on (Settings → Library), they come back as you left them at the next
  start.
- **+** adds a game from no store (see [Games added by hand](#games-added-by-hand)).

Hovering a cover shows its title, a settings button and Play (or Install). Clicking a cover opens
the game's page; going back (the **Library** link or Escape) finds the grid scrolled where you
left it. The install and settings buttons open their choices in a dialog over the library, which
stays in place.

Cover size is set in Settings → Library, from 75% to 160%. Covers and key art are drawn from
copies made at the size they are shown, so they stay sharp at every size and screen scale.

### A game's page

The game's key art fills the top of the page, with its title, the big button (**Play**,
**Install**, **Resume**, **Updating**…) and the favorite button. Below, cards give:

- **Time played** and **Last played**. Play time is the total GOG records, so it counts sessions
  from GOG Galaxy and other launchers too. GOG keeps no "last played" date that a launcher can
  read: that one only counts sessions started from Slatty;
- the **Cloud saves** status;
- **Achievements**: how many are unlocked, and the latest ones.

A Steam game shows Steam's own play time and **Runs through Steam** instead of cloud saves; a game
added by hand shows an **Added by hand** card instead of cloud saves and achievements (see
[Steam games](#steam-games) and [Games added by hand](#games-added-by-hand)). The footer gives the
installed version and size.

The tools open in drawers beside the page:

| Where | Drawer |
|---|---|
| Install button | Version, download and disk size, free space, platform, folder, language, DLC, Proton and game version; Start or Discard |
| Sliders (top right, installed games) | **Game settings**: folder, launch option, Proton build, controller compatibility, isolation, game version, language and DLC |
| ⋮ (top right, GOG games and games added by hand) | **Manage**: verify, repair, check for update, uninstall. Verify, repair and updates show their progress and can be paused. For a GOG game, installed or not, **Download installers…** opens **Offline installers** (see [Offline installers](#offline-installers)) |
| Cloud saves card | Status, save folder, what a sync would do, **Check**, **Sync now**, conflict choices |
| Achievements card | Every achievement, from the most common to the rarest, with Unlock and Clear once allowed |
| **Details**, under Play | **Session**: each step of the launch and of the session, and **Stop game** while it runs |

Escape closes the drawer, then the page.

### Editing a game

Right-click a cover, or a game page's key art, and choose **Edit game**. You can change:

- the **Title**, and the **Sorting title** used when sorting by name ("Witcher 3" for "The
  Witcher 3");
- the **Cover** and the **Background** (the key art of the game page): click a picture to pick an
  image file (PNG, JPEG, WebP, GIF or BMP), copied into Slatty's data so the original can
  go. With SteamGridDB turned on (Settings → Advanced), **From SteamGridDB** searches its
  community's covers and backgrounds by name;
- **Hide game**: out of every shelf and search, listed only under Hidden games, where the same
  switch brings it back.

Nothing changes until **Save**. **Reset to default** goes back to the store's title and images and
shows the game again. These changes are kept apart from GOG's data, so refreshing the library keeps
them.

### Achievements tab

Every game with achievements, as a card with its cover, how many are unlocked and a progress bar,
the most completed first. Clicking one opens its achievements page. Which games have achievements
and cloud saves is read from GOG in the background and kept; **Refresh** reads it again.

### Downloads tab

- **Downloading**: the install or the offline installers in progress, with their folder, progress,
  size written, speed and time left, **Pause** (and **Cancel** for an install); and the downloads
  paused, cut off or failed, with **Resume** and **Discard** (**Remove** for installers).
- **Queue**: installs and offline installers started while another download runs (the button then
  reads **Add to queue**). One download runs at a time. Each starts once the ones before it are
  done, failed or cancelled. Drag a row by its handle to reorder, or take it out with ×: a row
  that never started is forgotten; one whose download had begun (resumed while another download
  ran, cut off, or paused by Slatty for a game or for **Download now**) keeps what it
  downloaded and goes back among the stopped downloads, paused, with **Resume** and **Discard**
  (**Remove** for installers). Set aside so, it does not hold the queue until you pause it again.
  **Download now** starts a row at once: the download running is paused and waits right after it,
  then goes on by itself. While a game runs, downloads wait for it to close (**Pause downloads
  while playing**, Settings → Installs); a Steam game too, once Slatty sees Steam run it
  (it looks while a game it started runs, and as its window comes to the front). Pausing the
  running download yourself holds the queue; when Download now started it, the download it paused
  goes on all the same.
  The queue is kept when Slatty closes and goes on at the next start.
- **Updates**: games waiting for their automatic update, and the one being applied.
- **Completed**: installs finished since Slatty started, with **Play**.
- **In Steam**, with Steam's games on: what the Steam client is downloading.

Sizes read as in Steam: in decimal megabytes and gigabytes, and the speed in megabits a second of
what arrives from the network (GOG sends it compressed, so it can differ from the size written).

### Notifications

A download done or failed, an update applied, an achievement unlocked in a game, cloud saves
needing attention: Slatty tells you as Steam does. While its window is in front, a card
shows in the corner for a few seconds (click it to open the game); otherwise, a desktop
notification with the game's cover. Each kind can be turned off in Settings → Notifications.

## Installing GOG games

Slatty installs a game's **Windows** build from GOG's Galaxy content system and runs it
through Proton, or its **native Linux** build when GOG offers one.

### Windows or Linux

For a game GOG offers on both, the Install drawer has a **Platform** menu, starting on the
**Default platform** of Settings → Installs (Windows until you change it). On the command line:
`--platform linux`.

A Linux build:

- comes from GOG's offline Linux installer, of which only the game's own files are downloaded, one
  by one, each checked against the installer's CRC-32. No copy of the installer is kept;
- runs natively through its `start.sh`, without Proton. On NixOS, where GOG's scripts and the
  libraries games load are not where they expect, it runs through `steam-run` (from Steam, or the
  `steam-run-free` package without Steam), else in umu's Steam Linux Runtime. Elsewhere, an
  interpreter missing at the path the script names is looked up in `PATH`;
- has no cloud saves (GOG lists save folders for Windows and macOS builds only) and reports no
  achievements (GOG's Linux builds lack the Galaxy SDK);
- comes in the single version GOG offers. Its DLC are installers of their own, installed into the
  game folder.

### The Install drawer

It shows the version, the download and disk sizes, and the free space on the drive (in red when
short). It starts from the **Default installation path** and **Default Proton** of Settings:

- **Install in** changes the folder for this game: type a path, or **Browse** with your desktop's
  folder chooser. The game gets its own folder inside, shown below the field;
- **Proton**: the build this game runs with (Game settings changes it later, keeping the prefix);
- **Language**: the languages GOG offers for this build. Many games come as one download holding
  every language: GOG then lists one, and the language is chosen in the game's own options;
- **DLC**: every DLC you own is installed by default, as Galaxy does;
- **Game version** (Windows builds): the newest unless you pick an older one, which then stays
  until you update it.

```sh
slatty install <id> --info        # what would be installed; downloads nothing
slatty install <id>               # install, with the remembered folder and Proton build
```

Some products in your library have nothing to install, such as a pack whose games you own on their
own. Their page shows **Not installable**, with the reason, once GOG has said so.

### While it downloads

The download shows on the game's page with its progress and speed; the big button pauses and
resumes it, and **Cancel** deletes it after asking. The rest of the launcher stays usable.

Files go into a hidden `.<Game>.slatty-partial` folder beside the destination, and the game folder
appears only once every file is verified. Every piece is checked against GOG's checksums, before
and after decompression. Missing disk space, an existing destination folder, or a path that would
leave the game folder are refused before anything is downloaded.

Closing the window while a download, an update, a cloud sync, a game or a Proton download runs
asks first and says what would be interrupted.

### When a download is interrupted

- **Cut off** (window closed, crash, power cut): it resumes by itself when Slatty starts
  again, signed in to GOG, one download at a time. On the command line, run the same `slatty
  install` again.
- **GOG out of reach** (offline, GOG failing or limiting requests): the download and the queue wait
  in place rather than failing one after the other. GOG is asked again every minute; **Try now**
  asks at once.
- **Paused on request (Pause, or × on a download in the queue that had started), or stopped by an
  error**: listed at the top of the Library tab and in the Downloads tab, with **Resume** and
  **Discard**. An error is tried again only when you ask, since it would most likely fail the same
  way.

A resumed download keeps what it already has. Small files are checked again; files of 1 MiB or
more were written to disk before taking their name, and are kept as they are when their size is
right. If GOG replaced a game's Linux installer meanwhile, the download starts again from the
beginning on the new one. A download whose folder is gone (a network share not mounted, a drive
unplugged) stops as failed rather than start over on another disk.

**Discard download** (or `slatty install <id> --cancel`) deletes only the hidden partial folder.
Once every file is in place, an install can only be finished, not discarded: install the game
again, then uninstall it if you do not want it.

## Playing

Press **Play** (or run `slatty launch <id>`).

### What happens when you press Play

1. **Proton fixes.** The game is looked up in umu's database, in the copy that comes with its
   Proton build (UMU-Proton, GE-Proton and Proton-CachyOS carry one), so umu applies the fixes its
   community wrote for that game. Nothing leaves the computer for this, unless **Proton fixes
   online** is on (see [Settings](#privacy)).
2. **First launch only:** the Wine prefix is created, then GOG's post-install setup runs once per
   installed build: GOG's install scripts or setup programs, then the redistributables the game
   declares (Visual C++, DirectX…), silently. Offline, or when a step fails, the game still starts
   and the setup runs again next time, three times at most. `slatty setup <id> --dry-run` shows
   what it would run.
3. **Cloud saves** are synced. If the same file changed on both sides, the launch stops and asks
   you to choose. Offline, the game starts with your local saves. At the first launch, this is
   where your saves from GOG's cloud arrive.
4. **Comet** starts, so the game can report achievements: only for a Windows game that ships
   GOG's Galaxy SDK, and only once **Achievements in game** is on (Settings → Privacy). Some games
   need Comet's Galaxy service in their prefix to reach it: it is registered before the first such
   session.
5. **The game runs.** The session lasts until **every** game process has ended, not only the first
   one. Achievements are shown as the game unlocks them.
6. **After the game:** Comet stops, cloud saves are uploaded, the session is added to your play
   time on GOG (sessions under a minute are not sent; GOG ignores them), and the achievements GOG
   recorded are listed.

A game's output goes to its log (see [Where data is stored](#where-data-is-stored)).

### Launch options

Some games can be started several ways: the game, and tools such as a configuration program. The
first Play of such a game asks which one to start, and the choice is kept; **Launch** in Game
settings changes it.

### Controller compatibility

For a Windows game run through Proton, **Game settings → Controller → Controller compatibility**
can help when a controller is not recognized, for example a DualSense in a game expecting Xbox
input. It is **off by default for every game**. Turn it on only for games that need it; the choice
is saved per game, kept when changing Proton builds, and applies from the next launch in both
the interface and the command line.

This uses Proton's SDL input mode, supported by UMU-Proton, GE-Proton and Proton-CachyOS; Valve's
Proton builds (Proton Experimental, Proton 10.0) have no such mode, and the switch changes nothing
with them.
It does not need Steam running. Games may show Xbox button icons, and native PlayStation features
such as adaptive triggers may be unavailable. It does not provide Steam Input's remapping or
profiles.
Native Linux games, games run through Wine alone, and Steam-managed games do not use this option.

Started with `PROTON_ADD_CONFIG=sdlinput` or `PROTON_USE_SDL=1` in its environment, Slatty passes
them to every game, whatever its switch: start it without them to let the switch decide.

### Stopping a game

**Stop game** in the Session drawer (or Ctrl+C on the command line) asks the game to quit; a second
time forces it. Pressed before the game started, it keeps the game from starting.

### Isolation from your files

An isolated game sees its own folder, its Wine prefix and a home folder of its own instead of
yours: Wine's `z:` drive shows nothing of your files, and the game sees only its own processes.
Windows games run through Proton start isolated, unless **Isolate new Windows games** is off in
Settings → Privacy when they are installed. Linux games do not, since their saves may be in your
home folder. The Session drawer says when a game runs isolated.

**Isolation** in Game settings turns it on or off for one game, from its next launch:

```sh
slatty isolation <id>          # show
slatty isolation <id> --off    # access to your files
slatty isolation <id> --on
```

What to know:

- It needs bubblewrap (`bwrap`). The tools the game runs through (umu's runtime, the Proton build)
  are then read-only to it, so a game cannot change what other games run. They are copied at each
  launch instead of linked: about a second more on btrfs or XFS, about 750 MB written on ext4.
- A Proton build made to run outside the Steam Linux Runtime (Proton-tkg, some custom builds)
  cannot isolate a game: pick another build, or turn isolation off for that game.
- An isolated Linux game runs in umu's Steam Linux Runtime (`umu-run` is needed), on NixOS too.
  It does not find the saves it made in your home folder: copy them to the same place under its
  own home folder.
- An isolated game reaches no service over D-Bus: a Linux game cannot keep the screen awake that
  way or ask GameMode for performance, and a Windows game does not see Bluetooth devices through
  Wine (gamepads are not affected).
- A game run through Wine alone (imported with `--runner wine`) cannot be isolated.
- A launch is refused, with the reason, when isolation could not hold: a game folder whose path
  contains `:`, a file in the game's folder named like the program umu is given (`sh`, `sc`,
  `Z:\…\game.exe`), or a working directory that is a link out of the game folder. Rename or remove
  what the message names, or turn isolation off for that game.
- Proton's logs (`PROTON_LOG=1`) of an isolated game go to its own home folder.

Isolation keeps a game out of your files; it is not a wall against a program written to break out.
[SECURITY.md](../SECURITY.md#games-and-isolation) says what it leaves open.

## Cloud saves

Saves sync by themselves before and after each game session. The game page's Cloud saves card
opens a drawer with what a sync would do, **Check** and **Sync now**.

```sh
slatty cloud status <id>     # what a sync would do; changes nothing
slatty cloud sync <id>
slatty cloud diff <id>       # local and cloud copies, file by file
```

When the same file changed on both sides, nothing is overwritten: the drawer (or `--prefer local`
or `--prefer remote`) lets you choose, and the copy you do not keep goes to the backups. Every
file replaced is backed up first, on either side. Deletions are passed on only with
`slatty cloud sync <id> --allow-deletions`, and never when a folder looks empty or moved.

A save that changes during the sync, because the game or anything else writes it, is left alone
and reported: sync again once it stops changing. If you switch GOG accounts while a game runs, its
saves and achievements are not checked with the new account at the end: sign back in, then sync.

[Cloud saves](cloud-saves.md) explains every rule.

## Achievements

The Achievements tab and each game's page list achievements as GOG records them, with how common
each one is. A game whose achievements are all unlocked says **Completed** in place of 100%, its
progress lit with the theme's accent, there and in the couch mode.

**Earned in game:** with **Achievements in game** on (Settings → Privacy, off until you turn it
on), Comet runs beside Windows games that ship GOG's Galaxy SDK and sends what they unlock to GOG,
as GOG Galaxy does. While it runs, any program on this computer can get your GOG access token from
it: [SECURITY.md](../SECURITY.md#comet) explains. Support depends on the game; see Comet's
[compatibility list](https://github.com/imLinguin/comet/blob/main/docs/wiki/Game-Compatibility.md).

**By hand:** with **Manual achievements** on (Settings → Advanced), **Unlock**, **Clear** and
**Unlock all** show beside a game's achievements, for any game you own, installed or not. On the
command line:

```sh
slatty achievements <id> --unlock NEGLECT "Steel Soul"
slatty achievements <id> --unlock-all
slatty achievements <id> --clear NEGLECT
```

Achievements are matched by key, id or exact name, and you are asked before anything changes.
Manual changes show on your public GOG profile, dated today, and are probably against GOG's terms
of use. A game that keeps its own record can unlock a cleared achievement again when it next runs.

## Updates, languages and DLC

### Updates

With **Update games automatically** on (Settings → Installs, on until turned off), the window
checks the games Slatty installed at start, at sign-in and every six hours, and updates
those on their newest build one after the other, never while a game runs, an install or offline
installers download, or saves sync. They wait in the Updates part of the Downloads tab; while a game updates, its page
shows **Updating** instead of Play. With **Pause downloads while playing** on, an update being
applied when a game starts pauses, and goes on once the game closes (at the next start if
Slatty is closed first).

A game on an older build you chose stays on it until you update it, or choose the newest build
again. **Check for update** and **Update now** are in the game's Manage drawer.

```sh
slatty update                  # check every game
slatty update <id> --check
slatty update <id>             # apply, or finish an interrupted update
```

An update replaces only what changed: unchanged files are kept; a changed file is rebuilt from
GOG's binary patch when there is one, or its unchanged parts are copied from the installed file
and only the rest is downloaded; files the new build no longer has are removed, but only those
Slatty installed.

An update cut off half way keeps the game from starting until it is finished: it resumes at the
next start, or with `slatty update <id>`. A paused one waits for **Finish update**, at the top of
the Library tab, or **Update now** in Manage. One refused or paused before any file changed (not
enough space, GOG not answering) leaves the game playable.

### Language and DLC

**Game settings** names the installed language and, when GOG offers others, lets you switch; it
also adds and removes DLC. They stay on the installed build and download only what differs.

```sh
slatty content <id>                      # installed and offered
slatty content <id> --language fr-FR
slatty content <id> --add-dlc <id>
slatty content <id> --remove-dlc <id>
```

## Verify, repair, uninstall

The game's Manage drawer (⋮) offers them for games Slatty installed.

- **Verify** reads every file and checks it against the installed build; **Repair** downloads again
  only the damaged pieces of the files that are missing or wrong. Both run at the lowest priority,
  so the desktop and a running game stay responsive.
- **Uninstall** deletes the files Slatty installed, nothing else: saves, mods or settings
  in the game folder stay, and are listed. The Wine prefix, where most saves live, stays unless you
  choose to delete it too; then its `users` folder, Wine's registry files and `C:\ProgramData` are
  copied to the backups first.

```sh
slatty verify <id> [--repair]
slatty uninstall <id> [--delete-prefix]
```

## Offline installers

GOG lets you keep the setup programs of the games you own, which install them without GOG or any
launcher. For a GOG game, installed or not, **Manage** (⋮) → **Download installers…** opens
**Offline installers**: choose the system (Windows, Linux or macOS, as GOG offers them), one or
more languages and the DLC, then **Download**. A DLC without the languages chosen comes in
English. When GOG offers the installers in one language only, it is named rather than offered:
many such installers hold every language of the game, chosen in its own options.

The installers go into a folder named after the game, under the installers folder
(**Settings → Installs**, `GOG` in your download folder until changed). They download through the
Downloads tab's queue, one download at a time with the installs, and resume where they stopped
after a pause, a closed window or a crash; if their folder is gone by then (a drive unplugged),
they stop as failed rather than start over on another disk. Each file is checked
against the MD5 sums GOG keeps for it before it takes its name; parts found damaged are
downloaded again. A file GOG keeps no sums for is taken on its size, and the notice that ends the
download says it was not checked; sums GOG keeps that cannot be read stop the download before it
begins, and **Resume** reads them again. A file already in the folder at its size is checked
against GOG's sums too, without being downloaded again, unless the download checked it itself and
it has not changed since; one that does not match them stops the download and is left as it is
(move it away, then **Resume**), and one there at another size is never replaced. **Remove** in
the Downloads tab deletes only the files left unfinished.

```sh
slatty installers <id>                             # what GOG offers
slatty installers <id> --download --os linux --language fr de
```

## Steam games

The games of the account signed in to the Steam client can show beside GOG's: turn on **Steam
games** in Settings → Steam (off until then), or choose **Use my Steam games without GOG** on the
sign-in page.

- **Read from Steam's files, never written.** Slatty finds Steam (native, or Flatpak: not tried
  yet) and the account last signed in to it, and reads its games, their names and images, what is installed,
  play time and last session from the client's own files. No Steam password is asked for, and the
  client's settings and sign-in files (`config.vdf`, `local.vdf`, `ssfn*`, `registry.vdf`) are
  never read.
- **Run by the Steam client.** **Play**, **Install**, **Verify files in Steam** and **Uninstall in
  Steam** hand the game to the Steam client, which must be installed and signed in and does the
  work in its own window. A client not running yet starts in the tray, without its library window
  (Steam installed natively; Steam's Flatpak opens as the desktop starts it). While a Steam game
  runs, its page shows **Running**.
- **Downloads.** What the Steam client downloads shows on the game's page and in the Downloads tab
  under **In Steam**, and a notification says when it is done. Steam's own window pauses or cancels
  them.
- **Play time and last played** are Steam's records, read again when a game ends and when the
  window comes back to the front. They are never sent to GOG.
- **Achievements** show in the Achievements tab and on the game's page, as the Steam client keeps
  them: those of games played on this computer or whose page was shown in Steam. They cannot be
  changed from Slatty.
- **Images from Steam** (off until turned on): covers, backgrounds and logos the client has not
  saved on this computer, and achievement icons, are downloaded from Steam's image servers, without
  any account. Off, those games show without them.
- **Steam Web API** (off until turned on) fills in what the client has not saved here: games it has
  not described, the play time of games never played here, and achievements it keeps none of. It
  needs your own key, created on
  [steamcommunity.com/dev/apikey](https://steamcommunity.com/dev/apikey): paste it in **Web API
  key**; it goes to the keyring and is never shown again. A profile whose game details are private
  may get nothing back.
- **What stays Steam's:** updates, cloud saves, achievements in game and launch options. Steam's
  games cannot run isolated from your files.

With both Images from Steam and Steam Web API off, nothing at all is asked of Steam's servers.

**A game you own on both** shows once, as its GOG game, matched through the Steam release GOG's
database gives for it (or, for a game it does not describe, by a title written alike). Its page
offers **Play the Steam version** or **Install the Steam version** under the GOG build's buttons;
it counts as installed when either is, and its play time adds both, or shows one store's once
preferred in **Play time on both** (Settings → Library). Its cloud saves and achievements are
GOG's.

**GOG is the better choice where you have it:** GOG's games can run isolated, need no client
running, and their saves and achievements reach GOG without one.

`slatty steam status` says whether Steam is found, who is signed in, and how many of its games
there are and are installed. If a Steam update changes the format of its list of apps, no Steam
game can show until Slatty reads the new one: Settings → Steam and `slatty steam status`
say so.

## Games added by hand

Games bought elsewhere, or made by you, join the library by the program that starts them. The
**+** above the library opens **Add a game**:

- the **program** (typed, or picked with Browse): a Windows `.exe`, or a Linux program, script or
  AppImage; which one it is comes from the file itself, not its name;
- a **title** (the program's name until you give one), its **arguments**, the game's **folder**
  when the program sits deeper in it, the **Proton** build for a Windows program, and whether it
  runs **isolated** (by default, as a game installed now).

A game that comes as an installer or an archive is installed first, with **Install it first, from
an installer or archive**:

- a **Windows installer** runs in a Wine prefix made for the game (finish it in its own window).
  A folder named after the title, in **Install into**, is drive **D:** there: choose D:\ as the
  installation folder in the installer, then pick the program it installed. Isolated, the
  installer reads its own folder and writes only in that one. Installed elsewhere (its C: drive),
  the program is looked for in the prefix; thrown away, the folder goes only if nothing was put in
  it. A launcher the installer put on C: (as some online games' are, which then download and update
  the game on D:) can be the program: the game's folder is then the one on D:, which an isolated
  launcher sees and updates, and nothing else of yours. So it is for a program the installer put
  deeper on D:, such as a launcher in a folder of its own that downloads the game beside it, unless
  you choose another **Game folder**;
- a **Linux installer** (MojoSetup `.sh`, the format of GOG's offline installers) or an
  **archive** (`.zip`, `.tar.gz`, `.tar.xz`) is unpacked, never run, into a folder named after the
  title in **Install into**. Paths that would leave that folder, and links leading out of it, are
  left out.

**Install into** is the default installation path (Settings → Installs) until changed; a folder of
that name already holding files is refused.

Once added, the game gets an id `local-<number>` and is played like any other, isolated or not. Its
page shows an **Added by hand** card. It has no cloud saves and no achievements, its play time stays
on this computer, and GOG is never told of it, nor is umu's database asked about it.

- **Locate…** in Game settings finds a game moved elsewhere: its program must be at the same place
  in the folder chosen.
- **Remove from library** (Manage) takes it out, asking first. Its files are yours and stay, except
  those Slatty unpacked, which are deleted while they are as they were unpacked (a save
  or setting the game rewrote stays). You can also delete the prefix Slatty made for it;
  its saves, and a game its installer put in it, are copied to the backups first.

An isolated game can write in its folder, so the folder cannot be one through which it would reach
more: your home folder or one holding it, a folder of programs, `~/.config`, Slatty's own
folders, another game's folder (or one inside or holding it), or one holding the default
installation path. Put the game in a folder of its own.

The [command line](command-line.md#games-added-by-hand) has `slatty add` and `slatty remove`.

## Emulated games

Games of other consoles join the library too, from your own disc images, files or folders, each
started full screen in its system's emulator:

| System | Emulator | What it starts |
|---|---|---|
| PlayStation | DuckStation | `.cue`, `.chd`, `.pbp`, `.m3u`, `.iso`, `.img`, `.ecm` |
| PlayStation 2 | PCSX2 | `.iso`, `.chd`, `.cso`, `.zso`, `.cue` |
| PSP | PPSSPP | `.iso`, `.cso`, `.chd`, `.pbp` |
| PlayStation 3 | RPCS3 | a game's folder (`PS3_GAME/USRDIR/EBOOT.BIN` or `USRDIR/EBOOT.BIN`) |
| PlayStation 4 | shadPS4 | a game's folder (`eboot.bin` and `sce_sys/param.sfo`) |
| PlayStation 5 | Kyty | a game's folder (`eboot.bin` and `sce_sys/param.json`), or a `.zar` |
| GameCube / Wii | Dolphin | `.iso`, `.gcm`, `.rvz`, `.gcz`, `.wbfs`, `.ciso`, `.wia`, `.wad`, `.dol`, `.elf` |
| Switch | Eden | `.nsp`, `.xci`, `.nro` |

Slatty never downloads emulators, BIOS, firmware, keys or games: install the emulators yourself
(Slatty finds them in `PATH`), and give them what they need in their own settings.

In **Settings → Emulators**, each emulator says whether its program was found; **Program…**
chooses another one. **Add folder…** gives the folders its games are in, and **Scan** looks
through them (a few folders deep) and adds every game not in the library yet. The discs a
playlist (`.m3u`) or a cue sheet names are parts of that game, not games of their own. A game
of PlayStation 3, 4 or 5 is titled as it names itself; another one by its file's name, without
the tags of disc sets ("(USA)", "[!]").

One game at a time: **+** above the library, then **Run it in an emulator**, its **Emulator**,
its **Game file** (or folder) and, if you want, its **Title**.

An emulated game gets an id `emu-<number>`. Its page shows an **Emulated** card with its emulator;
it has no cloud saves and no achievements, its emulator keeps its saves and settings, its play
time stays on this computer, and GOG is never told of it. Emulators run as your own programs do:
they are not isolated from your files.

- **Locate…** in Game settings finds a game moved elsewhere, by its new file or folder.
- **Remove from library** (Manage) takes it out. Its files are never touched, and a scan does not
  add it again; adding it by hand does.

The library's filters have **Emulated**, and a menu of the systems, once the library has emulated
games.

## Games installed elsewhere

A GOG game installed by another tool can be registered, so it can be launched and synced:

```sh
slatty import <folder> --runner umu --proton <Proton folder> --prefix <prefix>
slatty import --from-heroic <id>      # read from Heroic's records, never changed
slatty forget <id>                    # drop it again; its files and prefix are untouched
```

Imported games cannot be verified or uninstalled: Slatty does not know which files are
theirs. A Windows game imported to run through Proton is isolated as a new install would be.
`slatty forget` refuses a game added by hand (use `slatty remove`) and a game while it runs or
another operation on it does.

## Proton builds

Every Proton menu lists, first, the builds Slatty downloaded, then those of Steam's
`compatibilitytools.d` (native, or Flatpak: not tried yet), Valve's builds (Proton Experimental,
stable, Hotfix) that Steam downloaded in any of its libraries, and those umu downloaded.

**Downloading builds.** Once **Proton downloads** is on (Settings → Runners, off until then),
**Check GitHub** shows the newest GE-Proton, Proton-CachyOS (its x86-64-v3 build on processors that
have those instructions) and UMU-Proton, each with **Install**. Each archive is checked against the
SHA-512 sum its release publishes before it is opened. A download stopped half way goes on from
where it was.

**`-latest`.** Each project gets an entry that always leads to its newest downloaded build:
`GE-Proton-latest`, `Proton-CachyOS-latest`, `UMU-Proton-latest`. A game set to one runs the
newest build from its next launch; a game set to a numbered build keeps it.

**Keep up to date** (off until turned on) checks once a day at start the projects games follow as
`-latest`, downloads a newer build in the background and switches to it once it is checked and
unpacked. A running game keeps the build it started with, and a `-latest` link never goes back to
an older version. The build before stays, for a game a new build breaks; older builds no game uses
are deleted. **Update now** does it at once.

**Share with Steam and Heroic** (off until turned on) links each downloaded build into Steam's
`compatibilitytools.d`, where Steam (after a restart) and Heroic list it. Only the numbered builds
are shared, not the `-latest` links, and a name already there is left alone. A build deleted here
disappears from Steam and Heroic too, even if one of their games used it.

A build is kept until you delete it (**Delete** under **Downloaded**), which is refused while a
game or the default uses it.

**Valve's builds** need Steam's client library. An isolated game run with one is given Steam's
`linux64` and `linux32` folders, read-only, and nothing else of Steam. A game that is not isolated
finds it in `~/.steam/sdk64`, which only Steam installed natively makes: with Steam's Flatpak
alone, such a launch is refused and says why.

Proton upgrades a game's prefix when its build changes, as under Steam; going back to an older
build usually works but is less tested.

```sh
slatty proton downloads on
slatty proton available
slatty proton install ge-proton      # or proton-cachyos, umu-proton
slatty proton list
slatty proton updates on
slatty proton share on
```

## Couch mode

A mode for a gamepad, full screen, for a television or a Steam Deck: the same library, downloads
and games as the desktop window, with large covers and a focus moved with the d-pad or the left
stick. Settings stay in the desktop window. Made for 1920×1080, it fills a larger screen (a 4K
television) the same way, drawn at the screen's full resolution: text and key art stay sharp, and
covers, which GOG makes smaller than they are drawn there, are enlarged as key art is.

It opens:

- from the gamepad button of the top bar;
- with `slatty-gui --couch`, or the **Couch mode** action of Slatty's desktop entry;
- by itself in a gamescope session, such as the Steam Deck's Game Mode, drawn at the screen's
  pixels as in the Desktop Mode (a `WINIT_X11_SCALE_FACTOR` of your own still wins). On a Steam
  Deck, add Slatty to Steam as a non-Steam game from the Desktop Mode, then start it from the Game
  Mode. Neither has been tried on a Steam Deck yet.

**Menu** → **Desktop mode** goes back to the desktop window; **Menu** → **Quit Slatty**
closes it, asking first if a download or a game runs. The menu opens with the Menu button (☰),
B or Escape at a tab, or a click on its hint at the bottom.

| Gamepad | Keyboard | What it does |
|---|---|---|
| D-pad, left stick | Arrow keys | Moves the focus; held, it repeats |
| A | Enter | Opens or presses what has the focus |
| B | Escape | Back; closes a notice, the keyboard, the menu or a list; at a tab, opens the menu |
| Y | Space | Adds the game with the focus to the favorites, or takes it out |
| X | `/`, or a letter typed | Search |
| LB, RB | Page Up, Page Down, Tab | The tab before or after |
| Menu (☰) | F1 | The menu |

The buttons are named after an Xbox pad: A is the lowest of the four on the right, B the right
one, X the left one, Y the top one.

- **Home** shows the key art of the game with the focus, and rows of covers: **Continue playing**
  (the 20 games played last), **Favorites** and **Installed**; the whole library while none of
  these has a game yet. The focus stays on the game chosen when the rows change (a favorite marked
  or taken out, a download ending, a game played); when its row goes, it moves with its game.
- **Library** shows every game by name. Above the covers (Up from the first row), the shelves
  **All**, **Installed** and **Favorites**, and **Search**, which opens a keyboard on the screen: A
  types the key with the focus, X erases, Y adds a space, B or **Done** closes it, keeping the
  search. A keyboard types into the search directly, and Enter ends it; a letter typed from Home
  or Library starts one. Once moved, the focus stays on its game when the shelf changes, and takes
  the next one when its game leaves the shelf.
- A game's page has its main button (Play, Stop while it runs, Install, Resume, or where its
  download stands), the favorite, and **Achievements**, a list to scroll. A game that can be
  started several ways asks which the first time, as in the desktop window. **Install** starts at
  once with the choices the Install drawer would start with: the default platform and Proton
  build, the installation path of Settings → Installs, every DLC owned; it waits in the queue while
  another download runs.
- **Downloads** lists the download running (Pause), those stopped (Resume, Discard or Remove) and
  the queue (Download now, Remove, which sets a download that started aside, paused, as × does).

The gamepad is read through udev, as the desktop gives it to the person at the computer; when
Slatty is started from Steam, Steam's controller support hands it the pad Steam presents.
The gamepad is ignored while the window is behind another one or a game runs: its buttons are
the game's.

## Settings

Settings lists its parts on the left; the list follows the page as it scrolls.

### Account

| Setting | What it does |
|---|---|
| **Signed in as** | The GOG account, with **Log out** |
| **Forget** | **Forget account**: signs out and deletes what Slatty keeps of the account (see [above](#signing-out-and-forgetting-an-account)) |
| **GOG** | Signed out: which games show, with **Sign in to GOG** |

### Library

| Setting | Default | What it does |
|---|---|---|
| **Games** | | How many games you own on GOG and when the list was read, with **Refresh library** |
| **Cover size** | 100% | From 75% to 160% |
| **Remember filters** | Off | The library's filters come back as you left them at the next start |
| **Play time on both** | Steam + GOG | With Steam's games on: for a game owned on both, both play times added up, or **Prefer Steam** / **Prefer GOG** (the other store's while the preferred one has none) |

### Steam

| Setting | Default | What it does |
|---|---|---|
| **Steam games** | Off | Shows the Steam account's games in the library (see [Steam games](#steam-games)) |
| **Images from Steam** | Off | Downloads the images the Steam client has not saved from Steam's image servers |
| **Steam Web API** | Off | Fills in what the client has not saved, with your own **Web API key** |

### Installs

| Setting | Default | What it does |
|---|---|---|
| **Default installation path** | `~/Games/GOG` | Where new games go. Type it or **Browse**; saved once it is an absolute path |
| **Installers folder** | `GOG` in your download folder | Where [offline installers](#offline-installers) go, each game's in a folder of its own. Saved like the installation path |
| **Default platform** | Windows | The build installed for a game GOG offers on both |
| **Default Proton** | | The build new Windows games start with. Each game can use another |
| **Update games automatically** | On | See [Updates](#updates) |
| **Pause downloads while playing** | On | The download or automatic update running pauses when a game starts and goes on once it closes; nothing else starts meanwhile, but **Download now** |

`slatty install --dir` and `--proton`, and `slatty installers --dir`, set the same defaults.

### Runners

| Setting | Default | What it does |
|---|---|---|
| **Proton downloads** | Off | Allows listing and downloading Proton builds from GitHub |
| **Keep up to date** | Off | Updates the builds games follow as `-latest`, with **Update now** |
| **Newest builds** | | The newest build of each project, with **Check GitHub** and **Install** |
| **Share with Steam and Heroic** | Off | Lists the downloaded builds in Steam and Heroic |
| **Downloaded** | | The builds Slatty downloaded, with **Delete** |

### Emulators

| Setting | Default | What it does |
|---|---|---|
| **Emulated games** | | **Scan** adds the games found in the emulators' folders (see [Emulated games](#emulated-games)) |
| Each emulator | Found in `PATH` | Its program (**Program…**, or **Use the one found instead**) and the folders of its games (**Add folder…**, **Remove**) |

### Appearance

| Setting | Default | What it does |
|---|---|---|
| **Theme** | Slatty | Slatty, Carbonfox, Everforest, Pastel Glow, Gruvbox Dark or Gruvbox Light |
| **Font** | Geist | Geist (built in) or a font family installed on the system |
| **Theme file** | | `~/.config/slatty/theme.toml`, with **Create theme file**, then **Edit** and **Reload**: see [Theming](theming.md) |

### Notifications

One switch for all notifications, then one for each kind: **Downloads**, **Game and Proton
updates**, **Achievements**, **Cloud saves**. All are on until turned off.

### Privacy

| Setting | Default | What it does |
|---|---|---|
| **Proton fixes online** | Off | A game the copy of umu's database in its Proton build does not list is looked up in umu's public database at its first launch, sending its GOG id. Off, it runs without fixes |
| **Achievements in game** | Off | Runs Comet beside games that use GOG's Galaxy, so what they unlock reaches GOG (see [Achievements](#achievements)) |
| **Isolate new Windows games** | On | Windows games installed from now on run [isolated from your files](#isolation-from-your-files) |
| **Play time on GOG** | On | Sends each session to GOG, as Galaxy does. Sessions played while off are never sent |

Slatty has no telemetry. [SECURITY.md](../SECURITY.md#what-leaves-your-computer) lists
everything that leaves your computer, and when.

### Advanced

| Setting | Default | What it does |
|---|---|---|
| **Manual achievements** | Off | Adds Unlock, Clear and Unlock all beside a game's achievements |
| **SteamGridDB** | Off | Offers SteamGridDB's covers and backgrounds in Edit game, with your own **API key** (free, from steamgriddb.com under Preferences → API). Searching sends the name you type and your key to SteamGridDB |

Keys you paste go to the system keyring and are never shown again; **Remove** deletes them.

### About

The version, the copyright, the address of the source code, and the licences of Slatty and of
what it includes.

## Where data is stored

Folders follow the XDG variables (`XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME`,
`XDG_STATE_HOME`); the defaults are shown. They are readable by you only.

| Path | What it holds |
|---|---|
| System keyring, `slatty` / `gog:<user id>` | Your GOG session |
| System keyring, `slatty` / `key:steamgriddb`, `key:steam-web-api` | Your SteamGridDB and Steam Web API keys, once saved |
| `~/.local/share/slatty/state.db` | Accounts, installed games, sessions, cloud sync history, settings, your changes to games |
| `~/.local/share/slatty/prefixes/<id>/` | Wine prefixes (most saves live here) |
| `~/.local/share/slatty/homes/<id>/` | The home folders of isolated games, a Linux game's saves among them. Kept when the game is uninstalled |
| `~/.local/share/slatty/backups/` | Copies made before any save is replaced or a prefix deleted. Never deleted by Slatty |
| `~/.local/share/slatty/manifests/<id>.json` | The files installed for each game |
| `~/.local/share/slatty/support/<id>/`, `redist/` | GOG's setup files for each game, and the redistributables shared by games |
| `~/.local/share/slatty/added/` | What each game added by hand starts, and the files unpacked for it |
| `~/.local/share/slatty/emulated/` | The emulator of each emulated game, and what it is given |
| `~/.local/share/slatty/custom/<id>/` | Covers and backgrounds you chose |
| `~/.local/share/slatty/protons/` | Proton builds downloaded from GitHub |
| `~/.local/share/slatty/umu-runtimes/` | The tool Linux games run through in umu's runtime |
| `~/.local/share/slatty/comet/`, `~/.config/slatty/comet/` | Comet's data and configuration |
| `~/.local/share/slatty/diagnostics/` | Cloud copies downloaded by `slatty cloud diff` |
| `~/.local/share/slatty/GalaxyCommunication.exe` | Comet's Galaxy service, unless `SLATTY_GALAXY_COMMUNICATION` names it elsewhere (the Nix package and development shell do) |
| `~/.config/slatty/theme.toml` | Your theme file, once created |
| `~/.cache/slatty/<user id>/` | Your GOG library, covers and images. Safe to delete |
| `~/.cache/slatty/steam/`, `steamgriddb/` | Images from Steam's servers and SteamGridDB's thumbnails. Safe to delete |
| `…/scaled/<width>/`, `…/art/<width>/` in those caches | The covers and key art made at the size they are drawn; key art copies take at most 2 GiB. Safe to delete |
| `~/.local/state/slatty/logs/` | Logs of each game, prefix creation, setup, Galaxy service and Comet: the last run of each and the two before (`.1`, `.2`). A game's logs go when it is uninstalled |
| `~/.local/state/slatty/locks/` | Locks that keep two operations off the same game |

Read a log before sharing it: a game's log holds whatever the game printed, which can include
identifiers of its own, and Comet's holds game client identifiers.

## Troubleshooting

| Symptom | What to check |
|---|---|
| A game does not start | Its log, `~/.local/state/slatty/logs/game-<id>.log`. `slatty launch-spec <id>` prints the exact command |
| The first launch fails while creating the prefix | `~/.local/state/slatty/logs/prefix-<id>.log` |
| A game misbehaves since it runs isolated (files or settings not found) | `slatty isolation <id> --off`, and tell us which game |
| "bubblewrap (bwrap) is needed", or an isolated game does not start | Install bubblewrap (Flatpak needs it too). `slatty doctor` says whether it can start a sandbox, and one inside another. On Ubuntu, see [below](#ubuntu-and-apparmor) |
| Achievements are not reported | **Achievements in game** must be on. Then `~/.local/state/slatty/logs/comet.log`; `slatty doctor` must find `GalaxyCommunication.exe` |
| `slatty doctor` says Comet's port is busy | Another Comet runs, usually Heroic's: close the game in Heroic |
| "secret storage unavailable" | A Secret Service keyring (GNOME Keyring, the one checked; KeePassXC or KWallet with its Secret Service bridge, not tried yet) must be running and unlocked |
| A cloud conflict blocks the launch | Open the Cloud saves drawer, or `slatty cloud diff <id>` then `slatty cloud sync <id> --prefer local` or `--prefer remote` |
| A session was cut off (crash, power cut) | The next launch says so. Check `slatty cloud status <id>` before playing |
| "another operation on this game is running" | An install, update, repair, uninstall, cloud sync or session of that game still runs, maybe from the command line, or the game itself still runs. Wait for it to end |
| A download stopped as failed: "is its drive mounted?" | The folder it was going into is gone: mount the drive or share, then **Resume** |
| The window looks wrong or slow after a graphics driver change | It starts with Vulkan alone once Vulkan was found to drive the graphics card, and goes back to every backend by itself after one start if Vulkan stops working. To choose yourself: `WGPU_BACKEND=vulkan`, `gl` or `vulkan,gl` |
| Something went wrong without an explanation | Start `slatty` or `slatty-gui` with `SLATTY_LOG=warn` |

### Ubuntu and AppArmor

Since Ubuntu 23.10 (24.04 LTS included), a program may start a sandbox only if an AppArmor profile
allows it, and bubblewrap has none on Ubuntu 24.04 as installed. umu needs bubblewrap for every
Windows game, isolated or not, as Heroic and Lutris do; an isolated game also needs it to start a
sandbox inside another. Until bubblewrap has a profile, `slatty doctor` reports it and launches say
so. Choose one of these, from the most to the least protective, then check with `slatty doctor`:

1. **Ubuntu's own profile for bubblewrap**, the one umu's Ubuntu package uses. It lets bubblewrap
   start a sandbox, but the programs inside cannot gain privileges. On Ubuntu 25.04 and later it is
   installed and active already; on 24.04:

   ```sh
   sudo apt install apparmor-profiles
   sudo install -m 644 /usr/share/apparmor/extra-profiles/bwrap-userns-restrict /etc/apparmor.d/
   sudo apparmor_parser -r /etc/apparmor.d/bwrap-userns-restrict
   ```

   Windows games then start. Isolated games may not, if the profile keeps the sandbox inside
   another from starting: `slatty doctor` tells. Turn isolation off for them, or use the next
   choice.
2. **A profile without restriction for bubblewrap**: it and the programs it starts may create user
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
3. **Lifting the restriction for the whole system**
   (`kernel.apparmor_restrict_unprivileged_userns = 0` in a file of `/etc/sysctl.d/`). Every
   program may then create user namespaces again: not recommended.

These steps come from Ubuntu's AppArmor profiles and from what umu, Lutris and Heroic users found;
they have not been tried with Slatty on Ubuntu yet.
