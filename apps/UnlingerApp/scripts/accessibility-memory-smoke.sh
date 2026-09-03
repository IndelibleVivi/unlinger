#!/bin/bash
# Reproduce the history/Accessibility path against a fixture-only App with an
# external RSS guard. The child never connects to the installed daemon and the
# script signals only the exact process it creates.
set -euo pipefail

cd "$(dirname "$0")/.."

DURATION_SECONDS="${UNLINGER_AX_SMOKE_SECONDS:-120}"
RSS_LIMIT_MIB="${UNLINGER_AX_RSS_LIMIT_MIB:-384}"
GROWTH_LIMIT_MIB="${UNLINGER_AX_GROWTH_LIMIT_MIB:-96}"

for value in "$DURATION_SECONDS" "$RSS_LIMIT_MIB" "$GROWTH_LIMIT_MIB"; do
    case "$value" in
        ''|*[!0-9]*)
            echo "smoke limits must be positive integers" >&2
            exit 2
            ;;
    esac
    [[ "$value" -gt 0 ]] || { echo "smoke limits must be positive integers" >&2; exit 2; }
done

CONSOLE_STATE="$(/usr/sbin/ioreg -n Root -d1)"
if [[ "$CONSOLE_STATE" == *'"CGSSessionScreenIsLocked"=Yes'* ]]; then
    echo "Accessibility memory smoke requires an unlocked console session" >&2
    exit 3
fi

scripts/bundle.sh

APP_BIN="$PWD/build/Unlinger.app/Contents/MacOS/UnlingerApp"
PROBE_SOURCE="$PWD/scripts/accessibility-tree-probe.swift"
WORK_DIR="$(mktemp -d "${TMPDIR%/}/unlinger-ax-memory-smoke.XXXXXX")"
chmod 700 "$WORK_DIR"
PROBE_BIN="$WORK_DIR/accessibility-tree-probe"
APP_PID=""
PROBE_PID=""
REMOVE_WORK_DIR=0

stop_owned_process() {
    local pid="$1"
    local expected="$2"
    [[ -n "$pid" ]] || return 0
    kill -0 "$pid" 2>/dev/null || return 0
    local command
    command="$(ps -p "$pid" -o command= 2>/dev/null || true)"
    [[ "$command" == "$expected"* ]] || {
        echo "refusing to signal unexpected pid $pid: $command" >&2
        return 1
    }
    kill -TERM "$pid"
    for _ in $(seq 1 20); do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" 2>/dev/null || true
            return 0
        fi
        if [[ "$(ps -p "$pid" -o state= 2>/dev/null | tr -d ' ')" == Z* ]]; then
            wait "$pid" 2>/dev/null || true
            return 0
        fi
        sleep 0.1
    done
    kill -KILL "$pid"
    wait "$pid" 2>/dev/null || true
}

cleanup() {
    local exit_status=$?
    if [[ -n "$PROBE_PID" ]] && kill -0 "$PROBE_PID" 2>/dev/null; then
        kill -TERM "$PROBE_PID" 2>/dev/null || true
        wait "$PROBE_PID" 2>/dev/null || true
    fi
    stop_owned_process "$APP_PID" "$APP_BIN" || true
    if [[ "$REMOVE_WORK_DIR" -eq 1 && "$exit_status" -eq 0 ]]; then
        case "$WORK_DIR" in
            "${TMPDIR%/}"/unlinger-ax-memory-smoke.*)
                [[ -d "$WORK_DIR" ]] && rm -rf "$WORK_DIR"
                ;;
            *)
                echo "refusing to remove unexpected work directory: $WORK_DIR" >&2
                ;;
        esac
    else
        echo "retained failed smoke evidence: $WORK_DIR" >&2
    fi
}
trap cleanup EXIT INT TERM

/usr/bin/xcrun swiftc \
    -framework ApplicationServices \
    "$PROBE_SOURCE" \
    -o "$PROBE_BIN"

UNLINGER_FIXTURE=browser-history-stress \
UNLINGER_FIXTURE_ROUTE=history \
UNLINGER_WINDOW=1 \
    "$APP_BIN" >"$WORK_DIR/app.log" 2>&1 &
APP_PID=$!

for _ in $(seq 1 40); do
    kill -0 "$APP_PID" 2>/dev/null || {
        echo "fixture App exited before its window became accessible" >&2
        sed -n '1,160p' "$WORK_DIR/app.log" >&2
        exit 1
    }
    if "$PROBE_BIN" "$APP_PID" >"$WORK_DIR/first-probe.log" 2>"$WORK_DIR/probe-error.log"; then
        break
    fi
    sleep 0.25
done

[[ -s "$WORK_DIR/first-probe.log" ]] || {
    echo "fixture App window never became accessible" >&2
    sed -n '1,160p' "$WORK_DIR/probe-error.log" >&2
    exit 1
}

"$PROBE_BIN" "$APP_PID" "$DURATION_SECONDS" >"$WORK_DIR/probe.log" 2>>"$WORK_DIR/probe-error.log" &
PROBE_PID=$!

RSS_LIMIT_KIB=$((RSS_LIMIT_MIB * 1024))
GROWTH_LIMIT_KIB=$((GROWTH_LIMIT_MIB * 1024))
INITIAL_RSS_KIB=""
MAX_RSS_KIB=0
FINAL_RSS_KIB=0

while kill -0 "$PROBE_PID" 2>/dev/null; do
    kill -0 "$APP_PID" 2>/dev/null || {
        echo "fixture App exited during Accessibility probing" >&2
        sed -n '1,160p' "$WORK_DIR/app.log" >&2
        exit 1
    }
    RSS_KIB="$(ps -p "$APP_PID" -o rss= | tr -d ' ')"
    [[ -n "$RSS_KIB" ]] || { echo "could not read fixture App RSS" >&2; exit 1; }
    [[ -n "$INITIAL_RSS_KIB" ]] || INITIAL_RSS_KIB="$RSS_KIB"
    FINAL_RSS_KIB="$RSS_KIB"
    printf '%s %s\n' "$(date +%s)" "$RSS_KIB" >>"$WORK_DIR/rss.log"
    (( RSS_KIB > MAX_RSS_KIB )) && MAX_RSS_KIB="$RSS_KIB"
    if (( RSS_KIB >= RSS_LIMIT_KIB )); then
        echo "fixture App crossed RSS cutoff: ${RSS_KIB} KiB >= ${RSS_LIMIT_KIB} KiB" >&2
        stop_owned_process "$APP_PID" "$APP_BIN"
        exit 1
    fi
    sleep 1
done

wait "$PROBE_PID" || {
    echo "Accessibility probe failed" >&2
    sed -n '1,160p' "$WORK_DIR/probe-error.log" >&2
    tail -n 20 "$WORK_DIR/rss.log" >&2 || true
    sed -n '1,160p' "$WORK_DIR/app.log" >&2
    exit 1
}
PROBE_PID=""

FINAL_RSS_KIB="$(ps -p "$APP_PID" -o rss= | tr -d ' ')"
(( FINAL_RSS_KIB > MAX_RSS_KIB )) && MAX_RSS_KIB="$FINAL_RSS_KIB"
GROWTH_KIB=$((FINAL_RSS_KIB - INITIAL_RSS_KIB))
if (( GROWTH_KIB > GROWTH_LIMIT_KIB )); then
    echo "fixture App retained too much RSS: +${GROWTH_KIB} KiB > +${GROWTH_LIMIT_KIB} KiB" >&2
    exit 1
fi

PROBE_COUNT="$(wc -l <"$WORK_DIR/probe.log" | tr -d ' ')"
TRANSIENT_PROBE_FAILURES="$(wc -l <"$WORK_DIR/probe-error.log" | tr -d ' ')"
echo "Accessibility memory smoke passed: probes=$PROBE_COUNT transient_probe_failures=$TRANSIENT_PROBE_FAILURES initial_rss_kib=$INITIAL_RSS_KIB final_rss_kib=$FINAL_RSS_KIB max_rss_kib=$MAX_RSS_KIB"
REMOVE_WORK_DIR=1
