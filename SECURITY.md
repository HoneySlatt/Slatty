# Security and privacy

Slatty signs in to your GOG account, writes into your game folders and saves, and starts
programs you did not write. This page says how it protects each of them, exactly what leaves your
computer, and where the protection ends.

## Reporting a vulnerability

Report security problems privately, through GitHub's private vulnerability reporting: on the
repository, open **Security**, then **Report a vulnerability**
(<https://github.com/HoneySlatt/Slatty/security/advisories/new>). Please do not open a
public issue for them.

Say what an attacker can do and under which conditions, how to reproduce it, and the version or
commit you tested.

## Supported versions

Only the latest release receives security fixes; until 1.0, older versions get none.

## Threat model in short

What is protected:

- your GOG session (the tokens that act for your account);
- your files, your saves and the launcher's own data;
- your privacy: nothing leaves the computer that this page does not list.

What is treated as hostile:

- **Everything that comes from the network**: GOG's answers, manifests, installers and cloud
  listings, Proton releases from GitHub, SteamGridDB's, Steam's and umu's answers. Paths are
  checked, sizes bounded before memory is taken (with the exceptions [below](#sizes)), and nothing
  is written outside the folder it belongs to.
- **Games.** A game is code you did not write. Windows games run isolated from your files by
  default ([below](#games-and-isolation)). Whatever a game leaves in its folder or prefix (a link, a
  pipe) is never followed by the launcher's own work there (updates, patches, setup programs, the
  Galaxy service), nor, for an isolated game, by the cloud sync.
- **Other users of the computer.** The launcher's folders and database are readable by you only,
  and Comet receives your tokens in a folder only you can open.

Out of scope: anyone with root or with your user account (they can read your files, and your
keyring once it is unlocked), a compromised system or package source, GOG itself (it controls your
account), and a game you run without isolation (it runs as you).

## Your account

- **No password.** You sign in on GOG's own page, in your browser. Slatty never asks for
  your GOG password.
- **Tokens in the keyring.** Your session is stored in the system keyring (Secret Service), entry
  `slatty` / `gog:<user id>`. Without a keyring, Slatty refuses to store it rather
  than write a file. The one exception is Comet's handoff file, below, deleted within seconds. Your
  SteamGridDB and Steam Web API keys, once given, are kept in the keyring too, and never shown
  again. Once you sign in to Epic Games, its session is kept there too, entry
  `slatty` / `epic:<account id>`.
- **Tokens stay out of logs, error messages and process arguments.** Some GOG endpoints take
  tokens in the address, so network errors are reported without it. One exception: an Epic game
  takes its sign-in code in its arguments (`-AUTH_PASSWORD=`), as Epic's own launcher gives it,
  and accepts it no other way. That code is asked for just before the game starts, works once and
  for five minutes, and gives a session of your Epic account to whatever uses it first. While it
  waits there, your other programs can read it in the game's command line, and so can other users
  of the computer unless `/proc` hides processes (`hidepid`). It never goes to a log or a file of
  Slatty's. A game asking for an ownership token gets it in a file of its prefix, readable by you
  only, deleted when the game ends.
- **Your token goes to GOG only.** It is sent to `https://` addresses on `gog.com` alone, whatever
  a GOG answer names, and a redirect never tells the next server where it came from.
- **Signing out** deletes the session from the keyring. Work still running keeps its copy in memory
  until it ends, and never writes it back.
- **Forget account** (Settings → Account, or `slatty auth logout --forget`) also deletes what
  Slatty keeps of the account: its name, play history, cloud sync history, library cache
  and Comet's achievement records. Games, saves, prefixes and save backups are never deleted. What
  the interface's own work for that account writes into its cache as it ends (a library refresh, a
  cover) is deleted again then; not when the account is forgotten from the command line while the
  interface runs.
- **Account changes.** Work started for one account never uses another's tokens: after a switch,
  its requests are refused and its late results dropped. Downloads, updates and other changes to
  installed games are the exception: they act with the account signed in when they start, so an
  install queued before a switch downloads with the new account's tokens.

### Comet

[Comet](https://github.com/imLinguin/comet) lets a game report achievements as you play
(**Achievements in game**, Settings → Privacy, off until you turn it on). Slatty starts it
only for a Windows game that ships GOG's Galaxy SDK, for the length of the session, and stops it
when the game ends, or when the launcher itself is closed or killed.

- It receives your tokens through a file readable by you only, in a folder of `$XDG_RUNTIME_DIR`
  checked to be yours alone. The file is deleted as soon as Comet listens. Slatty only
  talks to a Comet that runs as you. Your GOG user name is in its command line (Comet takes it
  nowhere else), which other users of the computer can read while it runs.
- **While it runs, any program on this computer can get your GOG access token from it.** Comet
  listens on 127.0.0.1:9977 and answers a token request from anything that connects, without
  asking who it is: your other programs, an isolated game, a Flatpak with network access, other
  users of the computer. An access token works for about an hour, and Comet keeps a fresh one as
  long as it runs. This is how Comet works, and Slatty cannot change it: that is why
  **Achievements in game** is off until you turn it on. Unlocking by hand (Settings → Advanced)
  does not need Comet.
- Comet talks to GOG for your account on its own (see the next section).
- Comet's log (`~/.local/state/slatty/logs/comet.log`) is written without the messages in which
  Comet can include your refresh token. A log written by a development
  version of Slatty before this protection can hold it: delete it, and sign out and in
  again if you ever shared it.

## What leaves your computer

No telemetry, no crash reports, no usage statistics.

**Slatty itself contacts:**

| Who | What they receive | When | Setting (default) |
|---|---|---|---|
| GOG: `auth`, `embed`, `users`, `galaxy-library`, `gamesdb`, `api`, `content-system`, `gog-cdn-fastly`, `remote-config`, `cloudstorage` and `gameplay.gog.com`, and the download and image servers GOG's answers name | What each feature needs: sign-in, your library, your avatar, downloads, cloud saves, achievements | When you use the feature | — |
| GOG, `gameplay.gog.com` | Each game of your library, one request each, for its play time | Each time the library loads | — |
| GOG: `content-system`, `gog-cdn-fastly`, `remote-config`, `auth` and `gameplay.gog.com` | For each game: whether it has cloud saves, and its achievements, through a token for the game, as Galaxy asks (about six requests a game) | When the library loads, for the games not read yet; for all on **Refresh** (Achievements tab) | — |
| GOG: `content-system.gog.com` (Windows builds), `api.gog.com` (Linux builds) | The installed games whose new builds are looked for | At start, at sign-in and every six hours | Update games automatically, Settings → Installs (on) |
| GOG, `gameplay.gog.com` | Each play session (game, start, length), as Galaxy sends them; sessions played while off are never sent | After a session | Play time on GOG, Settings → Privacy (on) |
| umu's game database, `umu.openwinecomponents.org` | A game's GOG product id, or an Epic game's app name | At its first launch, only when its Proton build's own copy of umu's database does not list it | Proton fixes online, Settings → Privacy (off) |
| GitHub: `api.github.com`, `github.com`, and the release servers it names | Which project's releases are asked for, without any account | When you list or download Proton builds; at start, at most once a day, for the builds games follow as `<project>-latest` | Proton downloads, Settings → Runners (off); Keep up to date (off) |
| SteamGridDB: `www.steamgriddb.com` and its image servers | The name you search, and your SteamGridDB API key in a header | When you search in Edit game | SteamGridDB, Settings → Advanced (off) |
| Valve, `api.steampowered.com` (Steam's Web API) | Your Steam Web API key and Steam id, in the address as the API takes them; the app ids whose achievements are asked for (how common they are is asked without the key) | As Steam's games load, and when a Steam game's achievements show while the client keeps none | Steam Web API, Settings → Steam (off) |
| Epic Games: `www.epicgames.com` (sign-in, in your browser), `account-public-service-prod03.ol.epicgames.com`, `library-service.live.use1a.on.epicgames.com`, `catalog-public-service-prod06.ol.epicgames.com` | Sign-in and your library: the items you own, then the catalog's description of each new one, with your Epic token | When you sign in, at start and on **Read again** | Signing in to Epic Games, Settings → Account (signed out) |
| Epic Games, `launcher-public-service-prod06.ol.epicgames.com` | Which game's newest build is asked for, with your Epic token | When an Epic game is installed | — |
| Epic Games: `account-public-service-prod03.ol.epicgames.com` (a sign-in code for the game), `ecommerceintegration-public-service-ecomprod02.ol.epicgames.com` (an ownership token, for a game that asks for one) | Your Epic token, and which game | Just before an Epic game starts | — |
| Epic's download servers, as its launcher service names them (such as `epicgames-download1.akamaized.net`, `download.epicgames.com`, `fastly-download.epicgames.com`) | The build's manifest and chunks, without any account | While an Epic game is installed | — |
| Epic's image servers, as the catalog names them (such as `cdn1.epicgames.com`) | The covers asked for, without any account | When an Epic game's cover is not in the cache yet | Signing in to Epic Games (signed out) |
| Valve, `shared.steamstatic.com` (Steam's image servers) | The app ids of the images asked for, without any account | When a Steam game's cover, background or logo is missing from the client's image cache, and for achievement icons; an image is kept once downloaded | Images from Steam, Settings → Steam (off) |

**The programs it starts contact:**

| Who | What they receive | When | Setting (default) |
|---|---|---|---|
| Valve, `repo.steampowered.com`, through umu-launcher | Which version of the Steam Linux Runtime is current; the runtime itself on first use and when it changes | At each launch through umu. For isolated games Slatty updates the runtime itself, at most once a day, just before a launch, and tells umu not to (`UMU_RUNTIME_UPDATE=0`); whether umu still asks Valve then has not been checked | — |
| Whatever a game's Proton fix downloads from: winetricks' sources (Microsoft and others), GitHub Pages, `download.amd.com`, Google Drive | Requests for the files the fix installs | When a game that has such a fix starts (fixes come with the Proton build) | — |
| GOG, through Comet: `auth.gog.com`; `gameplay.gog.com` (achievements, statistics, leaderboards, with your system's language); `notifications-pusher.gog.com` (a live connection signed in with your token, subscribed to your friends, presence, chat, library and notification topics); `cfg.gog.com` and the servers it names (Galaxy's "peer" component, downloaded into Comet's folder) | Whether GOG then shows you online, or playing, to your friends has not been checked | While a game that ships Galaxy's SDK runs | Achievements in game, Settings → Privacy (off) |

Every request names Slatty and its version in its `User-Agent`, except those to GOG's
cloud storage, which send Galaxy's, as heroic-gogdl does, and those to Epic's services, which send
the Epic Games Launcher's, as legendary does. Your Epic token goes only to the Epic addresses
above, never to an address an answer names (download servers included); GOG is never told of an
Epic game. Desktop notifications (Settings → Notifications, on) go to your desktop's notification
service, not to the network, but they can show game titles and achievement names on a lock screen.

## Your files

- **Private folders.** Slatty's folders (settings, data, cache, state) are made readable by
  you only (0700) at every start, and its database each time it is opened (0600): they hold your
  library, play history, save backups and Wine prefixes.
- **Backups are kept.** Before a save is replaced or deleted, locally or in the cloud, a copy is
  written to `~/.local/share/slatty/backups/` and flushed to the disk. Slatty never deletes
  backups; they hold your saves.
- **Logs** in `~/.local/state/slatty/logs/` hold the last run of each program the launcher starts
  and the two before (`.1`, `.2`; umu's runtime refresh keeps its last run only); a game's logs go
  when it is uninstalled. A game's log records whatever the game prints, which can include
  identifiers or tickets of its own (one game printed a GOG ticket there). Read a log before sharing
  it.
- **Core dumps.** If Slatty crashes and your system keeps core dumps (systemd-coredump does
  on many distributions), the dump can hold your session tokens.

## Downloads and what they write

- **Paths.** Every path from GOG's and Epic's manifests, installers and cloud listings is checked:
  none may leave the game or save folder. No file is written through a symbolic link that leads out of the
  game folder, and an update or uninstall that would delete through one deletes nothing. Temporary
  files get names never used before and never follow a link already there. These checks do not
  guard against another program changing the folders at the same moment.
- **Epic builds.** An Epic game's manifest is checked against the SHA-1 Epic's launcher service
  gives, over a request made with your token, before it is read, and every count and size in it is
  bounded before anything is allocated. Each file is checked against the manifest's SHA-1 before it
  takes its name, and the game folder appears only once every file is. Its downloads go over HTTPS,
  where the Epic Games Launcher uses plain HTTP.
- **Cloud saves.** Nothing is written or deleted through a link below a save folder. For an
  isolated game, a save folder reached through a link out of its prefix or game folder is refused.
  For a game that is not isolated, the save folder is reached as Wine sees it, links included,
  since Wine itself links a prefix's user folders to yours.
- **Games added by hand.** Their files are yours: removing one from the library never deletes them,
  except those Slatty unpacked, while they are as they were unpacked (a save or setting the
  game rewrote stays). Its prefix is deleted only when asked, its saves (and a game an installer put
  in it) copied first. An isolated game can write in its folder, so that folder cannot be one
  through which it would reach more: the whole disk, one holding your home folder, a folder of
  programs (`PATH`), the temporary folders or the session's sockets, nor one inside `/etc`, `/dev`,
  `~/.config`, `~/.local/bin`, `~/.ssh`, `~/.gnupg` or Slatty's own folders (but for the
  game's own prefix). Nor can it be another game's folder, hold one or lie inside one (an isolated
  game could rewrite what the other starts), nor hold the default installation path. A Linux
  installer or archive is unpacked, never run (a MojoSetup installer's script included), with the
  same path checks as GOG's installers. A Windows installer runs in the game's own prefix, isolated
  when asked: it then writes only in the game's folder (its drive D:) and reads its own folder,
  which passes the same checks. A launcher it put in the prefix, added as the program, runs with
  that same folder and its prefix, nothing more. GOG is never told of these games, nor is umu's database asked about them.
- **Content.** Galaxy chunks and files are checked against the MD5 sums of GOG's manifests, Linux
  installer entries against their size and CRC-32, files rebuilt from binary patches against GOG's
  sums, Proton archives against the SHA-512 sum published with their release. These catch a
  damaged download; they prove nothing against whoever can change the manifest or the release
  (GOG's servers, the project's GitHub account). Archives and installers are unpacked like a game:
  nothing is written outside their folder. A resumed download keeps the files of 1 MiB or more it
  synced to disk before naming them on their size alone; smaller ones are checked again.
- **Offline installers.** They are kept whole, never run or opened, in a folder named after the
  game under the installers folder (Settings → Installs, `GOG` in your download folder): outside
  Slatty's own folders, with the permissions your other downloads get. Each file's name is
  the last part of the address GOG's link leads to, refused unless it is a plain file name (no
  folder, no leading dot, no control character, at most 200 bytes). A file is written under a name
  of its own, its size and its game's id in it, never through a link left in its place, and takes
  its name only once checked against the MD5 sums GOG keeps for it (the whole file, and its parts
  to download again what is damaged); a file already in the folder at its size is checked against
  them too, only read, unless the download checked it itself and its inode, size, modification and
  change times have not changed since; a file GOG keeps no sums for is taken on its size (said not
  checked), and sums GOG keeps but that cannot be read stop the download before its bytes. Another
  file of the same name, or one that does not match GOG's sums, is never replaced, and
  removing a download deletes only what it left unfinished. A started download whose folder is
  gone (a drive unplugged) stops rather than start over on another disk.

### Sizes

Limits apply before memory or disk is taken: network answers (256 MiB), manifests once inflated
(512 MiB), download chunks (64 MiB), a game's own `goggame-*.info` and `start.sh` (16 MiB), each
Linux installer entry (the size its installer gives), a library listing (10,000 pages), the
dictionary of a `.tar.xz` added by hand (256 MiB), key art enlarged for display (a 4K screen's
pixels). Not yet bounded: a cloud save is held whole in memory, up to 4 GiB as it downloads and
without a limit as it uploads; the `.zip` archive or Linux installer of a game added by hand is
unpacked with each of its files in memory.

## Games and isolation

A game that is not isolated runs as you: a Windows game sees your whole system through Wine's `z:`
drive, as in every Wine or Proton launcher, and a Linux game sees what you see. Linux games are not
isolated by default, since their saves may be in your home folder.

**Isolated games** (Windows games installed with Proton, by default; see the
[user guide](docs/user-guide.md#isolation-from-your-files)) run in the container umu already uses,
pressure-vessel from the Steam Linux Runtime, told to share less, inside a bubblewrap sandbox
Slatty adds (`bwrap` must be installed). A Proton build that umu would start without that
container (one whose `toolmanifest.vdf` names no runtime, such as Proton-tkg) is refused for an
isolated game.

- **What it sees.** Its own folder (writable), its Wine prefix and a home folder of its own. umu's
  runtime, the Proton build, the tool a Linux game runs through and the setup files Slatty
  downloads for it are read-only, so a game cannot plant code that would run in other games. A
  runtime umu does not have yet is installed before the launch, outside the sandbox, never during
  it, where it would be writable.
- **What it does not see.** Your home folder, your `/tmp`, your other disks and your Steam
  installation (the client's settings and tokens), checked with `/home` and a network share each
  mounted on their own. With one of Valve's Proton builds, the game is given Steam's `linux64` and
  `linux32` folders, read-only, for the client library those builds load, and nothing else of
  Steam.
- **Processes.** It sees and signals only its own processes: your other programs are not visible to
  it, nor can it signal them.
- **Your environment cannot open it further.** The variables umu and its container turn into shared
  folders (`PRESSURE_VESSEL_SHARE_HOME`, the `STEAM_COMPAT_*` paths, `PROTON_LOG_DIR`, `WINEPREFIX`
  for a Linux game) are set by Slatty for every isolated launch.
- **A game cannot widen it between two sessions.** A file in its folder named like the program umu
  is given (`sh`, `sc`, `Z:\…\game.exe`), or a link named as its working directory, would have umu
  share the whole disk, or the container share where the link leads: the launch is refused instead.
  The daily runtime refresh and the sandbox check, which run outside the container, take nothing
  from the folder the launcher was started in. Links a game made in its own home's `.steam`, which
  would have your Steam installation shared, are taken away before each launch.
- **No D-Bus.** The session and system buses stay out: through them a program could have other
  services act for it (systemd starting a command outside the container, or, where your system
  allows it, suspending the computer or mounting a drive). From Wine's sources, Wine uses the system
  bus only to find drives and network adapters, which it showed the same without it, and for
  Bluetooth devices.

What isolation leaves open:

- **Display, sound, devices and network.** Wayland or X11, PipeWire or PulseAudio, the GPU, `/dev`
  (gamepads) and the network are shared, as games need them. The X11 socket lets any client read
  what other X clients receive, keyboard included; under Wayland, that is Xwayland's clients only.
  PipeWire's manager socket gives access to every stream, the microphone among them. Comet's port
  and the Unix sockets other programs open in the abstract namespace are reachable.
- **Your environment.** Apart from the variables above, the game inherits your environment: a token
  you keep in an environment variable is visible to it. The modules in `LD_PRELOAD` (MangoHud,
  GameMode) and your Vulkan layers are brought into the container, read-only, as pressure-vessel
  does for every game.
- **A program written to break out.** Isolation keeps games out of your files; it is not a boundary
  against code that exploits the kernel, bubblewrap, pressure-vessel or the X server.

Its cost: since the runtime is read-only, pressure-vessel copies it at each isolated launch instead
of linking it. On a file system with reflinks (btrfs, XFS) that added about a second to a launch;
on one without (ext4), the runtime's files, about 750 MB, are written at each launch (not measured
there).

## Steam

Once **Steam games** is on (Settings → Steam), Slatty reads the Steam client's files:

- **Read, as regular files of bounded size.** The account's games, their names and images, what is
  installed, achievements, and each game's play time and last session. `localconfig.vdf` also holds
  the client's download tokens and your friends' names: only play time and last session are kept
  from it. The client's settings and sign-in files (`config/config.vdf`, `local.vdf`, `ssfn*`,
  `registry.vdf`) are never read, nor its cached images through a link. Whatever the switch, two
  files are read at every start: `config/loginusers.vdf`, for the name of the account signed in to
  Steam (Settings → Steam names it), and `steamapps/libraryfolders.vdf`, for Valve's Proton builds
  in Steam's libraries.
- **Processes.** To tell a Steam game runs, the names of your processes are read, and the arguments
  of the client's `reaper` processes alone; and which processes Slatty runs under, so that
  the `reaper` it runs under when Steam started it is not taken for a game.
- **Writes into Steam's folders**, only these: **Share with Steam and Heroic** (Settings → Runners,
  off until turned on) puts links to the Proton builds Slatty downloaded in
  `compatibilitytools.d`; and before an isolated game runs with one of Valve's Proton builds, which
  live in a Steam library, Slatty does in that build's folder what its own `proton` script
  would do at launch (the empty folders, read-only files and timestamp file of a Steam update, and
  its lock file `dist.lock`), since the game cannot write there.
- **Network.** Steam's servers are asked nothing unless **Images from Steam** or **Steam Web API** is
  on. GOG is never asked about a Steam game, so it does not learn which ones you have.
- **The games themselves** are run by the Steam client, with its own access to your files, network
  and account, and cannot be isolated. Prefer a game's GOG build where you have the choice.

## What Slatty does not protect against

- Programs running as you, and anyone with root: they can act as you, read your files and, once it
  is unlocked, your keyring.
- Anything able to reach 127.0.0.1:9977 while Comet runs (see [Comet](#comet)).
- A game that is not isolated, and a program written to break out of the container.
- Steam's games and the Steam client (see [Steam](#steam)).
- GOG's servers, the GitHub account of a Proton project or umu's database turned malicious: their
  data is checked for shape, size and paths, not for intent.
- Someone watching your network: connections use TLS, but which hosts you contact, and when, can be
  seen.
