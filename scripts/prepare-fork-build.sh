#!/usr/bin/env bash
set -euo pipefail

test "$(uname -m)" = arm64

bash scripts/build-sidecar.sh
bash scripts/build-diarize.sh
bash scripts/build-gigaam.sh
bash scripts/build-mcp.sh
cargo test --manifest-path src-tauri/Cargo.toml --release --lib
swift test --package-path speaker-diarize -c release
