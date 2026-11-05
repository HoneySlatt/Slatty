# Slatty

<img width="3840" height="2121" alt="Slatty's library" src="https://github.com/user-attachments/assets/92b1c9ba-2551-44b2-867a-f3192db4f72a" />

A native Linux launcher for your GOG games, written in Rust. Your Steam and Epic games, the games
you add yourself and your games of other consoles sit in the same library.

Slatty signs in to your GOG account, installs your games (Windows builds through Proton, or GOG's
native Linux builds), keeps their cloud saves in sync and reports their achievements to GOG, all
without the GOG Galaxy client and without a web view. Windows games run isolated from your files
by default.

> **Status: early development.** The latest release, 0.2.0, runs on Linux. Slatty builds for
> Windows 10 and 11, and installs and plays a GOG game there from the command line: tested under
> Wine, not yet on Windows itself. Slatty is now paused until its new interface framework,
> slime-ui, can run it: 0.3 brings that interface, 0.4 Windows ([plan](docs/PLAN.md)). Until 1.0,
> commands and on-disk formats may still change ([changelog](CHANGELOG.md)).
>
> **Not affiliated with GOG, Valve or Epic Games.** GOG and Epic publish no API for launchers:
> Slatty uses the services their own clients use, understood by reverse engineering (see
> [credits](#credits)). They may change or stop working at any time.

## Features

- **Library**: covers kept for offline use, fluid at 10,000 games; shelves, sorting and filters;
  titles, covers and backgrounds you change, from a file or SteamGridDB; themes and your own theme
  file; a couch mode for a gamepad, on a television or a Steam Deck (not tried on a Deck yet).
- **Installing**: Windows builds from GOG's Galaxy content system and native Linux builds from its
  offline installers; a download queue that resumes after a crash or a power cut; every file
  checked against GOG's checksums before the game folder appears; updates in place with GOG's
  binary patches; DLC and languages; verify, repair, uninstall; offline installers kept.
- **Playing**: Windows games through
  [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) and the Proton build of
  your choice, isolated from your files; Linux games natively; controller compatibility per game;
  play time reported to GOG (can be turned off); GOG's post-install setup at the first launch.
- **Saves and achievements**: cloud saves synced before and after each session, decided on file
  contents, never replaced without a copy, conflicts never settled by themselves; achievements as
  GOG records them, unlocked in game through [Comet](https://github.com/imLinguin/comet) once you
  turn it on.
- **Beyond GOG**: Steam games, through the Steam client; Epic games, installed, played, updated
  and their cloud saves synced; games from no store; games of other consoles run in DuckStation,
  PCSX2, PPSSPP, RPCS3, shadPS4, Kyty, Dolphin or Eden; GOG games imported from another tool;
  Proton builds downloaded and kept up to date.

Everything has automated tests. What was also checked against the real services, games and
hardware, and what was not yet, is in [docs/compatibility.md](docs/compatibility.md).

## Requirements

- Linux with Wayland or X11, on x86-64. Development happens on NixOS under Wayland; X11 has not
  been tried yet.
- Hardware: aimed to run from a 2016 laptop (two-core Core i5-6200U, Intel HD 520, 4 GB, a hard
  disk) to a 4K 240 Hz desktop. Only the desktop is measured so far
  ([quality bar](docs/ROADMAP.md#quality-bar)).
- A Secret Service keyring: GNOME Keyring (the one checked), or KeePassXC or KWallet with its
  Secret Service bridge (not tried yet).
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) (checked with 1.4) and a
  Proton build: Steam's, umu's, one in `~/.local/share/Steam/compatibilitytools.d`, or one Slatty
  downloads (Settings → Runners).
- [bubblewrap](https://github.com/containers/bubblewrap), able to start a sandbox, for isolated
  games; on Ubuntu 23.10 and later it needs an AppArmor profile first
  ([how](docs/user-guide.md#ubuntu-and-apparmor), not tried yet).
- On NixOS, `steam-run` (or `steam-run-free`) for native Linux games; without it they run in umu's
  runtime.
- Optional: [Comet](https://github.com/imLinguin/comet) and its `GalaxyCommunication.exe` service,
  for achievements earned in game; an XDG desktop portal with a file chooser, to browse for files;
  the Steam client, for Steam games.

## Install

**With Nix** (flakes enabled):

```sh
nix run github:HoneySlatt/Slatty     # or, from a checkout: nix run .
nix profile install github:HoneySlatt/Slatty
```

The package brings umu-launcher, bubblewrap, Comet, `GalaxyCommunication.exe` and
`steam-run-free`, with a desktop entry. It was only used on NixOS so far.

**From source**, for development:

```sh
nix develop          # or `direnv allow`
cargo build
slatty doctor        # checks umu, bubblewrap, Comet, the keyring…
slatty-gui
```

Without Nix (not tried yet), [CONTRIBUTING.md](CONTRIBUTING.md#development-environment) lists what
to install.

## First steps

Start `slatty-gui`, click **Open the GOG sign-in page**, sign in in your browser, and paste back
the address you land on. Your library shows; pick a game and press **Install**.

The same from the command line:

```sh
slatty auth login                 # sign in through your browser
slatty library sync               # fetch your library
slatty library list               # game ids
slatty install <id> --info        # size and languages; downloads nothing
slatty install <id> --proton <Proton folder>
slatty launch <id>
```

The [user guide](docs/user-guide.md) covers the window, and the
[command line](docs/command-line.md) page every command.

## Security and privacy

- You sign in on GOG's own page; Slatty never sees your password. Your session is kept in the
  system keyring, never in a log or a process argument.
- No telemetry. Every host Slatty contacts besides GOG is listed in
  [SECURITY.md](SECURITY.md#what-leaves-your-computer); the optional ones are off until you turn
  them on.
- Windows games run isolated from your files by default;
  [SECURITY.md](SECURITY.md#games-and-isolation) says what isolation leaves open.
- Achievements earned in game go through Comet only once you turn them on: while it runs, any
  program on this computer can get your GOG access token from it ([more](SECURITY.md#comet)).
- Report vulnerabilities privately, as [SECURITY.md](SECURITY.md#reporting-a-vulnerability) says.

## Documentation

- [User guide](docs/user-guide.md), [command line](docs/command-line.md),
  [cloud saves](docs/cloud-saves.md), [theming](docs/theming.md): using Slatty
- [Architecture](docs/ARCHITECTURE.md) and its [decisions](docs/adr/README.md);
  [GOG](docs/gog-integration.md) and [Epic](docs/epic-integration.md) integration, every service
  used and its source; [compatibility](docs/compatibility.md); [testing](docs/testing.md) and
  [measurements](docs/measurements.md)
- [Roadmap](docs/ROADMAP.md) · [Plan](docs/PLAN.md) · [History](docs/history.md) ·
  [Changelog](CHANGELOG.md) · [Glossary](docs/glossary.md)
- [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [AI agents](AGENTS.md)

## Credits

Slatty stands on the work of others:

- [Comet](https://github.com/imLinguin/comet) (Apache-2.0) implements the Galaxy communication service
  games talk to; Slatty runs it beside them.
- [heroic-gogdl](https://github.com/Heroic-Games-Launcher/heroic-gogdl) (GPL-3.0-only) is the reference
  for Galaxy depots, cloud storage and sessions. Parts of Slatty follow its logic.
- [Heroic Games Launcher](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher) showed how
  save locations, Proton and Comet fit together.
- [gogapidocs](https://gogapidocs.readthedocs.io) documents many GOG endpoints.
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) runs Proton outside Steam.
- [Lucide](https://lucide.dev) (ISC; some icons from Feather, MIT) provides the icons, in
  `crates/gui/assets/icons`.
- [Geist](https://vercel.com/font) (SIL Open Font License 1.1) is the interface font, built in from
  `crates/gui/assets/fonts`, where its licence is.
- AMD's [FidelityFX Super Resolution 1](https://github.com/GPUOpen-Effects/FidelityFX-FSR) (MIT)
  gives the sharpening of enlarged key art (RCAS), ported in `crates/app/src/scaled.rs` with its
  notice.
- [oxidelta](https://github.com/sockudo/oxidelta) (MIT) applies GOG's xdelta3 patches.
- [rfd](https://github.com/PolyMeilex/rfd) (MIT) opens the desktop's file chooser.
- [SteamGridDB](https://www.steamgriddb.com) and its community provide covers and backgrounds in
  Edit game, once turned on, with your own API key.

## License

Copyright © 2026 HoneySlatt, emblem included. GPL-3.0-or-later. See [LICENSE](LICENSE);
[NOTICE](NOTICE) gives the licences of the works of
others Slatty includes. The Nix package installs both, with the font's and the icons' licences,
in `share/licenses/slatty`.
