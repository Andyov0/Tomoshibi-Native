#!/bin/bash
# Refuses an app whose core is not one to hand anybody, and says why.
#
#   apple/dmg/check-core.sh <Tomoshibi.app | executable>
#
# build-core.sh builds a debug core unless told `release`, and a release app
# around a debug or `local` core built and ran as well as any: the only
# difference was that the core admitted whatever deployment a link named
# (protocol api::admitted keys on debug assertions) and printed its relay
# timings. A checkout with no deployment.local builds for meet.example.com and
# reaches nothing, and that built cleanly too; and a core with no update key
# installs whatever build the deployment hands it. The core writes what it is
# into itself (protocol api BUILD_MARK: profile, host, update key), and this
# reads it out of the executable the core was linked into. Where this
# checkout names a host or an update key (TOMOSHIBI_HOST and
# TOMOSHIBI_UPDATE_KEY in the environment, else in deployment.local), the
# core must have been built with the same.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT="$HERE/../.."
TARGET=${1:?usage: check-core.sh <app or executable>}

if [ -f "$TARGET/Contents/Info.plist" ]; then
    BIN="$TARGET/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$TARGET/Contents/Info.plist")"
elif [ -f "$TARGET/Info.plist" ]; then
    BIN="$TARGET/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$TARGET/Info.plist")"
else
    BIN=$TARGET
fi
[ -f "$BIN" ] || { echo "check-core: no executable at $BIN" >&2; exit 1; }

refuse() { echo "check-core: $*" >&2; exit 1; }

MARKS=$(LC_ALL=C grep -a -o 'tomoshibi-core-build:[a-z]*:[a-z0-9.-]*:[0-9a-f]*;' "$BIN" | sort -u || true)
COUNT=$(printf '%s' "$MARKS" | grep -c . || true)
[ "$COUNT" -eq 1 ] || refuse "$BIN carries $COUNT core build marks, not one; is the core in it older than the mark, or are two cores linked?"

PROFILE=$(printf '%s' "$MARKS" | cut -d: -f2)
BUILT_FOR=$(printf '%s' "$MARKS" | cut -d: -f3)
KEY=$(printf '%s' "$MARKS" | cut -d: -f4 | tr -d ';')

[ "$PROFILE" = release ] || refuse "the core in $BIN is a $PROFILE build, which admits any deployment; run apple/build-core.sh release and build again"
[ "$BUILT_FOR" != meet.example.com ] || refuse "the core in $BIN is built for meet.example.com, which reaches nothing; set TOMOSHIBI_HOST or deployment.local (see deployment.example) and build again"
[ ${#KEY} -eq 64 ] || refuse "the core in $BIN has no update key and would install any build the deployment hands it; set TOMOSHIBI_UPDATE_KEY (see deployment.example) and build again"

# What this checkout says, as the core's build reads it: the environment
# first, then deployment.local, `// comments` allowed.
said() {
    local value=${!1:-}
    if [ -z "$value" ] && [ -f "$ROOT/deployment.local" ]; then
        value=$(sed -e 's#//.*##' "$ROOT/deployment.local" | awk -F= -v name="$1" '{gsub(/^[ \t]+|[ \t]+$/, "", $1)} $1 == name {v = $2; gsub(/^[ \t]+|[ \t]+$/, "", v); print v; exit}')
    fi
    printf '%s' "$value" | tr '[:upper:]' '[:lower:]'
}
WANT_HOST=$(said TOMOSHIBI_HOST)
WANT_KEY=$(said TOMOSHIBI_UPDATE_KEY)
[ -z "$WANT_HOST" ] || [ "$WANT_HOST" = "$BUILT_FOR" ] || refuse "the core in $BIN is built for $BUILT_FOR, and this checkout is for $WANT_HOST"
[ -z "$WANT_KEY" ] || [ "$WANT_KEY" = "$KEY" ] || refuse "the core in $BIN holds updates to another key than this checkout's TOMOSHIBI_UPDATE_KEY"

echo "core: release, for $BUILT_FOR, updates held to its key"
