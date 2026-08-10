#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

cargo fmt --all --check

FORBIDDEN_ASHA_ENGINE="asha""-engine"
FORBIDDEN_ASHA_DEMO="asha""-demo"
if rg -n "git\\s*=\\s*\".*rusty-engine|${FORBIDDEN_ASHA_ENGINE}|${FORBIDDEN_ASHA_DEMO}" Cargo.toml rust; then
  echo "forbidden remote/Asha dependency surfaced in active Rust source" >&2
  exit 1
fi
grep -F 'rusty-engine = { path = "../rusty-engine/rust/crates/rusty-engine" }' Cargo.toml >/dev/null

cargo metadata --format-version 1 --locked --no-deps > /dev/null
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -q --locked -p loading-bay-game --bin headless-door > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-encounter > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-beacon > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-beacon -- \
  --project content/projects/relay-annex.project.json > /dev/null
