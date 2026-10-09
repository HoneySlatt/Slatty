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
| 2026-10-09 | Update check (`slatty update`) | OK | Undertale reported up to date |
| 2026-10-09 | Owned DLC detection (`slatty install --info`) | OK | Cyberpunk 2077: Phantom Liberty owned and selected; free REDmod listed as not owned (not added to the account). The Witcher 3 GOTY: no separate DLC |
| 2026-10-09 | Play time | OK | Total read from GOG matches GOG Galaxy (Hollow Knight); a 4-minute Undertale session launched by slatty was accepted and counted; a 13-second one was not sent |
| 2026-10-09 | Binary patches | OK | Hollow Knight 1.5.12618 → 1.5.12620: 11 real GOG deltas (60 KB to 5.9 MB files, 111 to 390 bytes of delta) applied by oxidelta to the old files downloaded from GOG, each matching GOG's target MD5. Not yet a full update of an installed game |

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
