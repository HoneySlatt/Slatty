# Measurements

What the benchmarks and the measurements by hand found, against the budgets of the quality bar in
[`ROADMAP.md`](ROADMAP.md#quality-bar). One section per measurement, newest first. A section names
the machine, the build, the command, and the figures with their spread. How each is measured is in
[testing.md](testing.md#benchmarks), the fake profile of 10,000 games in
[testing.md](testing.md#the-application-at-10000-games).

Every figure here was taken with the Iced interface. Those before 2026-11-05 predate the method's
budgets.

## Flushing every installed file on Windows (2026-11-05)

Command: `syncing_every_small_file` ([testing.md](testing.md#benchmarks)), 2,000 files of 16 KiB
written under a temporary name and renamed, without a flush, then with one each, as an install does
on Windows ([ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)). Machine: Windows 11
Pro 26H2 in a KVM virtual machine on the high tier's computer, release build of 1f2d667; one run
each.

| Disk | Not flushed | Flushed | Each flush |
|---|---|---|---|
| C:, NTFS on a VirtIO disk | 2.37 s | 4.55 s | about 1.1 ms |
| A network share (SMB 3 to the host) | 9.39 s | 17.21 s | about 3.9 ms |

Flushing every file nearly doubles the time of writing small ones, on both. A virtual disk is not a
physical one: on real hardware a flush may cost more or less.

## The application at 10,000 games, by `slatty-measure` (2026-11-05)

Command: `slatty-measure headless` and `slatty-measure desktop`, each run twice, 10 runs a series
([testing.md](testing.md#the-application-at-10000-games)). Machine: Ryzen 7 5800X, Radeon RX 6900
XT, NVMe, Linux 7.3. Build: release, of the tree later committed as 5d8583d (`slatty-gui`
unchanged). Each figure is the median of its series, and its range.

In sway 1.12 without a screen, 3840×2160 at 240 Hz, scale 1.5, the window tiled at 2556×1413
(3834×2120 pixels):

| | First series | Second series | Budget |
|---|---|---|---|
| Start, warm: library in the state / shown / every cover shown (ms) | 71 (66–82) / 530 (518–576) / 663 (651–743) | 70 (67–79) / 530 (521–545) / 665 (658–683) | Shown ≤ 95 |
| Start, cold: the same (ms) | 156 (151–184) / 636 (619–662) / 764 (746–796) | 155 (151–169) / 627 (615–656) / 753 (744–799) | Shown ≤ 135 |
| Memory, every cover shown: resident / video (MB) | 154 (154–154) / 301 (301–301) | 154 (154–154) / 301 (301–301) | ≤ 140 / ≤ 256 |
| Memory after scrolling the whole grid: resident / video (MB) | 155 (155–155) / 452 (452–452) | 155 (155–155) / 452 (452–452) | — / ≤ 256 |
| Memory after 200 game pages with key art: resident / video (MB) | 222 (219–256) / 387 (387–388) | 224 (219–226) / 389 (375–395) | ≤ 200 / ≤ 256 |
| At rest 60 s, Wayland: frames / CPU ticks / wake-ups | 0 (0–0) / 0 (0–1) / 120 (120–122) | 0 (0–0) / 0 (0–1) / 120 (120–121) | 0 / 0 / 0 of Slatty's own |
| At rest 60 s, Xwayland: the same | 0 (0–0) / 0 (0–1) / 120 (120–122) | 0 (0–0) / 0 (0–1) / 120 (120–121) | As above |

On the owner's display, under Umbriel (the owner's own Wayland compositor), 3840×2160 at 240 Hz,
scale 1.5, variable refresh rate on, the window tiled beside the others at 1267×1388 (1901×2082
pixels):

| | First series | Second series | Budget |
|---|---|---|---|
| Start, warm: library in the state / shown / every cover shown (ms) | 88 (82–98) / 461 (451–549) / 518 (504–604) | 90 (81–321) / 724 (698–923) / 785 (755–1380) | Shown ≤ 95 |
| Scrolling, 1,200 frames: time between frames, p50 / p99 / max (ms) | 5.12 (4.29–13.11) / 36.78 (19.22–111.88) / 42.60 (21.68–247.54) | 5.13 (4.86–7.50) / 51.12 (36.47–65.08) / 71.17 (38.94–138.13) | p99 ≤ 5, max ≤ 6.25 |
| Frames over 4.17 / 6.25 / 16.7 ms | 1199 (921–1199) / 272 (197–1182) / 94 (91–272) | 1199 (1192–1199) / 428 (237–789) / 118 (93–186) | 0 over 6.25 |
| `view` while scrolling, p50 / p99 (ms) | 0.630 (0.502–1.403) / 0.860 (0.594–4.931) | 0.620 (0.579–0.796) / 1.722 (0.740–3.263) | None here; p99 ≤ 0.5 in the benchmark |
| The card's power at rest (W): without / with Slatty / without | 35 (35–40) / 35 (35–36) / 35 (34–36) | 36 (35–57) / 35 (35–36) / 36 (35–46) | No more than without |

What they say:

- **Every second series' median falls within the first's range but three.** Video memory after the
  game pages, 389 MB against 387–388 MB: what the card keeps of the key art just opened varies
  from run to run (375–395 MB in the second series). On the desktop, `shown` and every cover shown,
  724 and 785 ms against 451–549 and 504–604 ms, while the library reached the state as fast in
  both: the wait for the first frame below varied between the two series.
- **Start-up misses its budget, after the library is in.** The library is in the state 70 ms after
  start in sway, 88–90 ms on Umbriel, as fast as October's library shown, but the first frame
  shows 370 to 630 ms later. Of that, about 230 ms is sway's answer to the first frame Slatty
  sends, seen in its Wayland traffic (`WAYLAND_DEBUG=1`); sway answers a plain Vulkan program
  (`vkcube`) at once. The rest is not explained, nor whether Umbriel, whose figures show the same
  gap, waits the same way. October's 86–91 ms were measured on niri. Cold, the library also takes
  155 ms to reach the state, past the 135 ms budget before any frame.
- **Scrolling misses its budget by far.** In the runs' logs, each row of covers that comes into
  view costs a frame of about 30 ms and another of about 11 ms: 91 to 272 frames over 16.7 ms in
  1,200. Why is not established: Iced 0.14 decodes a cover on a thread of its own, then copies it
  into its texture atlas on the interface's thread, which a profile would have to show. Between
  them the median frame takes 5.1 ms, not the 4.17 ms of 240 Hz, and
  some runs were slow throughout (p50 up to 13 ms, 1,182 frames over 6.25 ms); why is not known.
  October's p99 of 4.6–4.9 ms was taken by a probe that is not kept, which may have scrolled
  through rows already read.
- **`view` takes 0.62 ms at the median while scrolling**, against 0.17 ms in October; why is not
  known. The benchmark's budget (p99 ≤ 0.5 ms, `library_at_10000_games`) does not apply to it as
  such, but the gap is to be understood.
- **The card's power at rest** holds: 35 W with Slatty, as without.
- **Memory is over its budgets**: 154 MB resident with every cover in (budget 140, October 135),
  224 MB after 200 game pages (budget 200, October 191).
- **Rest** holds but for the known wake-ups, 120 a minute ([PLAN](PLAN.md#known-debt)). Counted by
  hand per thread (`/proc/<pid>/task/*/status`, 30 s at rest, in sway without a screen), they are
  all one thread, `blocking-1`, of the pool of zbus (the D-Bus library), every 500 ms. It wakes so
  even here, where no session bus exists and the keyring cannot connect.
- **Video memory**, measured for the first time: about 300 MB once every cover shows, 452 MB after
  scrolling the whole grid (the covers' textures stay on the card), under 400 MB after the game
  pages: over the budget of 256 MB the owner set once it was measured, at all three points.

## The application at 10,000 games (2026-10-10 to 2026-10-12)

Machine: Ryzen 7 5800X, Radeon RX 6900 XT, niri at 3840×2160, 240 Hz, scale 1.5. Build: release.
Method: [testing.md](testing.md#the-application-at-10000-games), with a temporary probe that is not
in the repository.

The rows from 2026-10-10 predate covers and key art drawn from copies made at their size
(2026-10-12): the rows on covers have not been measured again since; the last row was, on
2026-10-12.

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
| Wake-ups at rest (2026-10-11, context switches of every thread in 60 s) | 120: one thread of the D-Bus library's pool, thought kept by the keyring's connection (not so: see 2026-11-05), wakes every 500 ms |
| Cover copies at the card's size (2026-10-12) | 21 s (82 s of processor time, at the lowest priority) the first time for 10,000 covers made at 225 pixels, about 37 KB each; 16 ms to find them once made |
| Start-up, scrolling and rest again (2026-10-12) | Library shown 86–91 ms after start, 125–130 ms with the program, its libraries, the profile and the shader caches dropped from the disk cache; scrolling p50 4.15 ms, p99 4.6–4.9 ms, none above 16.7 ms; `view` p50 0.17–0.19 ms, p99 0.29–0.33 ms; 116 MB resident after scrolling; at rest, 0 ticks and 120–122 wake-ups in 60 s |

## The couch mode at 10,000 games (2026-10-13)

Command: `library_at_10000_games` ([testing.md](testing.md#benchmarks)). Views 5–8 µs, a move of
the focus 7.5 µs, lists 3 ms, focus found again 0.45 ms, a favorite toggled 0.55 ms.

## Verifying a large file (2026-10-10)

Command: `verifying_a_large_file`, a 1.03 GB file listed in 1 MiB chunks. 0.37–0.44 s on four
threads (1.42–1.65 s on one).

## Unpacking a Proton build (2026-10-10)

Command: `unpacking_a_large_build`, a 1.1 GB tree of 3,275 files packed three ways, on NVMe.

| Archive | Unpacking | Peak memory |
|---|---|---|
| `.tar.gz`, 354 MB | 1.7 s; SHA-512 0.6 s | |
| `.tar.xz` in 44 blocks, 224 MB (`xz -T0 -6`) | 2.7 s (11.2–11.6 s on one thread) | 322 MB (15 MB on one thread) |
| `.tar.xz` in one block, 221 MB (`xz -T1 -6`) | 10.2–10.6 s | 15 MB |

## CI in three parts (2026-11-05)

`scripts/check.sh` in its three parts, the core's tests under Wine included
([ADR-0026](adr/0026-ci-runs-the-checks-in-three-parts-at-once.md)), against the iteration budgets
of the [quality bar](ROADMAP.md#quality-bar): a CI run ≤ 20 min, `scripts/check.sh` ≤ 10 min warm,
`cargo test -p slatty-core` ≤ 1 min.

On the high tier (Ryzen 7 5800X, 16 threads, NVMe), warm, after a one-line change to the core, so
that every build and the Nix package see a change; timed by the second, one run each:

| Command | Time |
|---|---|
| `nix develop -c cargo test -p slatty-core` | 24 s (17.6 s building, 3.0 s of tests) |
| `nix develop -c scripts/check.sh`, before the parts, without Wine | 4 min 57 s, of which the Nix package 3 min 52 s |
| `nix develop -c scripts/check.sh`, the three parts | 5 min 29 s: Linux 0 min 57 s, Windows 0 min 54 s (Wine's tests 20 s), the Nix package 3 min 37 s |

The same machine from cold, as CI builds (`CARGO_INCREMENTAL=0`): the `linux` part in 5 min 51 s,
leaving 3.3 GB in `target/debug`; the `windows` part in 3 min 26 s, 2.0 GB in `target/windows`.

On GitHub's `ubuntu-24.04`, from the times in the logs:

| Run | Linux | Windows | Nix package | Whole run |
|---|---|---|---|---|
| 41960332302 (86478df), cold, no cache yet | Failed at the link step after 12 min 14 s: Clippy 4 min 35 s, tests 6 min 39 s | 12 min 06 s: `cargo check` 6 min 59 s, Wine's tests 4 min 14 s | 14 min 36 s (`nix build` 13 min 54 s) | 14 min 37 s |
| 41962586465 (bba8aaf), Linux cold, Windows warm | 18 min 55 s: Clippy 6 min 47 s, tests 9 min 56 s, build 0 min 59 s | 1 min 53 s: `cargo check` 23 s, Wine's tests 42 s | 10 min 18 s (`nix build` 9 min 28 s) | 18 min 55 s |
| The same, run again, both caches warm | 3 min 50 s: Clippy 22 s, tests 1 min 28 s, build 47 s | 2 min 04 s: `cargo check` 26 s, Wine's tests 42 s | 9 min 13 s (`nix build` 8 min 35 s) | 9 min 13 s |

Times vary with the machine GitHub gives, for the same dependencies: `nix build` took from
8 min 35 s to 13 min 54 s in these runs (9 min 20 s and 9 min 53 s in the two before, as one job),
and the Linux part from cold 12 min 14 s without its build in one run, 18 min 55 s with it in the
next. Warm, the Nix package alone sets the run's time. The caches hold 524 MiB (Windows) and
895 MiB (Linux), compressed; restoring either takes 20 to 30 seconds.

## A CI run, every build cold (2026-11-05)

Run 41952293608 on GitHub's `ubuntu-24.04` (commit 2614db7), from the times in its log: 30 min 44 s
in all, against a budget of 20 ([quality bar](ROADMAP.md#quality-bar)). The run before took
40 min 33 s.

| Step of `scripts/check.sh` | Time |
|---|---|
| The Nix shell made ready | 0 min 39 s |
| Lints (Clippy) | 5 min 21 s |
| Tests | 8 min 05 s |
| Code documentation, dependencies, links | 0 min 22 s |
| The Windows build (`cargo check`) | 5 min 00 s |
| The Nix package (`nix build`, every crate built again in Nix's sandbox) | 9 min 53 s |
| The build | 1 min 10 s |
