# Contributing

Thanks for your interest. Slatty is young; an issue saying what works or fails with a given
game is as valuable as code.

If you work with an AI coding agent, point it to [AGENTS.md](AGENTS.md) first: it also holds the
rules the code depends on, which apply to everyone.

## Development environment

```sh
nix develop      # Rust toolchain, umu-launcher, bubblewrap, Comet, graphics libraries, udev
                 # (gamepads), cargo-deny and lychee
cargo build --workspace
nix develop .#windows -c cargo check --workspace    # the Windows build, from Linux
```

`nix develop` adds `target/debug` to `PATH`, so `slatty` and `slatty-gui` are the ones you just
built. [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) is the map of the code. `SLATTY_LOG=warn` (or
`debug`) prints the core's logs on stderr. [docs/testing.md](docs/testing.md) has the tests that
need real tools, the benchmarks, and how to run Slatty on a throwaway profile.

## Before sending a change

```sh
scripts/check.sh                          # every check below and more, as CI runs it
cargo fmt
cargo clippy --workspace --all-targets    # no warning
cargo test --workspace
cargo build --workspace                   # last: tests and clippy do not rebuild the binaries
```

`scripts/check.sh` says at its head what it runs. CI (`.github/workflows/ci.yml`) runs it unchanged
on every push to `main` and every pull request, and every Monday at 06:41 UTC even without a push,
so that new advisories against the dependencies and changes to the CI's environment show while
the project is at rest; a push cancels the run in progress. GitHub turns the weekly run off after
60 days without activity on a public repository: a click in the Actions tab turns it back on.
Build last: the person testing your change runs `target/debug/slatty-gui`.

- Keep behaviour in `slatty-core`, so both front ends share it; user-visible wording lives in the
  front ends.
- Add a test for every behaviour change, and a failing test first for every bug fix. Anything that
  writes or deletes user data needs one. Test data is visibly fake (`[FAKE]` titles, made-up ids).
- Never present a feature as working against GOG on the strength of simulated tests: a line goes
  into [docs/compatibility.md](docs/compatibility.md) only after a real check.
- Update the documentation in the same change ([below](#documentation)); new features go into
  [CHANGELOG.md](CHANGELOG.md).

[docs/testing.md](docs/testing.md) describes the benchmarks, the tests that need real Proton, umu or
Comet, and the checks against GOG and Steam.

## Documentation

Each kind of fact has one home; other documents link to it rather than copy it. Update it in the
same change:

| When you change… | Update |
|---|---|
| What an item does, its contracts, its limits | The code's documentation |
| A module or a flow | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| A decision (a framework, a platform, a format, a security choice, something tried and dropped) | A new ADR in [docs/adr/](docs/adr/README.md), from `0000-template.md`, and the index |
| A milestone's status, the known debt | [docs/PLAN.md](docs/PLAN.md); a finished stage moves to [docs/history.md](docs/history.md) |
| A stage reshaped, merged or dropped, a budget | [docs/ROADMAP.md](docs/ROADMAP.md), with an ADR |
| How something is tested or measured, a test that needs real tools | [docs/testing.md](docs/testing.md) |
| A figure measured | [docs/measurements.md](docs/measurements.md) |
| A real check against GOG, Steam, Epic or Windows (pass or fail) | [docs/compatibility.md](docs/compatibility.md), and README's summary of it |
| A panel, a setting, a stored file | [docs/user-guide.md](docs/user-guide.md) |
| A command or an option | [docs/command-line.md](docs/command-line.md), and the user guide where it shows the command |
| A GOG request or what is known about it | [docs/gog-integration.md](docs/gog-integration.md) |
| An Epic request or what is known about it | [docs/epic-integration.md](docs/epic-integration.md) |
| Cloud sync rules | [docs/cloud-saves.md](docs/cloud-saves.md) |
| A theme token | [docs/theming.md](docs/theming.md) |
| What leaves the computer, how secrets or files are protected | [SECURITY.md](SECURITY.md) |
| A term the project coins or uses in a sense of its own | [docs/glossary.md](docs/glossary.md) |
| Anything a user of the next version will notice | [CHANGELOG.md](CHANGELOG.md), and README's features |

## Sending a change

Open a pull request on GitHub with small commits, each with a short imperative subject that says
what changes for the user, and a body that explains why (with measurements when the change is about
speed). By sending a change you agree to publish it under the project's licence,
GPL-3.0-or-later.

## Dependencies

Each dependency is code that reads downloaded data or runs with the user's rights. Prefer the
standard library or a crate already in `Cargo.lock`; say why a new one is worth it, and check the
dependencies against the RustSec advisories and the policy in `deny.toml` (licences compatible with
the GPL, crates.io as the only source):

```sh
nix shell nixpkgs#cargo-audit -c cargo audit
nix shell nixpkgs#cargo-deny -c cargo deny check
```

Both download the advisory database. The last audit (2026-10-21) found no vulnerability, and
`cargo deny check` passes. Four warnings come with Iced 0.14 and should go with an Iced update:
`paste` (unmaintained; only in macOS's Metal backend, not built on Linux), `rustybuzz` and
`ttf-parser` (unmaintained; text and font reading, ignored in `deny.toml` with the reason), and
`lru` (unsound only when a key's `Drop` panics, which the glyph cache's keys never do). Three
come with zbus 5, used for the keyring and desktop notifications: `zbus_names`, `zvariant` and
`zvariant_derive` (unmaintained since they were merged into zbus 6, which the `keyring` crate
does not use yet; ignored in `deny.toml` with the reason). Image
decoding is limited to the formats Slatty shows (JPEG, PNG, WebP, GIF, BMP), so fewer
decoders face downloaded files.

## Code style

- Code, comments, documentation and commit messages are in English.
- Comment only what the code cannot say: a reason, a known limit, a source.
- Write the least code that solves the problem; prefer small, explicit functions over abstractions
  for needs nobody has yet. Match the surrounding code.

## Secrets and privacy

- Never log or print tokens, codes or addresses that carry them. Use `Secret` for sensitive values,
  and `Error::network`, which strips addresses.
- Never pass secrets as process arguments.
- A new request to a host other than GOG's needs a switch in Settings, off until turned on, and a
  line in [SECURITY.md](SECURITY.md#what-leaves-your-computer). No telemetry.

## Reverse-engineered services

When you rely on a GOG behaviour, cite where it comes from (a project, a document, a capture you
made) and record its status in [docs/gog-integration.md](docs/gog-integration.md).

Code derived from GPL projects such as heroic-gogdl is welcome, with a note naming the source.
heroic-gogdl and Heroic are GPL-3.0-only: what is derived from them is distributed under GPL-3.0.
Do not copy code from projects without a licence.
