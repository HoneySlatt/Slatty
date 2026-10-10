# Roadmap

There are no dates. Each milestone ends with results checked against GOG and recorded in
[compatibility.md](compatibility.md).

## Done

- **Foundations.**
  - Nix development shell.
  - Sign-in and keyring storage.
  - Library with offline cache.
  - Supervised game sessions.
  - Three-way cloud sync.
  - Comet integration.
  - Manual achievement management.
- **Interface redesign.** Library, Achievements, Downloads and Settings tabs; cover grid with
  shelves, sort, size and filters; game page with key art, play time, cloud and achievement
  summaries, tools in drawers; favorites; Settings with a side list that follows the scroll.
- **Customisation.** Built-in themes, a theme file for colours, corners and motion, the interface
  font, the cover size. Each game's title, sorting title, cover and background can be changed,
  and a game can be hidden.
- **Large libraries.** The library and Achievements grids build only the rows in view, so a
  library of 10,000 games stays fluid.
- **Interface.** Iced front end covering:
  - library;
  - game detail;
  - play and stop;
  - cloud status and conflicts;
  - achievements;
  - installs;
  - maintenance;
  - settings.
- **Installation.**
  - Galaxy generation 2 Windows builds: base game, one language.
  - A download queue, kept across restarts and reordered by dragging.
  - Native Linux builds from GOG's offline installers, read file by file; a default platform in
    Settings and a choice per install.
  - Staged and verified downloads, pause and resume.
  - Wine prefix created at first launch.
- **Launching.** A Proton build per game; native Linux games through `steam-run` or umu on NixOS;
  games with several launch options ask which one at the first Play.
- **Privacy and security.** Switches for umu's game database, play time reporting and Comet;
  Comet started only for games that ship the Galaxy SDK; private data folders; no write through
  a link leading out of a game folder; dependency audit.
- **Maintenance.** Verify, repair, uninstall.
- **Updates.** Detect a newer build; update in place, file by file, resumable. Unchanged chunks
  of changed files are copied locally instead of downloaded. GOG's binary patches (xdelta3) are applied
  when available.
- **DLC and languages.** Owned DLC installed by default; add, remove or switch language later.
- **Post-install setup.** GOG script interpreter or setup programs, game-folder dependencies, shared
  redistributables.
- **Galaxy dummy service.** Comet's `GalaxyCommunication` service registered in each prefix, for
  games whose Galaxy SDK needs it to report achievements.

## Planned

- **Steam games beside GOG's.** Driven by the installed Steam client, which installs, runs and
  syncs them; owned games and achievements through the user's own Web API key. The groundwork is
  done: game ids name their store. See [steam-plan.md](steam-plan.md) and
  [steam-feasibility.md](steam-feasibility.md).
- **Gamepad navigation.** A couch mode.
- **Translations of the interface.** It is in English for now.
- **More runners.** System Wine, managed Proton downloads.
- **Game isolation.** Run games without access to the whole home folder (today a Windows game
  sees every file through Wine's `z:` drive, and a Linux game runs as the user).
- **Packaging.** A Nix package (bringing `steam-run-free` for native games), then other
  distributions.
- **Windows host support.** The core avoids Linux-only assumptions outside the session supervisor
  and the runners.

## Out of scope

Store purchases, friends and chat, multiplayer services, an in-game overlay, mods, plugins.
