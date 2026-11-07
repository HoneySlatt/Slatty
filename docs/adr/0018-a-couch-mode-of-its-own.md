# 0018. A couch mode of its own

- Status: Accepted; amended by [0034](0034-what-slatty-is-to-become.md) (gamepads, focus and the
  on-screen keyboard from uwui; the Guide button; sounds)
- Date: 2026-10-12

## Context

Slatty is to be used from a sofa with a gamepad, and on a Steam Deck in its Game Mode (gamescope).
The desktop pages are made for a pointer: small targets, many controls per page. Iced gives the
focus only to text fields, so a gamepad cannot move through buttons on its own.

## Decision

**A couch mode with pages of its own, made for a sofa and a Steam Deck**: large covers, one focus,
the buttons to press shown, full screen, sized from a 1920 × 1080 base. It keeps its own focus
(`Couch`), moved by gamepads read through gilrs (with SDL's mappings) and by the keyboard, and
starts its work through the same messages as the desktop window, never doing the work again
(`app/src/couch.rs`, `pad.rs`, `gui/src/ui/couch.rs`). It starts by itself with `--couch` or under
gamescope.

## Alternatives considered

- **A gamepad on the desktop window**: a focus model forced on every desktop page, and targets too
  small to read from a sofa.

## Consequences

- Two sets of views over one state; a new action in the couch mode sends the desktop's message.
- Gamepads are read only while the couch mode is on and the window is in front.
- Not yet tried on a real Steam Deck ([ROADMAP](../ROADMAP.md)).
