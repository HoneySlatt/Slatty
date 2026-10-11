# Testing

## Automated tests

```sh
nix develop
cargo test
```

The automated tests check SlattyLauncher's own logic, against simulated services:

- **Cloud sync:** every three-way case, conflicts, deletions, empty or moved folders, network
  failures, concurrent changes, account switches, case differences, path escapes, locking. They run
  against an in-memory cloud with fault injection.
- **Installer:** staged publication, resume after interruption, corrupted chunks, tampered files,
  unsafe manifest paths, cancellation, disk space, existing destinations, repair, binary patches (applied, refused when the source changed or the result is wrong), reuse of unchanged
  or moved chunks. They run against an
  in-memory CDN.
- **Uninstall:** only recorded files are deleted, foreign prefixes are kept, and a refusal never
  leaves a half-done uninstall.
- **Session supervisor:** detached processes, `setsid` double forks, stop requests. These spawn real
  processes.
- **Interface:** rendered without a window with `iced_test`. Test data is marked `[FAKE]`.
- **Steam:** the Steam client's files (text and binary KeyValues, `appinfo.vdf` 40 and 41, stats
  files, manifests, `localconfig.vdf`) read from fake installations, damaged, cut or nested too deep
  ones refused; running games found in a fake `/proc`; Steam's Web API against a local server;
  nothing written into Steam's folders, and GOG never asked about a Steam game.

Passing tests show the logic behaves as designed. They do not show that GOG accepts the requests;
that is what the checks below are for.

### Interface snapshots

```sh
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui
```

This writes PNG snapshots of the tested screens. Tests take 10 October 2026 for today and UTC+2 for
the time zone, so that dates and times ("Yesterday", "Last refreshed 09/10/2026 00:53") and the
snapshots showing them are the same whatever the day and wherever they run. Their throwaway folder,
named anew each run, shows as `/tmp/slatty-test`: two runs give identical snapshots, so any
difference between a snapshot before and after a change comes from the change.

### Library at 10,000 games

```sh
cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture
```

This times the Library and Achievements pages with 10,000 games whose titles are long,
accented and partly not Latin, in no particular order: how long each view takes to build with the
list of games kept (as while scrolling), how long making that list again takes (after any other
message), and how much longer laying it out takes than with 14 such games. A view must build well
within a 16 ms frame. Layout times include the test renderer's own start-up, so only the difference means
something. They are those of a first display: the test lays out every widget anew, while the
application keeps what it laid out for a row as long as the row stays in view. Scrolling is
checked in the application itself.

### Unpacking a Proton build

```sh
SLATTY_BENCH_ARCHIVE=<archive.tar.gz or .tar.xz> TMPDIR=$HOME/.cache \
  cargo test --release -p slatty-core unpacking_a_large_build -- --ignored --nocapture
```

This times checking (SHA-512) and unpacking an archive laid out as GitHub's: one folder holding the
build, with a `proton` file in it. Reference, 2026-10-10, with a 1.1 GB tree of 3,275 files (a Rust
toolchain from the Nix store) packed three ways, on NVMe:

| Archive | Unpacking | Peak memory |
|---|---|---|
| `.tar.gz`, 354 MB (`tar czf`) | 1.7 s (3.1–3.2 s writing 8 KiB at a time, 3.4–3.6 s with miniz_oxide too); SHA-512 0.6 s | |
| `.tar.xz` in 44 blocks, 224 MB (`xz -T0 -6`) | 2.7 s (11.2–11.6 s on one thread) | 322 MB (15 MB on one thread) |
| `.tar.xz` in one block, 221 MB (`xz -T1 -6`) | 10.2–10.6 s | 15 MB |

For comparison, `xz -dc -T0` decodes the 44-block archive in 0.8 s and `xz -dc -T1` in 6.2 s,
without writing anything.

### Verifying a large file

```sh
SLATTY_BENCH_FILE=<file> cargo test --release -p slatty-core verifying_a_large_file -- --ignored --nocapture
```

This times checking a file listed in chunks of 1 MiB, as GOG lists them, from memory: the MD5
hashing that verify, repair and updates do for every installed file. Reference, 2026-10-10, a
1.03 GB file: 0.37–0.44 s on four threads (1.42–1.65 s on one, before files of 64 MiB or more were
checked in parts).

### The application at 10,000 games

Startup, memory and scrolling are measured in the release build itself, never with a real
profile:

- an isolated profile (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME` in a
  throwaway folder) holding a fake account in `state.db`, a `library.json` of 10,000 `[FAKE]`
  games, and a cached JPEG cover for each (342×482, about 86 KB, like GOG's);
- no keyring and no network: `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` and
  `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` pointing at a closed port;
- a temporary probe, not committed, that logs when the library and its covers are in, the resident
  memory, how long each `view` takes, and the time between frames while it scrolls the grid by
  itself; `dd if=<file> iflag=nocache count=0` drops the covers from the disk cache for a cold
  start.

Reference, 2026-10-10 (Ryzen 7 5800X, Radeon RX 6900 XT, niri at 3840×2160, 240 Hz, scale 1.5):

| | Result |
|---|---|
| Library shown | 120–130 ms after start; the same with 30 installed games on a drive answering each access in 0.2 s (a read-only FUSE passthrough that waits, standing in for a network drive waking up), where it took 24.1 s while their folders were read before |
| Every cover ready | 205–217 ms, read from the cache in one pass (446–535 ms with a task and a message per cover, 698–773 ms when covers were read into memory) |
| Resident memory, covers in | 135 MB (166 MB with a task per cover, 997 MB when covers were kept in memory) |
| 200 game pages with 2560×1440 key art opened in a row | 191 MB, flat after the first 50 (254 MB and growing before) |
| Scrolling, 1,200 frames | p50 4.2 ms (every frame at 240 Hz), p99 8.4 ms, max 8.6–13 ms; none above 16.7 ms (p50 5.6 ms, p99 12.1 ms, max 20 ms before the lists were kept) |
| `view` while scrolling | p50 0.17 ms, p99 0.25 ms (2.0 ms before the lists were kept, 5.5 ms before the sort keys were kept) |
| `library_at_10000_games` | view 0.13 ms, Achievements 0.16 ms, list made again 1.8–2.0 ms (1.9 and 2.7 ms per view before the lists were kept) |
| Idle CPU, nothing happening | 0 ticks in 15 s |

### Tests that need real tools

These are ignored by default:

```sh
# Real Proton: a throwaway prefix, a detached Windows process, prefix creation,
# the Galaxy dummy service (needs SLATTY_GALAXY_COMMUNICATION, set by the dev shell)
TMPDIR=$HOME/.cache SLATTY_TEST_PROTON=<Proton dir> cargo test -- --ignored proton prefix galaxy_service

# Real Comet with fake tokens: tokens never in argv nor in its log, handoff file removed, clean
# shutdown. Without a network (a namespace of its own), so that Comet's requests fail at once
# and nothing reaches GOG
unshare -rn sh -c 'ip link set lo up && cargo test -- --ignored comet'

# Isolation: a marker in the home folder stays hidden from a Linux script in umu's runtime and from
# Wine's z: drive under Proton, and shows when the same launch is not isolated; the game can
# neither change umu's runtime, the Proton build or the runtime tool, nor reach the user's other
# processes. A failing check writes a probe into the tools: give it copies (`cp -a
# --reflink=auto`) of ~/.local/share/umu, as <data>/umu, and of a Proton build. Offline: each
# launch is given a closed proxy
TMPDIR=$HOME/.cache XDG_DATA_HOME=<data> SLATTY_TEST_PROTON=<copy of a Proton dir> \
  cargo test -p slatty-cli --test isolation -- --ignored --test-threads=1
```

`TMPDIR` must point inside your home folder: the Steam runtime container used by umu cannot see
`/tmp/nix-shell.*`. The isolation tests need bubblewrap (`bwrap`), as isolated games do, and run
one at a time, since they set variables of the test process's own environment.

## Checks against GOG

These need a GOG account and write to it (cloud saves, achievements). Back up the saves of any game
you test with. Record each result in [compatibility.md](compatibility.md), successful or not. Close
Heroic or any other launcher using Comet first.

1. **Sign-in:** `slatty auth login`, then `slatty auth status`. While logged in,
   `ps -eo args | grep refresh_token` must find nothing but itself.
2. **Library:** `slatty library sync`. Then, offline, `slatty library list`.
3. **Install:** `slatty install <id> --info`, then `slatty install <id>`. Press Ctrl+C during the
   download and run the command again: it must resume and complete. The game folder must not exist
   before the end.
4. **First launch:** `slatty launch <id>`. Expected: the prefix is created, cloud is checked, Comet
   starts, the session ends only after the game has fully exited.
5. **Cloud round trip:** play and save, quit (upload expected), then `slatty cloud status <id>`
   (nothing left to do). Change the save from another machine, then sync: it must be downloaded and
   the previous local copy backed up.
6. **Achievement through Comet:** play until an achievement unlocks. It must be told within a
   second or two (a notification from the desktop while the game is in front), the end of the
   session must report it without a second notification, and it must appear on the GOG profile.
7. **Maintenance:** `slatty verify <id>`. Damage a file, then `slatty verify <id> --repair`. Finally
   `slatty uninstall <id>`: files added to the game folder must remain.

## Checks against Steam

These need the Steam client installed and signed in; SlattyLauncher only reads its files and hands
it requests. Record each result in [compatibility.md](compatibility.md).

1. **Status:** `slatty steam status` names the installation, the account, and how many of its games
   there are and are installed.
2. **Library:** turn on Settings → Steam → Steam games. The account's games show beside GOG's, with
   the client's covers, play time and last played; a game owned on GOG too shows once, as GOG's,
   once the GOG library was refreshed.
3. **Play:** Play on a Steam game's page. Steam starts it; the page says Running within a few
   seconds, and Play again once the game is closed, with its play time updated.
4. **Install, verify, uninstall:** Install on a game not installed, then follow the download on its
   page and in the Downloads tab under In Steam until the notification says it is done. Then
   Verify files in Steam and Uninstall in Steam: the page shows Install again once the launcher's
   window is in front.
5. **Achievements:** a game played on this computer lists its achievements, unlocked ones with their
   date, in its drawer and in the Achievements tab.
6. **Steam Web API** (with your own key): a game the client has not described, and the achievements
   of a game never played here, show once turned on, and go once turned off.
