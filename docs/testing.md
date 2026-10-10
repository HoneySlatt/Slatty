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

Passing tests show the logic behaves as designed. They do not show that GOG accepts the requests;
that is what the checks below are for.

### Interface snapshots

```sh
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui
```

This writes PNG snapshots of the tested screens.

### Library at 10,000 games

```sh
cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture
```

This times the Library and Achievements pages with 10,000 games whose titles are long,
accented and partly not Latin, in no particular order: how long each view takes to build, and how
much longer laying it out takes than with 14 such games. A view must build well within a 16 ms
frame. Layout times include the test renderer's own start-up, so only the difference means
something. They are those of a first display: the test lays out every widget anew, while the
application keeps what it laid out for a row as long as the row stays in view. Scrolling is
checked in the application itself.

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

Reference, 2026-10-10 (Ryzen 7 5800X, Radeon RX 6800, niri at 3840×2160, 240 Hz, scale 1.5):

| | Result |
|---|---|
| Library shown | about 105 ms after start |
| Every cover ready | 446–498 ms, cold or warm (698–773 ms when covers were read into memory) |
| Resident memory, covers in | 152 MB (997 MB when covers were kept in memory) |
| 200 game pages with 2560×1440 key art opened in a row | 191 MB, flat after the first 50 (254 MB and growing before) |
| Scrolling, 1,200 frames | p50 4.7 ms, p99 7.1 ms, max 9.6 ms; none above 16.7 ms |
| `view` while scrolling | p50 1.7 ms, p99 2.3 ms (5.5 ms before the sort keys were kept) |
| Idle CPU, nothing happening | 0 ticks in 15 s |

### Tests that need real tools

These are ignored by default:

```sh
# Real Proton: a throwaway prefix, a detached Windows process, prefix creation,
# the Galaxy dummy service (needs SLATTY_GALAXY_COMMUNICATION, set by the dev shell)
TMPDIR=$HOME/.cache SLATTY_TEST_PROTON=<Proton dir> cargo test -- --ignored proton prefix galaxy_service

# Real Comet with fake tokens: tokens never in argv, handoff file removed, clean shutdown
cargo test -- --ignored comet
```

`TMPDIR` must point inside your home folder: the Steam runtime container used by umu cannot see
`/tmp/nix-shell.*`.

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
6. **Achievement through Comet:** play until an achievement unlocks. The end of the session must
   report it, and it must appear on the GOG profile.
7. **Maintenance:** `slatty verify <id>`. Damage a file, then `slatty verify <id> --repair`. Finally
   `slatty uninstall <id>`: files added to the game folder must remain.
