# Testing

## Automated tests

```sh
nix develop
cargo test --workspace      # about 650 tests, in about 15 s; no network, no real game
```

They check Slatty's own logic against simulated services and fake data (`[FAKE]` titles,
made-up ids, throwaway folders named after the process id):

- **Cloud sync:** every three-way case, conflicts, deletions, empty, moved or remade folders,
  network failures, concurrent changes, account switches, case differences, unsafe names, locking.
  Against an in-memory cloud with fault injection.
- **Installer:** staged publication, resume after an interruption, a replaced Linux installer,
  corrupted chunks, tampered files, unsafe manifest paths, cancellation, disk space, existing
  destinations, repair, binary patches, reuse of unchanged or moved chunks. Against an in-memory
  CDN and fake Linux installers.
- **Uninstall and removal:** only recorded files are deleted, foreign prefixes kept, a refusal never
  leaves a half-done uninstall; games added by hand keep their files.
- **Running games:** launch commands, the isolation environment and its refusals, the session
  supervisor with detached processes and `setsid` double forks (real processes), Comet's unlocks
  read from its database.
- **Proton builds:** downloads, sums, unpacking without leaving the folder, `-latest` links, updates
  and pruning, sharing with Steam.
- **Steam:** the client's files (text and binary KeyValues, `appinfo.vdf` 40 and 41, stats files,
  manifests, `localconfig.vdf`) read from fake installations, damaged, cut or too deep ones refused;
  running games found in a fake `/proc`; the Web API against a local server; nothing written into
  Steam's folders, and GOG never asked about a Steam game.
- **Interface:** the real `update` and `view` driven by Iced's simulator, one module per area under
  `crates/gui/src/tests`.
- **Command line:** `crates/cli/tests` start real `slatty` processes against fake games and a fake
  Steam installation.

Passing tests show the logic behaves as designed. They do not show that GOG accepts the requests;
that is what the [checks below](#checks-against-gog) are for.

### Interface snapshots

```sh
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui
```

This writes PNG snapshots of the tested screens. Tests take 10 October 2026 for today and UTC+2 for
the time zone, and their throwaway folder shows as `/tmp/slatty-test`, so two runs give identical
snapshots: a difference before and after a change comes from the change.

## Benchmarks

These are ignored by default; run them in release.

```sh
# Library and Achievements pages at 10,000 games
cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture

# Finding the Proton builds with 2,000 games installed by Steam
cargo test --release -p slatty-core finding_proton_builds_in_a_large_library -- --ignored --nocapture

# Checking and unpacking a Proton archive laid out as GitHub's
SLATTY_BENCH_ARCHIVE=<archive.tar.gz or .tar.xz> TMPDIR=$HOME/.cache \
  cargo test --release -p slatty-core unpacking_a_large_build -- --ignored --nocapture

# Verifying a large file listed in 1 MiB chunks, as GOG lists them
SLATTY_BENCH_FILE=<file> cargo test --release -p slatty-core verifying_a_large_file -- --ignored --nocapture

# A download that keeps arriving for minutes is not cut off (local server, over two minutes)
cargo test -p slatty-core a_long_download_is_not_cut_off -- --ignored
```

**`library_at_10000_games`** builds both pages with 10,000 games whose titles are long, accented and
partly not Latin, in no particular order: how long a view takes with the lists kept (as while
scrolling), how long making the lists again takes (after any other message), and how much longer
laying out takes than with 14 games. It times the couch mode too: its home page and library, a
move of the focus (a message and a view, as each press of the d-pad), its lists made again, its
focus found again on its games after a message that forgets them, and a favorite toggled on Home
(2026-10-13: views 5–8 µs, a move 7.5 µs, lists 3 ms, focus found again 0.45 ms, a favorite
toggled 0.55 ms). A view must build well within a 16 ms frame. Layout times
include the test renderer's start-up, so only the difference means something; and the test lays
out every widget anew, while the application keeps what it laid out for a row in view. Scrolling is
measured in the application itself (below).

**Unpacking**, 2026-10-10, a 1.1 GB tree of 3,275 files packed three ways, on NVMe:

| Archive | Unpacking | Peak memory |
|---|---|---|
| `.tar.gz`, 354 MB | 1.7 s; SHA-512 0.6 s | |
| `.tar.xz` in 44 blocks, 224 MB (`xz -T0 -6`) | 2.7 s (11.2–11.6 s on one thread) | 322 MB (15 MB on one thread) |
| `.tar.xz` in one block, 221 MB (`xz -T1 -6`) | 10.2–10.6 s | 15 MB |

**Verifying**, 2026-10-10, a 1.03 GB file: 0.37–0.44 s on four threads (1.42–1.65 s on one).

## The application at 10,000 games

Start-up, memory and scrolling are measured in the release build itself, never with a real
profile:

- an isolated profile (`XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_STATE_HOME` in a
  throwaway folder) holding a fake account in `state.db`, a `library.json` of 10,000 `[FAKE]` games,
  and a cached JPEG cover for each (342×482, about 86 KB, like GOG's);
- no keyring and no network: `DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` and
  `HTTPS_PROXY`, `HTTP_PROXY` and `ALL_PROXY` pointing at a closed port;
- a temporary probe, never committed, that logs when the library and its covers are in, the
  resident memory, how long each `view` takes, and the time between frames while it scrolls the
  grid by itself; `dd if=<file> iflag=nocache count=0` drops the covers from the disk cache for a
  cold start.

Reference, 2026-10-10 (Ryzen 7 5800X, Radeon RX 6900 XT, niri at 3840×2160, 240 Hz, scale 1.5),
before covers and key art were drawn from copies made at their size (2026-10-12): the rows on
covers have not been measured again since; the last row was, on 2026-10-12.

| | Result |
|---|---|
| Library shown | 120–130 ms after start; the same with 30 installed games on a drive answering each access in 0.2 s (standing in for a network drive waking up), where it took 24.1 s while their folders were read before |
| Every cover ready | 205–217 ms, read from the cache in one pass |
| Resident memory, covers in | 135 MB (997 MB when covers were kept in memory) |
| 200 game pages with 2560×1440 key art opened in a row | 191 MB, flat after the first 50 |
| Scrolling, 1,200 frames | p50 4.2 ms (every frame at 240 Hz), p99 8.4 ms, none above 16.7 ms |
| `view` while scrolling | p50 0.17 ms, p99 0.25 ms |
| `library_at_10000_games` | view 0.13 ms, Achievements 0.16 ms, lists made again 1.8–2.0 ms |
| Idle CPU, nothing happening | 0 ticks in 15 s |
| Wake-ups at rest (2026-10-11, context switches of every thread in 60 s) | 120: one thread of the D-Bus library's pool, kept by the keyring's connection, wakes every 500 ms |
| Cover copies at the card's size (2026-10-12) | 21 s (82 s of processor time, at the lowest priority) the first time for 10,000 covers made at 225 pixels, about 37 KB each; 16 ms to find them once made |
| Start-up, scrolling and rest again (2026-10-12) | Library shown 86–91 ms after start, 125–130 ms with the program, its libraries, the profile and the shader caches dropped from the disk cache; scrolling p50 4.15 ms, p99 4.6–4.9 ms, none above 16.7 ms; `view` p50 0.17–0.19 ms, p99 0.29–0.33 ms; 116 MB resident after scrolling; at rest, 0 ticks and 120–122 wake-ups in 60 s |

## Tests that need real tools

These are ignored by default and run on purpose. Each names its test binary: a name filter could
pick up the isolation checks, which must be given copies.

```sh
# Real Proton: a throwaway prefix, a detached Windows process, prefix creation, the Galaxy dummy
# service (SLATTY_GALAXY_COMMUNICATION is set by the development shell). SLATTY_TEST_CWD, when
# set, is the folder the detached process starts in.
TMPDIR=$HOME/.cache SLATTY_TEST_PROTON=<Proton folder> \
  cargo test -p slatty-cli --test supervisor --test prefix -- --ignored

# Real Comet with fake tokens: tokens never in its arguments nor in its log, handoff file removed,
# clean shutdown. Without a network (a namespace of its own), so Comet's requests fail at once and
# nothing reaches GOG
unshare -rn sh -c 'ip link set lo up && cargo test -p slatty-cli --test comet -- --ignored'

# Isolation: a marker in the home folder stays hidden from a Linux script in umu's runtime and from
# Wine's z: drive under Proton, and shows when the same launch is not isolated; the game can
# neither change umu's runtime, the Proton build or the runtime tool, nor reach the user's other
# processes; an installer run for a game added by hand reads its own folder, installs on drive D:
# (the game's folder) and cannot write beside itself, and a launcher it put on C: sees the game on D:.
# A failing check writes a probe into the tools: give it copies (cp -a --reflink=auto)
# of ~/.local/share/umu, as <data>/umu, and of a Proton build. Offline: each launch is given a
# closed proxy
TMPDIR=$HOME/.cache XDG_DATA_HOME=<data> SLATTY_TEST_PROTON=<copy of a Proton folder> \
  cargo test -p slatty-cli --test isolation -- --ignored --test-threads=1
```

`TMPDIR` must point inside your home folder: umu's container cannot see `/tmp/nix-shell.*`. The
isolation tests need bubblewrap, as isolated games do, and run one at a time, since they set
variables of the test process's own environment.

## Checks against GOG

These need a GOG account and write to it (cloud saves, achievements). Back up the saves of any game
you test with. Record each result in [compatibility.md](compatibility.md), successful or not.
Close Heroic or any other launcher using Comet first, and turn **Achievements in game** on in
Settings → Privacy for steps 4 and 6 (the command line has no switch for it).

1. **Sign-in:** `slatty auth login`, then `slatty auth status`. While signed in,
   `ps -eo args | grep refresh_token` must find nothing but itself.
2. **Library:** `slatty library sync`. Then, offline, `slatty library list`.
3. **Install:** `slatty install <id> --info`, then `slatty install <id>`. Press Ctrl+C during the
   download and run the command again: it must resume and complete. The game folder must not exist
   before the end.
4. **First launch:** `slatty launch <id>` of a Windows game that ships the Galaxy SDK. Expected: the
   prefix is created, the setup runs, cloud saves are checked, Comet starts, and the session ends
   only once the game has fully exited.
5. **Cloud round trip:** play and save, quit (an upload is expected), then `slatty cloud status
   <id>` (nothing left to do). Change the save from another computer, then sync: it must be
   downloaded and the previous local copy backed up.
6. **Achievement through Comet:** play until an achievement unlocks. It must be told within a
   second or two (a desktop notification while the game is in front), the end of the session must
   report it without a second notification, and it must appear on the GOG profile.
7. **Maintenance:** `slatty verify <id>`. Damage a file, then `slatty verify <id> --repair`. Finally
   `slatty uninstall <id>`: files added to the game folder must remain.

## Checks against Steam

These need the Steam client installed and signed in; Slatty only reads its files and hands
it requests. Record each result in [compatibility.md](compatibility.md).

1. **Status:** `slatty steam status` names the installation, the account, and how many of its games
   there are and are installed.
2. **Library:** turn on Settings → Steam → **Steam games**. The account's games show beside GOG's,
   with the client's covers, play time and last played; a game owned on GOG too shows once, as
   GOG's, once the GOG library was refreshed.
3. **Play:** Play on a Steam game's page. Steam starts it; the page says Running within a few
   seconds, and Play again once the game is closed, with its play time updated.
4. **Install, verify, uninstall:** Install a game, then follow the download on its page and in the
   Downloads tab under In Steam until the notification says it is done. Then Verify files in Steam
   and Uninstall in Steam: the page shows Install again once the launcher's window is in front.
5. **Achievements:** a game played on this computer lists its achievements, unlocked ones with
   their date, in its drawer and in the Achievements tab.
6. **Steam Web API** (with your own key): a game the client has not described, and the
   achievements of a game never played here, show once turned on, and go once turned off.
