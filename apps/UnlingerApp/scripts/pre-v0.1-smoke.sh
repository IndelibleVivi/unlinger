#!/bin/bash
# Isolated pre-v0.1 v5 App socket smoke. This script owns every path it creates,
# starts only a source report-only daemon, and never addresses the installed
# generation-15 database, socket, plist, process, or service CLI.
set -euo pipefail
umask 077

cd "$(dirname "$0")/.."
REPO_ROOT="$(cd ../.. && pwd)"
SMOKE_STAGE="$(mktemp -d "${TMPDIR%/}/unlinger-pre-v01.XXXXXX")"
chmod 700 "$SMOKE_STAGE"
SMOKE_DIR="$(realpath "$SMOKE_STAGE")"
SOCKET="$SMOKE_DIR/unlingerd.sock"
DATABASE="$SMOKE_DIR/history.sqlite3"
LOCK="$SMOKE_DIR/unlingerd.lock"
LOG="$SMOKE_DIR/unlingerd.log"
DAEMON_PID=""
EXPECTED_LIVE_TESTS="$(swift test list | rg -c '^UnlingerAppTests\.LiveSocketTests/')"
if [[ "$EXPECTED_LIVE_TESTS" -lt 1 ]]; then
    echo "no LiveSocketTests were discovered" >&2
    exit 1
fi

cleanup() {
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
        kill -TERM "$DAEMON_PID"
        wait "$DAEMON_PID" || true
    fi
    rm -rf "$SMOKE_DIR"
}
trap cleanup EXIT INT TERM

start_daemon() {
    : > "$LOG"
    "$REPO_ROOT/target/debug/unlingerd" \
        --report-only \
        --interval-seconds 3600 \
        --database "$DATABASE" \
        --socket "$SOCKET" \
        --instance-lock "$LOCK" \
        >> "$LOG" 2>&1 &
    DAEMON_PID="$!"
    for _ in $(seq 1 100); do
        if [[ -S "$SOCKET" ]]; then return 0; fi
        if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
            echo "isolated daemon exited before socket readiness" >&2
            sed -n '1,160p' "$LOG" >&2
            return 1
        fi
        sleep 0.1
    done
    echo "isolated daemon socket did not appear" >&2
    sed -n '1,160p' "$LOG" >&2
    return 1
}

run_live_tests() {
    local pass_name="$1"
    local test_log="$SMOKE_DIR/swift-$pass_name.log"
    UNLINGER_LIVE_SOCKET="$SOCKET" swift test \
        --filter 'UnlingerAppTests.LiveSocketTests' 2>&1 | tee "$test_log"
    if rg -q 'No matching test cases were run|Test run with 0 tests' "$test_log"; then
        echo "live socket filter matched zero tests" >&2
        return 1
    fi
    if ! rg -q -F "Test run with $EXPECTED_LIVE_TESTS tests" "$test_log"; then
        echo "live socket pass did not execute all $EXPECTED_LIVE_TESTS discovered tests" >&2
        return 1
    fi
}

assert_private_mode() {
    local path="$1"
    local expected="$2"
    if [[ "$(stat -f '%Lp' "$path")" != "$expected" ]]; then
        echo "$path is not mode $expected" >&2
        return 1
    fi
}

echo "==> build isolated source daemon"
(cd "$REPO_ROOT" && cargo build -p unlinger-daemon)

echo "==> first report-only v5 App socket pass"
start_daemon
run_live_tests first

if lsof -a -p "$DAEMON_PID" -i >/dev/null 2>&1; then
    echo "isolated daemon unexpectedly owns an IP socket" >&2
    lsof -a -p "$DAEMON_PID" -i >&2
    exit 1
fi
assert_private_mode "$SMOKE_DIR" 700
assert_private_mode "$DATABASE" 600
assert_private_mode "$LOCK" 600
assert_private_mode "$LOG" 600
assert_private_mode "$SOCKET" 600

echo "==> restart the same isolated report-only database"
kill -TERM "$DAEMON_PID"
wait "$DAEMON_PID"
DAEMON_PID=""
rm -f "$SOCKET"
start_daemon

echo "==> second pass proves reconnect and durable receipt replay"
run_live_tests second

echo "isolated pre-v0.1 v5 App report-only smoke passed"
