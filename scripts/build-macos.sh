#!/usr/bin/env bash
# Build the macOS packages (docs/MACOS.md §5): one universal app (Intel and Apple
# silicon in one binary), ad-hoc signed — there is no Apple Developer ID yet, so it
# is not notarized — as a .dmg and as a zipped .app. Run on a Mac with the Rust
# targets aarch64-apple-darwin + x86_64-apple-darwin and the UI dependencies
# installed (pnpm install in apps/nanofreedisk).
#
#   scripts/build-macos.sh <out-dir> <version>
set -euo pipefail

OUT="$1"
VERSION="$2"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET=universal-apple-darwin
NAME="NANOFreeDisk-$VERSION-macos-universal"

# with the updater key: also NANOFreeDisk.app.tar.gz + .sig for in-app updates
UPDATER=()
[[ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]] && UPDATER=(--config src-tauri/tauri.updater.conf.json)
(cd "$ROOT/apps/nanofreedisk" && pnpm tauri build --target "$TARGET" --ci "${UPDATER[@]}")

BUNDLE="$ROOT/target/$TARGET/release/bundle"
APP="$BUNDLE/macos/NANOFreeDisk.app"
[[ -d "$APP" ]] || { echo "::error::$APP was not built"; exit 1; }

# both architectures, a valid (ad-hoc) signature, the minimum macOS we promise
for bin in "$APP"/Contents/MacOS/*; do
  archs=$(lipo -archs "$bin")
  echo "$(basename "$bin"): $archs"
  [[ "$archs" == *x86_64* && "$archs" == *arm64* ]] || { echo "::error::$bin is not universal ($archs)"; exit 1; }
done
codesign --verify --deep --strict --verbose=2 "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Signature|Identifier|flags" || true
min=$(/usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$APP/Contents/Info.plist")
echo "LSMinimumSystemVersion: $min"
[[ "$min" == "11.0" ]] || { echo "::error::unexpected LSMinimumSystemVersion $min"; exit 1; }
# src-tauri/Info.plist was merged: the folder-access prompts explain themselves
/usr/libexec/PlistBuddy -c "Print :NSDocumentsFolderUsageDescription" "$APP/Contents/Info.plist" >/dev/null ||
  { echo "::error::Info.plist usage descriptions are missing"; exit 1; }

mkdir -p "$OUT"
dmg=$(ls "$BUNDLE"/dmg/*.dmg | head -1)
cp "$dmg" "$OUT/$NAME.dmg"
# ditto keeps the signature, symlinks and extended attributes of the bundle
ditto -c -k --sequesterRsrc --keepParent "$APP" "$OUT/$NAME.app.zip"
if [[ -f "$BUNDLE/macos/NANOFreeDisk.app.tar.gz.sig" ]]; then
  cp "$BUNDLE/macos/NANOFreeDisk.app.tar.gz" "$OUT/$NAME.app.tar.gz"
  cp "$BUNDLE/macos/NANOFreeDisk.app.tar.gz.sig" "$OUT/$NAME.app.tar.gz.sig"
fi
(cd "$OUT" && shasum -a 256 "$NAME.dmg" "$NAME.app.zip")
