#!/usr/bin/env bash
# Runs a build of the Linux app under a headless Wayland compositor in the
# build image, and takes its picture: the compositor's screenshot of the
# whole output, and GTK's own rendering of the window.
#
#   linux/build/run-headless.sh <arch> --base <url> --out <dir> [options]
#
#   --base URL        the deployment (TOMOSHIBI_BASE); required, since a
#                     debug build without one opens on its home deployment
#   --out DIR         a directory on this machine for the pictures and the logs
#   --label NAME      what the files are called (default: <arch>-<appearance>)
#   --appearance day|night   the system's preference the app follows
#                     (libadwaita's ADW_DEBUG_COLOR_SCHEME, read as the
#                     system's own); default day
#   --size WxH        the output, which the kiosk shell gives the window
#                     whole (default 1100x720)
#   --show NAME       wait for this screen (app::Screen::name) before the
#                     pictures; default: the first one after starting
#   --language CODE   TOMOSHIBI_LANGUAGE (en, zh-Hans, zh-Hant, ja)
#   --open URI        once the pictures are taken, launch a second instance
#                     with URI and check that the first one was handed it
#   --profile NAME    the build to run (default debug)
#   --after MS        draw the pictures this long after the screen is shown
#                     (default 1500), for a state that takes longer to come
#   --seconds N       give up after this long (default 40)
#   --env NAME=VALUE  more of the app's environment; may be repeated
#
# The build it runs is build.sh's copy for this checkout,
# /target/out/<checkout>/<profile>/tomoshibi in the architecture's target
# volume (TOMOSHIBI_TARGET_VOLUME, as build.sh takes it).
#
# The container joins tomoshibi-linux-net, where server.sh's deployments
# are; give --base "$(linux/build/server.sh url <port>)". Exits non-zero if
# the screen never came, a picture is missing, or the handover failed.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
checkout=$(cd "$here/../.." && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-${DOCKER_CONTEXT:-default}}
docker=(docker --context "$context")

arch=${1:?"usage: run-headless.sh <aarch64|x86_64> --base <url> --out <dir> [options]"}
shift
deployment="" out="" label="" appearance=day size=1100x720 show="" language="" open_uri="" profile=debug seconds=40 after=""
extra_env=()
while [ $# -gt 0 ]; do
  case $1 in
    --base) deployment=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --appearance) appearance=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --show) show=$2; shift 2 ;;
    --language) language=$2; shift 2 ;;
    --open) open_uri=$2; shift 2 ;;
    --profile) profile=$2; shift 2 ;;
    --seconds) seconds=$2; shift 2 ;;
    --after) after=$2; shift 2 ;;
    --env) extra_env+=(-e "$2"); shift 2 ;;
    *) echo "run-headless.sh: unknown option $1" >&2; exit 64 ;;
  esac
done
[ -n "$deployment" ] || { echo "run-headless.sh: --base is required (a local deployment)" >&2; exit 64; }
[ -n "$out" ] || { echo "run-headless.sh: --out is required" >&2; exit 64; }
case $appearance in
  day) scheme=prefer-light ;;
  night) scheme=prefer-dark ;;
  *) echo "run-headless.sh: --appearance is day or night" >&2; exit 64 ;;
esac
case $size in *x*) width=${size%x*}; height=${size#*x} ;; *) echo "run-headless.sh: --size is WxH" >&2; exit 64 ;; esac
label=${label:-$arch-$appearance}
mkdir -p "$out"
out=$(cd "$out" && pwd)

# shellcheck source=common.sh
. "$here/common.sh"
platform_for "$arch"
ensure_image "$arch"
name=$(checkout_name "$checkout")
network=tomoshibi-linux-net
"${docker[@]}" network inspect "$network" >/dev/null 2>&1 \
  || "${docker[@]}" network create "$network" >/dev/null

app_env=(
  -e "TOMOSHIBI_BASE=$deployment"
  -e "TOMOSHIBI_DEBUG_LOG=/out/$label.app.log"
  -e "TOMOSHIBI_DEBUG_SNAPSHOT=/out/$label.gtk.png"
  -e "ADW_DEBUG_COLOR_SCHEME=$scheme"
)
[ -n "$show" ] && app_env+=(-e "TOMOSHIBI_SHOW=$show")
[ -n "$language" ] && app_env+=(-e "TOMOSHIBI_LANGUAGE=$language")
[ -n "$after" ] && app_env+=(-e "TOMOSHIBI_DEBUG_SNAPSHOT_AFTER_MS=$after")

# The last run's pictures of this label are removed inside the container
# (headless-inside.sh), not here: removed outside the docker host, the
# host's view of a shared directory can still hold them, and creating the
# same names from the container failed with "No such file or directory",
# every second run.
exec "${docker[@]}" run --rm \
  --name "tmsb-headless-$name-$arch-$$" \
  --platform "$platform" \
  --network "$network" \
  -v "$checkout:/work/$name:ro" \
  -v "${TOMOSHIBI_TARGET_VOLUME:-tomoshibi-linux-target-$arch}:/target:ro" \
  -v "$out:/out" \
  -e "APP=/target/out/$name/$profile/tomoshibi" \
  -e "LABEL=$label" -e "WIDTH=$width" -e "HEIGHT=$height" \
  -e "SECONDS_LEFT=$seconds" -e "OPEN_URI=$open_uri" \
  "${app_env[@]}" ${extra_env[@]+"${extra_env[@]}"} \
  "tomoshibi-linux-build:$arch" \
  dbus-run-session -- bash "/work/$name/linux/build/headless-inside.sh"
