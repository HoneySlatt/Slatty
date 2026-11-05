# 0029. On Windows every file an install writes is flushed to the disk before it takes its name

- Status: Accepted
- Date: 2026-11-05

## Context

An install writes each file under a temporary name, then renames it. On Linux files of 1 MiB or
more are flushed to the disk before the rename (`installer::SYNCED_FROM`), and the filesystem once,
after many files (`fsutil::sync_filesystem`), as one call is far cheaper than one per file: a hard
disk takes about 10 ms for each. Windows has no flush of a whole filesystem for a user. Its smaller
files were never flushed: after a power cut during an install or an update, one could keep its name
without its bytes, and the game fail on it with nothing to say why.

## Decision

**On Windows every file is flushed before it takes its name** (`installer::synced_on_write`), large
or small, off the async threads, while other files download. A resumed install still takes only the
large files on their size, as on Linux, and checks the small ones again.

## Alternatives considered

- **Leave it, written as a limit**: no cost, but nothing tells the person to verify a game after a
  power cut, and `slatty verify` repairs only what it is asked to. Rejected by the owner.
- **Flush from 64 KiB**: cheaper, but safety in part, behind a threshold no measurement supports.
  Rejected.

## Consequences

- An install of many small files costs one flush each on Windows. Its cost is to be measured on
  Windows (`fsutil::tests::syncing_every_small_file`, [measurements.md](../measurements.md)).
