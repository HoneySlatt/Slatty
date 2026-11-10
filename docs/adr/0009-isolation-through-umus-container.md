# 0009. Isolation through umu's container, the tools read-only through bubblewrap

- Status: Accepted; amended by [0034](0034-what-slatty-is-to-become.md) (a game's network cut; Linux
  games isolated too) and [0036](0036-slattys-bar-against-the-market.md) (isolation measured apart;
  no virtual input device inside the sandbox)
- Date: 2026-10-10

## Context

A Windows game run through Proton can read and write everything its user can: documents, keys,
other games' saves. Every Proton game already runs inside pressure-vessel, the Steam Linux
Runtime's container, which umu starts; it keeps GPUs, sound and gamepads working and has options to
hide the home folder. But umu shares, writable, the tools it runs a game through (its runtime, the
Proton build), and both it and pressure-vessel share more than they are told (AGENTS.md, "Known
pitfalls").

## Decision

**An isolated game runs in umu's own container**, told a home folder of the game's own, the game
folder writable, its support files read-only, no D-Bus, a process namespace of its own, and none of
the variables that would make pressure-vessel share more (`runner::isolation_env`). **umu itself
runs inside bubblewrap**, the system as it is with the tools bound read-only over themselves
(`runner::read_only_tools`), so a game cannot change what other games run. Isolation is on by
default for Windows games. A launch whose sandbox cannot start is refused with the reason, never run
without isolation.

## Alternatives considered

- **A container of Slatty's own** (bubblewrap or Flatpak around the game): it would have to
  reproduce what pressure-vessel does for GPUs, audio and gamepads, and games would run differently
  than under the tool everyone tests with.
- **No isolation**: what every other launcher does today.

## Consequences

- Isolation keeps games out of files and D-Bus, not out of the system: it is not a security
  boundary against a hostile kernel exploit, and SECURITY.md says so.
- Every change to a launch is checked with the ignored isolation tests, which run the real
  container with a control (AGENTS.md, "Commands").
- On Ubuntu 23.10 and later without an AppArmor profile, bubblewrap can fail: the launch is
  refused rather than run unisolated.
- Windows has no equivalent a launcher can use without administrator rights: no isolation there
  ([0022](0022-windows-hosts.md)).
