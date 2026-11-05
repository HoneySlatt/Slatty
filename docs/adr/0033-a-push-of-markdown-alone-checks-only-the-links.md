# 0033. A push of Markdown alone checks only the links

- Status: Accepted; amends [0026](0026-ci-runs-the-checks-in-three-parts-at-once.md) (a fourth
  part, and not every part in every run)
- Date: 2026-11-06

## Context

CI runs every part of `scripts/check.sh` on every push to `main`
([ADR-0026](0026-ci-runs-the-checks-in-three-parts-at-once.md)): Clippy, the tests, the build for
Windows and its tests under Wine, the Nix package, 10 to 15 minutes. Much of the work is written
in Markdown: the PLAN, the measurements, the compatibility table, ADRs. For a push that changes only
those files, one check can fail: a link or an anchor between documents (lychee). The rest builds the
same code again, and every push waits for it before it counts as done.

## Decision

**The links get a part of their own**, `docs`: `scripts/check.sh docs`, taken out of `linux`. With
no argument the script still runs every part, as before a commit.

**CI runs `docs` in a job of its own, in every run.** The three parts that build the code run only
if a file other than Markdown (`*.md`) changed since the last run on `main` that passed, read by
a first job (`scope`) from GitHub's list of runs. A pull request and the weekly run check
everything, as does a push when no passed run is found, its commit cannot be fetched, or anything
in the comparison fails.

**The script keeps CI complete**: it checks that the matrix names the parts that build the code and
that the workflow runs `scripts/check.sh docs`.

## Alternatives considered

- **No CI for a push of Markdown alone** (`paths-ignore`): the simplest, but a broken link would
  show only at the next push of code, or on Monday. Rejected by the owner.
- **Every part in every run**, as before: nothing to maintain, but 10 to 15 minutes for each change
  to the documents. Rejected.
- **A comparison with the commit before the push** (`github.event.before`): no list of runs to read,
  but a push of code whose run the next push cancels (`cancel-in-progress`), or whose run failed,
  would then never be checked if the next push changes only Markdown. Rejected.
- **A third-party action that filters by path** (`dorny/paths-filter`): one more party with access
  to CI, for a few lines of shell. Rejected.

## Consequences

- A push of Markdown alone is done in about two minutes: the Nix shell, then lychee.
- A run whose code parts are skipped passes on its links alone. It counts as passed for the next
  push, which is sound: its code is that of a run that checked everything.
- Reading the list of runs needs the `actions: read` permission, given to the `scope` job alone.
- A Markdown file that the build reads (none does yet: no `include_str!` of one) would need its own
  extension in the comparison.
