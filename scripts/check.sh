#!/usr/bin/env bash
# Runs every automated check of the definition of done (AGENTS.md), in four parts, each in the
# order a failure is cheapest to find:
#   linux    formatting; clippy with warnings as errors, the interface's probe included; the
#            tests (no network, no real game); the code documentation with warnings as errors;
#            cargo deny (licences, advisories, sources); then the build, last.
#   windows  every crate checked for Windows in the `windows` shell, then the core's and the
#            command line's tests under Wine (ADR-0022, ADR-0024).
#   package  the Nix package.
#   docs     the links between Markdown files (lychee, offline); AGENTS.md within 1,500 words and
#            docs/PLAN.md within 2,000. A push to main that changes only Markdown files runs this
#            part alone in CI (ADR-0033).
# Before any part, it checks that CI runs every part. With no argument it runs every part in turn:
# what to run before a commit. `scripts/check.sh windows` runs one part. CI runs each part in a job
# of its own, at the same time (.github/workflows/ci.yml, ADR-0026), inside the Nix development
# shell (flake.nix), which provides every tool the script calls:
# `nix develop --no-update-lock-file --command scripts/check.sh <part>`. The dependency step needs
# network access (the advisory database); the tests never do. Tests that need real Proton, Comet or
# bubblewrap, and the benchmarks, are ignored here: docs/testing.md says how to run them.
set -euo pipefail
cd "$(dirname "$0")/.."

# The parts that build the code, one job each in CI's matrix; `docs` has a job of its own.
code_parts=(linux windows package)
parts=("${code_parts[@]}" docs)

step() { printf '\n==> %s\n' "$*"; }

linux() {
  step "format"
  cargo fmt --all --check

  step "lints"
  cargo clippy --workspace --all-targets --locked -- -D warnings
  # The measuring probe, off by default (docs/testing.md): linted so that it keeps building.
  cargo clippy -p slatty-gui --all-targets --features probe --locked -- -D warnings

  step "tests"
  cargo test --workspace --locked

  step "code documentation"
  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

  step "dependencies: licences, advisories, sources"
  cargo deny --locked check

  # Last: tests and lints do not rebuild the binaries people run from target/debug.
  step "build"
  cargo build --workspace --locked
}

windows() {
  step "Windows: every crate builds for Windows (ADR-0022)"
  nix develop .#windows --no-update-lock-file --command cargo check --workspace --locked

  # Without a display, Wine opens no window on the desktop running the tests (docs/testing.md).
  step "Windows: the core's and the command line's tests under Wine"
  env -u DISPLAY -u WAYLAND_DISPLAY \
    nix develop .#windows --no-update-lock-file --command \
    cargo test -p slatty-core -p slatty-cli --locked
}

package() {
  step "Nix package"
  nix build .#slatty --no-link --no-update-lock-file
}

docs() {
  step "links between Markdown files"
  git ls-files -z '*.md' | xargs -0 lychee --offline --include-fragments --no-progress

  # Every agent reads these two files at the start of every session, so their length is paid
  # each time.
  step "length of the files read at every session"
  words_at_most AGENTS.md 1500
  words_at_most docs/PLAN.md 2000
}

# Fails when a file holds more words than its limit.
words_at_most() {
  local words
  words=$(wc -w < "$1")
  if [[ $words -gt $2 ]]; then
    echo "$1 has $words words, over its limit of $2: every agent reads it at each session" \
      "(CONTRIBUTING.md, Before sending a change)." >&2
    exit 1
  fi
}

if [[ $# -eq 0 ]]; then
  set -- "${parts[@]}"
fi
# Every name is checked before any part runs, so that a mistyped one fails at once.
for part in "$@"; do
  if [[ " ${parts[*]} " != *" $part "* ]]; then
    echo "unknown part: $part (the parts: ${parts[*]})" >&2
    exit 2
  fi
done

# CI lists the code parts itself, for its matrix, and runs `docs` in a job of its own; a part
# missing there would never run in CI. Checked in every part, so that it still runs in CI whichever
# part is left out. It reads lines, not the YAML: a copy of a line elsewhere in the file would
# satisfy it.
step "CI runs every part"
matrix="part: [$(printf '%s, ' "${code_parts[@]}" | sed 's/, $//')]"
if ! grep -qF "$matrix" .github/workflows/ci.yml; then
  echo "the matrix of .github/workflows/ci.yml must be: $matrix" >&2
  exit 1
fi
if ! grep -qF "scripts/check.sh docs" .github/workflows/ci.yml; then
  echo ".github/workflows/ci.yml must run \`scripts/check.sh docs\` in a job of its own" >&2
  exit 1
fi

for part in "$@"; do
  "$part"
done

step "all checks passed: $*"
