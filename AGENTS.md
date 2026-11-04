# AGENTS.md

Instructions for AI coding agents (and the people driving them) working on Slatty. Read this file
fully at the start of every session, before changing anything. It does not replace
[CONTRIBUTING.md](CONTRIBUTING.md), which applies to every change; it adds what an agent most often
gets wrong in this repository.

## The project in one paragraph

Slatty is a native Linux launcher for GOG games, in Rust (edition 2024, Rust 1.90+) with an
[Iced](https://iced.rs) 0.14 interface, licensed GPL-3.0-or-later. It signs in to GOG, installs
Windows builds from GOG's Galaxy content system and native Linux builds from GOG's offline
installers, runs games through umu and Proton (or natively), syncs cloud saves, and reports
achievements through Comet. The same library also shows the user's Steam and Epic games, games added
by hand (`local-<n>`) and emulated games (`emu-<n>`). GOG publishes no API for this: every service
was understood by reverse engineering. The launcher handles people's accounts, saves and disks, so
**safety comes before features**. Its real state: released as 0.2.0, on Linux; the Windows port is
under way.

It is developed primarily by AI agents; the owner sets its direction and validates architecture
decisions. Anyone arriving in the repository, at any point in its life, must be able to understand
all of it from the repository alone. Documentation is part of every deliverable, held to the same
standard as the code.

## Read before working

1. [docs/PLAN.md](docs/PLAN.md): the current stage, its milestones and their status. The source of
   truth for what to do now.
2. [docs/ROADMAP.md](docs/ROADMAP.md): direction, stage order, exit criteria, out of scope. An
   overview, not a commitment.
3. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): crates, modules and main flows.
4. [docs/adr/](docs/adr/README.md): decisions already made. Do not decide them again.
5. [docs/compatibility.md](docs/compatibility.md): what was checked against real GOG services.
6. For cloud saves, [docs/cloud-saves.md](docs/cloud-saves.md) before touching `core/src/cloud`.

You have no memory of previous sessions. These files are the project's memory: what is not written
there, the next session will not know.

## Scope

- Work only on the stage and milestone marked as current in [docs/PLAN.md](docs/PLAN.md).
- Do not start later stages, and do not add abstractions that prepare for them.
- If a task needs something out of scope, stop and say so.

The roadmap sets direction and order; it does not say what will be built, nor how. Build something
because the current milestone needs it, never because the roadmap mentions it. When experience
contradicts the roadmap, propose a change; once accepted, record it in an ADR.

The interface stays on Iced until it moves to slime-ui
([ADR-0020](docs/adr/0020-gpui-set-aside-for-slime-ui.md)), and **Iced receives bug fixes only**:
every piece of interface work (a new page or panel, the Windows interface, the couch mode's work for
the Steam Deck, translations) waits for slime-ui. Do not start another interface framework.

## Commands

Everything runs inside the Nix development shell, which provides the toolchain, umu-launcher,
bubblewrap, Comet, `GalaxyCommunication.exe`, the graphics libraries, udev (gamepads), cargo-deny
and lychee:

```sh
nix develop -c scripts/check.sh                     # every check of the definition of done; what CI runs
nix develop -c cargo build --workspace              # always last: people run target/debug/*
nix develop -c cargo fmt
nix develop -c cargo clippy --workspace --all-targets   # must stay warning-free
nix develop -c cargo test --workspace               # about 15 s, no network, no real games
nix develop .#windows -c cargo check --workspace    # the Windows build, from Linux (ADR-0022)
```

`scripts/check.sh` runs, in order: formatting, clippy with warnings as errors, the tests, the code
documentation with warnings as errors, `cargo deny check` (licences, advisories, sources; it needs
network access), the links between Markdown files (lychee, offline), the Windows build, the Nix
package, then `cargo build --workspace`. CI runs it unchanged on every push to `main` and every pull
request (`.github/workflows/ci.yml`). Run it before reporting a milestone done.

Run `cargo build --workspace` after the tests and clippy: they do not rebuild the `slatty-gui`
binary, and the person testing your change runs `target/debug/slatty-gui`.

To run the program without touching anyone's data, point `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_CACHE_HOME` and `XDG_STATE_HOME` at a throwaway folder of made-up games, with
`DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent` so no keyring is reached
([docs/testing.md](docs/testing.md#the-application-at-10000-games) describes such a profile of
10,000 games, and how the figures there are measured).

More checks, run when your change touches their area:

```sh
# Interface snapshots (PNG) of the tested screens
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui

# Library performance at 10,000 games (see "Performance" below)
cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture

# Real Proton and real Comet (ignored by default; see docs/testing.md). Name the test binary:
# a name filter such as `proton` also runs the isolation checks below, on the real tools.
TMPDIR=$HOME/.cache SLATTY_TEST_PROTON=<Proton dir> \
  cargo test -p slatty-cli --test supervisor --test prefix -- --ignored
unshare -rn sh -c 'ip link set lo up && cargo test -p slatty-cli --test comet -- --ignored'   # offline

# Isolation: a game in umu's container sees nothing of the home folder and cannot change the
# tools other games run. A failing check writes a probe into umu's runtime and the Proton build:
# give it copies (`cp -a --reflink=auto`) of ~/.local/share/umu (under <data>/umu) and of a build.
TMPDIR=$HOME/.cache XDG_DATA_HOME=<data> SLATTY_TEST_PROTON=<copy of a Proton dir> \
  cargo test -p slatty-cli --test isolation -- --ignored --test-threads=1

# Dependency audit, after adding or updating a dependency (it downloads the advisory database;
# scripts/check.sh runs the policy, cargo deny)
nix shell nixpkgs#cargo-audit -c cargo audit
```

`SLATTY_LOG=warn` (or `debug`) prints the core's logs on stderr.

## Definition of done

A milestone is done only when all of the following hold:

- `scripts/check.sh` passes.
- The program was actually run and behaved as expected (on a throwaway profile, as above).
- New behaviour is covered by tests where a test is practical; a bug fix starts with a failing
  test (see "Tests").
- [docs/PLAN.md](docs/PLAN.md) reflects the new status.
- Every document affected by the change is updated (see "Documentation"), and was reread as a
  newcomer would read it.
- Any decision taken is recorded as an ADR.

Never report that something works without having verified it. If you could not verify, say exactly
what was not verified and why; a check against GOG, Steam, Epic or Windows that was not run for real
is said so (see "Honesty about GOG").

## Things an agent must not do on its own

These act on a real person's account, disk or system. Ask the human first, every time:

- **Run commands against GOG with their account.** `slatty install` and
  `slatty installers --download` download gigabytes, `slatty cloud sync` writes saves on both
  sides, `slatty achievements --unlock/--clear` changes a public profile, and `slatty launch`
  reports play time. Read-only commands (`slatty cloud status`, `slatty install --info`,
  `slatty installers <id>`, `slatty update --check`, `slatty library list`) are the safe way to
  check a change.
- **Handle secrets.** Never ask for, print, copy or paste a password, a GOG token or the address
  that carries the sign-in code. Never read the keyring. Sign-in is done by the person, in their
  browser.
- **Touch real user data.** Tests and experiments use `Dirs::under(<temporary folder>)` and
  `Db::in_memory()`, never `Dirs::from_system()`. Never delete or edit anything under
  `~/.local/share/slatty`, `~/.cache/slatty`, `~/.config/slatty`, a game folder or a Wine prefix.
- **Escalate privileges or change the system.** No `sudo`, no system configuration changes.
  If a task needs either, say what to run and let the person decide.
- **Publish.** No push, no remote branch, issue, pull request or release unless asked.
- **Add dependencies casually.** Each one is code that reads downloaded data or runs with the
  user's rights. Prefer the standard library or a crate already in `Cargo.lock`; justify a new one
  and run `cargo audit`.

## Rules the code depends on

### Secrets and privacy

- Tokens live only in the system keyring (`credentials`, `account`). Never in a file (except
  Comet's short-lived 0600 handoff), a log, an error message or a process argument.
- Wrap sensitive values in `secret::Secret`; its `Debug` hides the value.
- Turn `reqwest` errors into `Error::network(context, e)`, which strips the URL: some GOG
  endpoints carry tokens in the query string.
- Besides GOG and the download servers it names, Slatty itself contacts only umu's game
  database, GitHub (Proton builds), SteamGridDB, Steam's image servers and Steam's Web API, each
  off until turned on. The programs it
  starts contact others (umu asks Valve about its runtime, Comet talks to GOG): a change that adds
  such traffic counts too. A new outgoing request to another host needs a switch in Settings, off
  until turned on, and an entry in SECURITY.md ("What leaves your computer").
- Send the account's token only through `http::gog_only` for an address a GOG answer names.
- No telemetry, ever.

### Files and data safety

- Every path from GOG (manifests, installers, cloud listings) goes through
  `installer::safe_relative` or an equivalent check, and every write into a game folder through
  `installer::refuse_outside` / `leads_inside`: nothing is written through a symbolic link that
  leads out of the game folder.
- Write files staged, then rename them (`fsutil::write_atomic`, `fsutil::temp_sibling`). A game
  folder appears only once every file is verified.
- An operation that changes a game's files or saves holds `lock::game(dirs, game_id)` for its
  whole duration.
- A game id names files (locks, records, prefixes, covers): check it before building a path from
  it. A game id is GOG's product id, `steam-<app id>`, `epic-<app name>`, `local-<number>` for a
  game added by hand or `emu-<number>` for an emulated one: `store::of` tells which and accepts
  them all (locks, customisation, covers); `paths::check_owned_id` accepts all but Steam's, and
  guards whatever installs, records or deletes a game's files, so a Steam game's, which Steam
  owns, never is;
  `paths::check_game_id` accepts GOG's alone and guards every request to GOG about a game.
- A destructive action either refuses when the situation looks wrong, or keeps a copy first. Only
  files Slatty installed (its install record) are ever deleted from a game folder.
- Cloud sync decides with content hashes and per-file history, never dates. Conflicts never
  resolve themselves. Read docs/cloud-saves.md and add a test for every new case.
- The database schema changes only by **appending** a migration to `db::MIGRATIONS`. Never edit
  or reorder an existing one: users' databases already ran it.

### Honesty about GOG

- A feature is "verified" only after a check against real GOG services. Simulated tests never
  justify a line in docs/compatibility.md or a "Verified" status in docs/gog-integration.md.
  When you could not check something for real, say so in your summary.
- Cite where a GOG behaviour comes from (heroic-gogdl, Comet, gogapidocs, a capture) in
  docs/gog-integration.md. Code derived from GPL projects is welcome with a note naming the
  source; never copy code from projects without a licence.

## How the code is organised

```
crates/core   slatty-core  all behaviour; no interface code, typed errors, progress callbacks
crates/cli    slatty       command line, diagnostics, process-level integration tests
crates/app    slatty-app   the interface's state, messages, update and background work, free of Iced
crates/gui    slatty-gui   Iced interface: the views, the styles, what Iced runs for the state
```

### Core

- Behaviour belongs in `slatty-core` so both front ends share it. The core returns typed errors
  (`error::Error`) and events (`PlayEvent`, `InstallEvent`, `Progress`); user-visible wording
  lives in the front ends.
- Long operations take a `CancellationToken` and report progress through a callback.
- Network services sit behind traits (`galaxy::ContentSource`, `cloud::CloudTransport`,
  `installer::linux::Source`) so tests run against in-memory fakes with fault injection. Keep new
  network code testable the same way.
- Read-only requests go through `http::json` / `http::bytes`, which retry transient failures.
- Settings are key/value rows: add a getter/setter pair with a constant key in
  `core/src/settings.rs`, defaulting sensibly when the row is missing.

### Interface (Iced)

The interface follows Iced's state / message / update / view split, the state in `slatty-app`
and the views in `slatty-gui`:

- `crates/app/src/lib.rs` holds `App` (the state), the `Message` enum and navigation. `update`
  only dispatches. The state names no Iced type: its `Task` (`task`), what it listens to
  (`listen`), sizes, pictures, colours and keys are its own, and `slatty-gui` turns them into
  Iced's (`run`, `ui::handle`, `ui::scroll`, `theme`). Keep it so: `slatty-app` must build without
  Iced.
- `crates/gui/src/main.rs` starts Iced. Its `App` wraps the state (it dereferences to it), so the
  views are `impl App` blocks reading the state as their own; the state's modules are re-exported
  there, under the paths the views and tests use (`crate::install::…`).
- Each feature keeps its state, its message handling and its background tasks in its own module.
  Most have their own message enum and an `impl App { fn update_<feature>(…) }` block (`add`,
  `couch`, `downloads`, `edit`, `install`, `maintenance`, `notify`, `offline`, `runners`,
  `settings`, `steam`, `updates`; `play` has `PlayMsg` and `on_play`); `library`, `cloud`,
  `achievements` and `scaled` handle `Message` variants through methods of `App`. Follow the
  module you change.
- The couch mode (`couch.rs`, `pad.rs`, `ui/couch.rs`) keeps a focus of its own, since Iced gives
  none to buttons, and starts its work through the same messages as the desktop window: a new
  action there sends the desktop's message rather than doing the work again.
- Views live under `ui/`, one module per page or panel. Shared pieces are in `ui/widgets.rs`,
  `ui/panels.rs` (drawers and library dialogs); text shown to the user is made in `format.rs`.
- Nothing slow runs on the interface thread: use `Task::perform`, `tokio::task::spawn_blocking`
  for file and CPU work, and `work::progress_stream` for progress (throttled to four updates a
  second). `boot.rs` reads everything the first page needs on a thread begun as the program starts,
  while the window and graphics driver are set up, and nothing from game folders or Steam
  libraries: they can be on a network drive still waking up.
- **Views never name a colour.** Every colour and motion value comes from `theme::tokens()`, and
  widget styles are functions in `theme.rs`. A new token goes into `look::Tokens` (the state's,
  read from the theme file), into `theme::Tokens` if the views draw with it (Iced's colours), the
  theme file format and docs/theming.md. Controls and covers take their corners from the tokens; cards, drawers and
  dialogs still have corners of their own in `theme.rs` (and one in `ui/widgets.rs::inner`).
- Icons are Lucide SVGs in `crates/gui/assets/icons`, named in the `Icon` enum and given their
  file there (`crates/app/src/icons.rs`).
- Dialogs and menus are **layers over the page** (`ui/mod.rs`), each over an empty layer, and the
  notice banner keeps its place even when empty. Do not wrap the page in a different widget
  depending on what is open: Iced would rebuild it and lose its state (the library's scroll
  position).
- Interface text is in English, short and plain.

### Performance

The library must stay fluid at **10,000 games**. Measure library and Achievements changes with
`library_at_10000_games`, never with a small library, and judge them against a 16 ms frame. The
cover grid and the Achievements grid build only the rows in view (`library::GridWindow`), with
one child per row of the whole grid (`ui::widgets::grid_rows`) so that a row keeps its place, and
Iced its laid-out text, while it stays in view; keep any new grid or long list virtualized the
same way. The simulator rebuilds every widget each time, so it cannot show what Iced keeps between
frames: check scrolling in the real application too (docs/testing.md says how).

`view` runs after every message, scroll steps included: keep per-game work out of it, or make it
once (sort keys are kept by title in `library::SortKeys`; the games each tab lists are kept in
`library::Lists` until a message that can change them; scroll steps, frames, covers, progress and
the others that cannot are listed in `Message::keeps_lists`). A new message that comes many times a second, or with the others just
after start, and changes no list belongs in `keeps_lists`; one that changes what a list shows must
not be there, or must clear the lists itself when it does (`App::playtimes_fetched`, only when
sorted by play time). Sort keys of the whole library are made at start, off the interface thread
(`library::sort_names`). Images are shown from their cached
files (`image::Handle::from_path`, files named after their format): never keep image bytes in
`App`, which held about 880 MB of covers at 10,000 games. The grid draws covers through
`App::grid_cover` and the game page its key art through `App::shown_art`, which give the copy
`scaled.rs` made at the size drawn once it exists: a new place showing a cover or key art goes
through them too (`App::cover_at` for covers drawn larger than the grid's, as in the couch mode). Work that reads many files or keeps the processor busy (checking game files,
making those copies) runs at the lowest priority through `fsutil::in_background`.

## Tests

- Add a test for every behaviour change, and first a failing test for every bug fix. Anything
  that writes or deletes user data needs one.
- Test data is visibly fake: `[FAKE]` titles, made-up ids. Never real accounts, tokens or saves.
- Temporary folders are named after the process id and removed at the end.
- Core tests use the in-memory CDN (`installer/tests.rs`), the in-memory cloud
  (`cloud/tests.rs`) and fake installer sources (`installer/linux/tests.rs`).
- Interface tests (`crates/gui/src/tests/`, one module per area, helpers in `tests/mod.rs` such
  as `library_app()`, `render()`, `fake_plan()`) drive the real `update` and `view` with Iced's
  simulator. When the simulator cannot read a widget's text (pick lists), assert on the state.
  Tests of the state alone (no view) can live in `crates/app`, beside the module they test. The
  fake state the interface's tests start from is `slatty_app::testing` (feature `test-support`).
- CLI tests in `crates/cli/tests` spawn real processes. Ignored tests need real Proton or Comet
  and are run on purpose.

## Documentation

### Standard

The reader is a competent Rust developer who has never seen this project. From the repository
alone, they must be able to understand what every part does and why it is built that way, then
build, test and change it.

- Documentation describes the code as it is now, not as it is intended to be. Wrong documentation
  is worse than missing documentation.
- No reference to past sessions or conversations the reader cannot see; every term the project
  coins is defined where it first appears and in [docs/glossary.md](docs/glossary.md).
- State limits and known debt plainly. Do not present a partial feature as complete.
- Every public item has documentation that says what it does, its invariants, its errors and its
  panics.
- The public documentation is in English. A change that leaves a document wrong is not finished.

### One home per fact

A fact written in two places drifts. Each kind of fact has one home, and other documents link to it
instead of copying it:

- **The code's documentation**: what an item does, its contracts, its limits and their figures.
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)**: the modules and the flows across them.
- **ADRs**: why a decision was taken and what was rejected. Only their status changes afterwards.
- **[docs/PLAN.md](docs/PLAN.md)**: status and known debt, linking to where the facts live.
- **[docs/testing.md](docs/testing.md)**: what was measured, and how.
- **[docs/compatibility.md](docs/compatibility.md)**: what was checked against the real services.
- **[CHANGELOG.md](CHANGELOG.md)**: what changed for users, once, when it changes.

### What to update

Update it in the same change:

| When you change… | Update |
|---|---|
| A panel, a setting, a stored file | [docs/user-guide.md](docs/user-guide.md) |
| A command or an option | [docs/command-line.md](docs/command-line.md), and the user guide where it shows the command |
| A module or a flow | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| A decision (a framework, a platform, a format, a security choice, something tried and dropped) | A new ADR in [docs/adr/](docs/adr/README.md), copying `0000-template.md`, and the index |
| A GOG request or what is known about it | [docs/gog-integration.md](docs/gog-integration.md) |
| An Epic request or what is known about it | [docs/epic-integration.md](docs/epic-integration.md) |
| Cloud sync rules | [docs/cloud-saves.md](docs/cloud-saves.md) |
| A theme token | [docs/theming.md](docs/theming.md) |
| What leaves the computer, how secrets or files are protected | [SECURITY.md](SECURITY.md) |
| A real check against GOG, Steam, Epic or Windows (pass or fail) | [docs/compatibility.md](docs/compatibility.md), and README's summary of it |
| A milestone's status, the known debt | [docs/PLAN.md](docs/PLAN.md) |
| A stage reshaped, merged or dropped | [docs/ROADMAP.md](docs/ROADMAP.md), with an ADR |
| A term the project coins or uses in a sense of its own | [docs/glossary.md](docs/glossary.md) |
| Anything a user of the next version will notice | [CHANGELOG.md](CHANGELOG.md), and README's features |
| A benchmark, a test that needs real tools | [docs/testing.md](docs/testing.md) |

## Code rules

- Code, comments, docs and commit messages are in English.
- Comment only what the code cannot say: a reason, a limit, a source. Doc comments describe
  behaviour in plain sentences ("Whether…", "Starts what downloads next…"). No commented-out code.
- Write the least code that solves the problem: no abstraction without a second concrete use, no
  option nobody asked for, no handling of impossible cases. Match the surrounding code.
- Do not reimplement what the standard library provides without a measurement that justifies it.
- Every `unsafe` block carries a `// SAFETY:` comment explaining why it is sound.
- Follow the conventions the ADRs set. If no ADR covers a case, propose one instead of deciding
  silently.
- Edit Rust with an editor tool, not with `sed` or `perl` substitutions: Rust's `|`, `{}`, `#`
  and `'` make regex edits silently corrupt files.

## Consistency rules

- Before writing a helper, type or pattern, search the workspace for an existing one. Reuse or
  extend it.
- One way to do a given thing. If you find two, unify them or report it.
- Keep changes surgical, local to the milestone. Do not reformat or refactor code you were not asked
  to touch. Remove what your own change leaves unused; mention other dead code instead of deleting
  it, or note it in [docs/PLAN.md](docs/PLAN.md)'s known debt.
- If the code has drifted from [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) or an ADR, report it.
  Do not silently work around it.

## Commits

- Small commits, one change each, with a short imperative subject that says what changes for the
  user ("Build only the covers in view, for libraries of 10,000 games"), and a body explaining why,
  with measurements when the change is about speed.
- `scripts/check.sh` passes before each commit; do not commit code that fails it.

## When to stop and ask

Stop and ask the owner before proceeding when:

- A decision affects the architecture and no ADR covers it.
- You want to add a dependency (see "Things an agent must not do on its own").
- The request contradicts [docs/ROADMAP.md](docs/ROADMAP.md) or an ADR.
- The plan turns out to be wrong, or a milestone is larger than expected.
- You are about to delete or rewrite a significant part of the code.

Propose the options with their trade-offs, in plain terms, and a recommendation for the long term,
not the cheapest now. Put decisions one at a time. Do not work around a problem to keep going.

## Reporting

At the end of each milestone, report to the owner, in French:

- What was done.
- Notable design choices, why, and which alternatives were rejected.
- What deserves to be read first.
- Shortcuts taken and debt introduced.
- What was not verified, and why.
- The next step, on the owner's go.

A reviewing agent checks the milestone against this file and the ADRs before it is reported done.

## Working with several agents

- Each agent works on one milestone at a time, as [docs/PLAN.md](docs/PLAN.md) assigns it.
- Shared files ([docs/PLAN.md](docs/PLAN.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), the
  workspace's `Cargo.toml` and `Cargo.lock`, `flake.nix`) are edited by one agent at a time.
- Commit your own files by their paths, never with `git add -A`, so as not to take another agent's.

## Language

- Everything in the repository is in English: code, comments, commits, documentation.
- Talk to the owner in French.

## Known pitfalls

- **NixOS has no `/bin/bash`.** GOG's Linux `start.sh` scripts need it: native games start
  through `steam-run`, else umu without Proton (`runner::runtime_tool`). Tests that spawn native games
  remove both from `PATH`, because their sandboxes cannot see the test's temporary folder.
- **umu's runtime cannot see `/tmp/nix-shell.*`.** Point `TMPDIR` inside the home folder for
  tests that run Proton.
- **`steam-run` mounts an empty `/tmp` of its own.** A temporary folder under `/tmp` the
  environment names does not exist there (`runner::steam_run_temp` gives `/tmp` instead).
- **The user's environment is not always UTF-8.** Read it with `std::env::vars_os` or `var_os`:
  `std::env::vars` panics on a single variable that is not.
- **umu and its container share more than they are told.** umu shares, writable, the whole
  filesystem under `STEAM_COMPAT_INSTALL_PATH`, which it takes from the environment or from the
  program's folder when the program is a file, relative to the working directory too (a planted
  `sh` or `Z:\…\game.exe` in the game folder counts); pressure-vessel shares the folders
  `TMPDIR`, `TMP`, `TEMP`, `TEMPDIR`, `PROTON_LOG_DIR`, the `STEAM_COMPAT_*` path variables and
  `WINEPREFIX` (as `STEAM_COMPAT_DATA_PATH`) name, the working directory as it is on disk, and
  the Steam installation from `$HOME/.steam`; `PRESSURE_VESSEL_SHARE_HOME=1` in the user's
  environment wins over `PRESSURE_VESSEL_HOME`; its path lists know no escape for `:`. The launch
  inherits the user's whole environment. An isolated launch must keep all of these out
  (`runner::isolation_env`, `runner::check_shared`); check a change with the ignored isolation
  tests, which also run the same launch without the measure, or not isolated, as a control.
- **Comet listens on the fixed port 127.0.0.1:9977.** Another launcher's Comet (Heroic) makes it
  fail; `slatty doctor` reports it.
- **The running binary can be replaced by a rebuild.** The session supervisor is started through
  `/proc/self/exe`, not `current_exe()`.
- **GOG's CDN serves byte ranges but not suffix ranges**, and the API's installer sizes are
  rounded; the exact size comes from `Content-Range`.
- **GOG's content system has no Linux builds** ("Unsupported OS"); Linux builds come from the
  offline installers (`installer::linux`), and their build id is `linux:<version>`.
- **An Iced `stack` takes the size of its first layer**: overlays sit over an empty layer that
  fills the window.
