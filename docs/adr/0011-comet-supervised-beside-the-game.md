# 0011. Comet supervised beside the game, off until turned on

- Status: Accepted
- Date: 2026-10-12

## Context

GOG games report achievements and play data through GOG Galaxy's service, which their Galaxy SDK
talks to over a local port. Comet is an open replacement for that service. Its code keeps global
state, listens on the fixed port 127.0.0.1:9977, and while it runs, hands the account's access token
to any local program that asks.

## Decision

**Comet runs as a separate process beside the game, supervised by Slatty, only once the person
turned achievements on.** It gets its tokens through a 0600 file in a private folder, removed as
soon as it listens, never through its arguments; it stops with the game, with the launcher, and when
the launcher dies. Slatty reads the unlocks it records from its database, read-only.

## Alternatives considered

- **Comet as a library inside Slatty**: its global state and fixed port are not made for embedding,
  and its token exposure would last as long as the launcher.
- **Comet always on**: the token exposure would be permanent.

## Consequences

- Only one Comet runs at a time: another launcher's (Heroic's) makes it fail, which
  `slatty doctor` reports (AGENTS.md, "Known pitfalls").
- Some Galaxy SDK versions need a `GalaxyCommunication` Windows service in the prefix: Slatty
  registers Comet's dummy service there before the first session with Comet.
