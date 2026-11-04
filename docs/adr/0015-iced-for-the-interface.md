# 0015. Iced for the interface

- Status: Accepted; amended by [0019](0019-the-interfaces-state-free-of-any-framework.md) (the
  state leaves the Iced crate) and [0020](0020-gpui-set-aside-for-slime-ui.md) (Iced stays until
  slime-ui runs Slatty)
- Date: 2026-10-09

## Context

Slatty needed a native interface in Rust, GPU-drawn, fluid with large libraries, on Wayland and X11.
On 2026-10-09 the candidates were Iced, GPUI (the framework of the Zed editor), egui, and the GTK or
Qt bindings.

## Decision

**The interface is drawn with Iced** (0.14), following its state, message, update and view split.
Maintenance came first: Iced has versioned releases and documentation.

## Alternatives considered

- **GPUI**: no maintained release; its ecosystem pinned weekly snapshots of Zed published by a third
  party. It was tried again later and set aside ([0020](0020-gpui-set-aside-for-slime-ui.md)).
- **egui**: immediate mode, everything laid out again each frame, no identity of elements across
  frames.
- **GTK or Qt bindings**: large C or C++ dependencies, and an interface that looks like a desktop
  toolkit rather than a launcher.

## Consequences

- Iced gives the focus only to text fields: the couch mode keeps a focus of its own
  ([0018](0018-a-couch-mode-of-its-own.md)).
- Iced keeps no smaller copies of images and lays out every widget of a view: covers are made at
  the size drawn ([0016](0016-images-shown-from-files-at-the-size-drawn.md)) and grids build only
  the rows in view ([0017](0017-only-the-rows-in-view-are-built.md)).
- Iced draws with the processor (tiny-skia) where no GPU driver works, which `gui/src/graphics.rs`
  relies on.
- Two advisories of Iced's font and SVG code (`rustybuzz`, `ttf-parser`, unmaintained) are ignored
  in `deny.toml` until Iced moves on.
