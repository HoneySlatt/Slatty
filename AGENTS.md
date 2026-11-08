# AGENTS.md

Rules for AI agents working on Slatty. Read this file fully at the start of every session.
[CONTRIBUTING.md](CONTRIBUTING.md) applies to every change too.

## Project

- A native Linux launcher for GOG games, in Rust (edition 2024, 1.90+), with an
  [Iced](https://iced.rs) 0.14 interface, under GPL-3.0-or-later.
- It installs, runs (umu and Proton, or natively), syncs the saves of and reports the achievements
  (Comet) of GOG's games, through services GOG never documented: all reverse engineered.
- The library also shows Steam's and Epic's games, games added by hand and emulated games.
- It handles people's accounts, saves and disks: **safety comes before features**.
- Its direction: [ADR-0034](docs/adr/0034-what-slatty-is-to-become.md).
- 0.2.0 is released, on Linux; Slatty is paused until its port onto uwui ([PLAN](docs/PLAN.md)).

Anyone must be able to understand it from the repository alone.

## Read before working

- [docs/PLAN.md](docs/PLAN.md): its *Current* section and the current milestone.
- When planning, or when the task touches a decision: [docs/ROADMAP.md](docs/ROADMAP.md)
  (direction, stages, quality bar), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
  [docs/adr/](docs/adr/README.md). Never decide again what an ADR decided.
- Before touching `core/src/cloud`: [docs/cloud-saves.md](docs/cloud-saves.md).

You remember nothing from earlier sessions: what is not in these files is lost.

## Scope

- The current milestone, or what the owner asks. A request that changes the plan goes into the PLAN.
- No later stage, no abstraction prepared for one. Nothing is built because the roadmap mentions it.
- A task needs something out of scope: stop and say so.
- Iced receives bug fixes only; every piece of interface work waits for Stage 5, the port onto
  uwui ([ADR-0030](docs/adr/0030-ported-second-onto-uwui.md)). No other interface framework.

## Commands

In the Nix development shell; [CONTRIBUTING.md](CONTRIBUTING.md) has the details.

```sh
nix develop -c scripts/check.sh                     # the definition of done's checks; what CI runs
nix develop -c cargo build --workspace              # last: people run target/debug/slatty-gui
nix develop .#windows -c cargo check --workspace    # the Windows build, from Linux
```

- Run the program on a throwaway profile, never on the owner's data
  ([testing.md](docs/testing.md#the-application-at-10000-games)).
- [docs/testing.md](docs/testing.md) has where tests live, their fakes, the snapshots, the
  benchmarks, and the tests that need real Proton, Comet or bubblewrap: run those of your area.

## Definition of done

- `scripts/check.sh` passes.
- The program was run on a throwaway profile and behaved as expected.
- New behaviour is tested where practical; a bug fix starts with a failing test.
- The budgets the milestone touches are measured and met; the figures go to
  [docs/measurements.md](docs/measurements.md).
- [docs/PLAN.md](docs/PLAN.md) gives the new status.
- Every document affected is updated and reread as a newcomer would.
- Every decision a newcomer would question is an ADR.

A check against GOG, Steam, Epic or Windows that was not run for real is said so.

## Things an agent must not do on its own

Ask the owner first, every time:

- **Act on their accounts.** `slatty install`, `installers --download` (gigabytes), `cloud sync`
  (writes saves on both sides), `achievements --unlock/--clear` (a public profile), `launch` (play
  time). Check with read-only commands: `cloud status`, `install --info`, `installers <id>`,
  `update --check`, `library list`. Steam and real installations: see
  [CONTRIBUTING.md](CONTRIBUTING.md#real-accounts-and-installations).
- **Handle secrets.** Never ask for, print or copy a password, a token or the sign-in address.
  Never read the keyring. The person signs in, in their browser.
- **Touch real user data.** Tests use `Dirs::under(<temporary folder>)` and `Db::in_memory()`,
  never `Dirs::from_system()`. Never edit or delete under `~/.local/share/slatty`,
  `~/.cache/slatty`, `~/.config/slatty`, a game folder or a Wine prefix.
- **Publish.** No tag, release or registry upload, and no pull request or comment on anyone
  else's repository.
- **Add a dependency.** Prefer the standard library or a crate in `Cargo.lock`; justify a new one.
  Audit after adding or updating one ([CONTRIBUTING.md](CONTRIBUTING.md#dependencies)).

## Architecture rules

- `slatty-core` holds all behaviour, for the command line and the interface: no interface code,
  typed errors (`error::Error`) and events. User-visible wording lives in the front ends.
- `slatty-app`, the interface's state, builds without Iced
  ([ADR-0019](docs/adr/0019-the-interfaces-state-free-of-any-framework.md)); `slatty-gui` draws it.
- Long operations take a `CancellationToken` and report progress through a callback.
- Network services sit behind traits with in-memory fakes, new ones too.
- The interface follows the module you change and [its conventions](docs/ARCHITECTURE.md#interface):
  nothing slow on its thread, views never name a colour, dialogs are layers over the page.
- Interface text is short and plain.

### Performance

The library stays fluid at **10,000 games**
([why and how](docs/ARCHITECTURE.md#staying-fluid-at-10000-games)):

- library and Achievements changes are measured with `library_at_10000_games`, never with a small
  library, against a 16 ms frame; scrolling in the real application;
- a grid or long list builds only the rows in view; `view` does no per-game work
  (`Message::keeps_lists`);
- no image bytes in `App`: covers and key art go through `App::grid_cover`, `shown_art`, `cover_at`;
- heavy work runs at the lowest priority (`fsutil::in_background`).

## Rules the code depends on

### Secrets and privacy

- Tokens live only in the system keyring: never in a file (but Comet's short-lived 0600 handoff),
  a log, an error message or a process argument.
- Sensitive values are wrapped in `secret::Secret`.
- `reqwest` errors become `Error::network(context, e)`, which strips the URL.
- The account's token is sent only through `http::gog_only`, to an address a GOG answer names.
- A request to a new host, by Slatty or a program it starts, needs a switch in Settings, off until
  turned on, and an entry in [SECURITY.md](SECURITY.md#what-leaves-your-computer).
- No telemetry, ever.

### Files and data safety

- Every path from GOG goes through `installer::safe_relative`; every write into a game folder
  through `installer::refuse_outside` / `leads_inside`.
- Files are written staged, then renamed (`fsutil::write_atomic`). A game folder appears only once
  every file is verified.
- Changing a game's files or saves holds `lock::game(dirs, game_id)` throughout.
- A game id is checked before it names a path: `store::of`, `paths::check_owned_id` (never Steam's
  files), `paths::check_game_id` (GOG requests)
  ([ARCHITECTURE.md](docs/ARCHITECTURE.md#rules-every-part-follows)).
- A destructive action refuses when something looks wrong, or keeps a copy first.
- Only files Slatty installed (its install record) are deleted from a game folder.
- Cloud sync decides by content hashes and per-file history, never dates. Conflicts never resolve
  themselves. Every new case gets a test.
- The schema changes only by appending to `db::MIGRATIONS`; never edit or reorder a migration.

### Honesty about GOG

- "Verified" means checked against the real service. Simulated tests never justify a line in
  [compatibility.md](docs/compatibility.md) or a "Verified" in
  [gog-integration.md](docs/gog-integration.md).
- Cite the source of every GOG behaviour in gog-integration.md.
- Code derived from GPL projects names its source. Never copy code without a licence.

### Known pitfalls

Read [ARCHITECTURE.md](docs/ARCHITECTURE.md#known-pitfalls) before touching launches, Comet, the
environment or the tests that run Proton: NixOS has no `/bin/bash`, umu cannot see
`/tmp/nix-shell.*`, umu's container shares more than it is told.

## Tests

- Anything that writes or deletes user data has a test.
- Test data is visibly fake: `[FAKE]` titles, made-up ids; never real accounts, tokens or saves.

## Code rules

- Match the surrounding code. No reimplementation of the standard library without a measurement.
- Comments say what the code cannot: a reason, a limit, a source. No commented-out code.
- Doc comments describe behaviour in plain sentences ("Whether…", "Starts what downloads next…").
- Search for an existing helper first. One way to do a thing; two found: unify or report.
- Edit Rust with an editor tool, never `sed` or `perl`. Dead code you find goes to the PLAN's debt.
- Follow [ARCHITECTURE.md](docs/ARCHITECTURE.md#rules-every-part-follows) and the ADRs; where none
  covers a case, propose an ADR. Report drift from them; do not work around it.

## Documentation

- It describes the code as it is now, for a Rust developer who has never seen the project. Wrong
  documentation is worse than none; a change that leaves a document wrong is not finished.
- No reference to conversations the reader cannot see.
- Coined terms are defined where first used and in [docs/glossary.md](docs/glossary.md).
- Limits and debt are stated plainly; a partial feature is not presented as complete.
- Every public item says what it does, its invariants, its errors and its panics.
- Each fact has one home; others link to it
  ([CONTRIBUTING.md](CONTRIBUTING.md#documentation) says which).

## Commits

- One change per commit, once `scripts/check.sh` passes, added by its paths, never `git add -A`.
- Commit, push to `main` and open issues without asking once the checks pass; done means CI green.
- The subject, imperative, says what changes for the user; the body says why, with figures for
  speed.

## Stopping, reporting, several agents

- Stop and ask before a decision no ADR covers, a request against the roadmap or an ADR, a plan
  that turns out wrong or a milestone larger than expected. Never work around a problem to go on.
- A milestone closes with a review on the strongest model against this file, the ADRs and the
  quality bar, then a report to the owner: what was done, the choices and those rejected, what to
  read first, the figures, the debt, what was not verified, the next step.
- One milestone per agent, as the PLAN assigns it. Shared files (PLAN.md, ARCHITECTURE.md,
  `Cargo.toml`, `Cargo.lock`, `flake.nix`): one agent at a time.
