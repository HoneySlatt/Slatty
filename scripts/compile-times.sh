#!/usr/bin/env bash
# Times Slatty's builds in debug, the floor that uwui's budgets of compiling are compared with
# (ADR-0036), after SlimeEngine's scripts/compile-times.sh:
#   - the workspace from nothing;
#   - a function's body changed in the interface (slatty-gui), in its state (slatty-app) and in the
#     core (slatty-core), then a public function added to the core, each followed by
#     `cargo build -p slatty-gui`, what a person runs to see a change;
#   - the interface and the command line in release from nothing.
#
#     nix develop -c scripts/compile-times.sh [--runs <count>] [--cold-runs <count>]
#
# It builds a copy of the files git tracks, as they are now, in a temporary folder removed at the
# end: the repository's own `target` is not touched. rustc reuses its work when only a file's date
# changes, so each change is a real one: a statement `std::hint::black_box(<number>)` written at
# the start of a function, or a new public function. Each step starts from a workspace built
# again, untimed. `--runs` changes, 7 by default, and `--cold-runs` builds from nothing, 3; the
# release build runs once. The report, on standard output, gives each step's median and range, the
# packages cargo compiled and, for each run kept, the cores the rest of the machine used meanwhile.
#
# Another program's work would lengthen a build, and agents' sessions build and test on the same
# machine. So the script runs in a systemd scope of its own, whose processor time the kernel counts
# even for processes that have ended; the rest of the machine's is the total less the scope's. A
# step waits for the rest to use under 0.75 of a core over 30 s, and a run during which it used
# over 1.5 cores in 2 s is thrown away and made again, with a new change, up to 12 times. Stopping
# the scope stops everything it started: `systemctl --user stop slatty-compile-times.scope`.
#
# The functions changed are named by their first line below; a change that moves or renames one
# makes the script stop with the file's name, and the line here is updated.
set -euo pipefail
script=$(realpath "$0")
cd "$(dirname "$script")/.."

runs=7
cold_runs=3
usage="usage: scripts/compile-times.sh [--runs <count>] [--cold-runs <count>]"
while [ $# -gt 0 ]; do
  case $1 in
    --runs) runs=${2:?--runs needs a count}; shift ;;
    --cold-runs) cold_runs=${2:?--cold-runs needs a count}; shift ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
  shift
done
for count in "$runs" "$cold_runs"; do
  [[ $count =~ ^[1-9][0-9]*$ ]] || { echo "$usage" >&2; exit 2; }
done

cgroup=/sys/fs/cgroup$(sed -n 's/^0:://p' /proc/self/cgroup)
if [[ $cgroup != */slatty-compile-times.scope ]]; then
  exec systemd-run --user --scope --quiet --unit=slatty-compile-times -- \
    "$script" --runs "$runs" --cold-runs "$cold_runs"
fi

repo=$(pwd)
commit=$(git rev-parse --short HEAD)
git diff --quiet HEAD -- || commit="$commit, with the working tree's changes"
work=$(mktemp -d "${TMPDIR:-/tmp}/slatty-compile-times.XXXXXX")
trap 'rm -rf "$work"' EXIT
copy=$work/repo
results=$work/results
mkdir "$copy"
git ls-files -z | tar --null --files-from=- -cf - | tar -xf - -C "$copy"
cd "$copy"
# What cargo builds, and where, is the copy's own.
export CARGO_TARGET_DIR=$copy/target
unset CARGO_BUILD_TARGET_DIR RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER CARGO_BUILD_RUSTC_WRAPPER \
  CARGO_INCREMENTAL
ticks=$(getconf CLK_TCK)

# The processor time of the whole machine, in microseconds.
machine_usec() {
  awk -v hz="$ticks" '$1 == "cpu" { printf "%d", ($2 + $3 + $4 + $7 + $8 + $9) * 1000000 / hz }' \
    /proc/stat
}
# The processor time of this script's scope, in microseconds.
scope_usec() { awk '$1 == "usage_usec" { print $2 }' "$cgroup/cpu.stat"; }
# The cores the rest of the machine used over the next `seconds`, in hundredths.
others_load() {
  local m0 s0 m1 s1
  m0=$(machine_usec); s0=$(scope_usec); sleep "$1"; m1=$(machine_usec); s1=$(scope_usec)
  echo $(( ((m1 - m0) - (s1 - s0)) / ($1 * 10000) ))
}
# Waits until the rest of the machine used under 0.75 of a core over 30 s.
wait_quiet() {
  until [ "$(others_load 30)" -lt 75 ]; do :; done
}
# Builds the whole workspace, untimed, so that a step pays only for its own change.
catch_up() {
  [ ! -d "$CARGO_TARGET_DIR" ] || cargo build --locked --workspace >/dev/null 2>&1 ||
    cargo build --locked --workspace
}

counter=0
# Times `cargo build --locked <arguments>` after `prepare <number>`, the rest of the machine quiet
# before and throughout; a run it disturbed is made again with a new number.
step() {
  local label=$1 prepare=$2 tries=0 disturbed=$work/disturbed watcher output start end m0 s0 m1 s1
  shift 2
  while :; do
    tries=$((tries + 1))
    if [ $tries -gt 12 ]; then
      echo "compile-times: $label: disturbed 12 times, given up" >&2
      exit 1
    fi
    catch_up
    wait_quiet
    counter=$((counter + 1))
    "$prepare" "$counter"
    rm -f "$disturbed"
    ( while :; do [ "$(others_load 2)" -lt 150 ] || touch "$disturbed"; done ) &
    watcher=$!
    echo "compile-times: $label" >&2
    m0=$(machine_usec); s0=$(scope_usec); start=$(date +%s.%N)
    if ! output=$(cargo build --locked "$@" 2>&1); then
      kill "$watcher"
      echo "$output" >&2
      exit 1
    fi
    end=$(date +%s.%N); m1=$(machine_usec); s1=$(scope_usec)
    kill "$watcher"; wait "$watcher" 2>/dev/null || true
    if [ -e "$disturbed" ]; then
      echo "compile-times: $label: other work on the machine, made again" >&2
      continue
    fi
    printf '%s\t%s\t%s\t%s\n' "$label" \
      "$(awk "BEGIN { printf \"%.2f\", $end - $start }")" \
      "$(grep -c '^ *Compiling ' <<<"$output" || true)" \
      "$(awk "BEGIN { printf \"%.2f\", (($m1 - $m0) - ($s1 - $s0)) / 1000000 / ($end - $start) }")" \
      >>"$results"
    return
  done
}

# Writes `std::hint::black_box(<number>);` as the first statement of the function whose first line
# is `anchor`, or changes the number it gives.
change_body() {
  local file=$1 anchor=$2 number=$3
  if grep -q '// compile-times$' "$file"; then
    sed -i "s|std::hint::black_box([0-9]*); // compile-times$|std::hint::black_box($number); // compile-times|" "$file"
  else
    grep -qxF "$anchor" "$file" || { echo "compile-times: no such function in $file" >&2; exit 1; }
    awk -v anchor="$anchor" -v number="$number" \
      '{ print } $0 == anchor { print "        std::hint::black_box(" number "); // compile-times" }' \
      "$file" >"$file.new"
    mv "$file.new" "$file"
  fi
}
gui_changed() {
  change_body crates/gui/src/ui/library.rs \
    "    pub(super) fn library_page(&self) -> Element<'_, Message> {" "$1"
}
app_changed() {
  change_body crates/app/src/library.rs \
    "    pub fn selected_game(&self) -> Option<&LibraryGame> {" "$1"
}
core_changed() {
  change_body crates/core/src/db.rs "    pub fn open(path: &Path) -> Result<Self> {" "$1"
}
core_function_added() {
  printf '\n/// Added by scripts/compile-times.sh.\npub fn compile_times_%s() -> u32 {\n    %s\n}\n' \
    "$1" "$1" >>crates/core/src/db.rs
}
from_nothing() { rm -rf "$CARGO_TARGET_DIR"; }
# From nothing, and the sources as they were copied.
pristine() {
  from_nothing
  git -C "$repo" ls-files -z | (cd "$repo" && tar --null --files-from=- -cf -) | tar -xf - -C "$copy"
}

started=$(date -u +%FT%TZ)
for _ in $(seq "$cold_runs"); do
  step "the workspace from nothing" from_nothing --workspace
done
for _ in $(seq "$runs"); do
  step "the interface (slatty-gui) changed" gui_changed -p slatty-gui
  step "the interface's state (slatty-app) changed" app_changed -p slatty-gui
  step "the core (slatty-core) changed" core_changed -p slatty-gui
  step "a public function added to the core" core_function_added -p slatty-gui
done
step "release from nothing (slatty-gui, slatty-cli)" pristine --release -p slatty-gui -p slatty-cli

echo "Commit $commit; $(rustc --version); $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //'), $(nproc) threads; debug builds unless said; from $started to $(date -u +%FT%TZ)."
echo
echo "| Step | Median (range), s | Packages compiled | Cores used elsewhere, each run |"
echo "|---|---|---|---|"
awk -F'\t' '
  !($1 in seen) { seen[$1] = 1; order[++steps] = $1 }
  { times[$1] = times[$1] " " $2; packages[$1] = $3; others[$1] = others[$1] " " $4 }
  END {
    for (j = 1; j <= steps; j++) {
      s = order[j]; n = split(times[s], v, " "); asort(v)
      median = (n % 2) ? v[(n + 1) / 2] : (v[n / 2] + v[n / 2 + 1]) / 2
      printf "| %s | %.2f (%.2f–%.2f), %d runs | %s |%s |\n", s, median, v[1], v[n], n, packages[s], others[s]
    }
  }' "$results"
