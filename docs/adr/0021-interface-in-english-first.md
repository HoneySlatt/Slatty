# 0021. The interface in English first

- Status: Accepted; amended by [0034](0034-what-slatty-is-to-become.md) (French with the port)
- Date: 2026-10-09

## Context

Translating an interface while it changes every day multiplies the work of each change, and every
translation must be kept up to date.

## Decision

**The interface is in English, short and plain**, until it settles. Text shown to the user is made in
one place (`app/src/format.rs` and the views), so that translations can come later.

## Alternatives considered

- **Translations from the start**: each change of wording becomes several, while the interface is
  still being shaped.

## Consequences

- Translations are a planned stage of [ROADMAP.md](../ROADMAP.md), not started.
