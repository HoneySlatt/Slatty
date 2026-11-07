# 0023. A quality bar close to what is measured, its gaps closed in Stage 4

- Status: Accepted; amended by [0034](0034-what-slatty-is-to-become.md) (the wake-ups at rest fixed
  in Stage 5)
- Date: 2026-11-05

## Context

The project's method asks every project for a quality bar: the programs it is weighed against, and
budgets, each with how it is measured and where it is checked. Its defaults for desktop
applications include a library shown within 150 ms warm, nothing that wakes at rest, fuzzed parsers
of outside data, and benchmarks that fail CI when they exceed their budget.

Slatty had measured itself with Iced at 10,000 games before any budget existed
([measurements.md](../measurements.md)). It already does better than the defaults on start-up,
scrolling and memory. It misses one: a thread of zbus's pool, kept by the keyring's connection,
wakes 120 times a minute at rest. Its parsers of outside data have tests of hostile input, but no
fuzzing, which needs a nightly toolchain. `library_at_10000_games` runs by hand only.

Iced receives bug fixes only ([ADR-0020](0020-gpui-set-aside-for-slime-ui.md)).

## Decision

**Budgets close to the figures measured.** Each budget is set just above what Slatty measured on
the reference machine, not at the method's defaults: the library shown within 95 ms warm and 135 ms
cold, scrolling with a p99 between frames of 5 ms at 240 Hz, `view` within 0.5 ms, 140 MB with
every cover in. A change that costs speed or memory shows at once. The table is in
[ROADMAP.md](../ROADMAP.md#quality-bar).

**Zero wake-ups at rest, met in Stage 4.** The keyring's connection is opened only while a secret
is read or written. Until then, the miss is known debt in [PLAN.md](../PLAN.md#known-debt).

**Fuzzing in Stage 4.** The parsers of outside data (Steam's VDF and `appinfo.vdf`, GOG's
manifests, Linux installers, archives) are fuzzed with `cargo fuzz`. The nightly toolchain it needs
comes into the development shell then, with an ADR of its own.

**Benchmarks in CI in Stage 4.** `scripts/check.sh` runs `library_at_10000_games` and fails when
it exceeds limits set for CI's machines, written in the quality bar.

**What only an interface can show waits for slime-ui.** Input to visible response, animations at
the display's rate, reference images at scales 1 and 2, and accessibility are budgeted for
Stage 5, not on Iced.

## Alternatives considered

- **The method's defaults** (150 ms warm, and so on): Slatty already meets them with a wide
  margin, so a regression of half that margin would go unseen. Rejected by the owner.
- **Tolerating two wake-ups a second until zbus 6**: less work, but below the method's bar for a
  program that waits most of the time. Rejected.
- **Fuzzing and CI benchmarks before the rest of Stage 3**: they would catch regressions sooner,
  but delay the Windows release; the tests of hostile input and the benchmarks run by hand cover
  the meantime. Rejected.
- **No fuzzing, or benchmarks by hand only**: simpler, but a crash on hostile input or a slow
  regression could pass unseen. Rejected.

## Consequences

- Tight budgets leave little room for noise: a figure just above its budget is measured again
  before it counts as missed.
- Stage 4 gains three items ([ROADMAP.md](../ROADMAP.md#stage-4--smaller-work-outside-the-interface--07)).
- Start-up, scrolling, rest and memory stay measured by hand, with a probe not kept in the
  repository ([PLAN.md](../PLAN.md#known-debt)).
- The worst frame while scrolling was only checked against 16.7 ms so far; whether none goes above
  6.25 ms is measured at the next measurement of scrolling.
