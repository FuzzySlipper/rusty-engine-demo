#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

cargo fmt --all --check

FORBIDDEN_ASHA_ENGINE="asha""-engine"
FORBIDDEN_ASHA_DEMO="asha""-demo"
if rg -n "path\\s*=\\s*\".*rusty-engine|${FORBIDDEN_ASHA_ENGINE}|${FORBIDDEN_ASHA_DEMO}" Cargo.toml rust; then
  echo "forbidden sibling/Asha dependency surfaced in active Rust source" >&2
  exit 1
fi

cargo metadata --format-version 1 --locked --no-deps > /dev/null

EXPECTED_ENGINE_SOURCE='git+https://github.com/FuzzySlipper/rusty-engine.git?rev=b2ef146904082178f5edcd943af95fa4e7c5ce22#b2ef146904082178f5edcd943af95fa4e7c5ce22'
RESOLVED_GIT_SOURCES="$(sed -n 's/^source = "\(git+[^\"]*\)"$/\1/p' Cargo.lock | sort -u)"
if [[ "$RESOLVED_GIT_SOURCES" != "$EXPECTED_ENGINE_SOURCE" ]]; then
  echo "Cargo.lock does not resolve exactly the reviewed Rusty Engine revision" >&2
  printf '%s\n' "$RESOLVED_GIT_SOURCES" >&2
  exit 1
fi

cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -q --locked -p loading-bay-game --bin headless-door > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-encounter > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-beacon > /dev/null
cargo run -q --locked -p loading-bay-game --bin headless-beacon -- \
  --project content/projects/relay-annex.project.json > /dev/null
