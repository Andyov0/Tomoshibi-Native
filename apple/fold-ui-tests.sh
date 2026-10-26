#!/bin/zsh
# Runs the UI tests that need an iPhone Duo opened and closed (FoldStateTests,
# InnerColumnTests), and opens and closes it for them. The Duo simulator folds
# only from Device Hub's own buttons; what opening does to the app is move its
# scene to another display where it is a regular width, and this does that
# with `devicectl device appResize`, on each FOLD-STEP line a test prints alone
# on its line, moving the app to the simulator's resizable display at the
# inner display's size on its side (open) or upright (open-tall) and back
# (close).
#
# Against the local deployment on 127.0.0.1:8080, or TOMOSHIBI_UITEST_BASE,
# signed in as a user whose passphrase is in a file the app reads (never on
# the command line):
#   apple/fold-ui-tests.sh <Duo simulator udid> <passphrase file> [test ...]
# A test is a FoldStateTests method by its name, or Class or Class/method of
# any other class; with none, FoldStateTests runs whole. TOMOSHIBI_UITEST_USER
# names the user (uitest by default). The build and the log go to
# $TMPDIR/tomoshibi-fold-tests.noindex.
set -u
export DEVELOPER_DIR=${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}
HERE=$(cd "$(dirname "$0")" && pwd)
DEVICE=$1
PASSFILE=$2
shift 2
WORK=${TMPDIR:-/tmp}/tomoshibi-fold-tests.noindex
mkdir -p "$WORK"
LOG=$WORK/tests.log
only=()
if [ $# -eq 0 ]; then only=(-only-testing:TomoshibiUITests/FoldStateTests); fi
for t in "$@"; do
    case "$t" in
        */*|*Tests) only+=(-only-testing:TomoshibiUITests/$t) ;;
        *) only+=(-only-testing:TomoshibiUITests/FoldStateTests/$t) ;;
    esac
done

resize_pid=""
open_phone() {
    xcrun devicectl device appResize start --device "$DEVICE" --preferred-size "${1:-951x669}" > "$WORK/resize.log" 2>&1 &
    resize_pid=$!
}
close_phone() {
    # The session moves the app back to the outer display when it ends.
    if [ -n "$resize_pid" ]; then kill -INT "$resize_pid" 2>/dev/null; wait "$resize_pid" 2>/dev/null; resize_pid=""; fi
}
trap close_phone EXIT

# Handed to the test runner from xcodebuild's own environment, the prefix
# taken off. Every launch the tests make carries the local base: one without
# it reaches the production server.
export TEST_RUNNER_FOLD_DRIVEN=1
export TEST_RUNNER_TOMOSHIBI_BASE=${TOMOSHIBI_UITEST_BASE:-http://127.0.0.1:8080}
export TEST_RUNNER_TOMOSHIBI_MUTE=1
export TEST_RUNNER_TOMOSHIBI_SIGNIN="${TOMOSHIBI_UITEST_USER:-uitest}:$PASSFILE"
xcodebuild test -project "$HERE/Tomoshibi.xcodeproj" -scheme Tomoshibi-iOS \
    -destination "platform=iOS Simulator,id=$DEVICE" -derivedDataPath "$WORK/build" \
    "${only[@]}" > "$LOG" 2>&1 &
tests=$!
seen=0
while kill -0 $tests 2>/dev/null; do
    # As text whatever the log holds: once it held a byte that was not, grep
    # took it for binary, the steps went unseen, and a test waited for a move
    # that never came.
    # Only lines that are the step and nothing else: the build's own
    # warnings quote the test's source, print("FOLD-STEP open") among it.
    steps=$(grep -a -c -E '^FOLD-STEP (open|open-tall|close)$' "$LOG")
    while [ $seen -lt $steps ]; do
        seen=$((seen + 1))
        case "$(grep -a -E '^FOLD-STEP (open|open-tall|close)$' "$LOG" | sed -n "${seen}p")" in
            "FOLD-STEP open") echo "opening ($(date +%T))"; open_phone 951x669 ;;
            "FOLD-STEP open-tall") echo "opening upright ($(date +%T))"; open_phone 669x951 ;;
            "FOLD-STEP close") echo "closing ($(date +%T))"; close_phone ;;
        esac
    done
    sleep 1
done
wait $tests
result=$?
grep -a -E "Test Case .*(passed|failed|skipped)|error: -\[|\*\* TEST" "$LOG" | sort -u
exit $result
