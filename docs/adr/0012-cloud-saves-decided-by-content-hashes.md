# 0012. Cloud saves decided by content hashes and per-file history, never dates

- Status: Accepted
- Date: 2026-10-09

## Context

Syncing saves between a computer and GOG's (and Epic's) cloud decides, for each file, whether to
upload, download, or ask. A wrong decision loses a save. File dates are unreliable across machines,
time zones, Wine prefixes and copies, and the cloud's dates are those of the upload, not of the
game's write.

## Decision

**Sync is a three-way decision on content hashes**: what the file is here, what it is in the cloud,
and what both were at the last sync, kept per file. **Conflicts never resolve themselves**: the
person chooses. A copy of every save replaced or deleted is kept, on either side. The rules are in
[cloud-saves.md](../cloud-saves.md).

## Alternatives considered

- **Last modified wins**, as most launchers do: simple, and wrong whenever clocks or copies lie.
- **Upload after each session only**: loses saves made on another computer.

## Consequences

- Each new case of sync has a test (AGENTS.md, "Files and data safety"), and cloud-saves.md is
  read before touching `core/src/cloud`.
- Epic's snapshots go through the same rules, behind the `CloudTransport` trait.
