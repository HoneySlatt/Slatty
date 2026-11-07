# 0025. Slatty paused after M3.3 until slime-ui runs it; then slime-ui, Windows, packages

- Status: Accepted; amends [0020](0020-gpui-set-aside-for-slime-ui.md) (the work outside the
  interface no longer goes on while slime-ui is built); amended by
  [0030](0030-ported-second-onto-uwui.md) (the pause lasts until Slatty's turn in Stage 6d; 0.3
  released once Stage 5's exit holds), [0031](0031-iceds-figures-measured-again-during-the-pause.md)
  (one milestone during the pause) and [0034](0034-what-slatty-is-to-become.md) (Slatty goes on; the
  order after Stage 5)
- Date: 2026-11-05

## Context

[ADR-0020](0020-gpui-set-aside-for-slime-ui.md) moved Slatty's interface to SlimeEngine's interface
framework, slime-ui, once SlimeEngine's Stage 6d can run an application, and kept Iced for bug
fixes until then. It let the work outside the interface go on meanwhile: Windows for the core and
the command line (Stage 3, released as 0.3), then packages and the core's smaller items (Stage 4,
0.4), then the interface on slime-ui (Stage 5, 0.5).

On 2026-11-05, with M3.2 done, the owner decided to pause Slatty after M3.3 until slime-ui is
ready, and to go on then by replacing Iced. The owner built slime-ui so that their programs share
one framework: what Slatty needs of it and serves the others is built in slime-ui, in SlimeEngine,
under the rule SlimeEngine's ADR-0170 sets (an application's need enters the framework if it is
general, serving a game or the editor too, or costing them nothing).

## Decision

**Slatty stops after M3.3.** M3.3, playing a GOG game on Windows from the command line, is the last
milestone before the pause. Then no milestone starts until SlimeEngine's Stage 6d can run an
application. CI's weekly run goes on, so that a toolchain or a dependency that breaks the build is
seen during the pause.

**Then Stage 5, Stage 3's rest, Stage 4**, in that order. Stage 5 replaces Iced wholly with
slime-ui, Windows' window included; then the rest of Stage 3 (Epic and Steam, games added by hand,
achievements, the zip, the checks on real Windows), which then has the window; then Stage 4. The
stages keep their numbers, which accepted ADRs cite; the roadmap gives their order. The releases
follow that order: 0.3 brings slime-ui, 0.4 Windows, 0.5 the packages.

**What slime-ui lacks is built in slime-ui.** A need of Slatty's that serves the framework's other
clients is built in SlimeEngine, under its rules, never worked around in Slatty; what serves Slatty
alone stays in Slatty (SlimeEngine's ADR-0170).

## Alternatives considered

- **Go on outside the interface until slime-ui is ready**, as ADR-0020 planned: Windows and the
  packages sooner, but the work done on the core before the move, while the owner's effort goes to
  slime-ui. Rejected by the owner.
- **Finish Stage 3 before the pause**, releasing Windows from the command line as 0.3: a release
  for Windows sooner, but several more sessions and real checks on Windows 10 and 11 for a launcher
  with no window there, which few people would use. Rejected.
- **Fold Stage 3's rest into Stage 5**, so that Windows' first release has its window: one release
  fewer, but a larger Stage 5, already gated on SlimeEngine. Rejected by the owner.
- **Renumber the stages in their new order**: numbers that read in order, but accepted ADRs that
  cite the old ones would have to change. Rejected.

## Consequences

- The known debt stays as [PLAN.md](../PLAN.md#known-debt) records it during the pause, CI's
  30 minutes against a budget of 20 included.
- No release of Slatty for Windows before Stage 3 ends, after Stage 5. The stage audit and the
  mutation testing that [ADR-0024](0024-hardware-tiers-and-the-budgets-of-the-whole-loop.md) set
  for the end of Stage 3 come then.
- The rest of Stage 3 is planned again when it resumes, with the window that Stage 5 brings.
