#!/usr/bin/env bash
# Makes the Linux app's AppImage for one architecture.
#
#   linux/build/appimage.sh <aarch64|x86_64> <release|test> --out DIR [--no-build]
#
#   release   build.sh <arch> release, then an image to give out:
#             Tomoshibi-<version>-<arch>.AppImage. Needs TOMOSHIBI_HOST and
#             TOMOSHIBI_UPDATE_KEY in the environment, as build.sh does, and
#             refuses a core that is not a release one for that host and key
#             (check-core.sh), meet.example.com above all.
#   test      build.sh <arch> debug, then Tomoshibi-<version>-test-<arch>.AppImage,
#             whose core must be for meet.example.com. For the tests: the
#             distribution runs and the update from a local deployment.
#             TOMOSHIBI_TEST_UPDATE_KEY gives it a key of the tests' own.
#   --out     a directory on the Mac for the image and its list of contents
#   --no-build   package what build.sh last built for this checkout
#
# <version> is the one the update check compares and the deployment offers
# (client::dotted_version): linux/Cargo.toml's version, with the build from
# linux/src/app/client.rs joined on when it is above 1.
#
# The image is made in tomoshibi-linux-package:<arch> (package.Dockerfile),
# built on first use from the build image and remade when either changes;
# appimage-inside.sh is what runs there.
set -euo pipefail

arch=${1:?"usage: appimage.sh <aarch64|x86_64> <release|test> --out DIR [--no-build]"}
kind=${2:?"usage: appimage.sh <aarch64|x86_64> <release|test> --out DIR [--no-build]"}
shift 2
out="" build=true
while [ $# -gt 0 ]; do
  case $1 in
    --out) out=$2; shift 2 ;;
    --no-build) build=false; shift ;;
    *) echo "appimage.sh: unknown option $1" >&2; exit 64 ;;
  esac
done
[ -n "$out" ] || { echo "appimage.sh: --out is required" >&2; exit 64; }

here=$(cd "$(dirname "$0")" && pwd)
checkout=$(cd "$here/../.." && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-linux-build}
docker=(docker --context "$context")

case $kind in
  release)
    profile=release
    # The same refusal as check-core.sh's, before an hour of building rather
    # than after it.
    case ${TOMOSHIBI_HOST:-} in
      ''|meet.example.com) echo "appimage.sh: a release needs TOMOSHIBI_HOST, a real deployment's" >&2; exit 64 ;;
    esac
    case ${TOMOSHIBI_UPDATE_KEY:-} in
      ''|none) echo "appimage.sh: a release needs TOMOSHIBI_UPDATE_KEY, the public half of the release key" >&2; exit 64 ;;
    esac
    ;;
  test) profile=debug ;;
  *) echo "appimage.sh: <kind> is release or test" >&2; exit 64 ;;
esac

# shellcheck source=common.sh
. "$here/common.sh"
platform_for "$arch"
name=$(checkout_name "$checkout")

version=$(sed -n 's/^version = "\([0-9.]*\)".*/\1/p' "$checkout/linux/Cargo.toml" | head -1)
build_number=$(sed -n 's/^pub const BUILD: &str = "\([0-9]*\)";.*/\1/p' "$checkout/linux/src/app/client.rs" | head -1)
[ -n "$version" ] && [ -n "$build_number" ] || { echo "appimage.sh: no version in Cargo.toml or no BUILD in client.rs" >&2; exit 1; }
# version_with_build: the build joined on from the second.
dotted=$version
[ "$build_number" -gt 1 ] && dotted=$version.$build_number
if [ "$kind" = release ]; then image=Tomoshibi-$dotted-$arch.AppImage; else image=Tomoshibi-$dotted-test-$arch.AppImage; fi

if $build; then
  "$here/build.sh" "$arch" "$profile"
fi

# The packaging image, remade when its Dockerfile or the build image under
# it has changed: the libraries copied into an AppImage must be the ones the
# binary was linked against.
ensure_image "$arch"
base_id=$("${docker[@]}" image inspect "tomoshibi-linux-build:$arch" --format '{{.Id}}')
wanted=$( { shasum -a 256 "$here/package.Dockerfile" 2>/dev/null || sha256sum "$here/package.Dockerfile"; } | cut -d' ' -f1)
wanted=$wanted-$base_id
have=$("${docker[@]}" image inspect "tomoshibi-linux-package:$arch" --format '{{ index .Config.Labels "tomoshibi.package" }}' 2>/dev/null || true)
if [ "$have" != "$wanted" ]; then
  echo "building the image tomoshibi-linux-package:$arch" >&2
  "${docker[@]}" build --platform "$platform" \
    --build-arg "BUILD_IMAGE=tomoshibi-linux-build:$arch" --build-arg "ARCH=$arch" \
    --label "tomoshibi.package=$wanted" \
    -t "tomoshibi-linux-package:$arch" -f "$here/package.Dockerfile" "$here" >&2
fi

mkdir -p "$out"
out=$(cd "$out" && pwd)
release_env=()
if [ "$kind" = release ]; then
  release_env=(-e "TOMOSHIBI_HOST=$TOMOSHIBI_HOST" -e "TOMOSHIBI_UPDATE_KEY=$TOMOSHIBI_UPDATE_KEY")
fi

exec "${docker[@]}" run --rm \
  --name "tmsb-appimage-$name-$arch-$$" \
  --platform "$platform" \
  -v "$checkout:/work/$name:ro" \
  -v "${TOMOSHIBI_TARGET_VOLUME:-tomoshibi-linux-target-$arch}:/target:ro" \
  -v "$out:/out" \
  -e "ARCH=$arch" -e "KIND=$kind" -e "VERSION=$dotted" -e "IMAGE=$image" \
  -e "BIN=/target/out/$name/$profile/tomoshibi" -e "WORK=/work/$name" -e OUT=/out \
  ${release_env[@]+"${release_env[@]}"} \
  "tomoshibi-linux-package:$arch" \
  bash "/work/$name/linux/build/appimage-inside.sh"
