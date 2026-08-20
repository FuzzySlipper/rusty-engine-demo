#!/usr/bin/env bash
# Lean demo verification gate: typecheck, authored-package drift, canonical
# E1M1 content, TS unit tests, browser shell build, and Rust authority.
# Browser interaction and Tauri packaging are relevance-triggered checks.
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run typecheck
pnpm run gameplay:check
pnpm run check:content
pnpm run test:ts
pnpm run build:shell
./scripts/verify-rust.sh
