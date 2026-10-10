# SlattyLauncher

A native GOG launcher for Linux, written in Rust with an [Iced](https://iced.rs) interface.

It signs in to your GOG account, shows your library, installs Windows games from GOG's Galaxy
content system, runs them through Proton, keeps cloud saves in sync and reports achievements to
GOG through [Comet](https://github.com/imLinguin/comet), all without the official GOG Galaxy
client and without a web view.

> **Status: early development.** SlattyLauncher is usable by its author but has not been released.
> Interfaces, commands and on-disk formats may still change.
>
> **Not affiliated with GOG.** It relies on Galaxy services that GOG does not document for third
> parties. They were understood by reverse engineering (see [credits](#credits)) and may change
> or stop working at any time.

## What works

Every feature below has automated tests. The last column says whether it has also been checked
against real GOG services; details are in [docs/compatibility.md](docs/compatibility.md).

| Feature | Real GOG check |
|---|---|
| Sign-in in the system browser, tokens in the system keyring | yes |
| Library with covers, cached for offline use | yes |
| Install Windows builds (Galaxy depots), pause and resume, integrity checks | install yes, pause/resume not yet |
| Install native Linux builds from GOG's offline installers, file by file | yes (Hollow Knight: Silksong installed and played) |
| Launch through umu + Proton, follow the session until the last process exits | yes |
| Launch options: pick the game or one of its tools at the first Play, change it later | yes (Trails in the Sky) |
| Cloud saves: three-way sync, conflict handling, backups | download and upload yes |
| Achievements: list, report unlocks made in game through Comet | listing yes, in-game unlock not yet |
| Achievements: unlock manually (off until turned on in Settings → Advanced) | yes |
| Verify, repair and uninstall installed games | verify and repair yes, uninstall not yet |
| Updates: detect a newer build, update in place | detection yes, applying not yet |
| DLC and language: choose at install, add, remove or switch later | ownership detection yes, changes not yet |
| Post-install setup: GOG scripts, game-folder dependencies, redistributables | GOG script yes (Undertale), redistributables not yet |

Not supported yet: macOS and Windows hosts.
See [docs/roadmap.md](docs/roadmap.md).

## Requirements

- Linux with Wayland or X11. Development and testing happen on NixOS with niri.
- A Secret Service keyring (GNOME Keyring, KeePassXC, KWallet with its Secret Service bridge).
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) and a Proton build: a custom one
  (GE-Proton, Proton-CachyOS, …) in `~/.local/share/Steam/compatibilitytools.d`, one of Valve's
  (Proton Experimental, stable, Hotfix) downloaded by Steam in any of its libraries, or one umu
  downloaded.
- [Comet](https://github.com/imLinguin/comet) for achievements.
- Comet's `GalaxyCommunication.exe` dummy service, which some games need to report achievements.
- An XDG desktop portal with a file chooser (xdg-desktop-portal-gtk, -gnome, -kde…) and libdbus
  to browse for an install folder. Typing the path works without them.

The Nix development shell provides umu-launcher, Comet, `GalaxyCommunication.exe` (built from
Comet's sources) and every build dependency.

## Build and run

```sh
nix develop          # or `direnv allow`
cargo build
slatty doctor        # checks umu, Comet, the keyring and the Comet port
slatty-gui           # graphical interface
```

`nix develop` puts `target/debug` on `PATH`, so `slatty` and `slatty-gui` are available right
after `cargo build`. Without Nix you need Rust 1.90 or newer, `pkg-config`, the Wayland/X11 and
Vulkan development libraries, umu-launcher and Comet on `PATH`, and `GalaxyCommunication.exe` in
`~/.local/share/slatty/` (or its path in `SLATTY_GALAXY_COMMUNICATION`).

## Quick start

```sh
slatty auth login                      # sign in through your browser
slatty library sync                    # fetch your library
slatty install <game-id> --info        # see size and languages, downloads nothing
slatty install <game-id> --proton ~/.local/share/Steam/compatibilitytools.d/GE-Proton…
slatty launch <game-id>
```

Game ids are listed by `slatty library list`. The same actions are available in `slatty-gui`.
The [user guide](docs/user-guide.md) covers every command.

## Documentation

- [User guide](docs/user-guide.md): commands, interface, where data is stored, troubleshooting
- [Cloud saves](docs/cloud-saves.md): how sync works and how your saves are protected
- [Architecture](docs/architecture.md): crates, modules and main flows
- [Theming](docs/theming.md): colours, corners and motion from a file
- [GOG integration](docs/gog-integration.md): every service used, its source and what is verified
- [Compatibility](docs/compatibility.md): results of real tests
- [Testing](docs/testing.md): automated tests and manual checks against GOG
- [Roadmap](docs/roadmap.md)
- [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) · [AI agents](AGENTS.md)

## Credits

SlattyLauncher stands on the work of others:

- [Comet](https://github.com/imLinguin/comet) (Apache-2.0) implements the Galaxy communication
  service that games talk to; SlattyLauncher runs it beside each game.
- [heroic-gogdl](https://github.com/Heroic-Games-Launcher/heroic-gogdl) (GPL-3.0) is the reference
  for Galaxy depots, cloud storage and session handling. Parts of SlattyLauncher follow its logic.
- [Lucide](https://lucide.dev) (ISC) provides the interface icons, in `crates/gui/assets/icons`.
- [Geist](https://vercel.com/font) (SIL Open Font License 1.1) is the interface font, built into the
  binary from `crates/gui/assets/fonts`, where its licence is.
- [oxidelta](https://github.com/sockudo/oxidelta) (MIT) decodes GOG's xdelta3 patches.
- [rfd](https://github.com/PolyMeilex/rfd) (MIT) opens the desktop's folder chooser.
- [Heroic Games Launcher](https://github.com/Heroic-Games-Launcher/HeroicGamesLauncher) showed how
  save locations, Proton and Comet fit together.
- [gogapidocs](https://gogapidocs.readthedocs.io) documents many GOG endpoints.
- [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) runs Proton outside Steam.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
