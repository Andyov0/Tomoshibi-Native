#!/usr/bin/env bash
# A local Tomoshibi Server for the Linux app's runs and tests, in a container
# on the docker network the app's own containers join (run-headless.sh).
#
#   linux/build/server.sh start <server binary> <port>
#   linux/build/server.sh stop <port>
#   linux/build/server.sh url <port>        what TOMOSHIBI_BASE to give the app
#
# The binary is a Linux build of the server (Tomoshibi Server, built with
# GOOS=linux for the architecture of the build VM, which runs it natively;
# statically linked, so any image runs it). Four ports from <port> are used:
# <port> for the web, the API and the signalling, <port>+1 the media server's
# own HTTP on loopback, <port>+2 its UDP and <port>+3 its TCP. Each checkout
# that runs one picks <port> from its own range.
#
# The app reaches it as http://tmsb-server-<port>:<port> from any container
# on the network tomoshibi-linux-net. The web port and the media server's
# TCP port are also published on the VM's loopback, which is forwarded
# to the Mac's, for a browser there; media over UDP from the Mac is not
# forwarded (the media server hands out the container's own address), so a
# browser that has to receive media runs in a container on the network too.
#
# Everything the server writes is in the container and goes with it: every
# start is a deployment nobody has used.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-linux-build}
docker=(docker --context "$context")
network=tomoshibi-linux-net

usage() { echo "usage: server.sh start <server binary> <port> | stop <port> | url <port>" >&2; exit 64; }

what=${1:-}
case $what in
  start) binary=${2:-}; port=${3:-}; [ -n "$binary" ] && [ -n "$port" ] || usage ;;
  stop|url) port=${2:-}; [ -n "$port" ] || usage ;;
  *) usage ;;
esac
case $port in *[!0-9]*|'') usage ;; esac
name=tmsb-server-$port

case $what in
  url) echo "http://$name:$port"; exit 0 ;;
  stop) "${docker[@]}" rm -f "$name" >/dev/null 2>&1 || true; echo "stopped $name"; exit 0 ;;
esac

[ -f "$binary" ] || { echo "server.sh: no such file: $binary" >&2; exit 66; }
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")

# The build image runs it: it is on the machine already and has curl for
# the wait below.
# shellcheck source=common.sh
. "$here/common.sh"
platform_for aarch64
ensure_image aarch64

"${docker[@]}" network inspect "$network" >/dev/null 2>&1 \
  || "${docker[@]}" network create "$network" >/dev/null
"${docker[@]}" rm -f "$name" >/dev/null 2>&1 || true

# The development keys are the server's own published ones
# (dev/meet.yaml), for a deployment nothing outside this machine reaches.
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
"

"${docker[@]}" run -d --name "$name" \
  --platform linux/arm64 \
  --network "$network" --network-alias "$name" \
  -p "127.0.0.1:$port:$port" -p "127.0.0.1:$((port + 3)):$((port + 3))" \
  -v "$binary:/opt/tomoshibi-server:ro" \
  -e "MEET_CONFIG=$config" \
  --entrypoint bash \
  "tomoshibi-linux-build:aarch64" \
  -c 'printf "%s" "$MEET_CONFIG" > /tmp/meet.yaml && exec /opt/tomoshibi-server serve /tmp/meet.yaml' >/dev/null

for _ in $(seq 1 60); do
  if "${docker[@]}" exec "$name" curl -fsS "http://127.0.0.1:$port/api/deployment" >/dev/null 2>&1; then
    echo "started $name: http://$name:$port on $network (and http://127.0.0.1:$port on the Mac)"
    exit 0
  fi
  sleep 0.5
done
echo "server.sh: the server did not answer; its log:" >&2
"${docker[@]}" logs --tail 40 "$name" >&2 || true
exit 1
