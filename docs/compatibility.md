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
| 2026-10-10 | Linux install plan (`slatty install --platform linux --info`) | OK | Hollow Knight 1.5.12620: GOG's content system answers `Unsupported OS` for Linux, the offline installer is served in byte ranges (not suffix ranges); its zip starts 795 171 bytes in, after the script; 1 785 game files, 1.13 GiB to download, 4.88 GiB on disk |
| 2026-10-10 | Native Linux install and launch | OK | Hollow Knight: Silksong 1.0.30000, installed from the interface into /mnt/share/games. GOG's `start.sh` asks for `/bin/bash`: on NixOS it runs through `steam-run` (played by the user) or, without it, through umu 1.4.4 without Proton in the sniper runtime (game running with Mesa OpenGL and PulseAudio, stopped cleanly from the launcher) |
| 2026-10-10 | Comet only for games with the Galaxy SDK | OK | Firewatch (ships `Galaxy64.dll`): Comet started with the game and stopped after it. Undertale (no Galaxy SDK): Comet not started, no achievement check |
| 2026-10-09 | Manual achievement unlock | OK | Hollow Knight, `--unlock NEGLECT`; read back from GOG with its date |
| 2026-10-09 | Update check (`slatty update`) | OK | Undertale reported up to date |
| 2026-10-09 | Owned DLC detection (`slatty install --info`) | OK | Cyberpunk 2077: Phantom Liberty owned and selected; free REDmod listed as not owned (not added to the account). The Witcher 3 GOTY: no separate DLC |
| 2026-10-09 | Play time | OK | Total read from GOG matches GOG Galaxy (Hollow Knight); a 4-minute Undertale session launched by slatty was accepted and counted; a 13-second one was not sent |
| 2026-10-09 | Binary patches | OK | Hollow Knight 1.5.12618 → 1.5.12620: 11 real GOG deltas (60 KB to 5.9 MB files, 111 to 390 bytes of delta) applied by oxidelta to the old files downloaded from GOG, each matching GOG's target MD5. Not yet a full update of an installed game |
| 2026-10-09 | CDN endpoint choice | OK | On a 17 MB/s connection, GOG listed fastly first and gcore second. fastly gave 0.8 to 1 MB/s on Baldur's Gate 3 and timed out on Cyberpunk 2077; gcore gave 3.6 to 13 MB/s. Measuring each endpoint and using the fastest brought 24-chunk samples from 1 to 2 MB/s to 5 to 7.4 MB/s |
| 2026-10-09 | Late chunks asked again | OK | Same connection, idle (14 to 18 MB/s raw). Most chunks take 0.6 to 1.5 s, a few 6 to 17 s; an endpoint can also stall for tens of seconds. Twelve alternated 24-chunk downloads through the install code: 15.3 MB/s on average and 14.0 MB/s at worst with copies of late chunks and writes at their offset, against 14.1 and 9.3 MB/s without. Four chunks per file instead of two was slower (13.8 against 15.8 MB/s) |
| 2026-10-09 | umu game ids | OK | umu's database answered for GOG ids: Cyberpunk 2077 umu-1091500, Baldur's Gate 3 umu-1086940; Hollow Knight is not listed (umu-0). Not yet checked in a game that needs a fix |

## Runners

| Date | Runner | Result | Notes |
|---|---|---|---|
| 2026-10-09 | Proton-GE (Steam Linux Runtime 4) with umu-launcher 1.4.4 | Fails | umu picks the `steamrt4-arm64` runtime on x86_64 and nothing starts. Use Proton-CachyOS or UMU-Proton |

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
| 2026-10-09 | Undertale (1456487183) | 1.08 | Installed by slatty | `slatty verify --repair` after changing one byte of `data.win` | OK | Only the damaged chunk was downloaded; 50 MiB (5 of 6 chunks) copied from the damaged file; SHA-256 identical to the original afterwards |
| 2026-10-09 | Undertale (1456487183) | 1.08 | Installed by slatty | Post-install setup (`slatty setup`) | OK | Script interpreter downloaded and run under Proton; it wrote the `GOG.com\\Games\\1456487183` registry keys (build id, executable) |
| 2026-10-09 | Hollow Knight (1308320804) | 1.5.12620 | Installed by slatty | First launch with cloud saves | OK | Before the first launch Check found 9 cloud files and a sync was refused (no prefix yet); at the first launch the prefix was created, the 9 files downloaded, and the user's game was there |
| 2026-10-09 | Hollow Knight (1308320804) | 1.5.12620 | Installed by slatty | Controller (DualSense, USB) | OK | Played with the DualSense outside Steam; reported working perfectly by the user |
| 2026-10-09 | Alan Wake (1207659037) | — | Download from the interface | Cancel while downloading | OK | Stopped at about 1 GB; the hidden partial folder and the install job were deleted, nothing else touched |
| 2026-10-09 | Stardew Valley (1453375253) | 1.6.15 | CLI download, killed with SIGKILL at 154 MiB | Crash, then a damaged file | OK | The job stayed `downloading`, 4 half-written temporary files, no game folder. A finished file was then overwritten in the middle (4 KiB of zeros, as after a power cut). The resumed install found and fetched it again, cleared the temporary files, and `slatty verify` found every file intact. Uninstalled afterwards |
| 2026-10-09 | Hollow Knight (1308320804) | 1.5.12620 | Uninstalled with its prefix, installed again | Cloud saves | Fixed | The save history outlived the prefix: the new, empty save folder read as "deleted by the user", nothing was downloaded at first launch and the game started from scratch. Nothing was lost (prefix backup, cloud untouched); the save was put back from the backup. A missing or emptied save folder now downloads the cloud copies |

## Not tested against GOG yet

- Pausing and resuming an install.
- Uninstall.
- Reusing chunks during an update (checked during a repair, see above).
- Applying an update (no installed game had one yet).
- Installing, adding or removing DLC; switching language.
- Installing shared redistributables (Visual C++, DirectX) during setup.
- A game-folder dependency.
- Cloud deletions.
- An achievement earned in game and reported through Comet.
- A game that needs the Galaxy dummy service. The registration itself was checked under
  UMU-Proton 10.0-4 in a throwaway prefix.
