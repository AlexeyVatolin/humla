#!/usr/bin/env bash
set -euo pipefail

TAG="v${RELEASE_VERSION:?}"
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock src/content/releases.ts
git commit -m "Release $TAG"
git tag "$TAG"
git push --atomic origin "${SOURCE_COMMIT:?}:refs/heads/${FORK_BRANCH:?}" "refs/tags/$TAG"

gh release create "$TAG" --draft --title "Humla $RELEASE_VERSION" \
  --notes 'Russian transcription, local AI and Zoom meeting reminders.' \
  "src-tauri/target/release/bundle/dmg/Humla_${RELEASE_VERSION}_aarch64.dmg" \
  src-tauri/target/release/bundle/macos/Humla.app.tar.gz \
  src-tauri/target/release/bundle/macos/Humla.app.tar.gz.sig \
  src-tauri/target/release/bundle/latest.json
gh release edit "$TAG" --draft=false --latest
