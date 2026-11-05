# 0024. Hardware tiers from a 2016 laptop, and budgets for the whole working loop

- Status: Accepted; extends 0023; amended by 0026
- Date: 2026-11-05

## Context

The quality bar of [ADR-0023](0023-a-quality-bar-close-to-what-is-measured.md) measured one
machine, the owner's (Ryzen 7 5800X, Radeon RX 6900 XT, 4K at 240 Hz). The project's method then
gained rules Slatty's bar lacked:

- aiming at the top never narrows who can run a program: the lowest machine supported is named,
  chosen as low as the bar allows, and measured, never assumed;
- the working loop has budgets too: a CI run within 20 minutes;
- a crash leaves a report the person can attach to an issue;
- `unsafe` code is checked under Miri, and tests by mutation testing, where they can be.

Slatty's CI took 40 minutes on 2026-11-05, every build starting cold. Running the core's tests
under Wine, by hand, had just found five Windows bugs that nothing else would have shown. A crash
left no report. About twenty `unsafe` blocks are justified by their comments alone.

## Decision

**Three hardware tiers.** The lowest is a 2016 laptop: Core i5-6200U (two cores), Intel HD 520,
4 GB, a hard disk, 1080p at 60 Hz. Its GPU is the lowest with Vulkan on both Linux and Windows,
so the interface on slime-ui needs no second renderer. The Steam Deck is the middle tier, its own
budgets set at Stage 6; the owner's machine is the high tier. On the lowest tier Slatty holds
60 Hz at 10,000 games and every feature works; its budgets are in
[ROADMAP.md](../ROADMAP.md#quality-bar). Unmeasured tiers and GPU makers are said so.

**The working loop.** A CI run within 20 minutes, `scripts/check.sh` within 10, the core's tests
within one, reached with a cache of the Nix store and Cargo's builds in CI. `scripts/check.sh`
runs the core's tests under Wine.

**Crashes leave a report** in the logs folder, with no secret in it; the command line prints its
path, and the interface tells it at the next start, once on slime-ui.

**Two budgets of a launcher's own**: a download fills the link with at most one core; a cloud
sync with nothing changed ends within 50 ms for 100 saves, as it runs before every game.

**Measured first, then set**: video memory, from the application at 10,000 games in a headless
compositor.

**Miri with the fuzzing of Stage 4**, from the same nightly toolchain; **mutation testing**
(`cargo mutants`) at each stage audit, from the end of Stage 3, starting with the code that can
lose data (cloud sync, installs, uninstalls).

## Alternatives considered

- **An older lowest tier** (a 2013 Haswell laptop): more machines, but Intel offers no Vulkan for
  its GPU on Windows and an incomplete one on Linux: the interface would need a fallback renderer
  to build and test. Rejected.
- **The Steam Deck as the lowest tier**: one known machine, but far stronger than the modest ones
  the rule asks to cover. Rejected.
- **A CI budget of 30 minutes**, with no work on CI: every fix would wait half an hour for green.
  Rejected. **Parallel jobs without a cache**: no stale cache to reason about, but more compute,
  and CI would no longer be `scripts/check.sh` as it is. Kept as the second step if the cache is
  not enough.
- **Wine tests in a CI job of their own**, or by hand: a Windows regression could be committed
  that only CI, or the next Windows milestone, would see. Rejected.
- **A crash report without the notice**, or only with slime-ui: the person would not know it
  exists, or the command line, Windows' only interface in Stage 3, would have none. Rejected.
- **A video memory ceiling of 256 MB set at once**: a figure measured by nothing. Rejected.
- **Miri now**: Miri cannot run the Windows calls M3.3 adds, so it would gain little for a
  toolchain more to keep. **Mutation testing now on the cloud**: earlier knowledge, but it delays
  M3.3. Both rejected.
- **A launch budget** (click to the game's process) and **power at rest on the lowest tier**: the
  first is dominated by Proton and the game, the second needs the real machine. Not set.

## Consequences

- None of these budgets is met or measured yet; each gap is in the PLAN's known debt.
- The figures of the lowest tier stay unmeasured until such a machine runs Slatty.
- The CI's cache comes before the next milestone's work, since every change waits on CI.
