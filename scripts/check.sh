#!/usr/bin/env bash
# Runs every automated check of the definition of done (AGENTS.md), in three parts, each in the
# order a failure is cheapest to find:
#   linux    formatting; clippy with warnings as errors; the tests (no network, no real game); the
#            code documentation with warnings as errors; cargo deny (licences, advisories, sources);
#            the links between Markdown files (lychee, offline); then the build, last.
#   windows  every crate checked for Windows in the `windows` shell, then the core's tests under
#            Wine (ADR-0022, ADR-0024).
#   package  the Nix package.
# Before any part, it checks that CI runs every part. With no argument it runs every part in turn:
# what to run before a commit. `scripts/check.sh windows` runs one part. CI runs each part in a job
# of its own, at the same time (.github/workflows/ci.yml, ADR-0026), inside the Nix development
# shell (flake.nix), which provides every tool the script calls:
# `nix develop --no-update-lock-file --command scripts/check.sh <part>`. The dependency step needs
# network access (the advisory database); the tests never do. Tests that need real Proton, Comet or
# bubblewrap, and the benchmarks, are ignored here: docs/testing.md says how to run them.
set -euo pipefail
cd "$(dirname "$0")/.."

parts=(linux windows package)

step() { printf '\n==> %s\n' "$*"; }

linux() {
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

  # Last: tests and lints do not rebuild the binaries people run from target/debug.
  step "build"
  cargo build --workspace --locked
}

windows() {
  step "Windows: every crate builds for Windows (ADR-0022)"
  nix develop .#windows --no-update-lock-file --command cargo check --workspace --locked

  # Without a display, Wine opens no window on the desktop running the tests (docs/testing.md).
  step "Windows: the core's tests under Wine"
  env -u DISPLAY -u WAYLAND_DISPLAY \
    nix develop .#windows --no-update-lock-file --command cargo test -p slatty-core --locked
}

package() {
  step "Nix package"
  nix build .#slatty --no-link --no-update-lock-file
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

# CI lists the parts itself, for its jobs; a part missing there would never run in CI. Checked in
# every part, so that it still runs in CI whichever part is left out. It reads the line, not the
# YAML: a copy of the line elsewhere in the file would satisfy it.
step "CI runs every part"
matrix="part: [$(printf '%s, ' "${parts[@]}" | sed 's/, $//')]"
if ! grep -qF "$matrix" .github/workflows/ci.yml; then
  echo "the matrix of .github/workflows/ci.yml must be: $matrix" >&2
  exit 1
fi

for part in "$@"; do
  "$part"
done

step "all checks passed: $*"
