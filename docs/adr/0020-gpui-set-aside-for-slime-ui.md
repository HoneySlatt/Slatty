# 0020. GPUI tried and set aside; Slatty is the pilot of SlimeEngine's interface framework

- Status: Accepted; amends [0015](0015-iced-for-the-interface.md) (Iced stays until slime-ui runs
  Slatty); amended by [0025](0025-paused-after-m3-3-until-slime-ui.md) (Slatty paused after M3.3
  until slime-ui runs it), [0030](0030-ported-second-onto-uwui.md) (Slatty ported second, onto
  the framework now named uwui) and [0031](0031-iceds-figures-measured-again-during-the-pause.md)
  (the bar is Iced's figures measured again)
- Date: 2026-11-04

## Context

In early November 2026 the owner asked for a better interface (nicer, smoother motion), at the
lowest consumption, and on Windows too, and had GPUI, the framework of the Zed editor, studied as a
replacement for Iced. A prototype and then a crate, `slatty-gpui`, drew the state of `slatty-app`
([0019](0019-the-interfaces-state-free-of-any-framework.md)) with GPUI from a pinned revision of
Zed. What was measured, on the owner's machine (4K display at 240 Hz):

- scrolling 10,000 covers matched Iced (a frame at each refresh), once the overlay of each cover was
  built only for the hovered one;
- at rest under X11 and Xwayland, GPUI drew again at each refresh, about a tenth of a core, until a
  draft fix of Zed's was applied;
- images needed a bounded cache of Slatty's own to free their place in the GPU's memory, without
  which memory stayed at 455 MB after scrolling, against 116 MB with Iced; it was still about
  293 MB against 116 at the end;
- GPUI has no published version with its Linux renderer, no software renderer, and its Windows
  shaders need Microsoft's compiler.

The same day, the owner decided that their desktop programs move to the interface framework of
SlimeEngine, their game engine, rather than to GPUI, egui or Iced: one stack for all their work, built
for Linux (Wayland first) and Windows, driven by agents through the engine's MCP server, without a
fork to maintain. SlimeEngine records it in its ADR-0170 (applications as the framework's third
client, Stage 6d) and ADR-0171 (the interface's effects, at their lowest cost).

## Decision

**GPUI is set aside.** `slatty-gpui` and the GPUI dependency were removed from `main` (commit
`e5f3431`); the crate's history stays in `main`'s history, from commit `996612f`.

**Slatty moves to SlimeEngine's interface framework (slime-ui) once it can run applications**, as
the pilot of SlimeEngine's Stage 6d: ported in this repository during that stage, with a checkpoint
where the owner uses it. **Until then, Iced draws Slatty, and receives bug fixes only**: every piece
of interface work, the Windows interface, the Steam Deck's couch mode and translations included,
waits for slime-ui and is done there, rather than on Iced first. `slatty-app` stays free of any
framework.

**The bar is Iced's figures** ([testing.md](../testing.md)): the library shown in 86 to 91 ms,
scrolling at a frame per refresh with none over 16.7 ms, no processor time at rest under Wayland
and Xwayland, 116 to 135 MB of memory. What Slatty asks of a framework was inventoried for
SlimeEngine's planning, need by need, with the code that asks it.

## Alternatives considered

- **GPUI**: no release, follows Zed's needs (macOS first), needed a fork for Linux fixes, its idle
  and memory behaviour above Iced's, and no software rendering.
- **Staying on Iced for good**: no work, but no accessibility, Iced's limits on focus and images,
  and two frameworks in the owner's work.
- **egui**: lays everything out again each frame and keeps no identity agents could name.

## Consequences

- Until the move, the work that goes on is the work outside the interface: Windows for the core and
  the command line, packages, the core's smaller items ([ROADMAP.md](../ROADMAP.md)).
- The move is a stage of the roadmap, gated on SlimeEngine's Stage 6d, which brings the Windows
  interface with it; its exit is Iced's figures matched.
- A fake profile of 10,000 games, outside the repository, is kept to measure the pilot against
  Iced.
