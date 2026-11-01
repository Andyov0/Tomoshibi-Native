#!/usr/bin/env bash
# The Linux app's update, end to end: an AppImage on a plain distribution
# finds a newer build on a local deployment, downloads it, checks it against
# the key it was built with, puts it in its own place and starts it.
#
#   linux/build/update-check.sh <arch> <distro> --old <AppImage> --shelf DIR
#                               --server <binary> --port P --out DIR [--label L]
#
#   --old     the image to start, a test image (appimage.sh test) whose core
#             holds updates to a key of the tests' own (TOMOSHIBI_TEST_UPDATE_KEY)
#
# For an architecture the VM runs through Rosetta (x86_64), the image the
# shelf offers must have its AppImage magic zeroed before it is signed, as
# distro-inside.sh zeroes it in the image it starts: the update is started by
# the app itself, and Rosetta's binfmt entry does not match the magic
# (package.Dockerfile).
#   --shelf   the deployment's apps directory: apps.json, its linux-<arch>
#             line signed with that key, and the newer image it names
#   --server  a Linux build of the server for the VM's architecture, one that
#             knows the Linux shelves
#   --port    the server's port, from the caller's range (it uses that one)
#   --expect  installed (the default), or refused: the shelf's line is not
#             signed by the app's key, and the run passes only when the app
#             says so, downloads nothing and leaves its image as it was
#
# The server runs in a container of its own on tomoshibi-linux-net with the
# shelf mounted read-only, as server.sh runs one with no apps; the app runs in
# distro-check.sh's container on the same network, and the run passes only
# when the new version has started, has removed the image it replaced, and the
# image in its place is byte for byte the build the shelf offers. The server
# is removed at the end, whatever happened.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-linux-build}
docker=(docker --context "$context")
network=tomoshibi-linux-net

usage="usage: update-check.sh <arch> <distro> --old <AppImage> --shelf DIR --server <binary> --port P --out DIR [--label L]"
arch=${1:?$usage} distro=${2:?$usage}
shift 2
old="" shelf="" server="" port="" out="" label="" expect=installed
while [ $# -gt 0 ]; do
  case $1 in
    --old) old=$2; shift 2 ;;
    --shelf) shelf=$2; shift 2 ;;
    --server) server=$2; shift 2 ;;
    --port) port=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --expect) expect=$2; shift 2 ;;
    *) echo "update-check.sh: unknown option $1" >&2; exit 64 ;;
  esac
done
for given in old shelf server port out; do
  [ -n "${!given}" ] || { echo "$usage" >&2; exit 64; }
done
shelf=$(cd "$shelf" && pwd)
server=$(cd "$(dirname "$server")" && pwd)/$(basename "$server")

# The build the shelf offers this architecture, and its hash, which the image
# in place must have at the end.
offered=$(jq -r --arg s "linux-$arch" '.[$s] | max_by(.version) | .file' "$shelf/apps.json")
want=$(jq -r --arg s "linux-$arch" '.[$s] | max_by(.version) | .sha256' "$shelf/apps.json")
[ -f "$shelf/$offered" ] || { echo "update-check.sh: the shelf names $offered, which is not in it" >&2; exit 66; }
have=$( { shasum -a 256 "$shelf/$offered" 2>/dev/null || sha256sum "$shelf/$offered"; } | cut -d' ' -f1)
[ "$have" = "$want" ] || { echo "update-check.sh: $offered does not hash to its line" >&2; exit 1; }

# shellcheck source=common.sh
. "$here/common.sh"
platform_for aarch64
ensure_image aarch64
"${docker[@]}" network inspect "$network" >/dev/null 2>&1 || "${docker[@]}" network create "$network" >/dev/null

name=tmsb-update-server-$port
label=${label:-update-$distro-$arch}
mkdir -p "$out"
"${docker[@]}" rm -f "$name" >/dev/null 2>&1 || true
# The server's own log beside the app's, for what it handed over and to whom.
stop() {
  "${docker[@]}" logs "$name" >"$out/$label.server.txt" 2>&1 || true
  "${docker[@]}" rm -f "$name" >/dev/null 2>&1 || true
}
trap stop EXIT

config="port: $((port + 1))
bind_addresses: [\"127.0.0.1\"]
rtc:
  udp_port: $((port + 2))
  tcp_port: $((port + 3))
  use_external_ip: false
keys:
  APIdevelopment: development-secret-not-for-anything-reachable
logging:
  level: info
meet:
  listen: \":$port\"
  database: \"/tmp/meet.db\"
  source_url: \"https://github.com/5t-RawBeRry/Tomoshibi\"
  rooms:
    opened_by: anyone
    remember: 72h
  apps:
    mac_dir: /apps
"
"${docker[@]}" run -d --name "$name" --platform linux/arm64 \
  --network "$network" --network-alias "$name" \
  -v "$server:/opt/tomoshibi-server:ro" -v "$shelf:/apps:ro" \
  -e "MEET_CONFIG=$config" --entrypoint bash "tomoshibi-linux-build:aarch64" \
  -c 'printf "%s" "$MEET_CONFIG" > /tmp/meet.yaml && exec /opt/tomoshibi-server serve /tmp/meet.yaml' >/dev/null

for _ in $(seq 1 60); do
  "${docker[@]}" exec "$name" curl -fsS "http://127.0.0.1:$port/api/deployment" >/dev/null 2>&1 && break
  sleep 0.5
done
# What the app will be told, asked as an app asks.
answer=$("${docker[@]}" exec "$name" curl -fsS -A "Tomoshibi/0.0.1 (Linux)" "http://127.0.0.1:$port/api/apps/latest/linux-$arch") \
  || { echo "update-check.sh: the deployment offers no linux-$arch build; its log:" >&2; "${docker[@]}" logs --tail 30 "$name" >&2; exit 1; }
echo "the deployment offers: $(printf '%s' "$answer" | jq -c '{platform, version, size, file, signed: (.sig != null)}')"

case $expect in
  installed) to=$want ;;
  # The image must still be the one started, byte for byte, which
  # distro-inside.sh knows.
  refused) to=refused ;;
  *) echo "update-check.sh: --expect is installed or refused" >&2; exit 64 ;;
esac
"$here/distro-check.sh" "$arch" "$distro" "$old" --out "$out" --label "$label" \
  --network "$network" --base "http://$name:$port" --seconds 180 \
  --env TOMOSHIBI_DEBUG_UPDATE=install --env TOMOSHIBI_DEBUG_DESKTOP=add --env "UPDATE_TO=$to"
