# 0017. Grids and long lists build only the rows in view

- Status: Accepted
- Date: 2026-10-12

## Context

The library must stay fluid at 10,000 games, within a 16 ms frame. Iced builds and lays out every
widget of a view after each message, scroll steps included; a grid of 10,000 cards built whole
misses frames.

## Decision

**The cover grid, the Achievements grid and the list beside the library build only the rows in view,
plus one of margin** (`library::GridWindow`), **with one child per row of the whole grid**
(`ui::widgets::grid_rows`), a space of the same height out of view: a row still in view after a
scroll stays the same child, and Iced keeps what it laid out for it. **The lists each tab shows are
kept** (`library::Lists`) until a message that can change them (`Message::keeps_lists`), and sort
keys are made once per title at start.

## Alternatives considered

- **Pagination**: no scrolling through the whole library.
- **Fewer, larger pages of covers**: still thousands of widgets.

## Consequences

- Every new grid or long list is virtualized the same way, and measured with
  `library_at_10000_games`, never with a small library (AGENTS.md, "Performance").
- A new message that comes often and changes no list belongs in `Message::keeps_lists`.
