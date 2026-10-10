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
something.

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
