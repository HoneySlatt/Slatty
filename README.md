# Slatty

<img width="3840" height="2121" alt="Slatty's library" src="https://github.com/user-attachments/assets/92b1c9ba-2551-44b2-867a-f3192db4f72a" />

A native Linux launcher for your GOG games, written in Rust with an [Iced](https://iced.rs)
interface. Your Steam games and the games you add yourself can sit in the same library.

Slatty signs in to your GOG account, installs your games (Windows builds through Proton,
or GOG's native Linux builds), keeps their cloud saves in sync and reports their achievements to
GOG, all without the GOG Galaxy client and without a web view. Windows games run isolated from your
files by default.

> **Status: early development.** 0.1.0 is Slatty's first release. Until 1.0, commands and
> on-disk formats may still change ([changelog](CHANGELOG.md)).
>
> **Not affiliated with GOG or Valve.** GOG publishes no API for launchers: Slatty uses the
> services GOG Galaxy uses, understood by reverse engineering (see [credits](#credits)). They may
> change or stop working at any time.

## Features

**Library**
- Your GOG games with their covers, kept for offline use, fluid at 10,000 games.
- Shelves (all, installed, favorites, hidden), sorting, and filters by store, system,
  achievements and cloud saves.
- Change any game's title, sorting title, cover and background, from a file or from SteamGridDB;
  hide the ones you do not want to see.
- Built-in themes, your own theme file, the font and the cover size of your choice.
- A couch mode for a gamepad, full screen, for a television or a Steam Deck, where it starts by
  itself in the Game Mode (not tried on a Steam Deck yet).

**Installing**
- Windows builds from GOG's Galaxy content system, native Linux builds from GOG's offline
  installers, read file by file.
- A download queue you reorder by dragging; downloads that resume after a crash or a power cut,
  and wait while GOG is out of reach.
- Every file checked against GOG's checksums; the game folder appears only once all are right.
- Updates in place (automatically for games on their newest build), with GOG's binary patches
  where GOG offers them; DLC and language changes; verify, repair and uninstall.
- GOG's offline installers kept in a folder, for Windows, Linux or macOS, checked against GOG's
  MD5 sums.

**Playing**
- Windows games through [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) and
  the Proton build of your choice, per game; Linux games natively. A game that can be started
  several ways asks which at its first Play.
- Optional controller compatibility per game, through the SDL input mode of UMU-Proton, GE-Proton
  and Proton-CachyOS.
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
- Epic Games Store games, after signing in to Epic: shown in the library with their covers, not
  installable from Slatty yet.
- Games from no store, added by the program that starts them, or from an installer or archive.
- Games of other consoles, from your own files, run in DuckStation, PCSX2, PPSSPP, RPCS3, shadPS4,
  Kyty, Dolphin or Eden: a scan of their folders adds them.
- GOG games installed by another tool, imported from their folder or from Heroic.
- Proton builds (GE-Proton, Proton-CachyOS, UMU-Proton) downloaded from GitHub, checked, and kept
  up to date once you turn it on.

Everything has automated tests. What has also been checked against real services, games and
hardware is recorded in [docs/compatibility.md](docs/compatibility.md): sign-in, library, Windows
and Linux installs, updates, DLC and languages, pausing and resuming, playing isolated, cloud
saves both ways and their deletions, achievements earned in game, verify, repair and uninstall,
offline installers, the download queue, Proton builds kept up to date, games added by hand,
Steam's client and Web API, Epic sign-in and library, controller compatibility, and the couch mode
with a real gamepad.
Not checked yet: a redistributable asking for a restart, the account shown online to GOG friends,
an offline installer whose checksums GOG does not keep, Ubuntu with AppArmor, Steam as a Flatpak,
and the couch mode on a Steam Deck or on a television.

Linux only, tested and packaged on x86-64; macOS and Windows hosts are not supported. See the
[roadmap](docs/roadmap.md).

## Requirements

- Linux with Wayland or X11, on x86-64 (the platform tested). Development happens on NixOS with
  niri; X11 has not been tried yet.
- A Secret Service keyring: GNOME Keyring (the one checked), or KeePassXC or KWallet with its
  Secret Service bridge (not tried yet).
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) (checked with 1.4) and a
  Proton build: one in `~/.local/share/Steam/compatibilitytools.d`, one of Valve's that Steam
  downloaded, one umu downloaded, or one Slatty downloads (Settings → Runners).
- [bubblewrap](https://github.com/containers/bubblewrap), able to start a sandbox, for isolated
  games. On Ubuntu 23.10 and later it needs an AppArmor profile first, for umu as well
  ([how](docs/user-guide.md#ubuntu-and-apparmor); those steps were not tried yet).
- On NixOS, `steam-run` (or `steam-run-free`) for native Linux games; without it they run in umu's
  runtime.
- Optional:
  - [Comet](https://github.com/imLinguin/comet) and its `GalaxyCommunication.exe` service, for
    achievements earned in game;
  - an XDG desktop portal with a file chooser (xdg-desktop-portal-gtk, -gnome, -kde…) and libdbus,
    to browse for folders and images (typing a path works without);
  - the Steam client, for Steam games.

## Install

**With Nix** (flakes enabled):

```sh
nix run github:HoneySlatt/Slatty     # or, from a checkout: nix run .
nix profile install github:HoneySlatt/Slatty
```

The package brings umu-launcher, bubblewrap, Comet, `GalaxyCommunication.exe` (built from Comet's
sources) and `steam-run-free`, and installs a desktop entry and its icon. It was only used on
NixOS; Nix on another distribution was not tried yet.

**From source**, for development:

```sh
nix develop          # or `direnv allow`
cargo build
slatty doctor        # checks umu, bubblewrap, Comet, the keyring…
slatty-gui
```

`nix develop` provides the toolchain, umu-launcher, bubblewrap, Comet and
`GalaxyCommunication.exe` (not `steam-run`), and puts `target/debug` on `PATH`. Without Nix
(not tried yet) you need Rust 1.90 or newer, a C compiler, `pkg-config`, the Wayland, X11, Vulkan
and libudev development files, umu-launcher and Comet on `PATH`, and `GalaxyCommunication.exe` in
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
  [SECURITY.md](SECURITY.md#what-leaves-your-computer) with the setting that controls it, where
  there is one; the
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
- [Epic Games integration](docs/epic-integration.md): every Epic service used, its source, what is
  verified
- [Compatibility](docs/compatibility.md): results of real tests
- [Testing](docs/testing.md): automated tests, benchmarks and checks against GOG and Steam
- [Roadmap](docs/roadmap.md) · [Changelog](CHANGELOG.md)
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
