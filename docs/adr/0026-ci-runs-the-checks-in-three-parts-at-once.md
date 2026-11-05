# 0026. CI runs `scripts/check.sh` in three parts at once, with a Cargo cache

- Status: Accepted; amends 0024 (how the CI's budget is reached)
- Date: 2026-11-05

## Context

[ADR-0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md) set a CI run within
20 minutes, to be reached with a cache of the Nix store and of Cargo's builds, and had
`scripts/check.sh` run the core's tests under Wine. CI then ran `scripts/check.sh` as one job, every
build cold: 30 min 44 s, before the Wine tests
([measurements.md](../measurements.md#a-ci-run-every-build-cold-2026-11-05)).

What the figures showed:

- The Nix package takes 10 minutes of the run. `buildRustPackage` builds every crate again in
  Nix's sandbox from the source, which changes with every commit: no Cargo cache reaches it, and a
  cache of the Nix store keeps only what did not change, the dependencies' sources.
- With a cache, one job would still hold the package's 10 minutes after the rest: about 17 to
  18 minutes warm, Wine included. A cache missed or evicted would bring the run back above
  35 minutes.
- GitHub keeps 10 GB of caches for a repository. Built from cold without incremental builds,
  Linux's `target/` holds 3.3 GB and Windows' 2.0 GB, before compression: both fit.
- The checks fall into three groups that share no build: Linux (Clippy, tests, documentation,
  build), Windows (`cargo check`, the core's tests under Wine, in `target/windows`), and the Nix
  package. From cold, the first takes about 16 minutes and the package 10.

## Decision

**`scripts/check.sh` has three parts**: `linux`, `windows` and `package`. With no argument it runs
all three in turn, as before: what a person or an agent runs before a commit. With names, it runs
those parts.

**CI runs each part in a job of its own, at the same time**, each `scripts/check.sh <part>` in the
same Nix shell. A run takes as long as its slowest part. One part failing does not stop the others.

**A check keeps CI complete**: before any part, the script checks that the CI's matrix names every
part it has, so that a part added to the script, or taken out of the matrix, fails every job.

**One Cargo cache per build part** (`linux`, `windows`), through GitHub's own `actions/cache`: the
downloaded crates and `target/`. Its key is the dependencies (`Cargo.lock`) and the toolchain
(`flake.lock`); in between, only Slatty's crates are built. No fallback to an older key, so that a
cache never keeps the builds of past versions. Incremental builds are off in CI, which makes the
cache smaller.

## Alternatives considered

- **A cache of the Nix store** (ADR-0024's plan): it would keep the development shells, which Nix's
  public cache already serves in about 40 seconds, and the dependencies' sources of the package,
  not its build. Dropped.
- **A cache alone, one job** (ADR-0024's plan): CI stays `scripts/check.sh` in one piece, but the
  package's 10 minutes stay, a warm run comes close to the budget, and a lost cache puts it far
  over. Rejected.
- **The parts and the cache, with the package built by crane**, which builds the dependencies
  apart so that a cache of the Nix store keeps them: the fastest package, in CI and at home, but a
  new build dependency, a larger `flake.nix` and a store cache to keep. Left for later, if the
  package becomes the run's limit.
- **The package built weekly and before releases only**: one job, about 20 minutes cold, but a
  broken package (`Cargo.lock`, a file left out of its source) would be seen days later, and CI
  would no longer run every check. Rejected.
- **A third-party cache action** (`Swatinem/rust-cache`): it trims `target/` by itself, but it is
  one more party with access to CI. GitHub's own action does the job. Rejected.
- **A fallback to an older cache** (`restore-keys`): a dependency update would start warm, but the
  cache would grow with every version kept in `target/`. Rejected.

## Consequences

- A run needs three machines instead of one; GitHub provides them freely to a public repository.
- CI is still `scripts/check.sh`, cut into its parts: the jobs and the script cannot drift apart
  without every job failing.
- A change to `Cargo.lock` or `flake.lock` means one run with cold builds, about 16 minutes. The
  caches of the old key stay until GitHub drops them, after 7 days unused or when the repository's
  10 GB are full; a pull request that changes a lock file keeps caches of its own. Several such
  changes in a week could push `main`'s caches out, for one cold run each.
- The figures are in [measurements.md](../measurements.md#ci-in-three-parts-2026-11-05).
