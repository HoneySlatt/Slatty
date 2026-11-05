# Review: a save folder made again (2026-11-05)

- **Scope**: correctness, data safety, tests and documentation of the fix that adds a save
  folder's creation time to its identity (`cloud::sync::folder_identity`, `platform::Identity`),
  before commit 16cd286.
- **How**: one fresh reviewer, given the diff, AGENTS.md and cloud-saves.md, not the author's
  reasoning.

## Findings

| Finding | Severity | What became of it |
|---|---|---|
| FAT32 and exFAT drives on Windows can give a folder made again its old index, and Windows its old creation time within 15 s | Low, documentation | Stated in cloud-saves.md's known limits |
| Wine may report the last write as a folder's creation time, so every sync would ignore history | Low, to check under Wine | Confirmed under Wine; Windows' creation times no longer used (a6a64b3) |
| The test needs a filesystem that keeps creation times | Low, tests | Said in the test's comment |
| Going back to 0.2 makes every synced folder count as made again, once | Information | Said in the CHANGELOG and cloud-saves.md |
| A migration's comment still said "device and inode" | Nit | Fixed |

Checked and fine: the old `device:inode` identities stay trusted; `offline::stamp` is unchanged;
no extra system call.
