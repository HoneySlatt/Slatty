# 0016. Images shown from files, made at the size drawn

- Status: Accepted
- Date: 2026-10-12

## Context

A library of 10,000 games has 10,000 covers, and each game page shows key art up to 3840 pixels
wide. Holding their bytes in the interface's state cost about 880 MB at 10,000 games. Iced draws an
image larger than its place by sampling four of its pixels, without smaller copies to read from,
which makes covers grainy, and enlarges a smaller one softly.

## Decision

**Images are cached as files and shown from their files** (`image::Handle::from_path`): the state
holds paths, never image bytes. **Each cover is made once at the card's width in pixels**, and key
art at the window's width rounded up to 1920, 2560 or 3840 pixels, by `scaled.rs`, with Lanczos3 (and
AMD's RCAS when enlarged), at the lowest priority, those in view first. Every place that shows a
cover or key art goes through `App::grid_cover`, `App::cover_at` or `App::shown_art`.

## Alternatives considered

- **Bytes in memory**: 880 MB at 10,000 games.
- **The originals drawn at any size**: grain in the grid, and a full-size decode for each card.

## Consequences

- The copies take disk space: key art copies at most 2 GiB, those shown longest ago going first.
- Making the copies of 10,000 covers takes about 21 s the first time, at the lowest priority
  ([testing.md](../testing.md)).
