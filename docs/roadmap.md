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
- **Updates.** Detect a newer build; update in place, file by file, resumable.
- **DLC and languages.** Owned DLC installed by default; add, remove or switch language later.
- **Post-install setup.** GOG script interpreter or setup programs, game-folder dependencies, shared
  redistributables.

## Next

- **Galaxy dummy service.** Some games report achievements only when it is registered in the
  prefix.
- **Smaller updates.** Reuse unchanged chunks of changed files, or GOG's binary patches.

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
