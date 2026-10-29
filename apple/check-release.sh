#!/bin/bash
# Refuses a release build of the apps that names any TOMOSHIBI_ switch.
#
#   apple/check-release.sh <Tomoshibi.app or a binary> [...]
#
# Every switch the tests drive the apps by is an environment variable named
# TOMOSHIBI_something, and each belongs in a debug build alone: compiled
# inside #if DEBUG and read only in a rehearsal. The released Mac app once
# compiled and honoured two whole test drivers (TOMOSHIBI_DEBUG_HOST and the
# watch switches) and a handful of loose ones, so that whoever could start it
# could have it invite a stranger, close the room, click its own buttons or
# hear nobody. ReleaseSwitchesTests holds the sources to the rule; this holds
# the binary that ships, where a name compiled in is a switch that may still
# be read. A release built from these sources names none, so any name found
# here is a fault to fix, not one to add to a list.
#
# Every Mach-O file in a bundle is read: the app, and the extensions and
# frameworks inside it.
set -euo pipefail
[ $# -gt 0 ] || { echo "usage: $0 <Tomoshibi.app or binary> [...]" >&2; exit 2; }

found=0
checked=0
for target in "$@"; do
    [ -e "$target" ] || { echo "check-release: no such file: $target" >&2; exit 2; }
    while IFS= read -r -d '' file; do
        file -b "$file" | grep -q '^Mach-O' || continue
        checked=$((checked + 1))
        names=$(strings -a "$file" | grep -oE 'TOMOSHIBI_[A-Z0-9_]+' | sort -u || true)
        if [ -n "$names" ]; then
            found=1
            echo "check-release: $file names switches a release build must not have:" >&2
            echo "$names" | sed 's/^/    /' >&2
        fi
    done < <(find "$target" -type f -print0)
done

[ "$checked" -gt 0 ] || { echo "check-release: no Mach-O file in $*" >&2; exit 2; }
[ "$found" -eq 0 ] || exit 1
echo "check-release: $checked binaries, no TOMOSHIBI_ switch named"
