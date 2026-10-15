#!/bin/bash
# Builds the core for every Apple slice the apps run on, generates the Swift
# bindings from the library that was actually built, and packs the slices into
# TomoshibiCore/TomoshibiCoreFFI.xcframework for the Swift package to wrap.
#
#   apple/build-core.sh            # debug
#   apple/build-core.sh release
#
# Generated files are not committed: they are only ever what this script last
# produced from the core beside it.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
CORE="$HERE/../core"
PROFILE=${1:-debug}
FLAG=""; [ "$PROFILE" = release ] && FLAG="--release"
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export DEVELOPER_DIR=${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}

TARGETS=(aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim)
cd "$CORE"
for t in "${TARGETS[@]}"; do
    cargo build --lib $FLAG --target "$t"
done
# The generator is built for the same target directory as the macOS slice,
# whose WebRTC download is already there, rather than for the host's own.
cargo build --bin uniffi-bindgen --target aarch64-apple-darwin

OUT="$HERE/TomoshibiCore"
GEN=$(mktemp -d)
"$CORE/target/aarch64-apple-darwin/debug/uniffi-bindgen" generate \
    --library "$CORE/target/aarch64-apple-darwin/$PROFILE/libtomoshibi_core.a" \
    --language swift --out-dir "$GEN"

# One Swift file and one C module per crate that exports anything (the core
# and the protocol crate today). All the Swift goes into the one package
# target, so types from either crate are visible to the other's bindings, and
# the C modules share one module.modulemap in the XCFramework's headers.
rm -f "$OUT"/Sources/TomoshibiCore/*.generated.swift
for swift in "$GEN"/*.swift; do
    name=$(basename "$swift" .swift)
    cp "$swift" "$OUT/Sources/TomoshibiCore/$name.generated.swift"
done
HEADERS="$GEN/headers"; mkdir -p "$HEADERS"
cp "$GEN"/*.h "$HEADERS/"
cat "$GEN"/*.modulemap > "$HEADERS/module.modulemap"

rm -rf "$OUT/TomoshibiCoreFFI.xcframework"
ARGS=()
for t in "${TARGETS[@]}"; do
    ARGS+=(-library "$CORE/target/$t/$PROFILE/libtomoshibi_core.a" -headers "$HEADERS")
done
xcodebuild -create-xcframework "${ARGS[@]}" -output "$OUT/TomoshibiCoreFFI.xcframework" >/dev/null
rm -rf "$GEN"
echo "built $PROFILE core into $OUT"
