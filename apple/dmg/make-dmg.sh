#!/bin/bash
# The Mac app as a disk image to drag it from: Tomoshibi.app beside a link to
# Applications, over a background that says to drag one onto the other, in a
# window laid out for it, compressed and, given an identity, signed.
#
#   apple/dmg/make-dmg.sh <Tomoshibi.app> <out.dmg> ["Developer ID Application: …"]
#
# The window's layout is Finder's to write (.DS_Store), so Finder is asked to
# arrange it; the first time, macOS asks whether this may control Finder. If
# that is refused the image still works, with Finder's own arrangement.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
APP=$1
OUT=$2
IDENTITY=${3:-}
NAME=Tomoshibi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
STAGE="$WORK/stage"
mkdir -p "$STAGE/.background"
ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"
swift "$HERE/background.swift" "$STAGE/.background/background.png"

RW="$WORK/rw.dmg"
hdiutil create -quiet -srcfolder "$STAGE" -volname "$NAME" -fs HFS+ -format UDRW -ov "$RW"
ATTACHED=$(hdiutil attach -readwrite -noverify -noautoopen "$RW")
DEVICE=$(echo "$ATTACHED" | grep -E '^/dev/' | head -1 | awk '{print $1}')
MOUNT=$(echo "$ATTACHED" | grep -o '/Volumes/.*' | head -1)
DISK=$(basename "$MOUNT")

osascript <<EOS || echo "Finder would not arrange the window; the image keeps Finder's own layout" >&2
tell application "Finder"
    tell disk "$DISK"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {200, 120, 840, 548}
        set options to the icon view options of container window
        set arrangement of options to not arranged
        set icon size of options to 112
        set text size of options to 13
        set background picture of options to file ".background:background.png"
        set extension hidden of item "$NAME.app" to true
        set position of item "$NAME.app" of container window to {170, 190}
        set position of item "Applications" of container window to {470, 190}
        close
        open
        update without registering applications
        delay 1
        close
    end tell
end tell
EOS

chmod -Rf go-w "$MOUNT" || true
sync
hdiutil detach -quiet "$DEVICE" || { sleep 2; hdiutil detach -force -quiet "$DEVICE"; }
rm -f "$OUT"
hdiutil convert -quiet "$RW" -format UDZO -imagekey zlib-level=9 -o "$OUT"
if [ -n "$IDENTITY" ]; then
    codesign --force --sign "$IDENTITY" --timestamp "$OUT"
fi
echo "made $OUT"
