# 0037. Core dumps kept off for Slatty and the programs it starts, on Linux

- Status: Accepted
- Date: 2026-11-11

## Context

Slatty holds the account's session tokens in memory while it runs: GOG's and Epic's, read from
the keyring. A crash on a system that keeps core dumps (systemd-coredump does on most
distributions) writes that memory to the disk, readable by the user and by anything that runs as
the user, and often shared in a bug report. ADR-0036 keeps secrets out of core dumps.

The tokens are not only in `secret::Secret`. Each one is copied into the buffers of reqwest,
hyper and rustls when it is sent, into serde_json's when it is read, into zbus's and the
keyring crate's when it comes from the keyring, and memory once freed is not wiped. Marking
`Secret`'s memory with `MADV_DONTDUMP` would leave all of these in the dump.

Two settings keep a whole process out of a dump, on Linux:

- its core size limit (`RLIMIT_CORE`) at 0: the kernel writes no core file, and systemd-coredump
  stores none ("the core dump will only be processed when the related process resource limits
  (RLIMIT_CORE) are sufficient", systemd-coredump(8)). The kernel does not enforce it for a
  handler it pipes the dump to (core(5); `fs/coredump.c`: "Normally core limits are irrelevant
  to pipes"): it still pipes the whole memory, and the handler decides. Programs started inherit
  it.
- `prctl(PR_SET_DUMPABLE, 0)`: no dump at all, before any handler is chosen (`fs/coredump.c`,
  Linux 6.12: `if (!__get_dumpable(cprm.mm_flags)) goto fail;`); `fs.suid_dumpable`, which
  systemd sets to 2, applies only to processes whose credentials changed, as a set-user-ID
  program's. It also makes most of `/proc/<pid>` unreadable to the user (`environ`, `maps`,
  `smaps`, `fd`, `exe`) and refuses a debugger, which `slatty-measure`, gdb, strace and perf
  need. A program started drops it.

## Decision

**Each Slatty process sets its core size limit to 0 as it starts, on Linux**
(`platform::no_core_dumps`): the command line, the interface and the session supervisor. The hard
limit is left as it is. The programs Slatty starts inherit it: games, umu, Proton and Comet, which
is handed the tokens too. The programs that never see a token and could outlive Slatty, the Steam
client and what the desktop opens (`auth::spawn_reaped`), get the limit back
(`platform::with_core_dumps`): a Steam client started by Slatty would otherwise pass it to every
game it starts until it quits.

**Nothing is done on Windows yet.** Windows Error Reporting writes a full dump to the disk only
where the person or their organisation turned on its local dumps; otherwise it sends at most a
small dump to Microsoft, with the person's consent. Its exclusion of memory blocks
(`WerRegisterExcludedMemoryBlock`) would leave the copies in the dump, as `MADV_DONTDUMP` would,
and excluding Slatty from it writes a lasting key in the person's registry. To be settled with the
interface on Windows or with the crash reports.

## Alternatives considered

- **`MADV_DONTDUMP` on `Secret`'s memory**: targeted, but the copies above stay in the dump.
  Rejected.
- **`PR_SET_DUMPABLE` at 0**: no dump of Slatty under any crash handler, apport and ABRT
  included, but Comet left out of it; and `slatty-measure` and debuggers lose Slatty, and desktop
  tools that read `/proc/<pid>/exe` may fail to recognise its window. Rejected by the owner.
- **Both**: the strongest, with the costs of both. Rejected by the owner.

The owner first chose with `PR_SET_DUMPABLE` described as leaving a dump readable by root where
`fs.suid_dumpable` is 2; that was wrong, as the kernel's code above shows. Told so, the owner kept
the decision, the Steam client given its limit back.
- **Waiting for the crash reports**, as ADR-0036 first planned: the tokens stay exposed until then.
  Rejected by the owner.

## Consequences

- A crash of Slatty leaves no core dump to study, nor does a crash of a game it started, unless
  the game raises its own limit again.
- A crash handler other than systemd-coredump (apport, ABRT) is still piped the memory and may
  ignore the limit: not checked.
- `slatty-measure`, gdb and strace keep working on Slatty.
