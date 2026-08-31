#!/bin/bash
# demo-window.sh — one-command real-machine demo for UnlingerApp.
#
# Ensures an isolated report-only source daemon is running (own temp
# database/socket/instance-lock — never touches the installed generation),
# then opens the windowed app pointed at it. Idempotent: re-running while
# everything is up just opens a fresh window.
#
#   scripts/demo-window.sh        start daemon if needed + open window
#   scripts/demo-window.sh stop   quit the app, stop the daemon, remove state
set -euo pipefail

cd "$(dirname "$0")/.."
REPO_ROOT="$(cd ../.. && pwd)"

STAGING="${TMPDIR%/}/unlinger-demo"
# The daemon refuses anything but an owner-private 0700 socket parent.
mkdir -p -m 700 "$STAGING"
chmod 700 "$STAGING"
# The daemon rejects symlinked lock paths; TMPDIR is usually a /var symlink.
DEMO_DIR="$(realpath "$STAGING")"
SOCK="$DEMO_DIR/unlingerd.sock"
DAEMON_PATTERN="unlingerd --report-only --database $DEMO_DIR/"

if [[ "${1:-}" == "stop" ]]; then
    killall UnlingerApp 2>/dev/null || true
    pkill -f "$DAEMON_PATTERN" 2>/dev/null || true
    rm -rf "$DEMO_DIR"
    echo "demo stopped and state removed"
    exit 0
fi

if ! pgrep -f "$DAEMON_PATTERN" >/dev/null; then
    (cd "$REPO_ROOT" && cargo build -p unlinger-daemon)
    nohup "$REPO_ROOT/target/debug/unlingerd" --report-only \
        --database "$DEMO_DIR/history.sqlite3" \
        --socket "$SOCK" \
        --instance-lock "$DEMO_DIR/unlingerd.lock" \
        > "$DEMO_DIR/daemon.log" 2>&1 &
    for _ in $(seq 1 40); do [[ -S "$SOCK" ]] && break; sleep 0.5; done
    [[ -S "$SOCK" ]] || { echo "daemon socket never appeared; see $DEMO_DIR/daemon.log" >&2; exit 1; }
    echo "isolated report-only daemon up (state: $DEMO_DIR)"
fi

[[ -d build/Unlinger.app ]] || scripts/bundle.sh
UNLINGER_SOCKET_PATH="$SOCK" UNLINGER_WINDOW=1 \
    nohup build/Unlinger.app/Contents/MacOS/UnlingerApp >/dev/null 2>&1 &
echo "window opened — close it anytime; the Dock icon brings it back."
