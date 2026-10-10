# Contributing

Thanks for your interest. SlattyLauncher is young; issues describing what works or fails with a
given game are as valuable as code.

## Development environment

```sh
nix develop      # Rust toolchain, umu-launcher, Comet, graphics libraries
cargo build
cargo test
```

`nix develop` adds `target/debug` to `PATH`.

## Before sending a change

```sh
cargo fmt
cargo clippy --all-targets
cargo test
```

- Keep the core free of user-interface code. Both front ends call `slatty-core`.
- Add tests for behaviour, especially anything that writes or deletes user data.
- Never present a feature as working against GOG on the strength of simulated tests. Add a line to
  [docs/compatibility.md](docs/compatibility.md) only after a real check.
- Update the documentation when behaviour changes: user guide, cloud saves, GOG integration.

## Dependency audit

Check the dependencies against the RustSec advisories (this downloads the advisory database):

```sh
nix shell nixpkgs#cargo-audit -c cargo audit
```

The last audit (2026-10-10) found no vulnerability. Four warnings come with Iced 0.14 and should go
with an Iced update: `paste` (unmaintained; only in macOS's Metal backend, not built on Linux),
`rustybuzz` and `ttf-parser` (unmaintained; text and font reading), and `lru` (unsound only when a
key's `Drop` panics, which the glyph cache's keys never do). Image decoding is limited to the
formats SlattyLauncher shows (JPEG, PNG, WebP, GIF, BMP), so fewer decoders face downloaded files.

## Code style

- Code, comments and commit messages are in English.
- Comment only what the code cannot say: a non-obvious reason, a known limit, a source.
- Prefer small, explicit functions over abstractions written for hypothetical needs.
- User-visible text lives in the front ends. The core returns typed errors and events.

## Secrets and privacy

- Never log or print tokens, codes or URLs that carry them. Use `Secret` for sensitive values, and
  `Error::network`, which strips URLs.
- Never pass secrets as process arguments.
- Test data must be clearly fake (`[FAKE]` titles, made-up ids).

## Reverse-engineered services

When you rely on a GOG behaviour:

- cite where it comes from (a project, a document, a capture you made);
- record its status in [docs/gog-integration.md](docs/gog-integration.md).

SlattyLauncher is GPL-3.0-or-later. Code derived from GPL projects such as heroic-gogdl is welcome,
with a note naming the source. Do not copy code from projects without a license.

## Commits

Small commits with a short imperative subject line, and a body explaining why when it is not obvious.
