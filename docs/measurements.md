# Measurements

What the benchmarks and the measurements by hand found, against the budgets of the quality bar in
[`ROADMAP.md`](ROADMAP.md#quality-bar). One section per measurement, newest first. A section names
the machine, the build, the command, and the figures with their spread. How each is measured is in
[testing.md](testing.md#benchmarks), the fake profile of 10,000 games in
[testing.md](testing.md#the-application-at-10000-games).

Every figure here was taken with the Iced interface, before the method's budgets existed.

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
| Wake-ups at rest (2026-10-11, context switches of every thread in 60 s) | 120: one thread of the D-Bus library's pool, kept by the keyring's connection, wakes every 500 ms |
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
