# 0032. GPL-3.0-or-later kept, on a framework under MIT OR Apache-2.0

- Status: Accepted
- Date: 2026-11-05

## Context

Slatty is licensed GPL-3.0-or-later ([ADR-0002](0002-gpl-3-0-or-later.md)), so that it can build on
heroic-gogdl's and Heroic's GPL-3.0 code. On 2026-11-05 the owner put uwui, the framework Slatty
moves to ([ADR-0030](0030-ported-second-onto-uwui.md)), under MIT OR Apache-2.0 (SlimeEngine's
ADR-0181), and SlimeEngine itself too (SlimeEngine's ADR-0182). A newcomer may expect Slatty to
follow.

GPL-3.0 programs can use code under either MIT or Apache-2.0. `deny.toml` already allows both.

## Decision

**Slatty stays GPL-3.0-or-later.** It uses uwui under its licence, as it uses its other permissive
dependencies.

## Alternatives considered

- **MIT OR Apache-2.0, like uwui and the engine**: one licence across the owner's work, but Slatty
  could no longer build on heroic-gogdl's and Heroic's code, the reason for ADR-0002. Rejected by
  the owner.

## Consequences

- Code can move from uwui into Slatty, keeping its notice. It does not move from Slatty into uwui
  or the engine without being relicensed by its author, and never when it derives from GPL code.
