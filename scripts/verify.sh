#!/usr/bin/env bash
# Lean demo verification gate: C# managed/NativeAOT product authority, the
# browser shell, immutable E1M1 source/provenance, and boundary checks.
# Browser interaction and desktop packaging remain relevance-triggered checks.
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run typecheck
pnpm run check:content
pnpm run test:ts
pnpm run build:shell
./scripts/verify-csharp-spine.sh
pnpm run audit:boundary
