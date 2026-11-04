# 0002. GPL-3.0-or-later

- Status: Accepted
- Date: 2026-10-21

## Context

GOG publishes no API for what Slatty does: every service was understood by reverse engineering, and
much of that knowledge lives in other launchers' code. heroic-gogdl and Heroic, the most complete
sources for GOG's content system, are licensed GPL-3.0-only. Building on their code, with a note
naming the source, saves rediscovering what they already found.

## Decision

**Slatty is licensed GPL-3.0-or-later.** A part derived from GPL-3.0-only code is distributed under
GPL-3.0, with a note naming its source.

## Alternatives considered

- **A permissive licence (MIT, Apache-2.0)**: it would forbid reusing heroic-gogdl's and Heroic's
  code, and every GOG behaviour would have to be found again from captures.

## Consequences

- Every dependency's licence must be compatible with GPL-3.0-or-later; `deny.toml` lists the
  licences allowed and `cargo deny check` enforces them.
- Code from projects without a licence is never copied (AGENTS.md, "Honesty about GOG").
