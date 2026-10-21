# Compatibility

This page records tests made against real services (GOG, Steam's client) and real games. A result
obtained only with simulated services never appears here; what was not checked yet is listed at
the end.

Unless stated otherwise, tests ran on NixOS with niri (Wayland), umu-launcher 1.4.4 and
Proton-CachyOS. Results read:

- **OK**: works as intended;
- **Fails**: does not work, with the way around it;
- **Fixed**: a problem the test found, fixed since, with how the fix was checked;
- **Conflict, as designed**: the safety rule did its work.

Achievement support also depends on Comet. See its
[compatibility table](https://github.com/imLinguin/comet/blob/main/docs/wiki/Game-Compatibility.md).

## Services

| Date | Feature | Result | Notes |
|---|---|---|---|
| 2026-10-08 | Sign-in (browser, pasted address) | OK | Tokens stored in GNOME Keyring; none visible in the process list |
| 2026-10-08 | Session refresh | OK | Access token lasts about one hour; the refresh token is not rotated and the previous one stays valid |
| 2026-10-08 | Library | OK | Full gamesdb metadata; listing from cache |
| 2026-10-09 | Install plan (`slatty install --info`) | OK | Tomb Raider, DOOM (2016), Horizon Zero Dawn, Cyberpunk 2077: builds and metadata read |
| 2026-10-09 | Manual achievement unlock | OK | Hollow Knight, `--unlock NEGLECT`; read back from GOG with its date |
| 2026-10-09 | Update check (`slatty update`) | OK | Undertale reported up to date |
| 2026-10-09 | Owned DLC detection (`slatty install --info`) | OK | Cyberpunk 2077: Phantom Liberty owned and selected; free REDmod listed as not owned (not added to the account). The Witcher 3 GOTY: no separate DLC |
| 2026-10-09 | Play time | OK | Total read from GOG matches GOG Galaxy (Hollow Knight); a 4-minute Undertale session launched by slatty was accepted and counted; a 13-second one was not sent |
| 2026-10-09 | Binary patches | OK | Hollow Knight 1.5.12618 → 1.5.12620: 11 real GOG deltas (60 KB to 5.9 MB files, 111 to 390 bytes of delta) applied by oxidelta to the old files downloaded from GOG, each matching GOG's target MD5. Not yet a full update of an installed game |
| 2026-10-09 | CDN endpoint choice | OK | On a 17 MB/s connection, GOG listed fastly first and gcore second. fastly gave 0.8 to 1 MB/s on Baldur's Gate 3 and timed out on Cyberpunk 2077; gcore gave 3.6 to 13 MB/s. Measuring each endpoint and using the fastest brought 24-chunk samples from 1 to 2 MB/s to 5 to 7.4 MB/s |
| 2026-10-09 | Late chunks asked again | OK | Same connection, idle (14 to 18 MB/s raw). Most chunks take 0.6 to 1.5 s, a few 6 to 17 s; an endpoint can also stall for tens of seconds. Twelve alternated 24-chunk downloads through the install code: 15.3 MB/s on average and 14.0 MB/s at worst with copies of late chunks and writes at their offset, against 14.1 and 9.3 MB/s without. Four chunks per file instead of two was slower (13.8 against 15.8 MB/s) |
| 2026-10-09 | umu game ids | OK | umu's database answered for GOG ids: Cyberpunk 2077 umu-1091500, Baldur's Gate 3 umu-1086940; Hollow Knight is not listed (umu-0). Not yet checked in a game that needs a fix |
| 2026-10-10 | Linux install plan (`slatty install --platform linux --info`) | OK | Hollow Knight 1.5.12620: GOG's content system answers `Unsupported OS` for Linux, the offline installer is served in byte ranges (not suffix ranges); its zip starts 795 171 bytes in, after the script; 1 785 game files, 1.13 GiB to download, 4.88 GiB on disk |
| 2026-10-10 | Native Linux install and launch | OK | Hollow Knight: Silksong 1.0.30000, installed from the interface into a folder on a network share. GOG's `start.sh` asks for `/bin/bash`: on NixOS it runs through `steam-run` (played by the user) or, without it, through umu 1.4.4 without Proton in the sniper runtime (game running with Mesa OpenGL and PulseAudio, stopped cleanly from the launcher) |
| 2026-10-10 | Comet only for games with the Galaxy SDK | OK | Firewatch (ships `Galaxy64.dll`): Comet started with the game and stopped after it. Undertale (no Galaxy SDK): Comet not started, no achievement check |
| 2026-10-10 | Launch options | OK | The Legend of Heroes: Trails in the Sky: the game and its two configuration tools offered at the first Play (hidden tasks left out), the choice kept and changed in Game settings (checked by the user) |
| 2026-10-10 | Products with nothing to install | OK | The Elder Scrolls IV: Oblivion GOTY Deluxe is owned twice: as a pack (1242989820, no Galaxy build, `is_installable` false) and as the game (1458058109, 5.17 GiB build). `slatty install 1242989820 --info` says the pack has nothing to install, and its page shows Not installable with the reason (checked by the user); the game plans normally |
| 2026-10-10 | SteamGridDB covers and backgrounds | OK | The user's own API key saved from Settings → Advanced; in Edit game, a search by name, the games found, the chosen game's grids and heroes as previews, one downloaded and saved as the cover or background (checked by the user) |
| 2026-10-10 | Manual achievement clear | OK | Hollow Knight, NEGLECT: GOG answered 204 to a null `date_unlocked` and the achievement read back locked, at once and 15 s later. Hollow Knight was then launched: Comet uploaded the game's achievements 17 s in, and NEGLECT stayed locked. A clear made on 2026-10-09 had not lasted (NEGLECT was unlocked again with its first date); why is not known |
| 2026-10-11 | Large install from the interface | OK | DOOM (2016), 73.8 GB on disk, into a folder on a network share; about 27 MB/s written (shown as 26.2 MiB/s then) |
| 2026-10-11 | Post-install setup and Galaxy service with a game's Proton fixes | OK | DOOM (2016), Proton-CachyOS 11.0: its fixes add `+r_renderAPI 1` to every program run with its id, so the first setup's DirectX installer refused its command line and `reg add` failed (Galaxy service not registered). With `PROTONFIXES_DISABLE=1` for those programs, `slatty setup --force` ran clean, the service registered at the next launch (`GalaxyCommunication.exe` copied), and the game reached Comet and ran (checked by the user) |
| 2026-10-11 | Desktop notifications | OK | DOOM's download complete, the window in the background: shown by the desktop shell's notification service on niri, with the game's cover (checked by the user) |

## Steam

Checked with the user's own Steam client, installed natively. SlattyLauncher only reads its files
and hands it requests.

| Date | Feature | Result | Notes |
|---|---|---|---|
| 2026-10-11 | Library and Play through the Steam client | OK | 207 games of the account: the library with the covers the client cached, and a game started through the client (`steam://rungameid`), checked by the user |
| 2026-10-11 | Steam's files: play time, achievements | OK (read only) | Play time and last session of 178 of the account's games read from `localconfig.vdf` (133 kB, in 3 ms); achievements of 183 games read from the client's stats files, 10,962 of them, their counts matching the client's library cache for every game it counted |
| 2026-10-11 | In the interface: images, games owned on both, a game running, achievements | OK | Checked by the user: Images from Steam filled in the missing covers; games owned on both stores shown once; a game started through the client shown as Running, and its play time updated once it ended; Steam's achievements shown in the Achievements tab |
## Runners

| Date | Runner | Result | Notes |
|---|---|---|---|
| 2026-10-09 | Proton-GE (Steam Linux Runtime 4) with umu-launcher 1.4.4 | Fails | umu picks the `steamrt4-arm64` runtime on x86_64 and nothing starts. Use Proton-CachyOS or UMU-Proton |
| 2026-10-10 | Isolation: umu-launcher 1.4.4, sniper runtime, UMU-Proton 10.0-4 (`/home` and a network share each mounted on their own) | OK | A marker in the home folder stays hidden from a script in the runtime and from `Z:` under Proton, including for a program given by its path; the game's folder is writable and its own home receives what it writes there. Not isolated, the same launches see the marker. Found on the way: umu shared the whole of `/home` or of the network share through `STEAM_COMPAT_INSTALL_PATH`, and the container shares `TMPDIR`, `TMP`, `TEMP` and `TEMPDIR`. No real game yet |
| 2026-10-10 | Isolation without the D-Bus session bus: same setup | OK | With `DBUS_SESSION_BUS_ADDRESS=disabled:` the container has no session bus socket (`/run/pressure-vessel/bus`, `$XDG_RUNTIME_DIR/bus`); not isolated, it has one. Proton created a prefix and ran a program without it. An address to a missing socket stops the container from starting (`bwrap: Can't find source path`). Hollow Knight then played as before (see Games) |
| 2026-10-10 | Isolation without the D-Bus system bus: same setup | OK | With `DBUS_SYSTEM_BUS_ADDRESS=disabled:` the container has no `/run/dbus/system_bus_socket`; not isolated, it has one. Proton created a prefix without it, and `ipconfig /all` and the drive letters Wine shows were identical with and without it. Hollow Knight then played as before (see Games) |
| 2026-10-10 | Isolation against the user's environment and the game's own folder: same setup | Fixed | Checked with the container's mount table and a probe, each case with a control that saw the leak: `PRESSURE_VESSEL_SHARE_HOME=1` in the environment showed the whole home folder; `STEAM_COMPAT_INSTALL_PATH`, `STEAM_COMPAT_MOUNT_PATHS`, `STEAM_EXTRA_COMPAT_TOOLS_PATHS`, `PROTON_LOG_DIR`, `STEAM_COMPAT_TOOL_PATH`, `STEAM_COMPAT_APP_LIBRARY_PATH(S)`, `STEAM_RUNTIME_SCOUT` and, for a Linux game, `WINEPREFIX` each showed the folder they named; `~/.steam` and `~/.local/share/Steam` were shown read-only to every game; a file named `sh` in a Linux game's folder had umu share the whole of `/home`; a link in the game folder used as working directory showed where it led; a folder with `:` in its path was not shared at all (`\:` is no escape). All closed by `runner::isolation_env` and `runner::check_shared`, under the sniper runtime and UMU-Proton 10.0-4 (`cli/tests/isolation.rs`) |
| 2026-10-11 | Valve's Proton Experimental (11.0-100, Steam Linux Runtime 4) with umu-launcher 1.4.4, isolated | Fixed | Hollow Knight showed `Assertion failed! Program: C:\windows\system32\steam.exe, File: ../src-lsteamclient/steamclient_main.c, Line: 375, Expression: "!status"` (Proton's log: `steamclient_init unable to load native steamclient library`): its `lsteamclient.so` loads `$HOME/.steam/sdk64/steamclient.so`, and the game's own home has none. Proton-CachyOS and GE-Proton look there too but their `steam.exe` goes on without it. Checked in a throwaway home and prefix with `cmd /c exit`: links at `.steam/sdk64` and `.steam/sdk32` were removed by pressure-vessel before the launch; folders of those names holding a `steamclient.so` link to Steam's `linux64` and `linux32`, shared read-only, stayed, and `steam.exe` ran through (only `steamclient_init_registry Failed to connect to Steam` left, as wanted). Not yet checked with the game itself |
| 2026-10-11 | Isolation of the tools and of the user's processes: umu-launcher 1.4.4, sniper runtime, UMU-Proton 10.0-4, bubblewrap, offline | Fixed | Probes with a control each, in copies of umu's data and of the Proton build: an isolated game could write into umu's runtime, into the Proton build and into umu's own `umu-sniper` tool (all shared writable), and signal or read the command line of the user's other processes. With the runtime, the Proton build and SlattyLauncher's own runtime tool bound read-only, and `PRESSURE_VESSEL_SHARE_PID=0`, each was refused; umu's shim beside its runtimes was never reachable. Prefix creation, the Galaxy service and a session with detached Windows processes still worked. pressure-vessel then copies the runtime instead of linking it: an isolated launch took 2.3 s instead of 1.4 s on btrfs. No real game yet |
| 2026-10-11 | umu's runtime refresh, umu-launcher 1.4.4, offline with debug logging | OK | Through umu's `UMU_NO_PROTON`, umu first sent a request to `api.github.com`; through SlattyLauncher's own runtime tool, only to `repo.steampowered.com`, and `true` ran in the sniper runtime (exit 0) |

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
| 2026-10-09 | Stardew Valley (1453375253) | 1.6.15 | CLI download, killed with SIGKILL at 154 MiB | Crash, then a damaged file | OK | The job stayed `downloading`, 4 half-written temporary files, no game folder. A finished file was then overwritten in the middle (4 KiB of zeros, as after a power cut). The resumed install found and fetched it again, cleared the temporary files, and `slatty verify` found every file intact. Uninstalled afterwards. Since 2026-10-12, a resumed install keeps the files of 1 MiB or more it synced to disk on their size alone: a file damaged in place like this one is now found by `slatty verify`, not by the resume |
| 2026-10-09 | Hollow Knight (1308320804) | 1.5.12620 | Uninstalled with its prefix, installed again | Cloud saves | Fixed | The save history outlived the prefix: the new, empty save folder read as "deleted by the user", nothing was downloaded at first launch and the game started from scratch. Nothing was lost (prefix backup, cloud untouched); the save was put back from the backup. A missing or emptied save folder now downloads the cloud copies |
| 2026-10-10 | Hollow Knight (1308320804) | 1.5.12620 | Installed by slatty under `/home` (its own mount), isolated | Play isolated | OK | Played 2 min with a DualSense (played and checked by the user): graphics, sound and gamepad fine; cloud checked before, 5 saves uploaded after, so the game wrote them in its prefix. Comet started; no achievement earned, so reaching it from the container was not shown then (it was since, with Blasphemous). Its own home holds only caches (Mesa shaders, protonfixes' log, PulseAudio): shaders are compiled again at the first isolated launch |
| 2026-10-10 | Firewatch (1459256379) | — | Installed on a network share (its own mount), isolated | Play isolated | OK | Played 1 min (played and checked by the user); cloud checked before (8 uploaded) and after (4 uploaded); Comet started; no achievement earned. Its own home holds only caches, as above |
| 2026-10-10 | Hollow Knight (1308320804) | 1.5.12620 | Same, isolated without the D-Bus session bus | Play isolated | OK | Played 1 min (played and checked by the user); cloud checked, 5 saves uploaded after; Comet started. Nothing changed from the session with the bus |
| 2026-10-10 | Hollow Knight (1308320804) | 1.5.12620 | Same, isolated without either D-Bus bus | Play isolated | OK | Played 4 min (played and checked by the user); cloud checked, 2 saves uploaded after; Comet started; play time sent. Nothing changed |
| 2026-10-10 | Hollow Knight (1308320804) | 1.5.12620 | Same, isolated with `HOME` set to its own home and the user's environment kept out | Play isolated | OK | Played 1 min (played and checked by the user); cloud checked, 5 saves uploaded after; Comet started; play time sent. Nothing changed |
| 2026-10-10 | Cyberpunk 2077 (1423049311) | — | Installed by slatty under `/home`, isolated, Proton-CachyOS 11.0 | Play isolated, with umu's fixes | OK | Played 3 min, no problem seen (played and checked by the user); cloud checked, 1 save uploaded after; Comet started; play time sent. Proton's fixes ran in the container with the GOG database and umu-1091500 (log in its own home): global defaults applied (`OPENSSL_ia32cap`), no fix of its own listed for it |
| 2026-10-12 | Blasphemous | — | Installed by slatty, isolated | First launch, play isolated, achievement earned in game | OK | Played 5 min (played and checked by the user). First launch: prefix created, GOG's setup script run and the Galaxy service registered, 4 cloud saves downloaded, Comet started. An achievement earned in game reached Comet from the container: shown as unlocked during the session (desktop notification) and recorded on GOG as it ended. 2 saves uploaded after; play time sent |
| 2026-10-15 | Wuthering Waves (added by hand) | 3.0.1.0 (launcher) | Windows installer run from Add a game, isolated, DW-Proton; installed into a folder on a second SSD (drive D:) | Install, then the launcher on C: as the program | OK | The installer put Kuro's launcher on C: without asking; the launcher listed D: and downloaded the game there (133 GB). Added with the launcher as its program and isolated, the launcher found the game on D: (checked by the user). The launcher shows "Failed to retrieve space info" for C: and D: and calls them "Unknown"; it did not stop the download. The game was added from the command line (`--installed-by`) after an earlier attempt removed from the library had left its prefix without its mark; playing and an update not tried yet |
| 2026-10-15 | Arknights: Endfield (added by hand) | 1.6.0 (launcher) | Windows installer run from Add a game, isolated, DW-Proton; installed into a folder on a second SSD (drive D:) | Install, then the launcher on D: as the program | OK | The installer put its launcher (GRYPHLINK) on D:, in a folder of its own; the launcher downloaded the game on D:. Added with the launcher as its program, isolated, the game folder being the whole D: folder (filled in by hand: the interface running predated it being the default), the install ended and worked (checked by the user). Playing and an update not tried yet |
| 2026-10-19 | Bleak Faith: Forsaken | — | UMU-Proton (version not noted), SlattyLauncher started with `PROTON_ADD_CONFIG=sdlinput` | Controller (DualSense, USB) through Proton's SDL input mode | OK | The controller did not work normally in the game, and worked with the SDL mode (checked by the user); individual inputs, vibration and DualSense features were not reported one by one. The per-game switch, which sets the same mode, was not used |

## Not checked for real yet

**GOG**

- Pausing an install by hand and resuming it (resuming after a crash was checked, Stardew Valley).
- Applying an update to an installed game, and reusing chunks during one (checked during a repair).
- Installing, adding or removing DLC; switching language.
- Shared redistributables other than DOOM's DirectX (Visual C++), a game-folder dependency, and a
  redistributable that asks for a restart (exit code 3010).
- Cloud deletions.
- A game that needs the Galaxy dummy service. Its registration was checked under UMU-Proton 10.0-4
  in a throwaway prefix, and at Blasphemous's first launch, which did not show whether the game
  needed it.
- Updates and DLC of a native Linux build.
- Whether a session with Comet shows the account online, or playing, to GOG friends.
- Offline installers kept in a folder (`slatty installers`, **Download installers…**): GOG's offer
  for each system, language and DLC, the checksum XML (and an address answering 404), the names
  taken from the address a link leads to, and a pause in the middle of a file. GOG's offer of a
  Linux installer and its byte ranges were checked for the Linux install (above).
- The download queue with real downloads: **Download now**, and downloads and automatic updates
  pausing while a game runs (a Steam game too), then going on once it closes.

**Running games**

- Isolated: a game with a Proton fix of its own (Cyberpunk 2077 got only the global defaults), and
  a Linux game.
- Valve's Proton Experimental with a real game, isolated (only a throwaway prefix, above).
- Proton builds downloaded from GitHub kept up to date (Keep up to date), and shared with Steam and
  Heroic.
- Games added by hand: a Linux installer, an archive, an AppImage; a game updated by a launcher a
  Windows installer put on C: (one installed and found again, above).
- Ubuntu with AppArmor: the steps of the user guide were never tried.
- Controller compatibility: the per-game switch in a real game (the same SDL mode was checked by
  starting SlattyLauncher with `PROTON_ADD_CONFIG=sdlinput`, above), and the mode with GE-Proton
  and Proton-CachyOS.

**Steam**

- Install, Verify and Uninstall through the client, and following a download's progress.
- The Steam Web API.
- Steam installed as a Flatpak.

**Couch mode**

- A real gamepad: moves, a held direction repeating, plugging and unplugging.
- A Steam Deck: started from Steam, and by itself in Game Mode (gamescope).
- A television: the layout made for 1920×1080 filling a 4K screen, covers and key art copied at
  the size drawn. It was only drawn at 3840×2160 in a headless Wayland compositor.
