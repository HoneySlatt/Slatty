# 0031. Iced's figures measured again, by a command, during the pause

- Status: Accepted; amends [0020](0020-gpui-set-aside-for-slime-ui.md) (the bar is Iced's figures
  measured again; the fake profile's generator committed) and
  [0025](0025-paused-after-m3-3-until-slime-ui.md) (one milestone runs during the pause)
- Date: 2026-11-05

## Context

Slatty's port onto uwui must run at least as well as Iced, on figures measured first
([ADR-0030](0030-ported-second-onto-uwui.md)). The figures that SlimeEngine's planning holds are
those of [measurements.md](../measurements.md#the-application-at-10000-games-2026-10-10-to-2026-10-12),
and they have three weaknesses:

- They were taken from 2026-10-10 to 2026-10-12, under niri. The owner's desktop now runs another
  compositor, Umbriel.
- They came from a temporary probe that was never committed: no command takes them again
  ([debt.md](../debt.md)).
- Some rows predate covers drawn from copies made at their size, and video memory was never
  measured.

A comparison is fair only when both frameworks are measured on the same system, at the same time,
by the same command. [ADR-0025](0025-paused-after-m3-3-until-slime-ui.md) lets no milestone start
during the pause.

## Decision

**One milestone runs during the pause, M5.0.** It commits the probe and a generator of the fake
profile of 10,000 games, turns the measurements into one command, and runs it on the owner's
machine, twice, to show that its figures repeat. They go to [measurements.md](../measurements.md),
where SlimeEngine's planning of Stage 6d can take them. The probe is a measuring tool, not
interface work: it changes nothing that a person sees.

**The same command measures the port.** At Stage 5, Iced, from a commit recorded in
measurements.md, and uwui are measured side by side, on the same machine and system.

**A window opens on the owner's desktop only with their agreement**, each time it runs. What needs
no real display runs in a headless compositor.

## Alternatives considered

- **The first milestone of Stage 5**: the pause stays whole, and the measurement uses the very system
  of the port. But Stage 6d would be planned on October's figures, taken under another compositor.
  Rejected by the owner.
- **October's figures kept**: no work, but no command takes them again, they were taken on another
  system, and the port would have nothing to be compared with side by side. Rejected.

## Consequences

- The debt "the performance figures are not reproducible by a command" is paid by M5.0, and video
  memory is measured first, as the owner decided on 2026-11-05.
- The probe stays in the repository behind a feature that is off by default, so that a normal build
  pays nothing for it. A fake profile kept outside the repository, as ADR-0020 had it, is no longer
  needed.
- The bar of the port is Iced's figures from the side-by-side run; SlimeEngine's ADR-0170 still
  quotes October's. Slatty's own quality bar does not move.
