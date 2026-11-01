#!/usr/bin/env bash
# The inside of distro-check.sh, on a session bus of its own
# (dbus-run-session). Everything it starts is its own child and stops with
# the container.
#
# With UPDATE_TO set (update-check.sh), the app, told by
# TOMOSHIBI_DEBUG_UPDATE=install to take the update its deployment offers,
# downloads it, puts it in its own place and starts it; this waits for the
# new version to say it has started and has removed the image it replaced,
# takes the new window's picture, and checks that the image in place is,
# byte for byte, the build offered (UPDATE_TO is its SHA-256).
#
# With TOMOSHIBI_DEBUG_DESKTOP=add, it also checks the desktop entry the
# start wrote.
set -euo pipefail

export XDG_RUNTIME_DIR=/tmp/runtime
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
export GTK_A11Y=none NO_AT_BRIDGE=1
log=/out/$LABEL
rm -f "$log".*

say() { echo "$*" | tee -a "$log.check.txt"; }

. /etc/os-release
say "system: $PRETTY_NAME, $(uname -m), $(ldd --version 2>&1 | head -1)"

# Nothing of GTK 4 or libadwaita on the system: what draws the window is
# what the image brought.
found=$( { ldconfig -p | grep -E 'libgtk-4\.so|libadwaita-1\.so' || true; find / -xdev \( -name 'libgtk-4.so*' -o -name 'libadwaita-1.so*' \) 2>/dev/null || true; } )
if [ -n "$found" ]; then
  say "the system has GTK 4 or libadwaita of its own:"; say "$found"; exit 1
fi
say "the system has no GTK 4 and no libadwaita"

weston --backend=headless --renderer=pixman --shell=kiosk --debug --idle-time=0 \
  --width="$WIDTH" --height="$HEIGHT" --socket=wayland-1 >"$log.weston.log" 2>&1 &
weston_pid=$!
for _ in $(seq 1 100); do [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && break; sleep 0.1; done
[ -S "$XDG_RUNTIME_DIR/wayland-1" ] || { say "weston did not start; see $LABEL.weston.log"; exit 1; }
export WAYLAND_DISPLAY=wayland-1

# Where somebody keeps an AppImage they downloaded, and where one that
# replaces itself can.
mkdir -p "$HOME/Applications"
app_image=$HOME/Applications/$(basename "$IMAGE")
cp "$IMAGE" "$app_image"
chmod 755 "$app_image"
say "image: $(basename "$IMAGE"), $(stat -c %s "$app_image") bytes, sha256 $(sha256sum "$app_image" | cut -d' ' -f1)"
if [ "${EMULATED:-0}" = 1 ]; then
  # Run through Rosetta, whose binfmt entry does not match an ELF header
  # with the AppImage magic in its padding (package.Dockerfile): zeroed in
  # this copy, which changes nothing the runtime reads.
  printf '\000\000\000' | dd of="$app_image" bs=1 seek=8 conv=notrunc status=none
  say "emulated: the AppImage magic is zeroed in this copy, for Rosetta's binfmt"
fi
started_sha=$(sha256sum "$app_image" | cut -d' ' -f1)

# Every library the image's programs and libraries ask for that neither the
# image nor the system has. A plain Debian 13 had no libgpg-error, which
# linuxdeploy leaves to the system, and the app did not start there.
missing=$(
  cd /tmp && rm -rf squashfs-root && "$app_image" --appimage-extract >/dev/null 2>&1 \
    && cd squashfs-root \
    && for file in usr/bin/tomoshibi usr/lib/*.so*; do LD_LIBRARY_PATH=$PWD/usr/lib ldd "$file" 2>/dev/null | grep 'not found' || true; done | sort -u
)
rm -rf /tmp/squashfs-root
if [ -n "$missing" ]; then say "libraries neither the image nor the system has:"; say "$missing"; exit 1; fi
say "every library the image asks for is in it or on the system"

export APPIMAGE_EXTRACT_AND_RUN=1
export TOMOSHIBI_DEBUG_LOG=$log.app.log TOMOSHIBI_DEBUG_SNAPSHOT=$log.gtk.png
"$app_image" >"$log.stderr" 2>&1 &
app_pid=$!

# What the app starts again after an update is not this shell's child; it
# stops with the container, as everything in it does.
finish() {
  kill "$app_pid" 2>/dev/null || true
  kill "$weston_pid" 2>/dev/null || true
}
trap finish EXIT

shot() {
  sleep 0.3
  ( cd /tmp && rm -f wayland-screenshot*.png && weston-screenshooter >/dev/null 2>&1 \
      && mv wayland-screenshot*.png "$1" ) || say "weston-screenshooter failed; the GTK picture alone is there"
}

deadline=$(( $(date +%s) + SECONDS_LEFT ))

# What a start with TOMOSHIBI_DEBUG_DESKTOP=add wrote: the entry naming this
# image, with the scheme, and the entry as the scheme's default.
check_desktop() {
  local entry=$HOME/.local/share/applications/app.tomoshibi.Linux.desktop
  for _ in $(seq 1 40); do [ -f "$entry" ] && break; sleep 0.25; done
  [ -f "$entry" ] || { say "no desktop entry was written"; return 1; }
  grep -qx "Exec=\"$app_image\" %U" "$entry" || { say "the entry does not run this image:"; say "$(grep '^Exec=' "$entry")"; return 1; }
  grep -qx 'MimeType=x-scheme-handler/tomoshibi;' "$entry" || { say "the entry does not open tomoshibi:// links"; return 1; }
  grep -qx 'x-scheme-handler/tomoshibi=app.tomoshibi.Linux.desktop' "$HOME/.config/mimeapps.list" \
    || { say "the entry is not the scheme's default"; return 1; }
  [ -f "$HOME/.local/share/icons/hicolor/scalable/apps/app.tomoshibi.Linux.svg" ] || { say "no icon was written"; return 1; }
  say "desktop: the entry runs $app_image, opens tomoshibi:// links by default, and has its icon ($(ls "$HOME/.local/share/icons/hicolor" | tr '\n' ' '))"
}

if [ -z "${UPDATE_TO:-}" ]; then
  until grep -q "snapshot written" "$log.app.log" 2>/dev/null; do
    if ! kill -0 "$app_pid" 2>/dev/null; then say "the app exited early; see $LABEL.stderr"; tail -20 "$log.stderr"; exit 1; fi
    if [ "$(date +%s)" -ge "$deadline" ]; then say "the window never settled; see $LABEL.app.log"; exit 1; fi
    sleep 0.25
  done
  shot "$log.weston.png"
  say "first window: $(grep -m1 '^showing ' "$log.app.log" || true); $(grep -m1 '^Tomoshibi ' "$log.app.log" || true)"
  if [ -n "${TOMOSHIBI_DEBUG_DESKTOP:-}" ] && [ "$TOMOSHIBI_DEBUG_DESKTOP" = add ]; then check_desktop; fi
  exit 0
fi

# An update that must be refused: the app says the build is not signed by
# its key, downloads nothing, and goes on as it was.
case ${UPDATE_TO:-} in
  refused)
    until grep -q 'update: the check failed' "$log.app.log" 2>/dev/null; do
      if [ "$(date +%s)" -ge "$deadline" ]; then say "the app never said the check failed; see $LABEL.app.log"; exit 1; fi
      sleep 0.25
    done
    # Long enough for a download and a restart, had either begun.
    sleep 5
    say "update: $(grep -m1 'update: the check failed' "$log.app.log")"
    if grep -q 'update: downloading\|update: closing' "$log.app.log"; then say "the app went on with a build it should have refused"; exit 1; fi
    kill -0 "$app_pid" 2>/dev/null || { say "the app is not running any more"; exit 1; }
    have=$(sha256sum "$app_image" | cut -d' ' -f1)
    [ "$have" = "$started_sha" ] || { say "the image changed: $have"; exit 1; }
    leftovers=$(ls -A "$HOME/Applications" | grep -v "^$(basename "$app_image")\$" || true)
    [ -z "$leftovers" ] || { say "something was left beside the image: $leftovers"; exit 1; }
    shot "$log.weston.png"
    say "refused: the image is unchanged (sha256 $have) and nothing was downloaded beside it"
    exit 0
    ;;
esac

# The update. The first start may close before its window has been drawn
# long enough for a snapshot (a local deployment hands the build over in a
# moment), so nothing here waits for one: it waits for the new version to say
# it has started and has removed the image it replaced (update/mod.rs
# at_start), then for the new window's own snapshot.
until grep -q "removed the image this one replaced" "$log.app.log" 2>/dev/null; do
  if [ "$(date +%s)" -ge "$deadline" ]; then say "the new version never said it had started; see $LABEL.app.log"; tail -20 "$log.stderr"; exit 1; fi
  sleep 0.25
done
say "update: $(grep -c '^Tomoshibi ' "$log.app.log") starts in the log:"
grep '^Tomoshibi \|^update: ' "$log.app.log" | tee -a "$log.check.txt"

have=$(sha256sum "$app_image" | cut -d' ' -f1)
if [ "$have" != "$UPDATE_TO" ]; then say "the image in place is $have, not the build offered ($UPDATE_TO)"; exit 1; fi
say "the image in place is the build offered: sha256 $have"
if [ -e "$app_image.old" ]; then say "the replaced image is still there"; exit 1; fi
say "the replaced image is gone; the folder holds: $(ls -A "$HOME/Applications" | tr '\n' ' ')"
[ -x "$app_image" ] || { say "the new image is not executable"; exit 1; }

# The new window's snapshot: one written after the old start said it was
# closing.
closing=$(grep -n 'update: closing' "$log.app.log" | tail -1 | cut -d: -f1)
until tail -n +"${closing:-1}" "$log.app.log" | grep -q 'snapshot written'; do
  if [ "$(date +%s)" -ge "$deadline" ]; then say "the new window never settled"; exit 1; fi
  sleep 0.25
done
shot "$log.after.weston.png"
cp "$log.gtk.png" "$log.after.gtk.png"
if [ -n "${TOMOSHIBI_DEBUG_DESKTOP:-}" ] && [ "$TOMOSHIBI_DEBUG_DESKTOP" = add ]; then check_desktop; fi
say "update done"
