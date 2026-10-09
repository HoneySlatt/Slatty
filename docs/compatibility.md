# Compatibility

This page records tests made against real GOG services and real games. A result obtained only with
simulated services never appears here.

Achievement support also depends on Comet. See its
[compatibility table](https://github.com/imLinguin/comet/blob/main/docs/wiki/Game-Compatibility.md).

Unless stated otherwise, tests ran on NixOS with niri (Wayland), umu-launcher 1.4.4 and
Proton-CachyOS.

## Services

| Date | Feature | Result | Notes |
|---|---|---|---|
| 2026-10-08 | Sign-in (browser, pasted address) | OK | Tokens stored in GNOME Keyring; none visible in the process list |
| 2026-10-08 | Session refresh | OK | Access token lasts about one hour; the refresh token is not rotated and the previous one stays valid |
| 2026-10-08 | Library | OK | full gamesdb metadata; listing from cache |
| 2026-10-09 | Install plan (`slatty install --info`) | OK | Tomb Raider, DOOM (2016), Horizon Zero Dawn, Cyberpunk 2077: builds and metadata read |
| 2026-10-09 | Manual achievement unlock | OK | Hollow Knight, `--unlock NEGLECT`; read back from GOG with its date |

## Games

| Date | Game (id) | Version | Setup | Feature | Result | Notes |
|---|---|---|---|---|---|---|
| 2026-10-08 | Tomb Raider (1724969043) | 1.0 | Imported from Heroic | Launch and session tracking | OK | Session ended after the last process exited |
| 2026-10-08 | Tomb Raider (1724969043) | 1.0 | Imported from Heroic | Achievement list | OK | 35 achievements read with a game-scoped token |
| 2026-10-08 | Tomb Raider (1724969043) | 1.0 | Imported from Heroic | Cloud, first sync | Conflict, as designed | `profile.dat` differed on both sides with no shared history |
| 2026-10-08 | Tomb Raider (1724969043) | 1.0 | Imported from Heroic | Cloud download | OK | `slatty cloud diff`: content arrives decompressed |
| 2026-10-08 | Tomb Raider (1724969043) | 1.0 | Imported from Heroic | Cloud upload (`--prefer local`) | OK | Previous cloud copy kept in backups |
| 2026-10-09 | Undertale (1456487183) | 1.08 | Installed by slatty | Galaxy install | OK | 126 MiB downloaded, 221 files verified, 1 support file skipped |
| 2026-10-09 | Undertale (1456487183) | 1.08 | Installed by slatty | First launch | OK | Prefix created, cloud checked (empty), Comet 0.3.2 started, end of session detected |
| 2026-10-09 | Undertale (1456487183) | 1.08 | Installed by slatty | `slatty verify` | OK | 221/221 files intact |

## Not tested against GOG yet

- Pausing and resuming an install.
- Repair and uninstall.
- Cloud deletions.
- An achievement earned in game and reported through Comet.
