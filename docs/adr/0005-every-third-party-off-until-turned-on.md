# 0005. Every third party off until turned on; no telemetry

- Status: Accepted
- Date: 2026-10-10

## Context

A launcher sits between a person and the services that know what they play. Besides GOG, Slatty
can ask umu's game database, GitHub (Proton builds), SteamGridDB, Steam's image servers and Steam's
Web API, and the programs it starts contact others (umu asks Valve about its runtime, Comet talks to
GOG). A person should know, and choose, what leaves their computer.

## Decision

**Every request to a host other than GOG's and the download servers GOG names has a switch in
Settings, off until turned on, and an entry in [SECURITY.md](../../SECURITY.md) ("What leaves your
computer").** The traffic of the programs Slatty starts counts as Slatty's.

**No telemetry, ever.**

## Alternatives considered

- **Third parties on by default**, as most launchers do: covers and fixes without a step, at the
  cost of telling several companies what a person plays without asking.
- **Anonymous usage statistics**: nothing Slatty needs to work, and one more thing to trust.

## Consequences

- A new outgoing request to another host is a change of SECURITY.md and of Settings, never only of
  code (AGENTS.md, "Secrets and privacy").
- Features that need a third party show what is missing until turned on (covers from SteamGridDB,
  images Steam's client lacks).
