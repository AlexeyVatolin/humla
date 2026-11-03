#!/usr/bin/env bash
set -euo pipefail

bash scripts/build-sidecar.sh
bash scripts/build-diarize.sh
bash scripts/build-gigaam.sh
bash scripts/build-mcp.sh
HOST_TARGET=$(rustc -vV | sed -n 's/^host: //p')
cargo test --manifest-path src-tauri/Cargo.toml --release --lib --target "$HOST_TARGET"
swift test --package-path speaker-diarize -c release
