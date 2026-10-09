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

`slatty-gui` has three tabs: **Library**, **Achievements** and **Settings**. Your GOG avatar, top
right, opens Settings. It comes from your public GOG profile.

**Library** shows your games as covers. Above the grid:

- **All**, **Installed**, **Favorites** choose the shelf;
- the sort menu orders by name, most recently played or most played;
- the slider sets the cover size;
- the filter button shows games for Windows or Linux, with achievements, or with cloud saves.

Hovering a cover shows its title, a settings button and a play (or install) button. Clicking it
opens the game page.

**The game page** shows the key art, Play (Install when the game is not installed), the favorite
button, play time and last session, cloud save status and achievement progress. Play time counts
only sessions started by SlattyLauncher. The tools open in panels:

| Where | Panel |
|---|---|
| Sliders button (top right) | Game settings: folder, Proton, language and DLC |
| ⋮ button (top right) | Manage: verify, repair, check for update, uninstall |
| Cloud saves, **Manage →** | Check, sync, resolve conflicts |
| Achievements card | Full list, unlock or clear |
| Install button | Version, size, language, DLC, start, pause, discard |

Escape closes the panel, then the game page.

**Achievements** lists every game with achievements, by completion. Clicking a game opens its own
achievements page in the same tab, where you can unlock or clear them. SlattyLauncher reads which games
have achievements and cloud saves from GOG in the background and keeps the answer in
`~/.cache/slatty/<user id>/overview.json`; **Refresh** reads it again.

## Installing games

SlattyLauncher installs the **Windows** build of a game from GOG's Galaxy content system and runs it
through Proton.

```sh
slatty install <game-id> --info
```

`--info` shows the version, the download and disk size, the languages offered and the
redistributables the game declares. It downloads nothing.

```sh
slatty install <game-id> --proton ~/.local/share/Steam/compatibilitytools.d/<Proton build> --dir ~/Games/GOG
```

`--proton` and `--dir` are remembered, so later installs need only the game id. The default folder
is `~/Games/GOG`. Use `--language fr-FR` (or any language listed by `--info`) for another language.

**DLC.** Every owned DLC is installed by default, as Galaxy does. Use `--no-dlc` for the base game
only, or `--dlc <id>…` to pick. `--info` lists the DLC of the build, with their size and whether you
own them.

How an install behaves:

- **Staged download.** Files go to a hidden `.<Game>.slatty-partial` folder next to the
  destination. The game folder appears only once every file has been verified.
- **Integrity checks.** Every chunk is checked twice against GOG's checksums, before and after
  decompression.
- **Pause and resume.** Ctrl+C (or Pause in the interface) stops the download. Running the same
  command again resumes it: files already on disk are re-checked and kept when correct.
- **Safety refusals.** Missing disk space, an existing destination folder, or a file path that
  would escape the game folder are refused before anything is downloaded.
- `slatty installs` lists installed games and interrupted installs.
- `slatty install <game-id> --cancel` abandons an interrupted install. It deletes only its hidden
  partial folder. In the interface, use **Discard download** in the Install panel.

Dependencies that ship files into the game folder are installed with the game. GOG's installer
scripts ("support" files) are kept in `~/.local/share/slatty/support/<id>/`; they are used by the
setup that runs at first launch (see below).

## Playing

```sh
slatty launch <game-id>
```

A launch goes through these steps:

1. On the first launch of a fresh install, the Wine prefix is created (`wineboot`), so that save
   folders exist.
   The post-install setup that GOG Galaxy performs runs once per installed build: GOG's script
   interpreter or each product's setup program (registry entries and similar), then the shared
   redistributables the game declares (Visual C++, DirectX…), installed silently. Offline or on
   failure the game still starts and the setup is retried at the next launch. `slatty setup <id>
   --dry-run` shows what it would run; `--force` runs it again.
2. Cloud saves are synchronised. If both sides changed, the launch stops and asks you to choose
   (see [cloud saves](#cloud-saves)). Offline, the game starts with your local saves.
3. Comet starts, so the game can report achievements. Before the first session, Comet's dummy
   `GalaxyCommunication` service is registered in the prefix; some games need it to reach Comet.
4. The game runs. SlattyLauncher waits until **every** game process has exited, not only the
   launcher.
5. Cloud saves are uploaded, Comet stops, and newly recorded achievements are listed.

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

[docs/cloud-saves.md](cloud-saves.md) explains the rules in detail.

## Achievements

```sh
slatty achievements <game-id>
```

This lists achievements as GOG records them, with how common each one is.

When a game is launched by SlattyLauncher, unlocks made in the game are sent to GOG by Comet.
Support depends on the game and its Galaxy SDK version; see
[docs/compatibility.md](compatibility.md).

Achievements can also be changed manually, for any game you own, installed or not:

```sh
slatty achievements <game-id> --unlock NEGLECT "Steel Soul"
slatty achievements <game-id> --unlock-all
slatty achievements <game-id> --clear NEGLECT
```

Achievements are matched by key, id or exact name. You are asked to confirm before anything is
written. Manual changes appear on your public GOG profile, dated today, and are probably against
GOG's terms of use.

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

When a file changed only in places, the parts that did not change are copied from the installed
file instead of downloaded. GOG cuts files into chunks (10 MiB on the games checked so far), so a
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
interface, use the game settings panel (sliders button).

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

The interface's **Settings** tab holds the account (log out), the library refresh, and:

- **Games folder:** where new games are installed.
- **Proton:** the build used for new installs, picked from `~/.local/share/Steam/compatibilitytools.d`.

`slatty install --dir` and `--proton` set the same values.

## Where data is stored

| Path | Content |
|---|---|
| System keyring, entry `slatty-launcher` / `gog:<user id>` | Session tokens |
| `~/.local/share/slatty/state.db` | Accounts, installed games, sessions, cloud sync history, settings |
| `~/.local/share/slatty/prefixes/<id>/` | Wine prefixes of installed games (most saves live here) |
| `~/.local/share/slatty/manifests/<id>.json` | Files installed for each game |
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
