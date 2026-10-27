# Roadmap

There are no dates. What has been checked against real services, and what not yet, is in
[compatibility.md](compatibility.md).

## Done

**Foundations**
- Sign-in in the system browser, session in the system keyring, refreshed under a lock every
  process shares.
- The GOG library with its covers, cached for offline use.
- Game sessions followed by a supervisor until the last process ends, even when the launcher closes.
- A command line and an interface on the same core.

**Installing and looking after games**
- Windows builds from GOG's Galaxy content system (generation 2): DLC and language chosen at
  install and changed later, any build GOG offers.
- Native Linux builds from GOG's offline installers, read file by file; a default platform and a
  choice per install.
- Staged, verified downloads; a queue reordered by dragging, kept across restarts; resume after a
  crash, wait while GOG is out of reach.
- Updates in place, automatic for games on their newest build, with GOG's binary patches and
  unchanged chunks copied rather than downloaded.
- Verify, repair, uninstall; post-install setup (GOG's scripts, setup programs, redistributables,
  game-folder dependencies); Comet's Galaxy service in each prefix.
- GOG's offline installers downloaded to keep, through the same queue, checked against GOG's MD5
  sums.

**Playing**
- Controller compatibility per game, off by default, using the SDL input mode in UMU-Proton,
  GE-Proton and Proton-CachyOS.
- umu and a Proton build per game; native Linux games through `steam-run` or umu's runtime on
  NixOS; launch options asked at the first Play.
- Isolation from your files, on by default for Windows games: umu's container with a home of the
  game's own, tools read-only through bubblewrap, no D-Bus, a process namespace of its own.
- Proton builds downloaded from GitHub (GE-Proton, Proton-CachyOS, UMU-Proton), checked against
  their sums, followed as `<project>-latest`, kept up to date and shared with Steam and Heroic once
  turned on.

**Saves and achievements**
- Three-way cloud sync on content hashes, conflicts that never settle themselves, a copy of every
  save replaced or deleted, on either side.
- Achievements listed, reported as games unlock them through Comet (once turned on), or changed by
  hand.
- Play time read from GOG and reported to it, as Galaxy does.

**Interface**
- Library, Achievements, Downloads and Settings tabs; shelves, sort, search and filters (store,
  system, achievements, cloud saves), remembered when asked.
- A game page with key art, play time, cloud and achievement cards, and its tools in drawers.
- Notifications in the window and on the desktop.
- Built-in themes, a theme file, the font and the cover size; each game's title, sorting title,
  cover and background (from a file or SteamGridDB), and hidden games.
- Fluid at 10,000 games: only the rows in view are built, lists kept, images shown from files at
  the size they are drawn.
- A couch mode for a gamepad, full screen: home rows, the library with search on a keyboard on the
  screen, game pages, installs and downloads; it starts by itself in the Steam Deck's Game Mode.
  Not yet tried on a Steam Deck.

**Beyond GOG**
- Steam's games in the same library, read from the client's files and run through it; one card for
  a game owned on both stores; images and data the client lacks from Steam's servers once turned
  on. No GOG account needed for them.
- Games from no store, added by their program, a Windows installer or a Linux installer or archive.
- Emulated games of eight systems (DuckStation, PCSX2, PPSSPP, RPCS3, shadPS4, Kyty, Dolphin,
  Eden), found by a scan of their folders or added one by one. Not yet played for real in each
  emulator.
- GOG games installed by another tool, imported from their folder or from Heroic.
- Epic Games Store: signing in and the account's games in the library, with their covers; a game's
  Windows build installed from the window or the command line, each file checked against Epic's
  checksum, played through Proton signed in to the account, its Proton build and isolation changed
  in Game settings, and uninstalled. Playing one has not been tried with a real game yet.

**Privacy, security and packaging**
- Switches for every third party, off until turned on; private data folders; no write through a
  link leading out of a game folder; a review before publication, and a dependency audit.
- A Nix package with `steam-run-free`, a desktop entry and AppStream data.

## Planned

- **Epic games kept up to date**: updated, verified and repaired; then Epic's cloud saves and
  achievements.
- **Translations** of the interface, in English for now.
- **Steam downloads** followed more exactly while the client downloads.
- **Packaging** for other distributions.
- **More runners**: Wine without Proton for the games Slatty installs (it exists for
  imported games).
- **Windows hosts**, some day: the core relies on Linux in many places today.

## Out of scope

Store purchases, friends and chat, multiplayer services, an in-game overlay, mods, plugins.
Slatty has no friends or chat feature; Comet, which it starts for achievements once turned
on, still subscribes to those topics on GOG for the account (see
[SECURITY.md](../SECURITY.md#comet)).
