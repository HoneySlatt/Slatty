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
- **Private files.** SlattyLauncher's folders (settings, data, cache, state) are made readable by
  you only (0700) at every start, and its database 0600: they hold your library, play times, save
  backups and Wine prefixes.
- **Game files stay in the game folder.** Paths from GOG's manifests, installers and cloud
  listings are checked before anything is written. Symbolic links from Linux installers are
  resolved on disk once all are made, and one that leads out of the game folder, even through
  another link, is removed. No file is written through a link that leads out of the game folder.

## What leaves your computer

- No telemetry.
- GOG's services and the download servers GOG names: your library, downloads, cloud saves,
  achievements and, unless turned off in Settings → Privacy, your play sessions (as Galaxy sends
  them). Sessions played while this is off are never sent later.
- umu's public game database (`umu.openwinecomponents.org`): a game's GOG product id, once, at its
  first launch, to pick its Proton fixes. It can be turned off in Settings → Privacy; the game then
  runs without fixes.

## Scope

SlattyLauncher runs games with your user rights and does not sandbox them beyond what umu and
Proton provide. A Windows game sees your files through Wine's drives (`z:` is the whole system, as
in every Wine or Proton launcher), and a Linux game runs as you.
