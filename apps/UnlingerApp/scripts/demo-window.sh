#!/bin/bash
# Foreground source demo. Ctrl-C or Quit stops only this invocation's children.
set -euo pipefail
if [[ $# -ne 0 ]]; then
    echo 'Run without arguments; stop in its original terminal with Ctrl-C or Quit the demo App.' >&2
    exit 2
fi
cd "$(dirname "$0")/.."
REPO_ROOT="$(cd ../.. && pwd)"
command -v python3 >/dev/null
cargo build --locked --manifest-path "$REPO_ROOT/Cargo.toml" -p unlinger-daemon
swift build
APP_BIN="$(swift build --show-bin-path)/UnlingerApp"
exec python3 scripts/demo-window.py "$REPO_ROOT/target/debug/unlingerd" "$APP_BIN"
