# 0001. Rust for the whole application; Proton, umu and Comet stay external tools

- Status: Accepted
- Date: 2026-10-09

## Context

Slatty installs and runs games from GOG and other stores on Linux. Running a Windows game needs
Wine with Valve's Proton patches, the runtime container games expect, and fixes per game; reporting
achievements to GOG needs a program that speaks the Galaxy SDK's protocol to the game. Each of these
is a large, specialised project that other launchers already rely on: Proton (Valve and its
community builds), umu-launcher (Proton outside Steam, with the Steam Linux Runtime and a database
of fixes), and Comet (an open replacement for GOG Galaxy's service).

The launcher itself handles accounts, tokens, saves and people's disks, where memory safety and
explicit errors matter.

## Decision

**Rust everywhere in the application.** The core, the command line and the interface are Rust
crates in one workspace.

**The tools stay external.** Proton, umu and Comet are separate programs that Slatty finds, starts
and supervises, with explicit boundaries: environment variables, command lines, files. Slatty
reimplements none of them.

## Alternatives considered

- **Rewriting what umu does** (the runtime container, the fixes): months of work duplicating a
  maintained project, and games would run differently than under the tool everyone tests with.
- **Embedding Comet as a library**: rejected in [0011](0011-comet-supervised-beside-the-game.md).
- **Another language for the interface** (a web view, Python): two languages and a boundary
  between them for no gain, and a web view would carry a browser engine.

## Consequences

- Slatty depends on the versions of these tools a system provides; the Nix package and the
  development shell bring them, and `slatty doctor` reports what is missing.
- Their quirks become Slatty's pitfalls (AGENTS.md, "Known pitfalls"): umu's container shares
  more than it is told, Comet listens on a fixed port.
