# Slatty

<img width="3840" height="2121" alt="Screenshot from 2026-10-16 05-19-20" src="https://github.com/user-attachments/assets/92b1c9ba-2551-44b2-867a-f3192db4f72a" />
A native Linux launcher for your GOG games, written in Rust with an [Iced](https://iced.rs)
interface. Your Steam games and the games you add yourself can sit in the same library.

Slatty signs in to your GOG account, installs your games (Windows builds through Proton,
or GOG's native Linux builds), keeps their cloud saves in sync and reports their achievements to
GOG, all without the GOG Galaxy client and without a web view. Windows games run isolated from your
files by default.

> **Status: early development.** Slatty is used daily by its author but has no release
> yet. Commands and on-disk formats may still change.
>
> **Not affiliated with GOG or Valve.** GOG publishes no API for launchers: Slatty uses the
> services GOG Galaxy uses, understood by reverse engineering (see [credits](#credits)). They may
> change or stop working at any time.

## Features

**Library**
- Your GOG games with their covers, kept for offline use, fluid at 10,000 games.
- Shelves (all, installed, favourites, hidden), sorting, and filters by store, system,
  achievements and cloud saves.
- Change any game's title, sorting title, cover and background, from a file or from SteamGridDB;
  hide the ones you do not want to see.
- Built-in themes, your own theme file, the font and the cover size of your choice.
- A couch mode for a gamepad, full screen, for a television or a Steam Deck, where it starts by
  itself in the Game Mode.

**Installing**
- Windows builds from GOG's Galaxy content system, native Linux builds from GOG's offline
  installers, read file by file.
- A download queue you reorder by dragging; downloads that resume after a crash or a power cut,
  and wait while GOG is out of reach.
- Every file checked against GOG's checksums; the game folder appears only once all are right.
- Updates in place (automatically for games on their newest build), with GOG's binary patches when
  there are; DLC and language changes; verify, repair and uninstall.
- GOG's offline installers kept in a folder, for Windows, Linux or macOS, checked against GOG's
  MD5 sums.

**Playing**
- Optional controller compatibility per game with UMU-Proton, GE-Proton and Proton-CachyOS.
- Windows games through [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) and
  the Proton build of your choice, per game; Linux games natively.
- Isolation from your files: an isolated game sees its own folder and a home of its own, and
  cannot change the tools other games run through.
- The session followed until the game's last process ends; play time reported to GOG as Galaxy
  does (can be turned off).
- GOG's post-install setup (install scripts, redistributables) at the first launch.

**Saves and achievements**
- Cloud saves synced before and after each session, decided on file contents. A conflict never
  settles itself, and every save replaced is copied first.
- Achievements as GOG records them; unlocked in game through
  [Comet](https://github.com/imLinguin/comet) once you turn it on, or by hand.

**Beyond GOG**
- Steam games, read from the Steam client's own files and played, installed and uninstalled
  through it. A game owned on both stores shows once. No GOG account needed for them.
- Games from no store, added by the program that starts them, or from an installer or archive.
- Proton builds (GE-Proton, Proton-CachyOS, UMU-Proton) downloaded from GitHub, checked, and kept
  up to date once you turn it on.

Everything has automated tests. What has also been checked against real services and games is
recorded in [docs/compatibility.md](docs/compatibility.md): sign-in, library, Windows and Linux
installs, playing isolated (Hollow Knight, Firewatch, Cyberpunk 2077, Blasphemous), cloud saves both
ways, an achievement earned in game, verify and repair, uninstall, a crash during a download, and
Steam's library. Not checked against real services yet: pausing an install by hand, applying an
update, DLC and language changes, Visual C++ redistributables, offline installers, downloads
pausing while a game runs, games added by hand, Steam installs, and keeping Proton builds up to
date; nor the couch mode with a real gamepad, on a Steam Deck or on a television.

A DualSense over USB was also checked by the user in Bleak Faith: Forsaken with UMU-Proton's
SDL input mode. The new per-game switch and controller behavior with GE-Proton and
Proton-CachyOS still need a real-game check.

Linux only, tested and packaged on x86-64; macOS and Windows hosts are not supported. See the
[roadmap](docs/roadmap.md).

## Requirements

- Linux with Wayland or X11, on x86-64 (the platform tested). Development happens on NixOS with
  niri.
- A Secret Service keyring (GNOME Keyring, KeePassXC, KWallet with its Secret Service bridge).
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) (checked with 1.4) and a
  Proton build: one in `~/.local/share/Steam/compatibilitytools.d`, one of Valve's that Steam
  downloaded, one umu downloaded, or one Slatty downloads (Settings → Runners).
- [bubblewrap](https://github.com/containers/bubblewrap), able to start a sandbox, for isolated
  games. On Ubuntu 23.10 and later it needs an AppArmor profile first, for umu as well
  ([how](docs/user-guide.md#ubuntu-and-apparmor)).
- On NixOS, `steam-run` (or `steam-run-free`) for native Linux games; without it they run in umu's
  runtime.
- Optional:
  - [Comet](https://github.com/imLinguin/comet) and its `GalaxyCommunication.exe` service, for
    achievements earned in game;
  - an XDG desktop portal with a file chooser (xdg-desktop-portal-gtk, -gnome, -kde…) and libdbus,
    to browse for folders and images (typing a path works without);
  - the Steam client, for Steam games.

## Install

**With Nix**, from a checkout:

```sh
nix run .            # or: nix build, then result/bin/slatty-gui
```

The package brings umu-launcher, bubblewrap, Comet, `GalaxyCommunication.exe` (built from Comet's
sources) and `steam-run-free`, and installs a desktop entry.

**From source**, for development:

```sh
nix develop          # or `direnv allow`
cargo build
slatty doctor        # checks umu, bubblewrap, Comet, the keyring…
slatty-gui
```

`nix develop` provides the toolchain and every tool above, and puts `target/debug` on `PATH`.
Without Nix you need Rust 1.90 or newer, `pkg-config`, the Wayland, X11 and Vulkan development
libraries, umu-launcher and Comet on `PATH`, and `GalaxyCommunication.exe` in
`~/.local/share/slatty/` (or its path in `SLATTY_GALAXY_COMMUNICATION`).

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

- You sign in on GOG's own page; Slatty never sees your password. Your session is kept in
  the system keyring, never in a log or a process argument.
- No telemetry. Besides GOG, every host contacted is listed in
  [SECURITY.md](SECURITY.md#what-leaves-your-computer) with the setting that controls it; the
  optional ones (GitHub, SteamGridDB, Steam's servers, umu's database) are off until you turn them
  on.
- Windows games run isolated from your files by default, inside umu's container and bubblewrap.
  Isolation keeps games out of your files; [SECURITY.md](SECURITY.md#games-and-isolation) says what
  it leaves open.
- Achievements earned in game go through Comet only once you turn **Achievements in game** on:
  while it runs, any program on this computer can get your GOG access token from it.
- Saves are never replaced or deleted without a copy, and a cloud conflict never settles itself
  ([how](docs/cloud-saves.md)).

Report vulnerabilities privately, as [SECURITY.md](SECURITY.md#reporting-a-vulnerability) says.

## Documentation

- [User guide](docs/user-guide.md): the window, settings, where data is stored, troubleshooting
- [Command line](docs/command-line.md): every command and option
- [Cloud saves](docs/cloud-saves.md): how sync decides, and how your saves are protected
- [Theming](docs/theming.md): colours, corners and motion from a file
- [Architecture](docs/architecture.md): crates, modules, main flows and design decisions
- [GOG integration](docs/gog-integration.md): every GOG service used, its source, what is verified
- [Compatibility](docs/compatibility.md): results of real tests
- [Testing](docs/testing.md): automated tests, benchmarks and checks against GOG and Steam
- [Roadmap](docs/roadmap.md) · [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [AI agents](AGENTS.md)

## Credits

Slatty stands on the work of others:

- [Comet](https://github.com/imLinguin/comet) (Apache-2.0) implements the Galaxy communication service
  games talk to; Slatty runs it beside them.
- [heroic-gogdl](https://github.com/Heroic-Games-Launcher/heroic-gogdl) (GPL-3.0) is the reference
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
  gives the sharpening of enlarged key art (RCAS), ported in `crates/gui/src/scaled.rs` with its
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
