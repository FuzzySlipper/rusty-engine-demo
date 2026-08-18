#!/usr/bin/env bash
# Lean demo verification gate (task 7052): typecheck, authored-package drift,
# canonical E1M1 content, TS unit tests, browser shell build, and the Rust
# gate. Boundary audit, native-host proof, and per-package TS suites remain
# available as explicit commands (audit:boundary, verify:native,
# test:platform/shell/engine-route) and in CI until task 7053 finalizes it.
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run typecheck
pnpm run gameplay:check
pnpm run check:content
pnpm run test:ts
pnpm run build:shell
./scripts/verify-rust.sh
