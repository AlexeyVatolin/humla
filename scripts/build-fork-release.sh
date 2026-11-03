#!/usr/bin/env bash
set -euo pipefail

node scripts/check-release-notes.mjs "${RELEASE_VERSION:?}"
pnpm tauri build --bundles app

APP=src-tauri/target/release/bundle/macos/Humla.app
OUT=src-tauri/target/release/bundle/dmg
DMG="$OUT/Humla_${RELEASE_VERSION}_aarch64.dmg"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT

codesign --verify --deep --strict "$APP"
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
