#!/usr/bin/env bash
# Says whether linux/Cargo.lock still holds every version of every crate
# that core/Cargo.lock does.
#
# The Linux app is its own workspace (Cargo.toml says why), so its lock is
# its own, and nothing stops `cargo update` here from moving a crate the core
# links. The core is tested and shipped on the other platforms with
# core/Cargo.lock; a crate of the core's at another version here is the
# core built differently on Linux. Run after any dependency change; it
# prints each version of the core's that this lock lacks and exits 1, or
# exits 0 when every one of them is here.
#
# A crate the app's own dependencies pull in at an older or newer major
# version beside the core's (bitflags 1 for bindgen 0.65, rustix 0.38 for
# v4l, nom 8 for libspa) is listed but is not a difference: cargo keeps one
# version per semver-compatible range, so the core's dependents still get
# exactly the version they got in the core's own lock, and the other one is
# a separate crate that only the app's side links. Crates only one of the
# two uses are not compared.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
core=${CORE_LOCK:-$here/../../core/Cargo.lock}
linux=${LINUX_LOCK:-$here/../Cargo.lock}

pairs() { awk '/^name = /{gsub(/"/, "", $3); name=$3} /^version = /{gsub(/"/, "", $3); print name, $3}' "$1" | sort -u; }

# Every core version of a crate the app's lock has under that name, and
# missing from the app's lock: the core moved.
missing=$(awk '
  NR == FNR { linux[$1] = linux[$1] " " $2; next }
  ($1 in linux) && index(linux[$1] " ", " " $2 " ") == 0 { print $1 ": core " $2 ", linux" linux[$1] }
' <(pairs "$linux") <(pairs "$core"))

# Versions only the app's side links, beside the core's.
extra=$(awk '
  NR == FNR { core[$1] = core[$1] " " $2; next }
  ($1 in core) && index(core[$1] " ", " " $2 " ") == 0 { print $1 " " $2 " beside the core'\''s" core[$1] }
' <(pairs "$core") <(pairs "$linux"))

if [ -n "$extra" ]; then
  echo "linux/Cargo.lock also holds, for the app's own dependencies:"
  echo "$extra" | sed 's/^/  /'
fi
if [ -n "$missing" ]; then
  echo "linux/Cargo.lock resolves these of the core's differently from core/Cargo.lock:"
  echo "$missing"
  exit 1
fi
echo "linux/Cargo.lock holds every version core/Cargo.lock does of the crates both use"
