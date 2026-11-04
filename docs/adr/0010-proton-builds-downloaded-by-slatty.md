# 0010. Proton builds downloaded by Slatty, not by umu

- Status: Accepted
- Date: 2026-10-21

## Context

Each game runs through a Proton build. umu can fetch a build itself, but it does so when a game is
launched, which then waits for half a gigabyte, and it replaces its "latest" build in place, under
games already running on it.

## Decision

**Slatty downloads Proton builds itself** (`core/src/protons.rs`), once **Proton downloads** is
turned on: from the project's GitHub release, checked against its SHA-512 sum, unpacked in a staging
folder and moved into place, **one folder per build**, followed through a `<project>-latest` link
that umu resolves at launch and that is replaced in one step once a newer build is in. The build
before is kept; a build a game, an install or the default uses is never deleted.

## Alternatives considered

- **Letting umu download at launch**: the first Play waits, and an update can change the build
  under a running game.
- **Only builds the person installed by hand**: no updates, and every person finds and checks
  builds alone.

## Consequences

- GitHub is a third party, so the downloads are off until turned on
  ([0005](0005-every-third-party-off-until-turned-on.md)).
- One lock covers each change of the builds or links, so the interface and the command line never
  interleave.
- Linux only ([0022](0022-windows-hosts.md)).
