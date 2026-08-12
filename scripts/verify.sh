#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run audit:boundary
pnpm run typecheck
pnpm run check:content
pnpm run test:ts
pnpm run test:platform
pnpm run test:shell
pnpm run test:engine-route
pnpm run test:performance-tools
pnpm run build:shell
./scripts/verify-rust.sh
./scripts/verify-native-host.sh
pnpm run smoke:e1m1
