#!/bin/bash
# Build UnlingerApp and assemble a private .app bundle.
#
# Produces build/Unlinger.app. The bundle ships LSUIElement=false and remains a
# regular Dock app alongside its status item, so a closed window can be reopened
# even when a third-party menu host cannot resolve that item. The packaged build
# uses an internal scratch path and strips
# removable-volume toolchain rpaths before signing, so launching the installed
# app never needs access to the source/build volume. It also embeds the SwiftPM
# resource bundle (localizations) and an ad-hoc code signature.
set -euo pipefail

cd "$(dirname "$0")/.."

CONFIG="${CONFIG:-release}"
OUT="build/Unlinger.app"
SCRATCH_PATH="${UNLINGER_SWIFTPM_SCRATCH_PATH:-${TMPDIR%/}/UnlingerSwiftPMBuild}"
mkdir -p "$SCRATCH_PATH"
BIN_DIR="$(swift build -c "$CONFIG" --scratch-path "$SCRATCH_PATH" --show-bin-path)"
RES_BUNDLE="$BIN_DIR/UnlingerApp_UnlingerKit.bundle"

# SwiftPM can leave removed resource files in an incremental bundle. Delete
# only this target's derived bundle so the packaged app reflects current
# source exactly; compiler/object caches remain intact.
rm -rf "$RES_BUNDLE"

echo "==> swift build -c $CONFIG"
swift build -c "$CONFIG" --scratch-path "$SCRATCH_PATH"

APP_BIN="$BIN_DIR/UnlingerApp"

[[ -x "$APP_BIN" ]] || { echo "missing executable at $APP_BIN" >&2; exit 1; }

echo "==> assembling $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS" "$OUT/Contents/Resources"
cp "$APP_BIN" "$OUT/Contents/MacOS/UnlingerApp"
cp "Resources/Info.plist" "$OUT/Contents/Info.plist"

# Xcode itself may live on a removable volume. SwiftPM then records that
# toolchain directory as an LC_RPATH even though the packaged app uses the
# system Swift runtime. dyld probing that path at launch triggers an unrelated
# removable-volume permission prompt under the app's identity.
while IFS= read -r rpath; do
    case "$rpath" in
        /Volumes/*)
            install_name_tool -delete_rpath "$rpath" "$OUT/Contents/MacOS/UnlingerApp"
            ;;
    esac
done < <(
    otool -l "$OUT/Contents/MacOS/UnlingerApp" | awk '
        $1 == "cmd" && $2 == "LC_RPATH" { capture = 1; next }
        capture && $1 == "path" {
            line = $0
            sub(/^[[:space:]]*path /, "", line)
            sub(/ \(offset [0-9]+\)$/, "", line)
            print line
            capture = 0
        }
    '
)

if otool -l "$OUT/Contents/MacOS/UnlingerApp" | rg -q 'path /Volumes/'; then
    echo "packaged executable retains a removable-volume LC_RPATH" >&2
    exit 1
fi
if strings "$OUT/Contents/MacOS/UnlingerApp" | rg -q '/Volumes/.*UnlingerApp_UnlingerKit\.bundle'; then
    echo "packaged resource fallback points at a removable volume" >&2
    exit 1
fi
if [[ -f "Resources/AppIcon.icns" ]]; then
    cp "Resources/AppIcon.icns" "$OUT/Contents/Resources/"
fi
if [[ -d "$RES_BUNDLE" ]]; then
    cp -R "$RES_BUNDLE" "$OUT/Contents/Resources/"
fi
# Ship the canonical fixtures (synthetic/redacted) inside the resource bundle
# so fixture scenarios work from a standalone .app away from the source tree.
if [[ -d "$RES_BUNDLE" ]]; then
    mkdir -p "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures/v3"
    mkdir -p "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures/v4"
    cp Contract/v3/*.json "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures/v3/"
    cp Contract/v4/*.json "$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/Fixtures/v4/"
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
[[ -f "$FIXTURE_DIR/v3/status-all-clear.json" ]] || { echo "missing v3 status fixture" >&2; exit 1; }
[[ -f "$FIXTURE_DIR/v3/mutation-committed.json" ]] || { echo "missing v3 mutation fixture" >&2; exit 1; }
[[ -f "$FIXTURE_DIR/v3/diagnostics.json" ]] || { echo "missing v3 diagnostics fixture" >&2; exit 1; }
[[ -f "$FIXTURE_DIR/v4/browser-overview-confirmed.json" ]] || { echo "missing v4 overview fixture" >&2; exit 1; }
if rg -l '"schema_version":2' "$FIXTURE_DIR" >/dev/null; then
    echo "stale v2 daemon fixture packaged as active" >&2
    exit 1
fi

EN_COPY="$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/en.lproj/Localizable.strings"
ZH_COPY="$OUT/Contents/Resources/UnlingerApp_UnlingerKit.bundle/zh-Hans.lproj/Localizable.strings"
[[ -f "$EN_COPY" && -f "$ZH_COPY" ]] || { echo "missing localization payload" >&2; exit 1; }
plutil -lint "$EN_COPY" "$ZH_COPY"

echo "done: $OUT"
