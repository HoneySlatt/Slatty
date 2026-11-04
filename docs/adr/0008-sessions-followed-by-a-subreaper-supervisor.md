# 0008. Game sessions followed by a subreaper supervisor

- Status: Accepted
- Date: 2026-10-12

## Context

A game's session must be measured (play time, reported to GOG), and its end must start what follows
it (cloud sync, Comet's stop). Many games start through launchers that exit early, and Wine leaves
service processes behind: the first process ending says nothing about the game ending. The session
must also outlive the interface, which a person may close while playing.

## Decision

**A launch starts the running binary again with a hidden argument, and that copy becomes the
session's supervisor**: it marks itself `PR_SET_CHILD_SUBREAPER`, starts the game in its own process
group, and reports events as JSON lines. Processes that detach are re-parented to it, so the session
ends only when no descendant is left. It is started through `/proc/self/exe`, which still works
after the file was replaced by an update or a rebuild.

## Alternatives considered

- **Waiting for the first process**: ends sessions too soon for launchers and Wine.
- **Watching processes by name or by scanning `/proc`**: fragile, and reads other programs'
  arguments.
- **cgroups through systemd**: depends on the init system and a user session bus.

## Consequences

- A session without a recorded end (a crash) is reported at the next launch.
- On Windows the supervisor uses a job object instead ([0022](0022-windows-hosts.md)).
