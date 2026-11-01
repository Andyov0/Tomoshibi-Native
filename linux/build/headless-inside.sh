#!/usr/bin/env bash
# The inside of run-headless.sh, in the container and on a session bus of
# its own (dbus-run-session), which the app's single instance registers on.
# Everything it starts is its own child and stops with the container.
set -euo pipefail

export XDG_RUNTIME_DIR=/tmp/runtime
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
# No accessibility bus in a container; without this every start warns.
export GTK_A11Y=none NO_AT_BRIDGE=1
log=/out/$LABEL

# The kiosk shell gives the one window the whole output, so the
# compositor's picture is the window and nothing else. --debug is what lets
# weston-screenshooter use the capture protocol; pixman draws without a GPU.
weston --backend=headless --renderer=pixman --shell=kiosk --debug --idle-time=0 \
  --width="$WIDTH" --height="$HEIGHT" --socket=wayland-1 >"$log.weston.log" 2>&1 &
weston_pid=$!
for _ in $(seq 1 100); do [ -S "$XDG_RUNTIME_DIR/wayland-1" ] && break; sleep 0.1; done
[ -S "$XDG_RUNTIME_DIR/wayland-1" ] || { echo "weston did not start; see $LABEL.weston.log" >&2; exit 1; }
export WAYLAND_DISPLAY=wayland-1 GDK_BACKEND=wayland

"$APP" >"$log.stderr" 2>&1 &
app_pid=$!

finish() {
  kill "$app_pid" 2>/dev/null || true
  wait "$app_pid" 2>/dev/null || true
  kill "$weston_pid" 2>/dev/null || true
  wait "$weston_pid" 2>/dev/null || true
}
trap finish EXIT

# The app writes its own picture once its screen has settled
# (ui::window::snapshot_when_settled); the compositor's is taken then too.
deadline=$(( $(date +%s) + SECONDS_LEFT ))
until grep -q "snapshot written" "$log.app.log" 2>/dev/null; do
  if ! kill -0 "$app_pid" 2>/dev/null; then echo "the app exited early; see $LABEL.stderr" >&2; exit 1; fi
  if [ "$(date +%s)" -ge "$deadline" ]; then echo "the screen never settled; see $LABEL.app.log" >&2; exit 1; fi
  sleep 0.25
done
# One frame more, so that the compositor has the frame GTK just rendered.
sleep 0.3
( cd /tmp && rm -f wayland-screenshot*.png && weston-screenshooter >/dev/null 2>&1 \
    && mv wayland-screenshot*.png "$log.weston.png" ) \
  || echo "weston-screenshooter failed; the GTK picture alone is there" >&2

status=0
if [ -n "$OPEN_URI" ]; then
  # A second launch must not stay: it hands the link to the first over the
  # session bus and exits.
  if timeout 15 "$APP" "$OPEN_URI" >"$log.second.stderr" 2>&1; then
    echo "the second instance exited at once" >>"$log.app.log"
  else
    echo "the second instance did not exit cleanly" >&2; status=1
  fi
  for _ in $(seq 1 40); do grep -q "a link was handed over" "$log.app.log" && break; sleep 0.25; done
  if grep -q "a link was handed over" "$log.app.log"; then
    echo "handover: the first instance was handed the link"
  else
    echo "handover: the first instance never got the link" >&2; status=1
  fi
fi

ls -l /out | grep -F "$LABEL." || true
exit $status
