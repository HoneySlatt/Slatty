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
- **Interface redesign.** Library, Achievements and Settings tabs; cover grid with shelves, sort,
  size and filters; game page with key art, play time, cloud and achievement summaries, tools in
  panels; favorites.
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
  - Staged and verified downloads, pause and resume.
  - Wine prefix created at first launch.
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

- **Store-like game pages.** Description, screenshots, changelog. GOG's descriptions are HTML,
  rendered through Iced's Markdown support.
- **Gamepad navigation.** A couch mode.
- **Translations of the interface.** It is in English for now.
- **More runners.** System Wine, per-game Proton choice, managed Proton downloads.
- **Native Linux builds.** GOG's offline installers.
- **Packaging.** A Nix package, then other distributions.
- **Windows host support.** The core avoids Linux-only assumptions outside the session supervisor
  and the runners.

## Out of scope

Store purchases, friends and chat, multiplayer services, an in-game overlay, mods, plugins.
