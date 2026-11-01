#!/usr/bin/env bash
# Says whether linux/Cargo.lock resolves any crate the core also uses to
# another version than core/Cargo.lock does.
#
# The Linux app is its own workspace (Cargo.toml says why), so its lock is
# its own, and nothing stops `cargo update` here from moving a crate the core
# links. The core is tested and shipped on the other platforms with
# core/Cargo.lock; a crate of the core's at another version here is the
# core built differently on Linux. Run after any dependency change; it
# prints each such crate and exits 1, or exits 0 when the two agree. Crates
# only one of the two uses are not compared.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
core=${CORE_LOCK:-$here/../../core/Cargo.lock}
linux=${LINUX_LOCK:-$here/../Cargo.lock}

pairs() { awk '/^name = /{gsub(/"/, "", $3); name=$3} /^version = /{gsub(/"/, "", $3); print name, $3}' "$1" | sort -u; }

differ=$(awk '
  NR == FNR { core[$1] = core[$1] " " $2; next }
  ($1 in core) && index(core[$1] " ", " " $2 " ") == 0 { print $1 ": linux " $2 ", core" core[$1] }
' <(pairs "$core") <(pairs "$linux"))

if [ -n "$differ" ]; then
  echo "linux/Cargo.lock resolves these differently from core/Cargo.lock:"
  echo "$differ"
  exit 1
fi
echo "linux/Cargo.lock agrees with core/Cargo.lock on every crate both use"
