# Changelog

All notable changes are listed here. Versions follow [Semantic Versioning](https://semver.org);
until 1.0, a minor version may change commands and on-disk formats.

## Unreleased

### Added

- Emulated games in the library: games of PlayStation 1 to 5, PSP, GameCube, Wii and Switch, from
  your own files, run full screen in DuckStation, PCSX2, PPSSPP, RPCS3, shadPS4, Kyty, Dolphin or
  Eden. Settings → Emulators finds each emulator, takes the folders of its games and scans them;
  **+** adds one game at a time. The library's filters have **Emulated** and a menu of systems.
- Epic Games Store games in the library: sign in under Settings → Account → Epic Games (or
  `slatty epic login`), and your Epic games show with their covers, and an **Epic** filter. They
  install like GOG's, from the same drawer and queue (or `slatty install epic-<app name>`), each
  file checked against Epic's checksum, and uninstall from Manage; they cannot be played from
  Slatty yet. `slatty epic list` lists them.
- `slatty installs --json` lists, for other programs (a game menu of your desktop), the games the
  Installed shelf shows, with their titles, the files of their covers and how to start them.

### Changed

- Playing, installing, verifying or uninstalling a Steam game starts the Steam client in the tray
  when it is not running, rather than with its library window.
- A Steam game shows **Starting…** on its page, and a clock on its cover, from Play until it runs.
- A game whose achievements are all unlocked says Completed in place of 100%, its progress lit
  with the theme's accent: on its page, in the Achievements tab and in the couch mode.
- Game settings no longer says controller compatibility was checked with UMU-Proton only: it was
  checked with GE-Proton and Proton-CachyOS too.

### Fixed

- In a Steam Deck's Game Mode, Slatty is no longer drawn about twice too large: it uses the
  screen's pixels, as in the Desktop Mode.

## 0.1.0 (2026-10-21)

The first public version.

### Library

- Sign-in to GOG in the system browser; the session in the system keyring.
- The library with its covers, cached for offline use, fluid at 10,000 games.
- Shelves (all, installed, favorites, hidden), sort, search, and filters by store (GOG, Steam,
  added by hand), system, achievements and cloud saves, remembered when asked.
- A game page with its key art, play time, cloud save and achievement cards, and its tools in
  drawers; an Achievements tab; notifications in the window and on the desktop.
- Each game's title, sorting title, cover and background can be changed, from a file or from
  SteamGridDB (once turned on), and a game can be hidden.
- Built-in themes, a theme file for colours, corners and motion, the interface font and the cover
  size. Covers and key art are drawn from copies made at the size they are shown.
- A couch mode for a gamepad or the keyboard, full screen: Home with the games played lately, the
  favorites and those installed under the key art of the game chosen; the library by shelf, with a
  search typed on a keyboard on the screen; game pages that play, install with the default
  choices and list achievements; the Downloads queue. It opens from the top bar, with
  `slatty-gui --couch` or the desktop entry's **Couch mode**, and by itself in a gamescope session
  such as the Steam Deck's Game Mode.

### Installing

- Windows builds from GOG's Galaxy content system, with their DLC and language, and native Linux
  builds from GOG's offline installers, read file by file.
- A download queue, kept across restarts and reordered by dragging; **Download now** starts a row
  at once and puts the download it pauses right after it; a row that started already, taken out
  of the queue, is paused with what it downloaded, without holding the queue. Downloads and
  automatic updates pause while a game runs (a Steam game too, once Slatty sees it run) and go on
  once it closes (can be turned off), resume after a crash or a power cut, and wait while GOG is
  out of reach.
- Every file checked against GOG's checksums; the game folder appears only once all are right.
- Updates in place, automatic for games on their newest build, with GOG's binary patches; DLC,
  language and version changes; verify, repair, uninstall.
- GOG's post-install setup at the first launch: install scripts, setup programs,
  redistributables.
- GOG's offline installers downloaded to keep, for Windows, Linux or macOS, in one or more
  languages, with their DLC's, from a game's Manage drawer (installed or not) through the download
  queue, into a folder chosen in Settings: each file checked against GOG's MD5 sums, one already in
  the folder too, never replaced when it does not match (a file GOG keeps no sums for is said not
  checked), damaged parts downloaded again, a download taken up where it stopped (`slatty
  installers` too).
- File checks at the lowest priority, so the desktop and a running game stay responsive.

### Playing

- Windows games through umu-launcher and a Proton build per game; native Linux games directly, or
  through `steam-run` or umu's runtime on NixOS; launch options asked at the first Play.
- Optional controller compatibility per game, through the SDL input mode of UMU-Proton, GE-Proton
  and Proton-CachyOS (checked with UMU-Proton only); off by default, saved in Game settings and
  applied to command-line launches too.
- Windows games isolated from your files by default: umu's container with a home of the game's
  own, the tools it runs through read-only inside bubblewrap, no D-Bus, its own processes only.
  Where bubblewrap cannot hold it (Ubuntu 23.10 and later without an AppArmor profile), the launch
  is refused with the reason and the way out, never run without isolation; `slatty doctor` checks
  it too.
- Sessions followed until the game's last process ends; play time reported to GOG as Galaxy does
  (can be turned off).
- Proton builds (GE-Proton, Proton-CachyOS, UMU-Proton) downloaded from GitHub and checked against
  their sums, followed as `<project>-latest`, kept up to date and listed in Steam and Heroic, each
  once turned on.

### Saves and achievements

- Cloud saves synced before and after each session, decided on content hashes; conflicts never
  settle themselves; a copy of every save replaced or deleted, locally and in the cloud.
- Achievements listed, reported as games unlock them through Comet once **Achievements in game**
  is turned on, or unlocked and cleared by hand once **Manual achievements** is.

### Beyond GOG

- Steam's games in the same library (off until turned on), read from the Steam client's own files
  and played, installed, verified and uninstalled through the client; no GOG account needed for
  them. Their play time, last session and achievements are Steam's own. A game owned on both stores
  shows once, its play times added up or one store's preferred. Images the client has not saved
  come from Steam's image servers, and what it has not described from Steam's Web API with your own
  key, each once turned on.
- Games from no store, added by the program that starts them, by a Windows installer (installing on
  a drive D: that is the folder chosen), or by a Linux installer or archive unpacked without running
  it, each into a folder of its own: a Windows program through Proton in a prefix of
  its own, a Linux program, script or AppImage natively, isolated like the others. Removing one
  never deletes its files, but for those Slatty unpacked, and GOG is never told of it.
- GOG games installed by another tool, imported from their folder or from Heroic.
- A command line (`slatty`) and an interface (`slatty-gui`) on the same core; a Nix package with a
  desktop entry and an icon, the entry's name being the window's application id.

### Security and privacy

Reviewed before publication; [SECURITY.md](SECURITY.md) tells the whole of it.

- Nothing is written through a link or into a pipe a game left in its folder, prefix or save
  folders; network answers, manifests, chunks and the files read from a game folder are bounded in
  size before memory is taken.
- The account's token goes only to GOG's own addresses, never leaves in a `Referer`, and stays out
  of logs and process arguments. Comet receives it through a folder of the user's alone, is taken
  as ready only when it runs as the user, and stops with the launcher even when the launcher is
  killed.
- Comet runs only once **Achievements in game** is turned on: while it runs, any local program can
  get the access token from it.
- umu's database is read from the Proton build's own copy; asking umu's server is off until turned
  on. The runtime refresh never has umu ask GitHub.
- **Forget account** deletes what is kept of an account on the computer; saves stay.
- Steam's files are only read, its sign-in and settings files never; GOG is never asked about a
  Steam game; Steam's servers are asked nothing until **Images from Steam** or **Steam Web API** is
  turned on, and the Web API key stays in the keyring.
- Every host contacted is listed in [SECURITY.md](SECURITY.md), with how to report a vulnerability
  privately.

Known limits are listed in [SECURITY.md](SECURITY.md) and [docs/compatibility.md](docs/compatibility.md).
