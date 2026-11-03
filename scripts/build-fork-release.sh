#!/usr/bin/env bash
set -euo pipefail

node scripts/check-release-notes.mjs "${RELEASE_VERSION:?}"
bash scripts/build-sidecar.sh
bash scripts/build-diarize.sh
bash scripts/build-gigaam.sh
bash scripts/build-mcp.sh
pnpm tauri build --bundles app --target "${BUILD_TARGET:?}"

APP="${BUNDLE_DIR:?}/macos/Humla.app"
OUT="$BUNDLE_DIR/dmg"
DMG="$OUT/Humla_${RELEASE_VERSION}_aarch64.dmg"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

codesign --verify --deep --strict "$APP"
file "$APP/Contents/MacOS/humla" | grep -q arm64
mkdir -p "$OUT"
ditto "$APP" "$STAGE/Humla.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname Humla -fs HFS+ -srcfolder "$STAGE" -ov -format UDZO "$DMG"
hdiutil verify "$DMG"
for previous in "$OUT"/*.dmg; do
  if [[ "$previous" != "$DMG" ]]; then
    rm -f -- "$previous" "$previous.sig"
  fi
done
node scripts/fork-update-manifest.mjs
