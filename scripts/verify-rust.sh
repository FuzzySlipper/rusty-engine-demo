#!/usr/bin/env bash
# Rust gate (task 7052): format check, lockfile consistency, the gameplay
# package parity bin, focused tests, and clippy on the two product crates.
# The forbidden-dependency grep ceremony moved to the on-demand
# `pnpm run audit:boundary`, which enforces the same rules repo-wide.
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

cargo fmt --all --check

cargo metadata --format-version 1 --locked --no-deps > /dev/null
cargo run --locked -p loading-bay-gameplay --bin gameplay-package-check
cargo test --locked -p loading-bay-gameplay -p loading-bay-game
cargo clippy --locked -p loading-bay-gameplay -p loading-bay-game --all-targets -- -D warnings
