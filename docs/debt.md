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
  only.
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
- **Windows gaps found while testing, not fixed**: `added::check_folder` reads `HOME`, which
  Windows does not set, so the profile folder is not refused there (M3.5); `import --from-heroic`
  looks for Heroic's folder from `HOME` or `XDG_CONFIG_HOME` too, so on Windows it reads a folder
  relative to where Slatty started (it only reads, and finds nothing); games added by hand are not
  flushed to the disk on Windows, which has no `fsutil::sync_filesystem` (M3.5, as installs do in
  [ADR-0029](adr/0029-every-installed-file-flushed-on-windows.md)).
- **Windows behaviour checked under Wine only**: `platform::lowest_priority`'s background mode (its
  disk priority shows nowhere under Wine; its processor priority does), and a thread that work in
  the background starts lowers itself only if it calls it (`installer::verify_file`'s do); a
  setup program started with administrator rights, whose output is not logged, which cannot be
  stopped and whose own programs are not followed
  ([ADR-0028](adr/0028-windows-asks-before-gogs-setup-at-launch.md)): M3.3's check on Windows saw
  its prompt come to the front, not these. Flushing every installed file was measured there, in a
  virtual machine only
  ([measurements.md](measurements.md#flushing-every-installed-file-on-windows-2026-11-05)).
