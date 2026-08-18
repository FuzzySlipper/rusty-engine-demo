#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

cargo fmt --all --check

FORBIDDEN_ASHA_ENGINE="asha""-engine"
FORBIDDEN_ASHA_DEMO="asha""-demo"
if rg -n "git\\s*=\\s*\".*rusty-engine|${FORBIDDEN_ASHA_ENGINE}|${FORBIDDEN_ASHA_DEMO}" Cargo.toml gameplay/Cargo.toml rust/crates/loading-bay-game/Cargo.toml rust gameplay/src; then
  echo "forbidden remote/Asha dependency surfaced in active Rust source" >&2
  exit 1
fi
grep -F 'rusty-engine = { path = "../rusty-engine/rust/crates/rusty-engine" }' Cargo.toml >/dev/null

cargo metadata --format-version 1 --locked --no-deps > /dev/null
cargo run --locked -p loading-bay-gameplay --bin gameplay-package-check
cargo test --locked -p loading-bay-gameplay -p loading-bay-game
cargo clippy --locked -p loading-bay-gameplay -p loading-bay-game --all-targets -- -D warnings
