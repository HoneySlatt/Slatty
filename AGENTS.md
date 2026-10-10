# AGENTS.md

Instructions for AI coding agents (and the people driving them) working on SlattyLauncher. Read
this file before changing anything. It does not replace [CONTRIBUTING.md](CONTRIBUTING.md), which
applies to every change; it adds what an agent most often gets wrong in this repository.

## The project in one paragraph

SlattyLauncher is a native GOG launcher for Linux, in Rust (edition 2024, Rust 1.90+) with an
[Iced](https://iced.rs) 0.14 interface. It signs in to GOG, installs Windows builds from GOG's
Galaxy content system and native Linux builds from GOG's offline installers, runs games through
umu and Proton (or natively), syncs cloud saves, and reports achievements through Comet. GOG
publishes no API for this: every service was understood by reverse engineering. The launcher
handles people's accounts, saves and disks, so **safety comes before features**.

Read before coding:

1. [docs/architecture.md](docs/architecture.md): crates, modules, main flows, design decisions.
2. [docs/roadmap.md](docs/roadmap.md): what is done, planned and out of scope.
3. [docs/compatibility.md](docs/compatibility.md): what was checked against real GOG services.
4. For cloud saves, [docs/cloud-saves.md](docs/cloud-saves.md) before touching `core/src/cloud`.

## Commands

Everything runs inside the Nix development shell, which provides the toolchain, umu-launcher,
Comet, `GalaxyCommunication.exe` and the graphics libraries:

```sh
nix develop -c cargo build --workspace              # always last: people run target/debug/*
nix develop -c cargo fmt
nix develop -c cargo clippy --workspace --all-targets   # must stay warning-free
nix develop -c cargo test --workspace               # about 15 s, no network, no real games
```

Run `cargo build --workspace` after the tests and clippy: they do not rebuild the `slatty-gui`
binary, and the person testing your change runs `target/debug/slatty-gui`.

More checks, run when your change touches their area:

```sh
# Interface snapshots (PNG) of the tested screens
SLATTY_SNAPSHOT_DIR=/tmp/slatty-ui cargo test -p slatty-gui

# Library performance at 10,000 games (see "Performance" below)
cargo test --release -p slatty-gui library_at_10000_games -- --ignored --nocapture

# Real Proton and real Comet (ignored by default; see docs/testing.md)
TMPDIR=$HOME/.cache SLATTY_TEST_PROTON=<Proton dir> cargo test -- --ignored proton prefix galaxy_service
cargo test -- --ignored comet

# Dependency audit, after adding or updating a dependency (downloads the advisory database)
nix shell nixpkgs#cargo-audit -c cargo audit
```

`SLATTY_LOG=warn` (or `debug`) prints the core's logs on stderr.

## Things an agent must not do on its own

These act on a real person's account, disk or system. Ask the human first, every time:

- **Run commands against GOG with their account.** `slatty install` downloads gigabytes,
  `slatty cloud sync` writes saves on both sides, `slatty achievements --unlock/--clear` changes
  a public profile, and `slatty launch` reports play time. Read-only commands
  (`slatty cloud status`, `slatty install --info`, `slatty update --check`, `slatty library list`)
  are the safe way to check a change.
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
- Besides GOG and the download servers it names, the only services contacted are umu's game
  database (can be turned off) and SteamGridDB (off until turned on). A new outgoing request to
  another host needs a switch in Settings and an entry in SECURITY.md ("What leaves your
  computer").
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
- A game id names files (locks, records, prefixes, covers): check it with `paths::check_game_id`
  before building a path from it. `lock::game`, install records, jobs and covers already do.
- A destructive action either refuses when the situation looks wrong, or keeps a copy first. Only
  files SlattyLauncher installed (its install record) are ever deleted from a game folder.
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
crates/gui    slatty-gui   Iced interface
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

The interface follows Iced's state / message / update / view split:

- `main.rs` holds `App` (the state), the `Message` enum and navigation. `update` only dispatches.
- Each feature (`install`, `downloads`, `library`, `settings`, `maintenance`, `cloud`, `play`,
  `edit`, …) keeps its state, its own message enum (`InstallMsg`, `DownloadsMsg`, …), an
  `impl App { fn update_<feature>(…) }` block and its background tasks in its own module.
- Views live under `ui/`, one module per page or panel. Shared pieces are in `ui/widgets.rs`,
  `ui/panels.rs` (drawers and library dialogs) and `ui/format.rs` (text shown to the user).
- Nothing slow runs on the interface thread: use `Task::perform`, `tokio::task::spawn_blocking`
  for file and CPU work, and `work::progress_stream` for progress (throttled to four updates a
  second). `boot.rs` reads everything the first page needs, off the interface thread.
- **Views never name a colour.** Every colour, radius and motion value comes from
  `theme::tokens()`, and widget styles are functions in `theme.rs`. A new token goes into
  `Tokens`, the theme file format and docs/theming.md.
- Icons are Lucide SVGs in `crates/gui/assets/icons`, added to the `Icon` enum in `icons.rs`.
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
once (sort keys are kept by title in `library::SortKeys`). Images are shown from their cached
files (`image::Handle::from_path`, files named after their format): never keep image bytes in
`App`, which held about 880 MB of covers at 10,000 games.

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
- CLI tests in `crates/cli/tests` spawn real processes. Ignored tests need real Proton or Comet
  and are run on purpose.

## Documentation

The public documentation is in English and must match the code. Update it in the same change:

| When you change… | Update |
|---|---|
| A command, a panel, a setting, a stored file | [docs/user-guide.md](docs/user-guide.md) |
| A module, a flow, a design decision | [docs/architecture.md](docs/architecture.md) |
| A GOG request or what is known about it | [docs/gog-integration.md](docs/gog-integration.md) |
| Cloud sync rules | [docs/cloud-saves.md](docs/cloud-saves.md) |
| A theme token | [docs/theming.md](docs/theming.md) |
| What leaves the computer, how secrets or files are protected | [SECURITY.md](SECURITY.md) |
| A real check against GOG (pass or fail) | [docs/compatibility.md](docs/compatibility.md) |
| A finished or newly planned milestone | [docs/roadmap.md](docs/roadmap.md), README's feature table |

## Style

- Code, comments, docs and commit messages are in English.
- Comment only what the code cannot say: a reason, a limit, a source. Doc comments describe
  behaviour in plain sentences ("Whether…", "Starts what downloads next…").
- Write the least code that solves the problem: no abstraction for a single use, no option nobody
  asked for, no handling of impossible cases. Match the surrounding code.
- Keep changes surgical. Do not reformat or refactor code you were not asked to touch; mention
  dead code instead of deleting it.
- Edit Rust with an editor tool, not with `sed` or `perl` substitutions: Rust's `|`, `{}`, `#`
  and `'` make regex edits silently corrupt files.

## Commits

- Small commits, one change each, with a short imperative subject that says what changes for the
  user ("Build only the covers in view, for libraries of 10,000 games"), and a body explaining why,
  with measurements when the change is about speed.
- Say when a commit was written with an AI agent, for example with a `Co-Authored-By:` trailer.
- `cargo fmt`, clippy and the tests pass before each commit.

## Known pitfalls

- **NixOS has no `/bin/bash`.** GOG's Linux `start.sh` scripts need it: native games start
  through `steam-run`, else umu without Proton (`UMU_NO_PROTON=1`). Tests that spawn native games
  remove both from `PATH`, because their sandboxes cannot see the test's temporary folder.
- **umu's runtime cannot see `/tmp/nix-shell.*`.** Point `TMPDIR` inside the home folder for
  tests that run Proton.
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
