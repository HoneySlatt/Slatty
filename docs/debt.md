# Known debt

This file lists what is known to fall short: bars missed, things not checked for real and gaps.
Each item says why it stays and where it is to be paid. An item is added when the debt is made and
removed when it is paid; the current stage's plan is in [PLAN.md](PLAN.md).

- **Iced's and zbus's unmaintained crates** (`rustybuzz`, `ttf-parser`, `zbus_names`, `zvariant`,
  `zvariant_derive`) are ignored in `deny.toml`, each with its reason; they go with Iced
  (Stage 5) and with zbus 6.
- **Not checked for real**: the couch mode on a Steam Deck (Stage 6) and on a television; emulated
  games in each emulator; the rest is listed in [compatibility.md](compatibility.md).
- **The lowest hardware tier and the Steam Deck are not measured**
  ([quality bar](ROADMAP.md#quality-bar)), nor Intel's and NVIDIA's GPUs: the budgets are measured
  on the high tier alone. The lowest tier is now the owner's laptop
  ([ADR-0034](adr/0034-what-slatty-is-to-become.md)), which can be measured; Windows on it cannot.
  Its display, storage and Vulkan driver are not yet read. Video memory is read from
  `drm-memory-vram` alone, which misses what an integrated GPU borrows from main memory.
- **Wake-ups at rest**: one thread of zbus's pool (`blocking-1`) wakes every 500 ms (120 wake-ups
  a minute, against a budget of 0 in the [quality bar](ROADMAP.md#quality-bar)). It was thought
  kept by the keyring's connection, but it wakes so with no session bus at all (M5.0): what keeps
  it is to be found. To be fixed in Stage 5, whose exit is within the quality bar
  ([ADR-0034](adr/0034-what-slatty-is-to-become.md)).
- **Start-up misses its budget**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): the first
  frame shows 370–630 ms after the library is in the state, of which about 230 ms is the
  compositor's answer to it; the rest is not explained. A cold start also takes 155 ms to put the
  library in the state, past the 135 ms budget. To be understood before uwui's port is measured
  against it.
- **Scrolling stalls on each new row of covers**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): a
  frame of about 30 ms each time, for a reason not yet found, and the median frame misses 240 Hz
  too. For uwui's port.
- **Memory over its budgets**
  ([M5.0](measurements.md#the-application-at-10000-games-by-slatty-measure-2026-11-05)): resident
  memory with every cover in and after 200 game pages; video memory at all three points (about
  300 MB, 452 MB and 388 MB, against 256 MB). For uwui's port.
- **The Nix package sets the CI's pace**: CI runs `scripts/check.sh` in three parts at once, with
  a Cargo cache ([ADR-0026](adr/0026-ci-runs-the-checks-in-three-parts-at-once.md)), and a warm
  run took 9 min 13 s, within its 20 minutes, the package's 9 minutes being all of it, as no Cargo
  cache reaches its build. A run whose Linux cache is cold, after a change to `Cargo.lock` or
  `flake.lock`, took 18 min 55 s once: close to the budget. Building the package with crane is the
  step left if runs come closer ([measurements.md](measurements.md#ci-in-three-parts-2026-11-05)).
- **The download and no-change sync budgets are not measured** (set 2026-11-05): their benchmarks
  are to be written.
- **A crash leaves no report** ([quality bar](ROADMAP.md#quality-bar)): to be built in the core and
  the command line; the interface's notice waits for uwui (Stage 5), as Iced receives bug fixes
  only. Nor are hours of use counted.
- **A core dump may hold session tokens** ([SECURITY.md](../SECURITY.md)): nothing in the code
  keeps secrets out of one (no `MADV_DONTDUMP`, no `PR_SET_DUMPABLE`). To be fixed with the crash
  reports ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- **Saves are not proven against cuts**: no bench interrupts a sync at each step, and the tests
  inject a full disk only into installs. GOG's and Epic's clouds take no conditional upload, so two
  devices syncing at once can overwrite each other within a window not yet measured
  ([cloud-saves.md](cloud-saves.md)). The bench comes before Stage 8
  ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- **Slatty is not quiet while a game runs**: Comet's unlocks are polled every second
  (`play::UNLOCKS_EVERY`), the window and its covers keep their memory, and in the couch mode every
  gamepad event is still read during play, though not sent (`pad::set_away`). For Stage 5, which makes Slatty step back during play
  ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- **Isolation copies its runtime at each launch** ([SECURITY.md](../SECURITY.md)): about a second
  on btrfs, about 750 MB written on ext4, the Steam Deck's file system, not measured. To be copied
  once per version at Stage 6 ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- **The sandbox's `/dev` follows the system's**: a security key plugged in or a terminal opened
  during play is not masked, and the input devices a game may open are writable, so it can send a
  gamepad events ([SECURITY.md](../SECURITY.md#games-and-isolation)). A fixed `/dev` would lose
  gamepads plugged in during play, and read-only input devices their rumble; no fix is planned.
- **Slatty's build times are not measured**: the floor for uwui's budgets of compiling waits for
  the owner's go ([ADR-0036](adr/0036-slattys-bar-against-the-market.md)).
- **The rivals are not measured**: Steam's and Heroic's launches, installs and cost during play,
  against which the lead of three times becomes figures
  ([quality bar](ROADMAP.md#quality-bar)).
- **CI runs on GitHub's Linux machines only**: Windows is checked by building from Linux and by
  the core's and the command line's tests under Wine, in `scripts/check.sh`'s `windows` part, not
  by a Windows runner.
  Clippy for Windows is not run, as the `windows` shell lacks it.
- **Links to files are tested on Linux only**: making a symbolic link on Windows needs a right
  users lack, so the tests of a link to a file planted by a game
  (`installer::tests::downloading_cannot_truncate_a_temporary_symlink_target`) are `#[cfg(unix)]`.
  Links to folders are tested on both, as junctions on Windows (`platform::link_folder`).
- **Some tests of shared behaviour run on Linux only**, as their setup uses file modes or links:
  `added::a_folder_too_wide_or_a_program_out_of_its_folder_is_refused`,
  `installer::tests::a_restart_that_cannot_delete_the_download_keeps_it`,
  `maintenance::a_game_whose_emptied_folder_cannot_be_removed_is_uninstalled`,
  `epic::download::an_unreadable_file_is_repaired`,
  `steam::local::what_steam_wrote_of_a_download_is_measured_in_its_folder`. To be made for Windows
  (a file held open, a lock) with the milestone of each area.
- **`import --from-heroic` cannot import a Windows game on Windows**: it takes every Windows
  game for one run through Wine or Proton, and fails on a game Heroic runs as it is. Not tried
  on Windows; for M3.5.
- **Windows behaviour checked under Wine only**: `platform::lowest_priority`'s background mode (its
  disk priority shows nowhere under Wine; its processor priority does), and a thread that work in
  the background starts lowers itself only if it calls it (`installer::verify_file`'s do); a
  setup program started with administrator rights, whose output is not logged, which cannot be
  stopped and whose own programs are not followed
  ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)): M3.3's check on Windows saw
  its prompt come to the front, not these. Flushing every installed file was measured there, in a
  virtual machine only
  ([measurements.md](measurements.md#flushing-every-installed-file-on-windows-2026-11-05)).
