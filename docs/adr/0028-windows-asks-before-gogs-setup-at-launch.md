# 0028. On Windows, GOG's setup runs once the person agreed, asked at launch

- Status: Accepted
- Date: 2026-11-05

## Context

After an install, GOG Galaxy runs each game's setup: GOG's script interpreter or the game's own
setup program, and Microsoft's redistributables (Visual C++, DirectX, PhysX). On Linux Slatty runs
them in the game's Wine prefix, which needs no rights. On Windows they install into the system:
some programs ask Windows for administrator rights themselves (Microsoft's installers), others
cannot start without them (`ERROR_ELEVATION_REQUIRED`). Galaxy has a service of its own, running
with those rights, to avoid the prompt. [ADR-0022](0022-windows-hosts.md) said Windows asks once the
person agreed in Slatty.

## Decision

**The agreement is asked before the launch that would run the setup.** On Windows, when a game's
setup is pending, `slatty launch` asks whether to run it, saying that Windows may ask for
administrator rights. `--setup` agrees without asking and `--no-setup` refuses; without a terminal
and without either, the setup waits. `slatty setup <id>`, asked for by the person, is the agreement
itself.

**Refused, the setup waits**: it stays pending, with no failure counted (`setup::SETUP_RUNS`), and
the game starts all the same (`PlayEvent::SetupNotAgreed`). The core runs the setup in a launch
only when the request says the person agreed (`PlayRequest::setup_agreed`).

**Agreed, a program that needs the rights is started through Windows' prompt** (`runas`), by the
session supervisor (`SessionHandle::start_elevating`). Refused there, the run stops and the setup
waits for the next launch.

## Alternatives considered

- **A setting, asked once for every game**: one question in all, but a consent given far from the
  moment it is used, and a command line with no settings command yet. Rejected by the owner.
- **Only `slatty setup`**: the plainest consent, but a game that needs its Visual C++ runtime would
  not start at its first launch until the person typed the command. Rejected by the owner.
- **A service with administrator rights, as Galaxy's**: no prompt after the first, but a program
  of Slatty's running with full rights all the time. Rejected.

## Consequences

- One question at the first launch of each build whose setup has not run.
- A program started with administrator rights runs outside the session's job object: its output is
  not logged, it cannot be stopped, and the programs it starts are not followed, its step ending
  when it exits (Microsoft's installers wait for theirs). Not yet checked on Windows.
- The interface on Windows (Stage 5) asks the same question before the launch.
