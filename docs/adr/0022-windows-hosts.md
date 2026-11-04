# 0022. Windows hosts: one code base, built from Linux, Linux's features kept Linux's

- Status: Accepted
- Date: 2026-10-28

## Context

Slatty runs on Linux. The owner wants it on Windows 10 and 11 (x86-64) too. Much of what Slatty does
on Linux has no Windows equivalent (umu, Proton, its container, `steam-run`, GOG's Linux builds),
and development happens on NixOS, without a Windows machine at hand.

## Decision

**One code base**: the same crates build for Linux and Windows; what differs by system is gathered
in the core's `platform` module, and Linux behaves exactly as before at every step.

**Built and tested from Linux**: cross-compiled in the `windows` development shell (Rust's
`x86_64-pc-windows-gnu` target with MinGW-w64), tests of the Windows code run under Wine. Wine is
not Windows: nothing about Windows is called verified before a person checked it on Windows 10 or
11, recorded in [compatibility.md](../compatibility.md).

**Windows games run directly** on Windows, without Proton or Wine; GOG's Windows builds, Epic's
games, Steam's games through its client and games added by hand are the first version's scope;
emulated games follow.
**What only Linux has stays Linux's**: umu and Proton, isolation, `steam-run`, GOG's Linux builds,
Proton builds; the interface does not offer them on Windows. Secrets go to Windows Credential
Manager; sessions are followed by a job object.

## Alternatives considered

- **Microsoft's toolchain (`x86_64-pc-windows-msvc`)**: the usual one on Windows, but not buildable
  from NixOS without Microsoft's SDK; kept for later if the MinGW build shows a problem it does not
  have.
- **A separate Windows program**: two code bases drifting apart.

## Consequences

- No isolation of Windows games from the person's files in the first version, and an unsigned
  `.exe` that Windows SmartScreen warns about.
- The steps of the port are milestones of [PLAN.md](../PLAN.md); what Slatty depends on Linux for is
  listed in [windows-port.md](../windows-port.md).
- The Windows interface waits for slime-ui ([0020](0020-gpui-set-aside-for-slime-ui.md)): the
  first Windows version is the core and the command line; the window comes with the move to
  slime-ui.
