#!/usr/bin/env bash
# Builds or tests the Linux app inside the build image, for one architecture.
#
#   linux/build/build.sh <arch> <what> [cargo arguments...]
#
#   arch   aarch64 | x86_64   (x86_64 runs under emulation on an arm64 host)
#   what   debug | release | local     cargo build with that profile
#          release-proof               cargo build --release for
#                                      meet.example.com with no update key
#                                      (below)
#          test                        cargo test of the Linux package
#          core-test                   cargo test of the core's workspace
#          check | clippy              cargo check / clippy of the Linux package
#          fmt                         cargo fmt --check of the Linux package
#                                      (fmt-fix writes the changes)
#          shell                       an interactive shell in the image
#
# The checkout is mounted at /work/<name of this checkout>; cargo's registry
# and git checkouts are named volumes shared by every checkout and both
# architectures (cargo locks them itself), and the target directory is one
# named volume per architecture, shared by every checkout. A crate from the
# registry is therefore compiled once per architecture for everybody, and a
# checkout's own crates (the core, the vendored LiveKit, the app) are kept
# apart from another checkout's because cargo names path crates by where they
# are. What a build makes is copied, under a lock, to
# /target/out/<checkout>/<profile>/tomoshibi in that volume, since
# target/<profile>/tomoshibi itself is whichever checkout built last.
#
# A debug build, and every test, is built for meet.example.com: the core bakes
# its home deployment in at build time (core/protocol/build.rs), and a build
# for tests must never reach a real one. Give a debug run its deployment with
# TOMOSHIBI_BASE at run time (run-headless.sh). A release build takes
# TOMOSHIBI_HOST and TOMOSHIBI_UPDATE_KEY from the environment of whoever runs
# this, and is refused without them; deployment.local is never read here.
#
# release-proof is the release profile built the way a debug build is, for
# meet.example.com and with no update key: the release configuration (no
# debug assertions, every #[cfg(not(debug_assertions))] path, the optimiser,
# the link) proven to build on both architectures by somebody who holds no
# deployment's host or key, as an integration does. The core allows it for
# the example host only (core/protocol/build.rs). It is copied to
# /target/out/<checkout>/release-proof/, never to release/, and is nothing
# to give out: it reaches no deployment, and check-core.sh, which
# appimage.sh runs on a release, refuses a core built for meet.example.com.
#
# One build at a time per target volume: cargo locks the build directory for
# the whole build, so a second checkout's build waits ("Blocking waiting for
# file lock") rather than running beside it. That is chosen, not suffered:
# the VM has 20 GiB for everybody and about 20 GB of disk left, and one
# target directory per checkout would compile every crate again in each and
# hold a copy of everything per checkout. A checkout that must not wait can
# be given a volume of its own with TOMOSHIBI_TARGET_VOLUME (the first build
# in it compiles everything).
#
# Environment:
#   TOMOSHIBI_DOCKER_CONTEXT   docker context (default linux-build)
#   TOMOSHIBI_BUILD_JOBS       cargo's parallel jobs (default 4): several
#                              checkouts build on one VM, and rustc and the
#                              linker at twelve jobs each ran it out of memory
#   TOMOSHIBI_TARGET_VOLUME    the target volume (default
#                              tomoshibi-linux-target-<arch>, shared)
#   TOMOSHIBI_HOST, TOMOSHIBI_UPDATE_KEY   release builds only
#   TOMOSHIBI_TEST_UPDATE_KEY  debug and test builds only: the public half of
#                              a key of the tests' own (default none)
set -euo pipefail

arch=${1:?"usage: build.sh <aarch64|x86_64> <debug|release|release-proof|local|test|core-test|check|clippy|fmt|fmt-fix|shell> [cargo args]"}
what=${2:-debug}
shift $(( $# >= 2 ? 2 : $# ))

here=$(cd "$(dirname "$0")" && pwd)
checkout=$(cd "$here/../.." && pwd)
context=${TOMOSHIBI_DOCKER_CONTEXT:-linux-build}
docker=(docker --context "$context")

# shellcheck source=common.sh
. "$here/common.sh"
platform_for "$arch"
ensure_image "$arch"

# The checkout's name, made safe for a path inside the container. Docker
# 29's bind mounts mangle a target path with characters beyond ASCII (the
# last byte of the path is repeated), so nothing of the host's path is used.
name=$(checkout_name "$checkout")
work=/work/$name

# The extra cargo arguments, quoted for the shell inside the container.
# Nothing at all when there are none: printf with no arguments would still
# print '' once, and cargo would be handed an empty argument.
extra=""
if [ $# -gt 0 ]; then extra=$(printf '%q ' "$@"); fi

case $what in
  debug|release|release-proof|local) profile=$what ;;
  test|core-test|check|clippy|fmt|fmt-fix|shell) profile=debug ;;
  *) echo "build.sh: unknown <what>: $what" >&2; exit 64 ;;
esac

target_volume=${TOMOSHIBI_TARGET_VOLUME:-tomoshibi-linux-target-$arch}
env_args=(-e CARGO_TARGET_DIR=/target -e CARGO_TERM_COLOR=${CARGO_TERM_COLOR:-auto}
  -e "CARGO_BUILD_JOBS=${TOMOSHIBI_BUILD_JOBS:-4}")
if [ "$profile" = release-proof ]; then
  # Whatever the caller's environment says, as for a test build.
  env_args+=(-e TOMOSHIBI_HOST=meet.example.com -e TOMOSHIBI_UPDATE_KEY=none)
elif [ "$profile" = release ] || [ "$profile" = local ]; then
  if [ -z "${TOMOSHIBI_HOST:-}" ] || [ "${TOMOSHIBI_HOST}" = meet.example.com ]; then
    if [ "$profile" = release ]; then
      echo "build.sh: a release build needs TOMOSHIBI_HOST and TOMOSHIBI_UPDATE_KEY in the environment" >&2
      exit 64
    fi
  fi
  env_args+=(-e "TOMOSHIBI_HOST=${TOMOSHIBI_HOST:-meet.example.com}")
  if [ -n "${TOMOSHIBI_UPDATE_KEY:-}" ]; then env_args+=(-e "TOMOSHIBI_UPDATE_KEY=$TOMOSHIBI_UPDATE_KEY"); fi
else
  # Whatever the caller's environment says: a test build is for nobody.
  # Its update key is none, unless the test gives it the public half of a
  # key of its own (TOMOSHIBI_TEST_UPDATE_KEY), for an update signed by
  # that key to be checked and installed end to end (appimage.sh test).
  # Never TOMOSHIBI_UPDATE_KEY, which is the release's: a debug build of
  # whoever ran a release in the same shell would otherwise take it.
  env_args+=(-e TOMOSHIBI_HOST=meet.example.com -e "TOMOSHIBI_UPDATE_KEY=${TOMOSHIBI_TEST_UPDATE_KEY:-none}")
fi

case $what in
  debug|release|release-proof|local)
    cargo_profile=$profile; [ "$profile" = debug ] && cargo_profile=dev
    [ "$profile" = release-proof ] && cargo_profile=release
    out_dir=$profile
    script="cargo build --profile $cargo_profile $extra --message-format=json-render-diagnostics > /tmp/messages.json \
      && exe=\$(jq -r 'select(.reason==\"compiler-artifact\" and .executable!=null and .target.name==\"tomoshibi\") | .executable' /tmp/messages.json | tail -1) \
      && mkdir -p /target/out/$name/$out_dir \
      && cp \"\$exe\" /target/out/$name/$out_dir/tomoshibi.part \
      && mv /target/out/$name/$out_dir/tomoshibi.part /target/out/$name/$out_dir/tomoshibi \
      && echo \"built /target/out/$name/$out_dir/tomoshibi (from \$exe)\" \
      && file -b /target/out/$name/$out_dir/tomoshibi"
    ;;
  test) script="cargo test $extra" ;;
  # The core's own workspace keeps its own profiles, so the dependencies'
  # line tables only (linux/Cargo.toml says why) are given on the line.
  core-test) script="cd $work/core && cargo --config 'profile.dev.package.\"*\".debug=\"line-tables-only\"' test --workspace $extra" ;;
  check) script="cargo check --all-targets $extra" ;;
  clippy) script="cargo clippy --all-targets $extra" ;;
  fmt) script="cargo fmt --check $extra" ;;
  fmt-fix) script="cargo fmt $extra" ;;
  shell) script="exec bash" ;;
esac

# A build holds a lock on the architecture's target volume until its binary
# is copied out: cargo releases its own lock when it finishes, and another
# checkout's build in that moment would put its binary where this one
# copies from. Tests rely on cargo's lock alone, so that a long test run
# does not hold up everybody's builds.
case $what in
  debug|release|release-proof|local) script="flock /target/.tomoshibi-build.lock bash -c $(printf '%q' "set -euo pipefail; $script")" ;;
esac
# No git inside: a worktree's .git file names the host's path to the
# repository, which is not mounted, and any git command run in the checkout
# stops at it. Nothing in the build reads git.
#
# macOS's bash 3.2 calls an empty array unbound under set -u, hence the
# ${name[@]+...} form for the one that may be empty.
tty=(); if [ -t 0 ] && [ -t 1 ]; then tty=(-it); fi
exec "${docker[@]}" run --rm ${tty[@]+"${tty[@]}"} \
  --name "tmsb-build-$name-$arch-$$" \
  --platform "$platform" \
  -v "$checkout:$work" \
  -v tomoshibi-linux-cargo-registry:/opt/cargo/registry \
  -v tomoshibi-linux-cargo-git:/opt/cargo/git \
  -v "$target_volume:/target" \
  "${env_args[@]}" \
  -w "$work/linux" \
  "tomoshibi-linux-build:$arch" \
  bash -c "set -euo pipefail; $script"
