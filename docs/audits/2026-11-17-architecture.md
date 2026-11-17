# Audit: architecture, stability, evolution and contribution (2026-11-17)

- **Scope**: the whole repository at commit `3d9c028`: whether contributors can understand
  Slatty, work on it and change its features without breaking what exists or piling up costly
  debt. Dimensions: the boundaries between the crates, ownership of state and work, store and
  platform boundaries, the port onto uwui, persistent formats and recovery, security, tests,
  documentation and contribution, and the owner's agent rules as they apply to Slatty.
- **How**: one reader on the strongest model, read-only: the documents, the ADRs, the code where
  each finding points, Git's history, and GitHub's issues and CI runs. Nothing was built, run or
  measured. Nothing was delegated: the delegation's protocol writes into the repository, which
  the audit's terms forbade. No independent review was made. About 91,000 lines of Rust; the views
  (`crates/gui/src/ui`), the isolation (`runner.rs`), Steam and emulation were not read in detail.
- **Evidence**: each finding says whether it rests on the code read, on a document, or is a
  hypothesis.

## Verdict

Sound where one file at a time protects the person's data: atomic writes flushed to the disk,
game locks shared between processes, transactional migrations that refuse a newer database,
checked paths, a three-way cloud decision by content, isolation. Weaker where several things must
change together: long operations live in the interface's state, errors carry English prose, the
JSON formats have no version, job states are strings, the store routing is copied into each front
end, and the state's rules are tested from the Iced crate. To consolidate and to restructure two
subsystems (the long operations and the error contract), not to rewrite.

## Findings

| # | Finding | Severity | What became of it |
|---|---|---|---|
| 1 | Repair, updates and uninstall replace or delete a game file the person changed (a mod that replaces a file), without a copy: `InstallRecord` keeps only path and size (`installer/record.rs`), and `maintenance::delete_listed` deletes every recorded path whatever it holds. The user guide says mods stay; only added files do. Games added by hand already delete a file only while it is as unpacked (`added::remove`) | High, data | Open; proposed as an exception to the pause. [Debt](../debt.md) |
| 2 | A cloud file whose listed hash changed while its content did not is downloaded again, the local copy backed up, every time (`cloud/sync.rs`, `download_replace`): GOG lists a hash of what was stored, and Slatty uploads gzip. A likely cause of M3.3's open failure, as GOG Galaxy ran beside Slatty in the virtual machine (hypothesis: the backups there tell). The fake cloud hashes the raw content, so no test sees it. Nothing records a sync's decisions | Medium | Open; proposed as an exception to the pause. [Debt](../debt.md) |
| 3 | Long operations are owned by the interface's state: the queue's rules over about ten fields of `App`, its order in a setting apart from the jobs, job states as strings read by the interface, a state not known resumed as a download, no queue in the command line | High, structure | Decided by the owner (option B of three): [ADR-0038](../adr/0038-long-operations-owned-by-the-core.md), attacked by a fresh review before it was written down, built in Stage 5's first milestone |
| 4 | Errors said to be typed carry English prose: 327 `Error::{Refused, Unsupported, NotFound}(String)` in the core, shown through `to_string()`; French ships with 0.3 and the agents' actions need errors they can act on | Medium-high, structure | Open; proposed before the pure models. [Debt](../debt.md) |
| 5 | No compatibility policy for stored formats: the JSON files (install records, games added by hand, emulated games, Epic's) have no version, and an older build that rewrites one drops the fields it does not know; the database is not copied before a migration; going back to an older version is not documented | Medium, data | Open; proposed as an ADR before 0.3. [Debt](../debt.md) |
| 6 | The store routing is in the front ends: `store::epic_app(id).is_some()` chooses the path 20 times in the state, 3 in the views, 9 in the command line. Safe by default (GOG's paths refuse other ids), but each new front end copies it | Medium, structure | Open; proposed before the port |
| 7 | 383 tests of the state live in `slatty-gui`, 40 in `slatty-app`; most use no Iced simulator. `Message::keeps_lists` ties the lists' freshness to message kinds, not to the data they read | Medium, port | First step of ADR-0038's milestone moves the queue's tests; the rest proposed with it |
| 8 | No trace after a failure: logs on stderr only, no panic hook, no journal of operations | Medium | Crash reports already debt; the journal of operations added to it |
| 9 | Limits: backups of saves never deleted; a save held whole in memory up to 4 GiB (`bytes_up_to(…, 4 << 30)`) against 3.25 GB on the lowest tier; a cloud sync cannot be cancelled; about 49 synchronous database writes from the interface's thread | Medium | Open. [Debt](../debt.md) |
| 10 | Governance: rules spread over ADRs that amend one another (the owner's method now asks one ADR per topic, stating the rule whole); the pause's written exceptions (M3.3, M5.0) narrower than its practice (13 code commits since 2026-11-06, security and bug fixes, one in M3.5's area); the code's rules in AGENTS.md and ARCHITECTURE.md both; `CHANGELOG.md` the most changed file; CONTRIBUTING still asks to check that no game runs, which the owner's rules dropped; the path without Nix untried | Medium | Open; proposed as documentation work |
| 11 | CI over its budget: run 43073135705 took 20 min 35 s on 2026-11-11, against 20 minutes | Low | Added to [the debt](../debt.md) |
| 12 | ADR-0034's optional pieces as Cargo features are not built: `slatty-core` has no feature | Low-medium | Open; cheaper once 6 is done |
| 13 | A queued install resumed after a change of account plans and downloads with the account signed in then (`Account::load`), not the one it was queued for | Low | Left to ADR-0038's milestone |
| 14 | Budgets: "no frame missed at 240 Hz" allows a p99 of 5 ms where a refresh is 4.17 ms; "lists made again ≤ 2.5 ms" is a function's budget that fills most of a 240 Hz frame if done on the interface's thread; M5.0's reference for "as good as Iced" scrolls at p99 36.8 ms | Information | Reported to the owner; no threshold changed |

## Held to be kept

Atomic writes (`fsutil::write_atomic`); the database's migrations and its refusal of a newer
version, with a test that makes each new migration carry its sample rows; `lock::game` and the
session lock that outlives the launcher; the pure three-way plan and a sync that every step leaves
safe to run again; ids refused where they do not belong (`store::of`, `paths::check_game_id`);
uninstall's whole list checked before anything is deleted; the staged folder and its mark; work
for an account dropped once it signed out (`App::account_task`); bounded hostile input (the zip
directory, network timeouts and retries); honest records of what was checked for real.

## The owner's agent rules

Rules added on 2026-11-16 to the owner's method, active for Claude Code: architecture and
stability scenarios in the quality bar, ownership and lifetimes in ARCHITECTURE.md, versioned
contracts and the recovery of a failed upgrade, one ADR per topic superseding the older, an index
of ADRs written by a script, changelog fragments in `changelog.d/`. Slatty has none of these yet;
each is proposed to the owner. Rules for agents alone (delegation, the length of AGENTS.md, one
milestone per session) stay out of what a human contributor must follow.

## Not checked

Nothing was run: no test, build or measure supports a finding. Finding 1 is certain for
uninstall; for repair and updates it rests on reading that found no copy of a changed file.
Finding 2's cause in M3.3 is a hypothesis, to be confirmed from the backups kept in the virtual
machine. Figures at 100,000 games were not measured.
