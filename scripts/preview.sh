#!/bin/bash
# One report-only observation with exclusively owned temporary state.
set -euo pipefail
umask 077
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PREVIEW_STAGE="$(mktemp -d "${TMPDIR:-/tmp}/unlinger-preview.XXXXXX")"
PREVIEW_DIR="$(cd "$PREVIEW_STAGE" && pwd -P)"
trap 'rm -rf "$PREVIEW_DIR"' EXIT
"$ROOT/target/release/unlingerd" --report-only --once \
    --database "$PREVIEW_DIR/history.sqlite3" \
    --socket "$PREVIEW_DIR/unlingerd.sock" \
    --instance-lock "$PREVIEW_DIR/instance.lock"
