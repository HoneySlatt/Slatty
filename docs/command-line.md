# Command line

`slatty` does everything the interface does, and a few things more (diagnostics, imports). Both
share the same data: a game installed from one shows in the other.

```sh
slatty <command> --help      # every command explains its options
```

This page lists every command. The [user guide](user-guide.md) explains what they do in context.

## Game ids

Most commands take a game id:

| Id | Game |
|---|---|
| `1207658924` | A GOG game: its product id |
| `steam-440` | A Steam game: `steam-` and its app id |
| `epic-Fortnite` | An Epic game: `epic-` and its app name |
| `local-3` | A game added by hand |

`slatty library list` shows them. GOG commands (install, cloud saves, achievements, updates) take
GOG ids only. Steam's games are played, installed and uninstalled through the Steam client, from
the interface; the command line only lists them.

## Account

| Command | What it does |
|---|---|
| `slatty auth login` | Opens GOG's sign-in page in your browser, then asks for the address you land on. `--no-browser` prints the page's address instead of opening it |
| `slatty auth status` | The account signed in, and when its access token expires |
| `slatty auth refresh` | Renews the session now (it renews itself when needed) |
| `slatty auth logout` | Deletes the session from the keyring. Cached data and saves stay |
| `slatty auth logout --forget` | Also deletes what Slatty keeps of the account (see [Signing out](user-guide.md#signing-out-and-forgetting-an-account)) |
| `slatty auth probe-rotation` | A developer's check: whether refreshing the session changes the refresh token |

## Library

| Command | What it does |
|---|---|
| `slatty library sync` | Downloads your GOG library into the local cache (titles and details; the interface downloads the covers) |
| `slatty library list [text]` | Lists your games from the cache, offline too: GOG's, Steam's once turned on, and those added by hand. `text` filters by title |
| `slatty installs` | Installed games, and installs that were interrupted |
| `slatty installs --json` | For other programs: the games the Installed shelf shows, hidden ones left out, as a JSON list of `id`, `title`, `cover` (the file of the cover the library shows, `null` until it is downloaded) and `steam_app` (for a game only Steam installed, to start through Steam; any other starts with `slatty launch <id>`). Reads local files only |

## Installing GOG games

```sh
slatty install <id> [options]
```

| Option | Effect |
|---|---|
| `--info` | Shows the version, sizes, languages, DLC and redistributables, and downloads nothing |
| `--platform windows\|linux` | Which build to install. Default: the setting, Windows until changed |
| `--dir <folder>` | The folder that receives the game's folder. Remembered as the default |
| `--proton <folder>` | The Proton build a Windows game runs with. Remembered as the default. Without one, the command lists the builds it found |
| `--language <code>` | A language the build offers (`fr-FR`), as `--info` lists them. Default: English |
| `--no-dlc` | The base game only. By default every DLC you own is installed |
| `--dlc <id>…` | Only these DLC |
| `--restart` | Throws away an interrupted install of this game and starts over |
| `--cancel` | Abandons an interrupted install: deletes what it downloaded and forgets it |

Run the same command again to resume an interrupted install; Ctrl+C pauses it.

## Offline installers

GOG's setup programs for a game, as its site offers them, kept in a folder rather than installed.

```
slatty installers <id> [options]
```

Without `--download`, lists the installers GOG offers (system, language, version, size), the DLC
that have some, and the folder they would go into, and downloads nothing.

| Option | Effect |
|---|---|
| `--download` | Downloads them, each file checked against GOG's MD5 sums. A file GOG keeps none for is said not checked |
| `--os windows\|linux\|mac` | Which system's installers. Default: Windows |
| `--language <code>…` | One or more languages GOG offers them in (`fr de`), as listed. Default: English, which a DLC without those languages comes in |
| `--no-dlc` | The game's installer only. By default those of every DLC GOG offers one for come with it |
| `--dlc <id>…` | Only these DLC |
| `--dir <folder>` | The folder that receives the game's folder of installers. Remembered as the default, which is `GOG` in your download folder |

Ctrl+C pauses; run the same command again to resume where it stopped. A file already in the
folder at its size is checked against GOG's sums, unless this download checked it itself and it
has not changed since; one that does not match them stops the download and is left as it is, and
one there at another size is never replaced.

## Playing

| Command | What it does |
|---|---|
| `slatty launch <id>` | Starts the game and follows it until its last process ends. Ctrl+C asks the game to quit, a second Ctrl+C forces it |
| `slatty launch <id> --no-cloud` | Without syncing cloud saves before and after |
| `slatty launch <id> --no-comet` | Without Comet: no achievements reported during this session |
| `slatty launch-spec <id>` | Prints the exact command, folder and environment a launch would use, including controller compatibility saved in Game settings |
| `slatty isolation <id>` | Whether the game runs isolated from your files |
| `slatty isolation <id> --on` / `--off` | Changes it from the next launch on |
| `slatty setup <id>` | Runs GOG's post-install setup again. `--dry-run` shows what it would do; `--force` runs it even if it already ran for this build |

## Cloud saves

| Command | What it does |
|---|---|
| `slatty cloud status <id>` | Where the saves are, and what a sync would do. Changes nothing |
| `slatty cloud sync <id>` | Syncs now |
| `slatty cloud sync <id> --prefer local\|remote` | Settles every conflict of this sync in favour of one side; the other side's copy is kept in the backups |
| `slatty cloud sync <id> --allow-deletions` | Also passes on files deleted on one side (never when a folder looks empty or moved) |
| `slatty cloud diff <id> [text]` | Compares local and cloud copies file by file. The cloud copies go to a folder of their own; nothing is changed. `text` keeps only paths containing it |

## Achievements

| Command | What it does |
|---|---|
| `slatty achievements <id>` | Lists a game's achievements as GOG records them, with how common each one is |
| `--unlock <name>…` | Unlocks these, without playing (key, id or exact name) |
| `--unlock-all` | Unlocks every locked one |
| `--clear <name>…` | Locks these again |
| `--yes` | Does not ask before changing anything |

Manual changes show on your public GOG profile. See [Achievements](user-guide.md#achievements).

## Looking after installed games

| Command | What it does |
|---|---|
| `slatty update` | Checks every game Slatty installed |
| `slatty update <id> --check` | Only says whether an update is available |
| `slatty update <id>` | Applies it, or finishes an interrupted one |
| `slatty update --automatic on\|off` | Automatic updates in the interface (on until turned off) |
| `slatty content <id>` | The installed language and DLC, and those offered |
| `slatty content <id> --language <code>` | Switches language |
| `slatty content <id> --add-dlc <id>…` / `--remove-dlc <id>…` | Adds or removes DLC |
| `slatty verify <id>` | Checks every file against the installed build |
| `slatty verify <id> --repair` | Downloads again the files that are missing or damaged |
| `slatty uninstall <id>` | Deletes the files Slatty installed, and nothing else. Asks first (`--yes` does not) |
| `slatty uninstall <id> --delete-prefix` | Also deletes the Wine prefix, its saves backed up first |

## Games installed elsewhere

| Command | What it does |
|---|---|
| `slatty import <folder> --runner umu --proton <folder> --prefix <folder>` | Registers a GOG Windows game installed by another tool, to run through Proton |
| `slatty import <folder> --runner wine --wine <program> --prefix <folder>` | The same, through Wine alone |
| `slatty import <folder> --runner native --game-id <id>` | A GOG Linux game (its id is needed) |
| `slatty import --from-heroic <id>` | A GOG game Heroic installed, read from Heroic's records without changing them |
| `slatty forget <id>` | Drops a game from Slatty's records without touching its files or prefix |

## Games added by hand

| Command | What it does |
|---|---|
| `slatty add <program> --title <title> [-- <arguments>]` | Adds a game by the program that starts it: a Windows `.exe`, a Linux program, script or AppImage |
| `--folder <folder>` | The game's folder, when the program sits deeper in it |
| `--working-dir <folder>` | Where the program starts. Default: its own folder |
| `--proton <folder>` | The Proton build a Windows program runs with. Default: the default one |
| `--isolated` / `--not-isolated` | Whether it runs isolated from your files. Default: as a game installed now |
| `slatty add --installer <setup.exe> --title <title> [--into <folder>]` | Runs a Windows installer in a prefix made for the game (finish it in its window), where a folder named after the title in `--into` (default: the default installation path) is drive D:, to install the game on; then says how to add what it installed |
| `slatty add --installer <file> --title <title> [--into <folder>]` | Unpacks a Linux installer (MojoSetup `.sh`) or an archive (`.zip`, `.tar.gz`, `.tar.xz`) without running it, into a folder named after the title in `--into` (default: the default installation path) |
| `slatty add <program> --title <title> --installed-by <id>` | Adds the program an installer put in place, under the id the installer was given. `slatty remove <id>` throws that install away instead |
| `slatty remove <id>` | Takes the game out of the library. Its files stay, except those Slatty unpacked |
| `slatty remove <id> --delete-prefix` | Also deletes the prefix Slatty made for it, its saves backed up first |

Emulated games (`emu-<number>`) are added in the application, under Settings → Emulators.
`slatty library list` lists them, `slatty launch <id>` starts them, and `slatty remove <id>` takes
them out of the library without touching their files; a scan does not add them again.

## Proton builds

| Command | What it does |
|---|---|
| `slatty proton list` | The Proton builds on this computer, those Slatty downloaded first |
| `slatty proton downloads on\|off` | Allows listing and downloading builds from GitHub (off until turned on) |
| `slatty proton available` | The newest build of each project on GitHub |
| `slatty proton install ge-proton\|proton-cachyos\|umu-proton` | Downloads a project's newest build, checked against its published sum |
| `slatty proton remove <name>` | Deletes a downloaded build (refused while a game or the default uses it) |
| `slatty proton updates on\|off` | Keeps the builds games follow as `<project>-latest` up to date, once a day at start (off until turned on) |
| `slatty proton update` | Does it now |
| `slatty proton share on\|off` | Lists the downloaded builds in Steam and Heroic (off until turned on) |

## Steam

| Command | What it does |
|---|---|
| `slatty steam status` | Whether Steam is found, the account signed in to it, how many of its games there are and how many are installed. Reads Steam's files and changes nothing |

## Epic Games

| Command | What it does |
|---|---|
| `slatty epic login` | Opens Epic's sign-in page in your browser, then asks for the page of text it ends on (or its `authorizationCode`). `--no-browser` prints the page's address instead of opening it |
| `slatty epic status` | The Epic account signed in, until when its session is valid, and how many games were read |
| `slatty epic list` | Reads the account's games again from Epic and lists them with their ids. `--offline` lists them from the cache |
| `slatty epic logout` | Deletes the Epic session from the keyring. The library cache stays |

Epic games cannot be installed or played from Slatty yet.

## Diagnostics

`slatty doctor` checks what Slatty needs: umu-launcher, bubblewrap (and whether it can
start a sandbox inside another, as isolated games need), Comet and its port, the Galaxy service
file, the keyring and the runtime folder.

`SLATTY_LOG=warn` (or `debug`) before any command prints warnings on the terminal:

```sh
SLATTY_LOG=warn slatty launch <id>
```
