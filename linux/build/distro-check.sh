#!/usr/bin/env bash
# Starts an AppImage on a plain distribution, under a headless compositor,
# and takes its first window's picture.
#
#   linux/build/distro-check.sh <arch> <distro> <AppImage> --out DIR [options]
#
#   distro      ubuntu-24.04 | debian-13 | fedora-41
#   --out DIR   a directory on this machine for the pictures and the logs
#   --label L   what the files are called (default <distro>-<arch>)
#   --network N the container's network (default none: nothing reachable,
#               the window is the one for a deployment that does not answer)
#   --base URL  TOMOSHIBI_BASE (default http://127.0.0.1:9, which refuses)
#   --size WxH  the output (default 1100x720)
#   --seconds N give up after this long (default 90: the first start under
#               emulation unpacks the whole image)
#   --env K=V   more of the app's environment; may be repeated
#
# The distribution is tomoshibi-linux-distro:<distro>-<arch>
# (distro.Dockerfile): the distribution's own image with weston, a font and
# a session bus, built on first use. The run checks that no GTK 4 and no
# libadwaita are anywhere on it before starting the image, so that a picture
# of the app is a picture of what the image brought.
#
# A container has no FUSE, so the image is started with
# APPIMAGE_EXTRACT_AND_RUN=1: the runtime unpacks it and runs it from the
# unpacked tree, which is the same AppRun and the same files a mount would
# show. The image is copied into the container's home first, where an app
# that replaces itself can.
#
# The image must be a test build (appimage.sh test), which reads
# TOMOSHIBI_BASE; a release build would open on its home deployment.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-${DOCKER_CONTEXT:-default}}
docker=(docker --context "$context")

usage="usage: distro-check.sh <aarch64|x86_64> <ubuntu-24.04|debian-13|fedora-41> <AppImage> --out DIR [options]"
arch=${1:?$usage} distro=${2:?$usage} image=${3:?$usage}
shift 3
out="" label="" network=none deployment=http://127.0.0.1:9 size=1100x720 seconds=90
extra_env=()
while [ $# -gt 0 ]; do
  case $1 in
    --out) out=$2; shift 2 ;;
    --label) label=$2; shift 2 ;;
    --network) network=$2; shift 2 ;;
    --base) deployment=$2; shift 2 ;;
    --size) size=$2; shift 2 ;;
    --seconds) seconds=$2; shift 2 ;;
    --env) extra_env+=(-e "$2"); shift 2 ;;
    *) echo "distro-check.sh: unknown option $1" >&2; exit 64 ;;
  esac
done
[ -n "$out" ] || { echo "distro-check.sh: --out is required" >&2; exit 64; }
[ -f "$image" ] || { echo "distro-check.sh: no such file: $image" >&2; exit 66; }
case $size in *x*) width=${size%x*}; height=${size#*x} ;; *) echo "distro-check.sh: --size is WxH" >&2; exit 64 ;; esac

# The distributions by digest, one per architecture (common.sh says why).
case $distro-$arch in
  ubuntu-24.04-aarch64) base=ubuntu:24.04@sha256:19cad59657037cffa0ac9a48107019b768900ff1071140965827513ac866bd5e ;;
  ubuntu-24.04-x86_64) base=ubuntu:24.04@sha256:78d4744ead1f5b19b04d0df558ab182a5300903900755e515b2ca4d10eb2044d ;;
  debian-13-aarch64) base=debian:13@sha256:39f862f398f8ea1a2ffee9bbe02c649be6779244bd6a834c4f71155172f801f2 ;;
  debian-13-x86_64) base=debian:13@sha256:e2914ec98e08b24b4cee3cefcb9f90dfb5519f8d215a2ccf0c9a49b10b6e43de ;;
  fedora-41-aarch64) base=fedora:41@sha256:e04278bca37cf98cc645106d1bc4e099db65991c2bc9c086d0664896cebe230c ;;
  fedora-41-x86_64) base=fedora:41@sha256:68bb1ba893be0c05991b2df55bc6571862bab7526fd6053b1ebacd53a2a75366 ;;
  *) echo "distro-check.sh: unknown distribution or architecture: $distro $arch" >&2; exit 64 ;;
esac
case $arch in aarch64) platform=linux/arm64 ;; x86_64) platform=linux/amd64 ;; esac

tag=tomoshibi-linux-distro:$distro-$arch
wanted=$( { shasum -a 256 "$here/distro.Dockerfile" 2>/dev/null || sha256sum "$here/distro.Dockerfile"; } | cut -d' ' -f1)-$base
have=$("${docker[@]}" image inspect "$tag" --format '{{ index .Config.Labels "tomoshibi.distro" }}' 2>/dev/null || true)
if [ "$have" != "$wanted" ]; then
  echo "building the image $tag" >&2
  "${docker[@]}" build --platform "$platform" --build-arg "BASE=$base" \
    --label "tomoshibi.distro=$wanted" -t "$tag" -f "$here/distro.Dockerfile" "$here" >&2
fi

# Whether the docker host runs this architecture's code itself or through
# an emulator (x86_64 on an arm64 host): distro-inside.sh then zeroes the
# AppImage magic in its copy of the image, which such an emulator's binfmt
# entry may not match (package.Dockerfile says more). A machine of the
# image's own architecture never needs it.
vm_arch=$("${docker[@]}" version --format '{{.Server.Arch}}' 2>/dev/null || echo unknown)
case $vm_arch-$arch in arm64-aarch64|amd64-x86_64) emulated=0 ;; *) emulated=1 ;; esac

label=${label:-$distro-$arch}
mkdir -p "$out"
out=$(cd "$out" && pwd)
image_dir=$(cd "$(dirname "$image")" && pwd)
image_name=$(basename "$image")

exec "${docker[@]}" run --rm \
  --name "tmsb-distro-$distro-$arch-$$" \
  --platform "$platform" \
  --network "$network" \
  -v "$image_dir:/images:ro" \
  -v "$here/distro-inside.sh:/opt/distro-inside.sh:ro" \
  -v "$out:/out" \
  -e "IMAGE=/images/$image_name" -e "LABEL=$label" \
  -e "WIDTH=$width" -e "HEIGHT=$height" -e "SECONDS_LEFT=$seconds" \
  -e "TOMOSHIBI_BASE=$deployment" -e "EMULATED=$emulated" \
  ${extra_env[@]+"${extra_env[@]}"} \
  "$tag" \
  dbus-run-session -- bash /opt/distro-inside.sh
