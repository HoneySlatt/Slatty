#!/usr/bin/env bash
# Runs every automated check of the definition of done (AGENTS.md), in the order a failure is
# cheapest to find. CI runs this script unchanged, inside the Nix development shell:
# `nix develop --no-update-lock-file --command scripts/check.sh`. The dependency step needs network
# access (the advisory database); the tests never do.
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "format"
cargo fmt --all --check

step "lints"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "tests"
cargo test --workspace --locked

step "code documentation"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

step "dependencies: licences, advisories, sources"
cargo deny --locked check

step "links between Markdown files"
git ls-files -z '*.md' | xargs -0 lychee --offline --include-fragments --no-progress

step "Windows: every crate builds for Windows (ADR-0022)"
nix develop .#windows --no-update-lock-file --command cargo check --workspace --locked

step "Nix package"
nix build .#slatty --no-link --no-update-lock-file

# Last: tests and lints do not rebuild the binaries people run from target/debug.
step "build"
cargo build --workspace --locked

step "all checks passed"
