#!/bin/bash
# Builds the core for Android, generates the Kotlin bindings from the library
# that was actually built, and puts both, with WebRTC's Java half, where the
# core module expects them.
#
#   android/build-core.sh            # debug
#   android/build-core.sh release
#
# As on Apple, the generated files are not committed: they are only ever what
# this script last produced from the core beside it.
#
# Only arm64-v8a: every phone that runs Android 10 or later on a recent chip
# is arm64, the owner's Fold6 has no 32-bit cores at all, and the emulator on
# an Apple silicon Mac runs arm64 images.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
CORE="$HERE/../core"
PROFILE=${1:-debug}
FLAG=""; [ "$PROFILE" = release ] && FLAG="--release"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"
export ANDROID_HOME=${ANDROID_HOME:-$HOME/Library/Android/sdk}
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}
TARGET=aarch64-linux-android
MODULE="$HERE/core"

cd "$CORE"
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
# The prebuilt WebRTC is fetched by webrtc-sys's build script; once it is
# there, its JNI entry points are read out of it below.
cargo ndk -t arm64-v8a -P 29 build --lib $FLAG

# WebRTC's Java classes reach their native halves by JNI name
# (Java_livekit_org_webrtc_...), so those functions have to be in the
# library and exported from it. webrtc-sys's build script says so to the
# linker, but cargo gives a dependency's link arguments to that dependency
# alone, never to the library that links it: without these the first call
# aborted with "No implementation found for ...
# SoftwareVideoEncoderFactory.nativeCreateFactory()". Each is named as
# undefined, so that the archive's object holding it is linked at all, and
# a second version script exports them beside rustc's own.
WEBRTC_A=$(find "$CORE/target" -path "*android-arm64-release/lib/libwebrtc.a" | head -1)
[ -n "$WEBRTC_A" ] || { echo "the Android WebRTC archive is not under $CORE/target" >&2; exit 1; }
LINK="$CORE/target/$TARGET/jni-link"
mkdir -p "$LINK"
"$NDK_BIN/llvm-readelf" -Ws "$WEBRTC_A" | grep -oE 'Java_livekit_org_webrtc_[A-Za-z0-9_]+' | sort -u > "$LINK/symbols.txt"
printf '{\n  global: Java_livekit_org_webrtc_*;\n};\n' > "$LINK/jni.map"
{ sed 's/^/-Wl,--undefined=/' "$LINK/symbols.txt"; echo "-Wl,--version-script=$LINK/jni.map"; } > "$LINK/link.rsp"
echo "exporting $(wc -l < "$LINK/symbols.txt" | tr -d ' ') WebRTC JNI functions"

# A shared library for Android only, asked for here rather than in Cargo.toml:
# a cdylib there would be linked for every Apple target too.
cargo ndk -t arm64-v8a -P 29 rustc --lib --crate-type cdylib $FLAG -- -C "link-arg=@$LINK/link.rsp"
# The generator runs on the Mac and reads the Android library's metadata.
cargo build --bin uniffi-bindgen --target aarch64-apple-darwin

LIB="$CORE/target/$TARGET/$PROFILE/libtomoshibi_core.so"
JNI="$MODULE/src/main/jniLibs/arm64-v8a"
mkdir -p "$JNI"
# Debug information is most of a debug build's 360 MB, and every install
# copies the library to the phone; the symbols stay in the unstripped one.
"$NDK_BIN/llvm-strip" --strip-debug -o "$JNI/libtomoshibi_core.so" "$LIB"

GEN="$MODULE/src/main/java"
rm -rf "$GEN/uniffi"
"$CORE/target/aarch64-apple-darwin/debug/uniffi-bindgen" generate --library "$LIB" --language kotlin --no-format --out-dir "$GEN"

# WebRTC's Java classes (livekit.org.webrtc.*), which its Android codec
# factories call through JNI; they come with the prebuilt WebRTC download.
JAR=$(find "$CORE/target" -path "*android-arm64-release/libwebrtc.jar" | head -1)
[ -n "$JAR" ] || { echo "libwebrtc.jar not found under $CORE/target" >&2; exit 1; }
mkdir -p "$MODULE/libs"
cp "$JAR" "$MODULE/libs/libwebrtc.jar"
echo "built $PROFILE core into $MODULE"
