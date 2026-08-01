#!/usr/bin/env bash
set -euo pipefail

DEMO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DEMO_ROOT"

pnpm run test:engine-revision
pnpm run audit:boundary
pnpm run typecheck
pnpm run check:content
pnpm run check:actor-animation
pnpm run test:ts
pnpm run test:platform
pnpm run test:shell
pnpm run test:performance-tools
pnpm run build:shell
pnpm run test:studio
pnpm run build:studio
./scripts/verify-rust.sh
pnpm run test:browser
