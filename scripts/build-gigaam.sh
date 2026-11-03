#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

ARCH=${BUILD_ARCH:-$(uname -m)}
case "$ARCH" in
  arm64) TRIPLE="aarch64-apple-darwin" ;;
  x86_64) TRIPLE="x86_64-apple-darwin" ;;
  *) exit 1 ;;
esac

cargo build --manifest-path src-tauri/Cargo.toml --release -p humla-gigaam --target "$TRIPLE"
mkdir -p src-tauri/binaries
cp "src-tauri/target/$TRIPLE/release/humla-gigaam" "src-tauri/binaries/humla-gigaam-$TRIPLE"
