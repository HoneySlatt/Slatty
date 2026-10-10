# Security

## Reporting a vulnerability

Please do not open a public issue for a security problem. Contact the maintainer privately through
the repository's security advisory feature once the project is published. Until then, contact the
author directly.

Include the steps to reproduce and the version or commit you tested.

## How SlattyLauncher handles your account

- **No password.** You sign in on GOG's own page, in your browser.
- **Tokens stay in the keyring.** Session tokens are stored only in the system keyring (Secret
  Service). Without a keyring, SlattyLauncher refuses to store them rather than writing a plain file.
- **Tokens stay out of logs and process lists.**
  - They never appear in logs, error messages or process arguments.
  - Some GOG endpoints take tokens in the URL, so network errors are reported without their URL.
- **Comet handoff.** Comet receives tokens through a file readable only by you, in a private runtime
  directory. The file is deleted as soon as Comet has read it.
- **Comet runs as little as it can.** It acts for your GOG account and listens on this computer
  (127.0.0.1:9977) while it runs, where any local program can reach it. It is started only for a
  game that ships GOG's Galaxy SDK, stopped when the game ends, and never started once
  **Achievements in game** is off (Settings → Privacy).
- **Logs.** Comet's log may contain game client identifiers. Review it before sharing.
- **Account changes.** Work started for one account never uses another's tokens: after a switch,
  its library, achievement, cloud and play time requests are refused, and its late results are
  dropped.
- **Private files.** SlattyLauncher's folders (settings, data, cache, state) are made readable by
  you only (0700) at every start, and its database 0600: they hold your library, play times, save
  backups and Wine prefixes.
- **Game files stay in the game folder.** Paths from GOG's manifests, installers and cloud
  listings are checked before anything is written. Symbolic links from Linux installers are
  resolved on disk once all are made, and one that leads out of the game folder, even through
  another link, is removed. No file is written through a link that leads out of the game folder,
  and an uninstall or update that would delete through one deletes nothing. Temporary files get
  names never used before and never follow a link already there. These checks do not guard
  against another program changing the folders at the same moment.

## What leaves your computer

- No telemetry.
- GOG's services and the download servers GOG names: your library, downloads, cloud saves,
  achievements and, unless turned off in Settings → Privacy, your play sessions (as Galaxy sends
  them). Sessions played while this is off are never sent later.
- umu's public game database (`umu.openwinecomponents.org`): a game's GOG product id, once, at its
  first launch, to pick its Proton fixes. It can be turned off in Settings → Privacy; the game then
  runs without fixes.
- SteamGridDB (`steamgriddb.com`), only once turned on in Settings → Advanced and only when you
  search it in Edit game: the name you search and your own SteamGridDB API key, kept in the system
  keyring and sent in a header. Pictures you look at come from its servers.
- GitHub (`api.github.com`, `github.com`, which sends the files from
  `release-assets.githubusercontent.com`), only once Proton downloads are turned on in
  Settings → Runners, and only when you list or download Proton builds or, once **Keep up to
  date** is on, at start at most once a day for the projects a game follows as
  `<project>-latest`: which project's releases are asked for, without any account. A downloaded archive is checked against the SHA-512 sum its
  release publishes before it is opened; this catches a damaged download, not a release replaced
  by whoever controls the project's GitHub account. It is unpacked like a Linux installer: nothing
  is written out of its folder.

## Scope

SlattyLauncher runs games with your user rights.

**Isolated games** (Windows games installed with Proton, by default; see the
[user guide](docs/user-guide.md#isolation-from-your-files)) run in the container umu already uses,
pressure-vessel from the Steam Linux Runtime, told to share less:

- the game sees its own folder (writable), its Wine prefix, a home folder of its own, and read-only
  umu's runtime and the setup files SlattyLauncher downloads for it;
- your home folder, your `/tmp` and other disks stay out of view (checked with `/home` and a
  network share each mounted on their own), so Wine's `z:` drive shows nothing of your files;
  so does your Steam installation (`~/.steam`, `~/.local/share/Steam`: the client's settings
  and tokens), which the container otherwise shows every game, read-only;
- your own environment cannot open it further: the variables umu and its container turn into
  shared folders (`PRESSURE_VESSEL_SHARE_HOME`, `STEAM_COMPAT_*` paths, `PROTON_LOG_DIR`,
  `WINEPREFIX` for a Linux game) are set by SlattyLauncher for every isolated launch;
- a game cannot widen it from inside between two sessions: a file in its folder named like the
  program umu is given (`sh`, `sc`, `Z:\…\game.exe`), or a link named as its working directory,
  would have umu share the whole disk or the container where the link leads; the launch is
  refused instead. The launcher's own work in the prefix and the game folder (cloud sync, the
  Galaxy service, the backup made before deleting a prefix) refuses to follow a link out of them;
- the D-Bus session and system buses stay out too: through them a program could have other
  services act for it, systemd starting a command outside the container among them, or, where
  your system's rules allow the active session, suspending the computer or mounting a drive.
  Wine sees the same network adapters and drives without them; it loses only Bluetooth devices
  reached through the system's Bluetooth service.

What isolation leaves open:

- **Display, sound and devices.** Wayland or X11, PipeWire or PulseAudio, the GPU, `/dev` (gamepads)
  and the network are shared, as games need them. The X11 socket lets any client read what other
  X clients receive (keyboard included); under Wayland, that is Xwayland's clients only. The
  PipeWire sockets include its manager socket (`pipewire-0-manager`), through which a program can
  reach every stream, the microphone among them. Comet's port (127.0.0.1:9977) is reachable, as
  are Unix sockets other programs open in the abstract namespace, which goes with the network.
- **Processes.** Other processes of yours are visible in `/proc`, though not their files or
  environment.
- **What your environment loads into the game.** The modules in `LD_PRELOAD` (MangoHud,
  GameMode) and the Vulkan layers of your user account are brought into the container, read-only,
  as pressure-vessel does for every game.

Isolation keeps a game out of your files; it is not a boundary against a program written to break
out. Games that are not isolated, and Linux games by default, see your files: a Windows game
through Wine's `z:` drive (the whole system, as in every Wine or Proton launcher), a Linux game as
you.
