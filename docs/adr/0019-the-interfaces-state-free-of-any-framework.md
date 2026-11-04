# 0019. The interface's state free of any framework

- Status: Accepted; amends [0015](0015-iced-for-the-interface.md) (the state leaves the Iced crate)
- Date: 2026-11-04

## Context

The interface's state (`App`, the `Message` enum, `update`, each feature's background work) lived
in the Iced crate and named Iced's types: its `Task`, `Subscription`, scroll viewport, image handle,
colours and keys. Moving to another framework would then have meant rewriting the whole state, not
only the views. A move was being studied at the time (GPUI, then the framework of SlimeEngine).

## Decision

**The state is a crate of its own, `slatty-app` (`crates/app`), that names no interface
framework**: its tasks (`task::Task`, a stream of outputs and of requests to the window, `Effect`),
what it listens to (`listen::Listen`), sizes and scrolls (`geometry`), pictures (`picture`), the look
(`look`, colours of its own) and keys are its own types. `slatty-gui` wraps the state, draws it with
Iced and turns its types into Iced's (`run`, `ui::handle`, `theme`). `slatty-app` must build without
Iced.

## Alternatives considered

- **Keeping the state in the Iced crate**: no work now, and a whole rewrite at any change of
  framework.
- **Rewriting the state for each framework**: loses the state's tests and its rules, written bug by
  bug.

## Consequences

- A new framework draws the same state with its own adapter and views; this is what makes the move
  to slime-ui simple ([0020](0020-gpui-set-aside-for-slime-ui.md)).
- The interface's tests start from a fake state shared through `slatty_app::testing` (feature
  `test-support`).
- About 290 lines of adapters in `gui/src/run.rs`.
