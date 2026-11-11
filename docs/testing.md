# Testing

## Automated tests

```sh
nix develop
cargo test --workspace      # about 840 tests, in about 15 s; no network, no real game
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
  `crates/gui/src/tests`; the state's own pieces (motion, tasks, pictures, the theme file) in
  `crates/app`, beside them. They start from a fake state (`slatty_app::testing`) and never run
  the state's tasks.
- **Command line:** `crates/cli/tests` start real `slatty` processes against fake games and a fake
  Steam installation.

How they are written:

- Throwaway folders are named after the process id and removed at the end.
- Core tests use the in-memory CDN (`installer/tests.rs`), the in-memory cloud (`cloud/tests.rs`)
  and fake installer sources (`installer/linux/tests.rs`).
- Interface tests live in `crates/gui/src/tests/`, one module per area, with helpers in
  `tests/mod.rs` (`library_app()`, `render()`, `fake_plan()`…). When the simulator cannot read a
  widget's text (pick lists), they assert on the state. Tests of the state alone, without a view,
  live in `crates/app`, beside the module they test.
- Tests that spawn native games remove `steam-run` and umu from `PATH`: their sandboxes cannot see
  the test's throwaway folder.

Passing tests show the logic behaves as designed. They do not show that GOG accepts the requests;
that is what the [checks below](#checks-against-gog) are for.

### Interface snapshots

```sh
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui
```

This writes PNG snapshots of the tested screens. Tests take 10 October 2026 for today and UTC+2 for
the time zone, and their throwaway folder shows as `/tmp/slatty-test`, so two runs give identical
snapshots: a difference before and after a change comes from the change.

### The Windows build

The port to Windows ([windows-port.md](windows-port.md)) is built from Linux in a shell of its
own, into `target/windows`; its tests run through Wine, in a Wine prefix under `target/windows`:

```sh
nix develop .#windows -c cargo check --workspace
env -u DISPLAY -u WAYLAND_DISPLAY nix develop .#windows -c cargo test -p slatty-core -p slatty-cli
```

`scripts/check.sh` runs both, in its `windows` part. Without a display, Wine opens no window on the
desktop running the tests. Tests of what only Linux
has (Proton, umu, isolation, Wine prefixes, file modes, pipes, symbolic links to files planted by
a game) are marked `#[cfg(unix)]`, and so are the command line's test files that run Linux
programs; so, for now, are some tests of shared behaviour whose setup needs Linux
([known debt](debt.md)). Where only part of a test needs Linux, that part alone is
marked. A link to a folder is a junction on Windows (`platform::link_folder`), which any user can
make. Wine is not Windows: a test passing there checks the code, not Windows' behaviour.

The command line's `play_windows.rs` plays a fake GOG game through the real supervisor: the game is
the test program itself, copied into a fake game folder, where its ignored test `fake_game` does
what the folder's `fake-game.txt` says (exit with a code, sleep until stopped, start itself and
exit).

A test program built for Windows also runs on Windows itself, without Rust: copy it from
`target/windows/x86_64-pc-windows-gnu/debug/deps/` (or `release/deps/` with `--release`) and run it
there, for instance `slatty_core-<hash>.exe --ignored --nocapture syncing_every_small_file`.

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

# What syncing each of 2,000 files of 16 KiB costs, as installs do on Windows (ADR-0029), on the
# disk SLATTY_MEASURE_DIR names (else the temporary folder); on Windows, its test program built
# with --release, as above
SLATTY_MEASURE_DIR=<folder> cargo test --release -p slatty-core syncing_every_small_file -- --ignored --nocapture
```

**`library_at_10000_games`** builds both pages with 10,000 games whose titles are long, accented and
partly not Latin, in no particular order: how long a view takes with the lists kept (as while
scrolling), how long making the lists again takes (after any other message), and how much longer
laying out takes than with 14 games. It times the couch mode too: its home page and library, a
move of the focus (a message and a view, as each press of the d-pad), its lists made again, its
focus found again on its games after a message that forgets them, and a favorite toggled on Home.
A view must build well within a 16 ms frame. Layout times
include the test renderer's start-up, so only the difference means something; and the test lays
out every widget anew, while the application keeps what it laid out for a row in view. Scrolling is
measured in the application itself (below).

What each benchmark found is in [measurements.md](measurements.md).

## The application at 10,000 games

Start-up, rest, memory and scrolling are measured in the release build itself, on a fake profile,
by one command, `slatty-measure` (`crates/measure`):

```sh
# Start, rest, memory and video memory, in a compositor without a screen: nothing shows.
# About 45 minutes.
nix develop -c cargo run --release -p slatty-measure -- headless target/measure
# Start, scrolling and the graphics card's power, on your own display: a window opens and closes
# by itself, again and again, for about five minutes; leave the computer alone meanwhile, its
# screen kept on (a screen turned off by an idle daemon sends the window no frame).
nix develop -c cargo run --release -p slatty-measure -- desktop target/measure
```

An agent runs `desktop` only with the owner's agreement, each time
([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)).

The command:

- makes the **fake profile** in the folder when it is missing or empty, and refuses any other
  folder it did not make (`cargo run --release -p slatty-measure -- profile <folder>` makes it
  alone): a `[FAKE]` account
  signed in, a `library.json` of 10,000 `[FAKE]` games (long titles, accented, partly not Latin,
  in no particular order, one in five with a Linux build), a cached JPEG cover for each (342×482,
  about 87 KB, as GOG's) and key art (2560×1440, about 1.2 MB) for 200 of them. 1.1 GB, made in
  about 12 s, identical on every machine. The folder holds a file that marks it as made by the
  command (`slatty-measure-profile`, the layout's version), a home folder for the program
  (`home/`), the runtime folders (`runtime/`) and the runs (`runs/`); delete it to start afresh;
- builds `slatty-gui` in release with its [probe](#the-probe), into `target/probe`, so that the
  release build in `target/release` stays without it;
- starts it with nothing of yours: an environment emptied but for `PATH`, `LD_LIBRARY_PATH` and
  `LANG`; its home and XDG folders in the profile; a session bus that does not exist
  (`DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent`: no keyring, no notifications); every request
  through a closed port (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`);
- for `headless`, starts sway without a screen (`WLR_BACKENDS=headless`, from the development
  shell) with one output of 3840×2160 at 240 Hz and scale 1.5, as the high tier's display, the
  window tiled on it, and Xwayland;
- stops the program by its own process, never through the compositor;
- reads the probe's log and `/proc`, and prints the figures: each the median and range of its runs
  (10, or `--runs`), with the machine, the commit and the window's size; percentiles within a run
  are by nearest rank. It writes them, and every run's logs, under `<folder>/runs/<time>/`.

Each series begins with a start that is not counted: the first start on a profile makes a copy of
every cover at the size drawn (about 20 s), and the drivers fill their caches; `headless`'s opens
the 200 game pages too, which make their key art's copies (about 3 s each, the first time). Then:

- **`headless`**: start, warm (the run before left everything in the disk cache) and cold (the
  profile's files, the program and every library it maps dropped from the disk cache, as
  `dd if=<file> iflag=nocache count=0` does; the libraries the desktop itself maps stay, as after a
  real start of the computer); memory, resident (`VmRSS`) and video (`drm-memory-vram`, or
  `drm-total-vram`, in the process's `/proc/<pid>/fdinfo`, which amdgpu fills), 3 s after every
  cover shows, after the probe scrolled the whole grid, after it opened 200 game pages with key art;
  rest, 10 s after every cover shows, for 60 s: frames drawn, processor ticks (`/proc/<pid>/stat`)
  and wake-ups of every thread (context switches in `/proc/<pid>/task/*/status`, voluntary and not),
  under Wayland, then Xwayland, which another start not counted comes before: there the window has
  another scale, and the covers' copies another size.
- **`desktop`**, on the session's Wayland display: start, warm; scrolling, the probe moving the grid
  16 logical pixels a frame (24 at scale 1.5) for 1,200 frames: the time between frames drawn
  (median, p99, max, frames over 4.17, 6.25 and 16.7 ms) and `view`'s time; the card's power
  (`power1_average`, else `power1_input`, of the first card that reports one) read every second for
  60 s without Slatty, 60 s with it at rest, and 60 s without again.

Limits:

- Frames are timed when they are drawn, before the compositor shows them.
- `shown` includes the time the window waits for the compositor and the graphics card, which
  is most of it ([measurements.md](measurements.md)). `library` in the log, when the state holds
  the library, does not depend on it.
- The scroll moves through rows never shown before in that run, whose covers the interface reads
  as they come into view, as a person scrolling a library opened a moment ago.
- A thread that ends during the 60 s of rest takes its wake-ups with it.
- Input to visible response is not measured: Iced does not report it.

The simulator of the interface's tests rebuilds every widget each time, so it cannot show what Iced
keeps between frames: scrolling is judged in the application itself. The figures are in
[measurements.md](measurements.md).

### The probe

`slatty-gui`'s `probe` feature (`crates/gui/src/probe.rs`), off by default, so that a normal build
pays nothing for it; `scripts/check.sh` lints it, so that it keeps building. With `SLATTY_PROBE`
naming a file, it writes there one event a line, `<time> <event> [values]`, the time in
nanoseconds since the Unix epoch:

| Event | When |
|---|---|
| `start` | The probe began, before the window opens |
| `window <width> <height> <scale>` | The window's size, in its units, or its scale changed |
| `library` | The state holds the library |
| `shown` | The first frame drawn with the library in |
| `covers` | The first frame drawn with every cover made at the size drawn |
| `frame` | A frame drawn |
| `view <nanoseconds>` | A view built, and how long it took |
| `begin <step>`, `end <step>` | The step of `SLATTY_PROBE_STEP` began, or ended |
| `no-art <game id>` | A game page whose key art did not show within 5 s |

`SLATTY_PROBE_STEP` asks for one step, which begins a second after `covers`: `scroll` moves the
library's grid 16 logical pixels a frame for 1,200 frames; `grid` moves it a screen a frame, down to
its end; `pages` opens the pages of 200 games with key art, one after the other, each until two
frames are drawn after its key art's copy arrives, or 5 s passed. The probe acts through the
interface's own messages and Iced's scroll operation: no input enters the session. During a step it
keeps its lines until the step ends, so that writing them does not weigh on the frames measured.

The format names no framework: an interface on another one that writes the same log is measured by
the same command ([ADR-0031](adr/0031-iceds-figures-measured-again-during-the-pause.md)).

## Build times

How long a person waits for a build, the floor that uwui's budgets of compiling are compared with
([ADR-0036](adr/0036-slattys-bar-against-the-market.md)):

```sh
# About an hour on the high tier, the machine otherwise at rest.
nix develop -c scripts/compile-times.sh [--runs <count>] [--cold-runs <count>]
```

It times, in debug, the workspace from nothing, then a real change in the interface, in its state
and in the core, and a public function added to the core, each followed by
`cargo build -p slatty-gui`; then the release build from nothing. It builds a copy of the tracked
files, never the repository's `target`. Agents' sessions build and test on the same machine, so
the script runs in a systemd scope of its own and throws away a run during which the rest of the
machine used over 1.5 cores; its head says how. `systemctl --user stop slatty-compile-times.scope`
stops it, with everything it started.

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
# processes, nor open a device that makes input devices (/dev/uinput), while it reads the gamepads
# it read before (an input device must be readable: connect a gamepad), nor write to a terminal
# open as it starts (one script(1) makes), while it makes its own; an installer run for a game
# added by hand reads its own folder, installs on drive D: (the game's folder) and cannot write
# beside itself, and a launcher it put on C: sees the game on D:.
# A failing check writes a probe into the tools: give it copies (cp -a --reflink=auto)
# of ~/.local/share/umu, as <data>/umu, and of a Proton build. Offline: each launch is given a
# closed proxy
TMPDIR=$HOME/.cache XDG_DATA_HOME=<data> SLATTY_TEST_PROTON=<copy of a Proton folder> \
  cargo test -p slatty-cli --test isolation -- --ignored --test-threads=1
```

`TMPDIR` must point inside your home folder: umu's container cannot see `/tmp/nix-shell.*`. The
isolation tests need bubblewrap, as isolated games do, and run one at a time, since they set
variables of the test process's own environment.

### The couch mode with a gamepad

gilrs's events cannot be made outside it: the tests feed the reading loop a fake source
(`pad::Source`), and a person checks the rest with a real gamepad, on the
[fake profile](#the-application-at-10000-games):

- **One reader for the whole run**: enter and leave the couch mode three times, then
  `cat /proc/<pid>/task/*/comm | grep -cx gilrs`, with the pid of that `slatty-gui`, prints 1.
- **No move lost or replayed**: press buttons with the couch mode closed, then enter it: the
  focus does not move until a button is pressed, and moves at the first one.
- **Repeats on time**: hold the left stick down on a long shelf for 3 s, a thumb resting on the
  right stick: the focus moves about 25 times (once, then every 110 ms from 400 ms), at a steady
  pace. The same with the d-pad.

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

## Checks on Windows

M3.3's check by hand ([PLAN.md](PLAN.md#m33--play-a-gog-game)), on Windows 10 or 11, with a GOG
account: it installs a game, plays it (play time is sent to GOG) and syncs its saves. Choose a small
GOG game with cloud saves; one whose setup installs a redistributable (Visual C++, DirectX) checks
the administrator prompt too. Back up its saves first if the account has played it. Record each
result in [compatibility.md](compatibility.md), successful or not, with the Windows version.

Build on Linux, then copy `target/windows/x86_64-pc-windows-gnu/release/slatty.exe` into a folder
of the Windows machine (it needs no other file), and run it from PowerShell in that folder:

```sh
nix develop .#windows -c cargo build --release -p slatty-cli
# For step 9, also the core's test program; its path is printed (slatty_core-<hash>.exe)
nix develop .#windows -c cargo test --release -p slatty-core --no-run
```

1. **Start:** `.\slatty.exe --version`. Note whether SmartScreen warns.
2. **Sign-in:** `.\slatty.exe auth login`, signing in in the browser, then `.\slatty.exe auth
   status`. In Credential Manager (Windows Credentials), Slatty's entry is there.
3. **Library:** `.\slatty.exe library sync`, then `.\slatty.exe library list`.
4. **Install:** `.\slatty.exe install <id> --info`, then `.\slatty.exe install <id>` (into
   `%USERPROFILE%\Games\GOG` unless `--dir` says otherwise). Press Ctrl+C during the download
   and run it again: it resumes and completes, the game folder appearing only at the end.
5. **First launch:** `.\slatty.exe launch <id>`. It asks whether to run GOG's setup: answer `y`.
   Note whether Windows' administrator prompt comes to the front or waits in the taskbar; accept
   it. The game starts; play two minutes and save (a session under a minute sends
   no play time).
6. **The session:** quit the game from its own menu: the command says the session ended after the
   minutes played, and that the cloud saves were uploaded and play time sent. Launch again and
   press Ctrl+C once: the game's window is asked to close; a second Ctrl+C ends it. Put the
   computer to sleep for a few minutes during a session: they are not counted.
7. **Setup refused:** `.\slatty.exe setup <id> --force`, and refuse Windows' prompt: it says the
   setup did not finish, and the next launch asks again. `.\slatty.exe launch <id> --no-setup`
   starts the game without asking.
8. **Saves:** `.\slatty.exe cloud status <id>` names the save folder in your own folders (Saved
   Games, Documents or AppData), and nothing is left to do. Change the save from another computer
   or Galaxy, then `.\slatty.exe cloud sync <id>`: it is downloaded, the previous copy backed up.
9. **Flushing every file:** `slatty_core-<hash>.exe --ignored --nocapture syncing_every_small_file`,
   with `SLATTY_MEASURE_DIR` set to a folder of the disk games go on; the two times go to
   [measurements.md](measurements.md).
10. **Maintenance:** `.\slatty.exe verify <id>`; damage a file and `.\slatty.exe verify <id>
    --repair`; then `.\slatty.exe uninstall <id>`: files added to the game folder stay.
11. **Refusals:** `.\slatty.exe isolation <id> --on` and `.\slatty.exe proton list` say they are
    not available on Windows.

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
