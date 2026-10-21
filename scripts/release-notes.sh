#!/usr/bin/env bash
# Release notes for a GitHub Release: the branded setup first and in bold (the
# recommended download), then the other packages, then the changelog section.
# Used by release.yml; also prints the notes for an existing release:
#   scripts/release-notes.sh <dist-dir|-> <version> <owner/repo> [changelog.md]
# With "-" instead of a folder, the file names are read from stdin (one per line),
# e.g. from `gh release view v0.1.0 --json assets -q '.assets[].name'`.
set -euo pipefail

DIST="$1"
VERSION="$2"
REPO="$3"
CHANGES="${4:-}"
BASE="https://github.com/$REPO/releases/download/v$VERSION"
SETUP="NANOFreeDisk-Setup-$VERSION-x64.exe"
DMG="NANOFreeDisk-$VERSION-macos-universal.dmg"

if [[ "$DIST" == "-" ]]; then
  mapfile -t FILES
else
  mapfile -t FILES < <(cd "$DIST" && ls -1)
fi

describe() {
  case "$1" in
    *-x64-setup.exe) echo "NSIS installer (per user, used by in-app updates)" ;;
    *-x64-vi-VN.msi) echo "MSI, Vietnamese UI (IT / Group Policy)" ;;
    *-x64.msi) echo "MSI, English UI (IT / Group Policy, winget)" ;;
    *-portable.zip) echo "Portable: unzip and run, data stays next to the exe" ;;
    *-macos-universal.dmg) echo "macOS 11+ (Intel and Apple silicon): open, drag to Applications" ;;
    *-macos-universal.app.zip) echo "macOS 11+: the same app, zipped" ;;
    SHA256SUMS.txt) echo "SHA-256 checksums of every file" ;;
    *.minisig) echo "minisign signature of SHA256SUMS.txt" ;;
    *.cdx.json) echo "Software bill of materials (CycloneDX)" ;;
    latest.json) echo "In-app updater manifest" ;;
    *) echo "" ;;
  esac
}

has_setup=false
has_dmg=false
for f in "${FILES[@]}"; do
  [[ "$f" == "$SETUP" ]] && has_setup=true
  [[ "$f" == "$DMG" ]] && has_dmg=true
done

echo "## NANOFreeDisk $VERSION"
echo
if $has_setup; then
  echo "### ⬇️ [**Download $SETUP**]($BASE/$SETUP)"
  echo
  echo "**Recommended for most people.** The NANOFreeDisk installer: pick the folder, install for you or for everyone on this PC, uninstall any time from Settings → Apps."
  echo
fi
if $has_dmg; then
  echo "### 🍎 macOS: [**$DMG**]($BASE/$DMG)"
  echo
  echo "One app for Intel and Apple silicon Macs, macOS 11 Big Sur or later. It is not notarized by Apple yet: the first time, right-click the app → **Open** (macOS 15+: System Settings → Privacy & Security → **Open Anyway**). See the README's macOS section."
  echo
fi
echo "| Package | What it is |"
echo "|---|---|"
if $has_setup; then
  echo "| ⭐ **[\`$SETUP\`]($BASE/$SETUP)** | **Recommended: NANOFreeDisk installer** |"
fi
for f in "${FILES[@]}"; do
  case "$f" in "$SETUP" | *.sig | "") continue ;; esac
  echo "| [\`$f\`]($BASE/$f) | $(describe "$f") |"
done
echo
echo "Verify any file with \`SHA256SUMS.txt\` (and \`SHA256SUMS.txt.minisig\` when present)."
echo
if [[ -n "$CHANGES" && -f "$CHANGES" ]]; then
  cat "$CHANGES"
fi
