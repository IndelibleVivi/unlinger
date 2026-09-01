#!/bin/bash
# Build UnlingerApp and assemble a private .app bundle.
#
# Produces build/Unlinger.app. The bundle ships LSUIElement=false and picks
# its activation policy at launch instead (menu-bar mode runs as an accessory
# agent; the UNLINGER_WINDOW=1 debug mode keeps a Dock icon so a closed window
# can be reopened). Also embeds the SwiftPM resource bundle (localizations)
# and an ad-hoc code signature so macOS will launch it.
set -euo pipefail

cd "$(dirname "$0")/.."

CONFIG="${CONFIG:-release}"
OUT="build/Unlinger.app"
BIN_DIR="$(swift build -c "$CONFIG" --show-bin-path)"
RES_BUNDLE="$BIN_DIR/UnlingerApp_UnlingerKit.bundle"

# SwiftPM can leave removed resource files in an incremental bundle. Delete
# only this target's derived bundle so the packaged app reflects current
# source exactly; compiler/object caches remain intact.
rm -rf "$RES_BUNDLE"

echo "==> swift build -c $CONFIG"
swift build -c "$CONFIG"

APP_BIN="$BIN_DIR/UnlingerApp"

[[ -x "$APP_BIN" ]] || { echo "missing executable at $APP_BIN" >&2; exit 1; }

echo "==> assembling $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "$APP_BIN" "$OUT/Contents/MacOS/UnlingerApp"
cp "Resources/Info.plist" "$OUT/Contents/Info.plist"
if [[ -f "Resources/AppIcon.icns" ]]; then
    cp "Resources/AppIcon.icns" "$OUT/Contents/Resources/"
fi
if [[ -d "$RES_BUNDLE" ]]; then
    cp -R "$RES_BUNDLE" "$OUT/Contents/Resources/"
fi
# Ship the canonical fixtures (synthetic/redacted) inside the resource bundle
# so fixture scenarios work from a standalone .app away from the source tree.
if [[ -d "$RES_BUNDLE" ]]; then
    mkdir -p "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures"
    cp Contract/v3/*.json "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures/"
fi

echo "==> ad-hoc codesign"
codesign --force --sign - "$OUT"

echo "==> verifying"
codesign --verify --deep --strict "$OUT"
plutil -lint "$OUT/Contents/Info.plist"
codesign -dv "$OUT" 2>&1 | grep -E "Identifier|Signature" || true
/usr/libexec/PlistBuddy -c "Print :LSUIElement" "$OUT/Contents/Info.plist"
APP_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$OUT/Contents/Info.plist")"
[[ -n "$APP_VERSION" ]] || { echo "missing app version" >&2; exit 1; }

FIXTURE_DIR="$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures"
[[ -f "$FIXTURE_DIR/status-all-clear.json" ]] || { echo "missing v3 status fixture" >&2; exit 1; }
[[ -f "$FIXTURE_DIR/mutation-committed.json" ]] || { echo "missing v3 mutation fixture" >&2; exit 1; }
[[ -f "$FIXTURE_DIR/diagnostics.json" ]] || { echo "missing v3 diagnostics fixture" >&2; exit 1; }
if rg -l '"schema_version":2' "$FIXTURE_DIR" >/dev/null; then
    echo "stale v2 daemon fixture packaged as active" >&2
    exit 1
fi

EN_COPY="$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/en.lproj/Localizable.strings"
ZH_COPY="$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/zh-Hans.lproj/Localizable.strings"
[[ -f "$EN_COPY" && -f "$ZH_COPY" ]] || { echo "missing localization payload" >&2; exit 1; }
plutil -lint "$EN_COPY" "$ZH_COPY"

echo "done: $OUT"
