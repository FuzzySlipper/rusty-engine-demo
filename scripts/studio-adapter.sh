#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo build --quiet --locked \
  --manifest-path "$ROOT/Cargo.toml" \
  --package loading-bay-game \
  --bin studio-adapter
exec "$ROOT/target/debug/studio-adapter"
