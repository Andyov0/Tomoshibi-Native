#!/usr/bin/env bash
# Refuses a Linux app whose core is not one to hand anybody, and says why.
#
#   linux/build/check-core.sh <executable>          a build to give out
#   linux/build/check-core.sh --test <executable>   a build for the tests
#
# The Linux counterpart of apple/dmg/check-core.sh and windows/check-core.ps1,
# for the same faults. A release app around a debug or `local` core builds
# and runs as well as any, and the only difference is that its core admits
# whatever deployment a link names (protocol api::admitted keys on debug
# assertions) and writes its relay timings out; a build for meet.example.com
# reaches nothing and builds cleanly too; and a core with no update key
# installs whatever build the deployment hands it, which on Linux means
# whatever file it hands over is put in the AppImage's place and run. The
# core writes what it is into itself (protocol api BUILD_MARK: the profile,
# the host and the update key), and this reads it out of the executable.
#
# A build to give out must say release, a real host and a key; where
# TOMOSHIBI_HOST and TOMOSHIBI_UPDATE_KEY are in the environment (what
# appimage.sh built it with), the core must have been built with the same.
# deployment.local is not read here, as build.sh does not read it either: a
# release is made with both given on the line, from wherever the release
# keeps them.
#
# A build for the tests (--test) is held to the other side: it must be for
# meet.example.com, so that a test image, which admits any deployment, can
# never be one that opens on a real deployment by itself. Its update key is
# whatever the test gave it (none, or a key of the tests' own).
set -euo pipefail

test_build=false
if [ "${1:-}" = --test ]; then test_build=true; shift; fi
BIN=${1:?usage: check-core.sh [--test] <executable>}
[ -f "$BIN" ] || { echo "check-core: no executable at $BIN" >&2; exit 1; }

refuse() { echo "check-core: $*" >&2; exit 1; }

MARKS=$(LC_ALL=C grep -a -o 'tomoshibi-core-build:[a-z]*:[a-z0-9.-]*:[0-9a-f]*;' "$BIN" | sort -u || true)
COUNT=$(printf '%s' "$MARKS" | grep -c . || true)
[ "$COUNT" -eq 1 ] || refuse "$BIN carries $COUNT core build marks, not one; is the core in it older than the mark, or are two cores linked?"

PROFILE=$(printf '%s' "$MARKS" | cut -d: -f2)
BUILT_FOR=$(printf '%s' "$MARKS" | cut -d: -f3)
KEY=$(printf '%s' "$MARKS" | cut -d: -f4 | tr -d ';')

if $test_build; then
    [ "$BUILT_FOR" = meet.example.com ] || refuse "the core in $BIN is built for $BUILT_FOR; a test image must be for meet.example.com, which reaches nothing (build.sh debug builds are)"
    if [ ${#KEY} -eq 64 ]; then held="updates held to a key of the tests' own"; else held="no update key"; fi
    echo "core: $PROFILE, for $BUILT_FOR, $held (a test image)"
    exit 0
fi

[ "$PROFILE" = release ] || refuse "the core in $BIN is a $PROFILE build, which admits any deployment; build it with build.sh release"
[ "$BUILT_FOR" != meet.example.com ] || refuse "the core in $BIN is built for meet.example.com, which reaches nothing; give TOMOSHIBI_HOST and build again"
[ ${#KEY} -eq 64 ] || refuse "the core in $BIN has no update key and would install any build the deployment hands it; give TOMOSHIBI_UPDATE_KEY and build again"

WANT_HOST=$(printf '%s' "${TOMOSHIBI_HOST:-}" | tr '[:upper:]' '[:lower:]')
WANT_KEY=$(printf '%s' "${TOMOSHIBI_UPDATE_KEY:-}" | tr '[:upper:]' '[:lower:]')
[ -z "$WANT_HOST" ] || [ "$WANT_HOST" = "$BUILT_FOR" ] || refuse "the core in $BIN is built for $BUILT_FOR, and this release is for $WANT_HOST"
[ -z "$WANT_KEY" ] || [ "$WANT_KEY" = "$KEY" ] || refuse "the core in $BIN holds updates to another key than the TOMOSHIBI_UPDATE_KEY given"

echo "core: release, for $BUILT_FOR, updates held to its key"
