#!/bin/bash
# Saves a picture of one app process's main window, and nothing else on the
# screen: the person's own windows are none of a test's business.
#   interop/app-window.sh <pid> <out.png>
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TOOL="$HERE/.window-of"
if [ ! -x "$TOOL" ]; then
    DEVELOPER_DIR=${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer} xcrun swiftc -O "$HERE/window-of.swift" -o "$TOOL"
fi
ID=$("$TOOL" "$1")
[ -n "$ID" ] || { echo "no window for pid $1" >&2; exit 1; }
screencapture -x -o -l "$ID" "$2"
