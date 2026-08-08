#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run test:engine-revision
pnpm run engine:freshness
pnpm run audit:boundary
pnpm run typecheck
pnpm run check:content
pnpm run test:ts
pnpm run test:platform
pnpm run test:shell
pnpm run test:performance-tools
pnpm run build:shell
pnpm run test:studio
pnpm run build:studio
./scripts/verify-rust.sh
./scripts/verify-native-host.sh
node scripts/doom-browser-smoke.mjs
pnpm run test:browser
