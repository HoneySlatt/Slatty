# 0035. Slatty's emblem: its author keeps its rights; in the program at Stage 5

- Status: Accepted
- Date: 2026-11-07

## Context

Slatty's code is licensed GPL-3.0-or-later ([ADR-0002](0002-gpl-3-0-or-later.md)), which lets
anyone use, change and redistribute it. Its README said "emblem included": the emblem drawn in the
program (`crates/gui/assets/logo.svg`) and the package's icons (`packaging/icons`) were released
under the GPL with 0.1.0 and 0.2.0.

The owner drew a new emblem, `logo/slatty.svg`, now at the top of the README: a gothic S in a
cracked ring, a small flame at its foot. An emblem names a project: under the code's licence, a fork
could present itself with it. uwui's logo is kept to uwui for that reason (SlimeEngine's
[ADR-0183](https://github.com/HoneySlatt/SlimeEngine/blob/main/docs/adr/0183-uwuis-logo.md)).

Iced receives bug fixes only until Stage 5 ([ADR-0030](0030-ported-second-onto-uwui.md)).

## Decision

The owner's decisions of 2026-11-07.

**The new emblem is not under the code's licence; its author keeps its rights.** It may be shown,
unchanged, to refer to Slatty, and shipped unchanged with Slatty by those who package it. It may
not be the emblem of another project, a fork included, nor suggest that a project is Slatty or
endorsed by it. The note beside it, [`logo/README.md`](../../logo/README.md), says so, and the
README names the exception.

**The program takes the new emblem at Stage 5**, with the interface on uwui: in its window and in
the package's icons. Until then the old emblem stays there.

**The old emblem stays under the GPL**, as it was released: a licence given cannot be taken back.

## Alternatives considered

- **The GPL, as the old emblem**: nothing to change, but anyone could reuse the emblem, a fork
  included. Rejected by the owner.
- **The new emblem in the program now**: one emblem everywhere at once, but an exception to the
  pause for a change of look on Iced. Rejected by the owner.

## Consequences

- The repository is under GPL-3.0-or-later but for `logo/`, whose note governs it.
- A fork of Slatty takes an emblem of its own.
- At Stage 5, [NOTICE](../../NOTICE) gains a line on the emblem shipped in the program, and the old
  emblem leaves the repository.
